use crate::database::Database;
use crate::models::ingredient_intelligence::*;
use crate::models::bulk_runs::BulkPickedItem;
use anyhow::{Context, Result};
use bigdecimal::BigDecimal;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use tracing::{info, instrument};
use chrono::Utc;

/// Service for intelligent ingredient management and auto-switching
pub struct IngredientIntelligenceService {
    database: Database,
}

impl IngredientIntelligenceService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    /// Analyze all ingredients in a run and calculate completion statuses
    #[instrument(skip(self))]
    pub async fn analyze_run_ingredient_statuses(&self, run_no: i32) -> Result<Vec<IngredientBatchStatus>> {
        info!("Analyzing ingredient completion statuses for run: {}", run_no);

        // Get all ingredients for the run (only bulk picking ingredients)
        let ingredients = self
            .database
            .get_bulk_run_ingredients(run_no)
            .await
            .context("Failed to get run ingredients")?;

        let mut ingredient_statuses = Vec::new();

        for ingredient in ingredients {
            // Only analyze ingredients that require bulk picking
            if ingredient.to_picked_bulk_qty > BigDecimal::from(0) {
                let batch_status = self.calculate_ingredient_batch_status(&ingredient).await?;
                ingredient_statuses.push(batch_status);
            }
        }

        // Sort by LineId for consistent ordering
        ingredient_statuses.sort_by_key(|status| status.line_id);

        info!("Found {} bulk picking ingredients for analysis", ingredient_statuses.len());
        Ok(ingredient_statuses)
    }

    /// Calculate detailed batch status for a single ingredient
    async fn calculate_ingredient_batch_status(&self, ingredient: &BulkPickedItem) -> Result<IngredientBatchStatus> {
        // Get all batches for this ingredient in the run
        let batches = self
            .database
            .get_ingredient_batches(ingredient.run_no, &ingredient.item_key)
            .await
            .unwrap_or_else(|_| Vec::new());

        let total_batches = batches.len() as i32;
        let completed_batches = batches
            .iter()
            .filter(|batch| {
                batch.picked_bulk_qty.as_ref().map(|qty| qty >= &batch.to_picked_bulk_qty).unwrap_or(false)
            })
            .count() as i32;

        let in_progress_batches = batches
            .iter()
            .filter(|batch| {
                batch.picked_bulk_qty.as_ref().map(|qty| qty > &BigDecimal::from(0) && qty < &batch.to_picked_bulk_qty).unwrap_or(false)
            })
            .count() as i32;

        let unpicked_batches = total_batches - completed_batches - in_progress_batches;

        let mut batch_status = IngredientBatchStatus {
            item_key: ingredient.item_key.clone(),
            line_id: ingredient.line_id,
            description: ingredient.description.clone().unwrap_or_else(|| ingredient.item_key.clone()),
            total_batches,
            completed_batches,
            in_progress_batches,
            unpicked_batches,
            status: IngredientCompletionStatus::Unpicked, // Will be calculated
            pack_size: ingredient.pack_size.clone(),
            completion_percentage: 0.0, // Will be calculated
        };

        batch_status.calculate_status();
        Ok(batch_status)
    }

    /// Create run coordination state for intelligent switching
    #[instrument(skip(self))]
    pub async fn initialize_run_coordination(&self, run_no: i32) -> Result<RunCoordinationState> {
        info!("Initializing run coordination state for run: {}", run_no);

        let ingredient_statuses = self.analyze_run_ingredient_statuses(run_no).await?;
        let coordination_state = RunCoordinationState::new(run_no, ingredient_statuses);

        info!(
            "Initialized coordination for run {} with {} ingredients, current: {}",
            run_no, coordination_state.total_ingredients, coordination_state.current_ingredient
        );

        Ok(coordination_state)
    }

    /// Evaluate auto-switching decision after batch completion
    #[instrument(skip(self))]
    pub async fn evaluate_auto_switch(
        &self,
        mut coordination_state: RunCoordinationState,
        completion_event: BatchCompletionEvent,
    ) -> Result<(RunCoordinationState, IngredientSwitchDecision)> {
        info!(
            "Evaluating auto-switch for run {} after batch {} completion",
            completion_event.run_no, completion_event.batch_number
        );

        // Update ingredient status with completed batch
        if let Some(ingredient_status) = coordination_state
            .ingredient_statuses
            .get_mut(&completion_event.ingredient)
        {
            ingredient_status.completed_batches += 1;
            ingredient_status.unpicked_batches = ingredient_status.unpicked_batches.saturating_sub(1);
            ingredient_status.calculate_status();
        }

        // Evaluate switching decision
        let switch_decision = coordination_state.evaluate_switch_decision(&completion_event);

        // If switching, update current ingredient and reset consecutive counter
        if switch_decision.should_switch {
            if let Some(ref next_ingredient) = switch_decision.next_ingredient {
                coordination_state.current_ingredient = next_ingredient.clone();
                coordination_state.consecutive_completed_batches = 0;
                coordination_state.last_switch_timestamp = Some(Utc::now());
                
                info!(
                    "Auto-switching from {} to {} after {} consecutive batches",
                    completion_event.ingredient, next_ingredient, switch_decision.consecutive_completed
                );
            }
        }

        Ok((coordination_state, switch_decision))
    }

    /// Get filtered ingredients for ItemKey search modal (hide completed ingredients)
    #[instrument(skip(self))]
    pub async fn get_available_ingredients_for_search(&self, run_no: i32) -> Result<Vec<BulkPickedItem>> {
        info!("Getting available ingredients for ItemKey search modal: run {}", run_no);

        let ingredient_statuses = self.analyze_run_ingredient_statuses(run_no).await?;
        let available_item_keys: Vec<String> = ingredient_statuses
            .iter()
            .filter(|status| !status.should_hide_from_search())
            .map(|status| status.item_key.clone())
            .collect();

        info!(
            "Filtered {} available ingredients from {} total (hiding completed)",
            available_item_keys.len(),
            ingredient_statuses.len()
        );

        // Get full ingredient data for available items
        let all_ingredients = self
            .database
            .get_bulk_run_ingredients(run_no)
            .await
            .context("Failed to get run ingredients")?;

        let filtered_ingredients = all_ingredients
            .into_iter()
            .filter(|ingredient| available_item_keys.contains(&ingredient.item_key))
            .collect();

        Ok(filtered_ingredients)
    }

    /// Optimize lot assignments across ingredients for operational efficiency
    #[instrument(skip(self))]
    pub async fn optimize_cross_ingredient_lots(&self, run_no: i32) -> Result<CrossIngredientLotOptimization> {
        info!("Optimizing lot assignments across ingredients for run: {}", run_no);

        let ingredient_statuses = self.analyze_run_ingredient_statuses(run_no).await?;
        let mut optimization = CrossIngredientLotOptimization {
            ingredient_lot_assignments: HashMap::new(),
            lot_ingredient_usage: HashMap::new(),
            pallet_sequence_per_ingredient: HashMap::new(),
            lot_zone_preferences: HashMap::new(),
        };

        for ingredient_status in ingredient_statuses {
            // Get optimal lot for each ingredient
            let optimal_lot = self
                .database
                .get_available_lots(&ingredient_status.item_key)
                .await
                .unwrap_or_default()
                .into_iter()
                .next();

            if let Some(lot_info) = optimal_lot {
                // Record ingredient -> lot assignment
                optimization
                    .ingredient_lot_assignments
                    .insert(ingredient_status.item_key.clone(), lot_info.lot_no.clone());

                // Record lot -> ingredient usage
                optimization
                    .lot_ingredient_usage
                    .entry(lot_info.lot_no.clone())
                    .or_insert_with(Vec::new)
                    .push(ingredient_status.item_key.clone());

                // Initialize pallet sequence for ingredient
                optimization
                    .pallet_sequence_per_ingredient
                    .insert(ingredient_status.item_key.clone(), 1);

                // Determine zone preference from bin
                let zone_preference = if let Some(ref bin) = lot_info.bin {
                    if bin.starts_with("A") {
                        "A".to_string()
                    } else if bin.starts_with("I") {
                        "I".to_string()
                    } else if bin.starts_with("K") {
                        "K".to_string()
                    } else {
                        "OTHER".to_string()
                    }
                } else {
                    "UNKNOWN".to_string()
                };

                optimization
                    .lot_zone_preferences
                    .insert(ingredient_status.item_key.clone(), zone_preference);
            }
        }

        info!(
            "Optimized lot assignments for {} ingredients with {} unique lots",
            optimization.ingredient_lot_assignments.len(),
            optimization.lot_ingredient_usage.len()
        );

        Ok(optimization)
    }

    /// Get next recommended ingredient based on current workflow state
    #[instrument(skip(self))]
    pub async fn get_next_recommended_ingredient(
        &self,
        coordination_state: &RunCoordinationState,
    ) -> Result<Option<String>> {
        // Find next ingredient that needs picking, prioritizing by LineId
        let next_ingredient = coordination_state
            .ingredient_statuses
            .values()
            .filter(|status| !matches!(status.status, IngredientCompletionStatus::AllCompleted))
            .min_by_key(|status| status.line_id)
            .map(|status| status.item_key.clone());

        info!(
            "Next recommended ingredient for run {}: {:?}",
            coordination_state.run_no, next_ingredient
        );

        Ok(next_ingredient)
    }

    /// Calculate run-wide completion metrics
    #[instrument(skip(self))]
    pub async fn calculate_run_completion_metrics(&self, run_no: i32) -> Result<RunCompletionMetrics> {
        let ingredient_statuses = self.analyze_run_ingredient_statuses(run_no).await?;

        let total_ingredients = ingredient_statuses.len() as i32;
        let completed_ingredients = ingredient_statuses
            .iter()
            .filter(|status| matches!(status.status, IngredientCompletionStatus::AllCompleted))
            .count() as i32;
        let partially_picked_ingredients = ingredient_statuses
            .iter()
            .filter(|status| matches!(status.status, IngredientCompletionStatus::PartiallyPicked))
            .count() as i32;
        let unpicked_ingredients = ingredient_statuses
            .iter()
            .filter(|status| matches!(status.status, IngredientCompletionStatus::Unpicked))
            .count() as i32;

        let overall_completion_percentage = if total_ingredients > 0 {
            (completed_ingredients as f64 / total_ingredients as f64) * 100.0
        } else {
            0.0
        };

        let total_batches: i32 = ingredient_statuses.iter().map(|s| s.total_batches).sum();
        let completed_batches: i32 = ingredient_statuses.iter().map(|s| s.completed_batches).sum();
        let batch_completion_percentage = if total_batches > 0 {
            (completed_batches as f64 / total_batches as f64) * 100.0
        } else {
            0.0
        };

        Ok(RunCompletionMetrics {
            run_no,
            total_ingredients,
            completed_ingredients,
            partially_picked_ingredients,
            unpicked_ingredients,
            overall_completion_percentage,
            total_batches,
            completed_batches,
            batch_completion_percentage,
            ingredient_details: ingredient_statuses,
        })
    }
}

/// Run completion metrics for dashboard and progress tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCompletionMetrics {
    pub run_no: i32,
    pub total_ingredients: i32,
    pub completed_ingredients: i32,
    pub partially_picked_ingredients: i32,
    pub unpicked_ingredients: i32,
    pub overall_completion_percentage: f64,
    pub total_batches: i32,
    pub completed_batches: i32,
    pub batch_completion_percentage: f64,
    pub ingredient_details: Vec<IngredientBatchStatus>,
}