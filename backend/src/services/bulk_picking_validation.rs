use crate::database::Database;
use anyhow::{anyhow, Context, Result};
use bigdecimal::{BigDecimal, ToPrimitive};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tracing::{info, instrument};

/// Validation errors specific to bulk picking operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BulkPickingValidationError {
    /// Quantity picked exceeds what is required for the batch
    QuantityExceedsRequired {
        user_input_bags: f64,
        user_input_kg: f64,
        required_bags: f64,
        required_kg: f64,
        message: String,
    },
    /// Insufficient quantity available in lot
    InsufficientLotQuantity {
        requested_kg: f64,
        available_kg: f64,
        lot_no: String,
        bin_no: String,
    },
    /// Ingredient does not require bulk picking
    IngredientNotBulkPickable {
        item_key: String,
        to_picked_bulk_qty: f64,
    },
    /// Batch is already completed
    BatchCompleted {
        batch_no: String,
        picked_qty: f64,
        required_qty: f64,
    },
    /// Invalid lot for ingredient
    InvalidLotForIngredient {
        lot_no: String,
        item_key: String,
        run_no: i32,
    },
    /// Generic validation error
    ValidationFailed {
        field: String,
        message: String,
    },
}

impl std::fmt::Display for BulkPickingValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QuantityExceedsRequired { message, .. } => write!(f, "{}", message),
            Self::InsufficientLotQuantity { 
                requested_kg, 
                available_kg, 
                lot_no, 
                bin_no 
            } => write!(
                f, 
                "Insufficient quantity in lot {}. Requested: {:.2} KG, Available: {:.2} KG in bin {}", 
                lot_no, 
                requested_kg, 
                available_kg, 
                bin_no
            ),
            Self::IngredientNotBulkPickable { item_key, to_picked_bulk_qty } => write!(
                f, 
                "Ingredient {} does not require bulk picking (ToPickedBulkQty: {})", 
                item_key, 
                to_picked_bulk_qty
            ),
            Self::BatchCompleted { batch_no, picked_qty, required_qty } => write!(
                f, 
                "Batch {} is already completed ({} of {} picked)", 
                batch_no, 
                picked_qty, 
                required_qty
            ),
            Self::InvalidLotForIngredient { lot_no, item_key, run_no } => write!(
                f, 
                "Lot {} is not valid for ingredient {} in run {}", 
                lot_no, 
                item_key, 
                run_no
            ),
            Self::ValidationFailed { field, message } => write!(f, "{}: {}", field, message),
        }
    }
}

impl std::error::Error for BulkPickingValidationError {}

/// Request for validating a pick operation
#[derive(Debug, Clone, Deserialize)]
pub struct PickValidationRequest {
    pub run_no: i32,
    pub row_num: i32,
    pub line_id: i32,
    pub item_key: String,
    pub lot_no: String,
    pub bin_no: String,
    pub user_input_bags: f64,
    pub pack_size: f64,
}

/// Response for pick validation
#[derive(Debug, Clone, Serialize)]
pub struct PickValidationResponse {
    pub is_valid: bool,
    pub error: Option<BulkPickingValidationError>,
    pub warnings: Vec<String>,
    pub calculated_kg: f64,
    pub remaining_bags_after_pick: f64,
    pub remaining_kg_after_pick: f64,
    pub lot_available_qty: f64,
    pub batch_completion_status: String,
}

/// Service for comprehensive bulk picking validation
pub struct BulkPickingValidationService {
    database: Database,
}

impl BulkPickingValidationService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    /// Comprehensive validation for pick operation matching BME4 business rules
    #[instrument(skip(self))]
    pub async fn validate_pick_operation(
        &self,
        request: PickValidationRequest,
    ) -> Result<PickValidationResponse> {
        info!("Validating pick operation for run {}, item {}, lot {}", 
              request.run_no, request.item_key, request.lot_no);

        let mut warnings = Vec::new();
        let calculated_kg = request.user_input_bags * request.pack_size;

        // 1. Validate ingredient requires bulk picking
        let ingredient_info = self.get_ingredient_info(&request).await?;
        if ingredient_info.to_picked_bulk_qty <= 0.0 {
            return Ok(PickValidationResponse {
                is_valid: false,
                error: Some(BulkPickingValidationError::IngredientNotBulkPickable {
                    item_key: request.item_key.clone(),
                    to_picked_bulk_qty: ingredient_info.to_picked_bulk_qty,
                }),
                warnings,
                calculated_kg,
                remaining_bags_after_pick: 0.0,
                remaining_kg_after_pick: 0.0,
                lot_available_qty: 0.0,
                batch_completion_status: "NOT_REQUIRED".to_string(),
            });
        }

        // 2. Check if batch is already completed
        if ingredient_info.picked_bulk_qty >= ingredient_info.to_picked_bulk_qty {
            return Ok(PickValidationResponse {
                is_valid: false,
                error: Some(BulkPickingValidationError::BatchCompleted {
                    batch_no: format!("{}-{}", request.run_no, request.row_num),
                    picked_qty: ingredient_info.picked_bulk_qty,
                    required_qty: ingredient_info.to_picked_bulk_qty,
                }),
                warnings,
                calculated_kg,
                remaining_bags_after_pick: 0.0,
                remaining_kg_after_pick: 0.0,
                lot_available_qty: 0.0,
                batch_completion_status: "COMPLETED".to_string(),
            });
        }

        // 3. Validate quantity doesn't exceed required (BME4 critical validation)
        let remaining_bags_to_pick = ingredient_info.to_picked_bulk_qty - ingredient_info.picked_bulk_qty;
        if request.user_input_bags > remaining_bags_to_pick {
            let required_kg = remaining_bags_to_pick * request.pack_size;
            return Ok(PickValidationResponse {
                is_valid: false,
                error: Some(BulkPickingValidationError::QuantityExceedsRequired {
                    user_input_bags: request.user_input_bags,
                    user_input_kg: calculated_kg,
                    required_bags: remaining_bags_to_pick,
                    required_kg,
                    message: format!("Quantity picked is more than Qty Required {:.0}", required_kg),
                }),
                warnings,
                calculated_kg,
                remaining_bags_after_pick: remaining_bags_to_pick,
                remaining_kg_after_pick: required_kg,
                lot_available_qty: 0.0,
                batch_completion_status: "OVER_PICK_ATTEMPT".to_string(),
            });
        }

        // 4. Validate lot availability
        let lot_info = self.get_lot_availability(&request).await?;
        if calculated_kg > lot_info.available_qty {
            return Ok(PickValidationResponse {
                is_valid: false,
                error: Some(BulkPickingValidationError::InsufficientLotQuantity {
                    requested_kg: calculated_kg,
                    available_kg: lot_info.available_qty,
                    lot_no: request.lot_no.clone(),
                    bin_no: request.bin_no.clone(),
                }),
                warnings,
                calculated_kg,
                remaining_bags_after_pick: remaining_bags_to_pick,
                remaining_kg_after_pick: remaining_bags_to_pick * request.pack_size,
                lot_available_qty: lot_info.available_qty,
                batch_completion_status: "INSUFFICIENT_INVENTORY".to_string(),
            });
        }

        // 5. Add warnings for edge cases
        if calculated_kg > lot_info.available_qty * 0.9 {
            warnings.push(format!(
                "Warning: Pick will consume {:.1}% of available quantity in lot {}",
                (calculated_kg / lot_info.available_qty) * 100.0,
                request.lot_no
            ));
        }

        if request.user_input_bags == remaining_bags_to_pick {
            warnings.push("This pick will complete the current batch".to_string());
        }

        // Calculate post-pick state
        let remaining_bags_after_pick = remaining_bags_to_pick - request.user_input_bags;
        let remaining_kg_after_pick = remaining_bags_after_pick * request.pack_size;
        let batch_completion_status = if remaining_bags_after_pick == 0.0 {
            "WILL_COMPLETE".to_string()
        } else {
            "IN_PROGRESS".to_string()
        };

        Ok(PickValidationResponse {
            is_valid: true,
            error: None,
            warnings,
            calculated_kg,
            remaining_bags_after_pick,
            remaining_kg_after_pick,
            lot_available_qty: lot_info.available_qty,
            batch_completion_status,
        })
    }

    /// Real-time numpad validation for progressive feedback
    #[instrument(skip(self))]
    pub async fn validate_numpad_input(
        &self,
        run_no: i32,
        row_num: i32,
        item_key: &str,
        input_bags: f64,
        pack_size: f64,
    ) -> Result<NumpadValidationResponse> {
        let ingredient_info = self.get_ingredient_info_minimal(run_no, row_num, item_key).await?;
        let remaining_bags = ingredient_info.to_picked_bulk_qty - ingredient_info.picked_bulk_qty;
        let max_allowed_kg = remaining_bags * pack_size;
        let input_kg = input_bags * pack_size;

        let validation_status = if input_bags > remaining_bags {
            "EXCEEDS_REQUIRED"
        } else if input_bags == remaining_bags {
            "EXACT_MATCH"
        } else if input_bags > remaining_bags * 0.8 {
            "NEAR_COMPLETE"
        } else {
            "VALID"
        };

        Ok(NumpadValidationResponse {
            is_valid: input_bags <= remaining_bags,
            validation_status: validation_status.to_string(),
            input_bags,
            input_kg,
            remaining_bags,
            remaining_kg: max_allowed_kg,
            max_allowed_bags: remaining_bags,
            max_allowed_kg,
            completion_percentage: (input_bags / ingredient_info.to_picked_bulk_qty) * 100.0,
            warning_message: if input_bags > remaining_bags {
                Some(format!("Maximum allowed: {:.0} bags ({:.0} KG)", remaining_bags, max_allowed_kg))
            } else {
                None
            },
        })
    }

    /// Get ingredient information for validation
    async fn get_ingredient_info(&self, request: &PickValidationRequest) -> Result<IngredientValidationInfo> {
        let ingredients = self.database.get_bulk_run_ingredients(request.run_no).await?;
        
        let ingredient = ingredients
            .iter()
            .find(|ing| ing.item_key == request.item_key && ing.row_num == request.row_num)
            .ok_or_else(|| anyhow!("Ingredient not found for validation"))?;

        let to_picked_bulk_qty = ingredient.to_picked_bulk_qty
            .to_f64()
            .ok_or_else(|| anyhow!("Invalid ToPickedBulkQty format"))?;

        let picked_bulk_qty = ingredient.picked_bulk_qty.as_ref()
            .and_then(|p| p.to_f64())
            .unwrap_or(0.0);

        Ok(IngredientValidationInfo {
            to_picked_bulk_qty,
            picked_bulk_qty,
            pack_size: ingredient.pack_size
                .to_f64()
                .ok_or_else(|| anyhow!("Invalid PackSize format"))?,
        })
    }

    /// Get minimal ingredient information for numpad validation
    async fn get_ingredient_info_minimal(&self, run_no: i32, row_num: i32, item_key: &str) -> Result<IngredientValidationInfo> {
        // Simplified query for real-time numpad validation
        let ingredients = self.database.get_bulk_run_ingredients(run_no).await?;
        
        let ingredient = ingredients
            .iter()
            .find(|ing| ing.item_key == item_key && ing.row_num == row_num)
            .ok_or_else(|| anyhow!("Ingredient not found"))?;

        let to_picked_bulk_qty = ingredient.to_picked_bulk_qty
            .to_f64()
            .ok_or_else(|| anyhow!("Invalid ToPickedBulkQty format"))?;

        let picked_bulk_qty = ingredient.picked_bulk_qty.as_ref()
            .and_then(|p| p.to_f64())
            .unwrap_or(0.0);

        Ok(IngredientValidationInfo {
            to_picked_bulk_qty,
            picked_bulk_qty,
            pack_size: 0.0, // Not needed for numpad validation
        })
    }

    /// Get lot availability information
    async fn get_lot_availability(&self, request: &PickValidationRequest) -> Result<LotValidationInfo> {
        // Use existing lot bins method to get availability
        let bins = self.database.get_bins_for_lot(
            request.run_no,
            &request.lot_no,
            &request.item_key
        ).await?;

        let bin_info = bins
            .iter()
            .find(|bin| bin.bin_no == request.bin_no)
            .ok_or_else(|| anyhow!("Bin not found in lot"))?;

        let available_qty = bin_info.available_qty
            .to_f64()
            .ok_or_else(|| anyhow!("Invalid available quantity format"))?;

        Ok(LotValidationInfo {
            available_qty,
            lot_status: "P".to_string(), // Default lot status for pickable lots
        })
    }
}

/// Numpad validation response for real-time feedback
#[derive(Debug, Clone, Serialize)]
pub struct NumpadValidationResponse {
    pub is_valid: bool,
    pub validation_status: String,
    pub input_bags: f64,
    pub input_kg: f64,
    pub remaining_bags: f64,
    pub remaining_kg: f64,
    pub max_allowed_bags: f64,
    pub max_allowed_kg: f64,
    pub completion_percentage: f64,
    pub warning_message: Option<String>,
}

/// Internal validation info structures
#[derive(Debug)]
struct IngredientValidationInfo {
    to_picked_bulk_qty: f64,
    picked_bulk_qty: f64,
    pack_size: f64,
}

#[derive(Debug)]
struct LotValidationInfo {
    available_qty: f64,
    lot_status: String,
}