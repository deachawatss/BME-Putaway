use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Ingredient completion status for intelligent switching
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IngredientCompletionStatus {
    AllCompleted,       // All batches picked (6/6)
    PartiallyPicked,    // Some batches picked (1-5/6)
    Unpicked,          // No batches picked (0/6)
}

/// Multi-batch ingredient tracking for auto-switching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngredientBatchStatus {
    pub item_key: String,
    pub line_id: i32,
    pub description: String,
    pub total_batches: i32,
    pub completed_batches: i32,
    pub in_progress_batches: i32,
    pub unpicked_batches: i32,
    pub status: IngredientCompletionStatus,
    pub pack_size: BigDecimal,
    pub completion_percentage: f64,
}

/// Cross-ingredient switching configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngredientSwitchConfig {
    pub switch_threshold: i32,              // Number of consecutive batches before auto-switch
    pub switch_mode: IngredientSwitchMode,  // How switching is triggered
    pub fallback_to_manual: bool,           // Allow manual override of auto-switching
    pub ingredient_priority: Vec<i32>,      // LineId priority order for switching
}

/// Switching behavior modes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IngredientSwitchMode {
    Consecutive,    // Switch after N consecutive batches
    Total,         // Switch after N total batches (regardless of order)
    UserPreference, // User-controlled switching only
}

/// Auto-switching decision result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngredientSwitchDecision {
    pub should_switch: bool,
    pub current_ingredient: String,
    pub next_ingredient: Option<String>,
    pub switch_reason: String,
    pub consecutive_completed: i32,
    pub total_completed: i32,
    pub remaining_ingredients: Vec<String>,
}

/// Multi-ingredient run coordination state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCoordinationState {
    pub run_no: i32,
    pub total_ingredients: i32,
    pub ingredient_statuses: HashMap<String, IngredientBatchStatus>,
    pub current_ingredient: String,
    pub consecutive_completed_batches: i32,
    pub switch_config: IngredientSwitchConfig,
    pub last_switch_timestamp: Option<DateTime<Utc>>,
}

/// Lot optimization across ingredients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossIngredientLotOptimization {
    pub ingredient_lot_assignments: HashMap<String, String>,  // ItemKey -> LotNo
    pub lot_ingredient_usage: HashMap<String, Vec<String>>,   // LotNo -> [ItemKey]
    pub pallet_sequence_per_ingredient: HashMap<String, i32>, // ItemKey -> Current Pallet Number
    pub lot_zone_preferences: HashMap<String, String>,       // ItemKey -> Preferred Zone (A, I, K)
}

/// Batch completion event for triggering auto-switching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCompletionEvent {
    pub run_no: i32,
    pub batch_number: String,
    pub ingredient: String,
    pub line_id: i32,
    pub picked_quantity: BigDecimal,
    pub completion_timestamp: DateTime<Utc>,
    pub user_id: String,
}

impl Default for IngredientSwitchConfig {
    fn default() -> Self {
        Self {
            switch_threshold: 3,                           // BME4 default: switch after 3 batches
            switch_mode: IngredientSwitchMode::Consecutive,
            fallback_to_manual: true,
            ingredient_priority: vec![],                   // Will be populated from LineId order
        }
    }
}

impl IngredientBatchStatus {
    /// Calculate completion status based on batch counts
    pub fn calculate_status(&mut self) {
        self.completion_percentage = if self.total_batches > 0 {
            (self.completed_batches as f64 / self.total_batches as f64) * 100.0
        } else {
            0.0
        };

        self.status = if self.completed_batches == self.total_batches {
            IngredientCompletionStatus::AllCompleted
        } else if self.completed_batches > 0 {
            IngredientCompletionStatus::PartiallyPicked
        } else {
            IngredientCompletionStatus::Unpicked
        };
    }

    /// Check if ingredient should be hidden from ItemKey search
    pub fn should_hide_from_search(&self) -> bool {
        matches!(self.status, IngredientCompletionStatus::AllCompleted)
    }
}

impl RunCoordinationState {
    /// Create new coordination state for a run
    pub fn new(run_no: i32, ingredients: Vec<IngredientBatchStatus>) -> Self {
        let mut ingredient_statuses = HashMap::new();
        let mut current_ingredient = String::new();

        // Find first unpicked or partially picked ingredient
        for ingredient in ingredients.iter() {
            ingredient_statuses.insert(ingredient.item_key.clone(), ingredient.clone());
            
            if current_ingredient.is_empty() && 
               !matches!(ingredient.status, IngredientCompletionStatus::AllCompleted) {
                current_ingredient = ingredient.item_key.clone();
            }
        }

        Self {
            run_no,
            total_ingredients: ingredient_statuses.len() as i32,
            ingredient_statuses,
            current_ingredient,
            consecutive_completed_batches: 0,
            switch_config: IngredientSwitchConfig::default(),
            last_switch_timestamp: None,
        }
    }

    /// Evaluate if ingredient switching should occur
    pub fn evaluate_switch_decision(&mut self, completion_event: &BatchCompletionEvent) -> IngredientSwitchDecision {
        // Update consecutive completed batches if same ingredient
        if completion_event.ingredient == self.current_ingredient {
            self.consecutive_completed_batches += 1;
        } else {
            self.consecutive_completed_batches = 1;
        }

        let should_switch = self.consecutive_completed_batches >= self.switch_config.switch_threshold;
        
        let next_ingredient = if should_switch {
            self.find_next_ingredient(&completion_event.ingredient)
        } else {
            None
        };

        let switch_reason = if should_switch {
            format!(
                "Completed {} consecutive batches for ingredient {}, switching to next ingredient",
                self.consecutive_completed_batches,
                completion_event.ingredient
            )
        } else {
            format!(
                "Continue with current ingredient ({}/{})",
                self.consecutive_completed_batches,
                self.switch_config.switch_threshold
            )
        };

        IngredientSwitchDecision {
            should_switch,
            current_ingredient: completion_event.ingredient.clone(),
            next_ingredient,
            switch_reason,
            consecutive_completed: self.consecutive_completed_batches,
            total_completed: self.get_total_completed_batches(&completion_event.ingredient),
            remaining_ingredients: self.get_remaining_ingredients(),
        }
    }

    /// Find next unpicked or partially picked ingredient by LineId order
    fn find_next_ingredient(&self, current_ingredient: &str) -> Option<String> {
        let current_line_id = self.ingredient_statuses
            .get(current_ingredient)
            .map(|status| status.line_id)?;

        // Find next ingredient by LineId ascending order
        let mut candidates: Vec<_> = self.ingredient_statuses
            .values()
            .filter(|status| {
                status.line_id > current_line_id && 
                !matches!(status.status, IngredientCompletionStatus::AllCompleted)
            })
            .collect();

        candidates.sort_by_key(|status| status.line_id);
        candidates.first().map(|status| status.item_key.clone())
            .or_else(|| {
                // If no higher LineId available, wrap around to lowest unpicked
                let mut all_candidates: Vec<_> = self.ingredient_statuses
                    .values()
                    .filter(|status| !matches!(status.status, IngredientCompletionStatus::AllCompleted))
                    .collect();
                all_candidates.sort_by_key(|status| status.line_id);
                all_candidates.first().map(|status| status.item_key.clone())
            })
    }

    /// Get total completed batches for an ingredient
    fn get_total_completed_batches(&self, ingredient: &str) -> i32 {
        self.ingredient_statuses
            .get(ingredient)
            .map(|status| status.completed_batches)
            .unwrap_or(0)
    }

    /// Get list of remaining unpicked/partially picked ingredients
    fn get_remaining_ingredients(&self) -> Vec<String> {
        self.ingredient_statuses
            .values()
            .filter(|status| !matches!(status.status, IngredientCompletionStatus::AllCompleted))
            .map(|status| status.item_key.clone())
            .collect()
    }
}