use crate::database::Database;
use crate::models::bulk_runs::*;
use crate::utils::timezone::{format_bangkok_date, get_bangkok_time};
use anyhow::{Context, Result};
use bigdecimal::BigDecimal;
use tracing::{info, instrument, warn};

/// Service for bulk run operations
pub struct BulkRunsService {
    database: Database,
}

impl BulkRunsService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    /// Search for bulk runs and return search response
    #[instrument(skip(self))]
    pub async fn search_bulk_runs(&self, query: &str, search_mode: &str) -> Result<Vec<BulkRunSearchResponse>> {
        info!("Processing bulk run search for query: {} (mode: {})", query, search_mode);

        let runs = self
            .database
            .search_bulk_runs(query, search_mode)
            .await
            .context("Failed to search bulk runs")?;

        let mut responses = Vec::new();
        let mut failed_runs = Vec::new();
        
        for run in runs {
            match self
                .database
                .get_bulk_run_ingredients(run.run_no)
                .await
                .context("Failed to get run ingredients")
            {
                Ok(ingredients) => {
                    let total_ingredients = ingredients.len() as i32;
                    let completed_ingredients = ingredients
                        .iter()
                        .filter(|ing| self.is_ingredient_complete(ing))
                        .count() as i32;

                    responses.push(BulkRunSearchResponse {
                        run,
                        ingredients,
                        total_ingredients,
                        completed_ingredients,
                    });
                }
                Err(e) => {
                    warn!(
                        "Failed to get ingredients for run {}: {}. Skipping run.",
                        run.run_no, e
                    );
                    failed_runs.push(run.run_no);
                    // Continue processing other runs instead of failing completely
                }
            }
        }

        if !failed_runs.is_empty() {
            warn!(
                "Failed to process {} runs: {:?}. Successfully processed {} runs.",
                failed_runs.len(), failed_runs, responses.len()
            );
        }

        Ok(responses)
    }

    /// Get detailed bulk run form data for frontend population
    #[instrument(skip(self))]
    pub async fn get_bulk_run_form_data(
        &self,
        run_no: i32,
        ingredient_index: Option<i32>,
    ) -> Result<Option<BulkRunFormData>> {
        info!(
            "Getting bulk run form data for run: {}, ingredient index: {:?}",
            run_no, ingredient_index
        );

        // Get the bulk run
        let run = match self.database.get_bulk_run(run_no).await? {
            Some(run) => run,
            None => {
                warn!("Bulk run not found: {}", run_no);
                return Ok(None);
            }
        };

        // Get individual batch records to find the correct unpicked batch
        let all_batches = self.database.get_bulk_run_batches(run_no).await?;

        if all_batches.is_empty() {
            warn!("No batches found for run: {}", run_no);
            return Ok(None);
        }

        info!("Found {} individual batches for run {}", all_batches.len(), run_no);

        // If ingredient_index is provided, we need to map it to the correct ingredient
        // and filter batches for that specific ingredient
        let current_batch = if let Some(idx) = ingredient_index {
            // Get aggregated ingredients to map index to item_key
            let ingredients = self.database.get_bulk_run_ingredients(run_no).await?;
            
            // Use database DESC ordering (LineId DESC) to match search modal and ensure consistent indexing

            if let Some(selected_ingredient) = ingredients.get(idx as usize) {
                info!("Manual ingredient selection: index {} maps to ItemKey: {}, LineId: {}", 
                      idx, selected_ingredient.item_key, selected_ingredient.line_id);
                
                // Filter batches for the selected ingredient
                let ingredient_batches: Vec<&BulkPickedItem> = all_batches.iter()
                    .filter(|b| b.item_key == selected_ingredient.item_key)
                    .collect();
                
                info!("Found {} batches for selected ingredient {}", 
                      ingredient_batches.len(), selected_ingredient.item_key);
                
                // Find the first unpicked batch for this specific ingredient
                let ingredient_batches_owned: Vec<BulkPickedItem> = ingredient_batches.into_iter().cloned().collect();
                
                if let Some(unpicked_batch) = self.find_next_batch_to_pick(&ingredient_batches_owned) {
                    unpicked_batch.clone()
                } else {
                    // If no unpicked batch for this ingredient, use the first batch of this ingredient
                    all_batches.iter()
                        .find(|b| b.item_key == selected_ingredient.item_key)
                        .cloned()
                        .or_else(|| all_batches.first().cloned())
                        .context("No batches available for selected ingredient")?
                }
            } else {
                warn!("Invalid ingredient index: {} (out of {} ingredients)", idx, ingredients.len());
                // Fallback to finding any unpicked batch
                self.find_next_batch_to_pick(&all_batches)
                    .cloned()
                    .or_else(|| all_batches.first().cloned())
                    .context("No batches available for fallback")?
            }
        } else {
            // No specific ingredient selected - find the first unpicked batch across all ingredients
            self.find_next_batch_to_pick(&all_batches)
                .cloned()
                .or_else(|| all_batches.first().cloned())
                .context("No batches available in run")?
        };

        info!("Selected batch for picking: RunNo={}, RowNum={}, LineId={}, ItemKey={}, RemainingQty={}", 
              current_batch.run_no, current_batch.row_num, current_batch.line_id, 
              current_batch.item_key, 
              &current_batch.to_picked_bulk_qty - current_batch.picked_bulk_qty.as_ref().unwrap_or(&BigDecimal::from(0)));

        // Also get aggregated ingredients for display purposes
        let ingredients = self.database.get_bulk_run_ingredients(run_no).await?;

        // Get inventory info for the current batch's ingredient
        let inventory = self
            .database
            .get_inventory_info(run_no, &current_batch.item_key)
            .await
            .context("Failed to get inventory info")?;

        // Calculate quantities based on the current batch
        let calculations = self.calculate_ingredient_quantities(&current_batch, &run);

        // Create ingredient view using the selected batch
        let ingredient_view = IngredientView {
            ingredient: current_batch.clone(),
            inventory,
            calculations,
        };

        // Find the actual index of the current batch's ingredient for navigation
        let actual_ingredient_idx = ingredients
            .iter()
            .position(|ing| ing.line_id == current_batch.line_id && ing.item_key == current_batch.item_key)
            .unwrap_or(0) as i32;

        // Create form fields
        let form_data = self.create_form_fields(
            &run,
            &ingredient_view,
            actual_ingredient_idx,
            ingredients.len() as i32,
        );

        Ok(Some(BulkRunFormData {
            run,
            current_ingredient: ingredient_view,
            form_data,
        }))
    }

    /// Calculate ingredient quantities based on bulk run data
    fn calculate_ingredient_quantities(
        &self,
        ingredient: &BulkPickedItem,
        _run: &BulkRun,
    ) -> IngredientCalculation {
        // Total needed = ToPickedBulkQty (already calculated by BME4)
        let total_needed = ingredient.to_picked_bulk_qty.clone();

        // Remaining to pick = ToPickedBulkQty - ISNULL(PickedBulkQty, 0)
        let picked_qty = ingredient
            .picked_bulk_qty
            .clone()
            .unwrap_or_else(|| BigDecimal::from(0));
        let remaining_to_pick = &total_needed - &picked_qty;

        // Completion percentage
        let completion_percentage = if total_needed > BigDecimal::from(0) {
            let percentage = &picked_qty / &total_needed * BigDecimal::from(100);
            percentage.to_string().parse::<f64>().unwrap_or(0.0)
        } else {
            0.0
        };

        IngredientCalculation {
            total_needed,
            remaining_to_pick,
            completion_percentage,
        }
    }

    /// Create form fields for frontend population
    fn create_form_fields(
        &self,
        run: &BulkRun,
        ingredient_view: &IngredientView,
        ingredient_index: i32,
        total_ingredients: i32,
    ) -> FormFields {
        // Get Bangkok time for picking date
        let bangkok_now = get_bangkok_time();
        let st_picking_date = format_bangkok_date(bangkok_now);

        // Calculate bags and KG values
        let pack_size = &ingredient_view.ingredient.pack_size;
        let total_needed_bags = &ingredient_view.calculations.total_needed;
        let remaining_bags = &ingredient_view.calculations.remaining_to_pick;
        
        // Calculate KG values: bags × pack_size 
        let total_needed_kg = total_needed_bags * pack_size;
        let remaining_kg = remaining_bags * pack_size;

        FormFields {
            fg_item_key: run.formula_id.clone(),
            st_picking_date,
            item_key: ingredient_view.ingredient.item_key.clone(),
            soh_value: ingredient_view.inventory.soh_value.to_string(),
            soh_uom: ingredient_view.inventory.soh_uom.clone(),
            bulk_pack_size_value: ingredient_view.inventory.bulk_pack_size_value.to_string(),
            bulk_pack_size_uom: ingredient_view.inventory.bulk_pack_size_uom.clone(),
            // Total needed - dual units
            total_needed_bags: format!("{total_needed_bags:.4}"),
            total_needed_bags_uom: "BAGS".to_string(),
            total_needed_kg: format!("{total_needed_kg:.4}"), 
            total_needed_kg_uom: ingredient_view.ingredient.uom.clone(), // Should be "KG"
            // Remaining to pick - dual units  
            remaining_bags: format!("{remaining_bags:.4}"),
            remaining_bags_uom: "BAGS".to_string(),
            remaining_kg: format!("{remaining_kg:.4}"),
            remaining_kg_uom: ingredient_view.ingredient.uom.clone(), // Should be "KG"
            ingredient_index,
            total_ingredients,
        }
    }

    /// Check if an ingredient is complete using batch-level completion logic
    /// FIXED: Now uses completion_status from database query instead of flawed aggregation comparison
    fn is_ingredient_complete(&self, ingredient: &BulkPickedItem) -> bool {
        match &ingredient.completion_status {
            Some(status) => status == "COMPLETE",
            None => {
                // Fallback to old logic if completion_status is not available
                match &ingredient.picked_bulk_qty {
                    Some(picked) => picked >= &ingredient.to_picked_bulk_qty,
                    None => false,
                }
            }
        }
    }

    /// Find the next batch that needs to be picked from individual batch records
    fn find_next_batch_to_pick<'a>(
        &self,
        batches: &'a [BulkPickedItem],
    ) -> Option<&'a BulkPickedItem> {
        info!("Finding next batch to pick from {} individual batches", batches.len());
        
        // Find the first unpicked batch (ToPickedBulkQty > PickedBulkQty)
        for batch in batches.iter() {
            let zero = BigDecimal::from(0);
            let picked_qty = batch.picked_bulk_qty.as_ref().unwrap_or(&zero);
            let remaining_qty = &batch.to_picked_bulk_qty - picked_qty;
            
            if batch.to_picked_bulk_qty > BigDecimal::from(0) && remaining_qty > BigDecimal::from(0) {
                info!("Found next unpicked batch: ItemKey: {}, RowNum: {}, LineId: {}, ToPickedQty: {}, PickedQty: {}, RemainingQty: {}", 
                      batch.item_key, batch.row_num, batch.line_id, 
                      batch.to_picked_bulk_qty, picked_qty, remaining_qty);
                
                return Some(batch);
            }
        }
        
        info!("No unpicked batches found - all batches are complete");
        None
    }


    /// Get available runs with NEW status
    #[instrument(skip(self))]
    pub async fn get_available_runs(&self) -> Result<Vec<BulkRun>> {
        info!("Getting available bulk runs");

        // Search for runs with NEW status (use partial mode for listing all runs)
        let all_runs = self.database.search_bulk_runs("", "partial").await?;

        let available_runs = all_runs
            .into_iter()
            .filter(|run| run.status == "NEW" || run.status == "IN_PROGRESS")
            .collect();

        Ok(available_runs)
    }

    /// Get next ingredient for auto-progression
    #[instrument(skip(self))]
    pub async fn get_next_ingredient(
        &self,
        run_no: i32,
        current_ingredient_index: i32,
    ) -> Result<Option<BulkRunFormData>> {
        let next_index = current_ingredient_index + 1;
        self.get_bulk_run_form_data(run_no, Some(next_index)).await
    }

    /// Check if all ingredients in a run are complete
    #[instrument(skip(self))]
    pub async fn is_run_complete(&self, run_no: i32) -> Result<bool> {
        let ingredients = self.database.get_bulk_run_ingredients(run_no).await?;

        if ingredients.is_empty() {
            return Ok(false);
        }

        let all_complete = ingredients
            .iter()
            .all(|ing| self.is_ingredient_complete(ing));
        Ok(all_complete)
    }

    /// Search for related ingredients in a bulk run for modal display
    /// 🔍 CRITICAL: Only returns ingredients that require bulk picking (ToPickedBulkQty > 0)
    #[instrument(skip(self))]
    pub async fn search_run_items(&self, run_no: i32) -> Result<Vec<RunItemSearchResult>> {

        let ingredients = self.database.get_unique_ingredients_for_search(run_no).await?;
        info!("🔢 DEBUG: Database returned {} bulk picking ingredients for run {}", ingredients.len(), run_no);
        
        let mut results = Vec::new();
        for ingredient in ingredients {
            let picked_qty = ingredient.picked_bulk_qty.as_ref()
                .map(|qty| qty.to_string())
                .unwrap_or_else(|| "0".to_string());

            info!("📋 DEBUG: Adding ingredient {} (LineId: {}, ToPickedBulkQty: {}, PickedBulkQty: {}) to search results",
                  ingredient.item_key, ingredient.line_id, ingredient.to_picked_bulk_qty, picked_qty);
            results.push(RunItemSearchResult {
                item_key: ingredient.item_key,
                description: ingredient.description.unwrap_or("Unknown Item".to_string()),
                location: ingredient.location.unwrap_or("Unknown".to_string()),
                line_id: ingredient.line_id,
                pack_size: ingredient.pack_size.to_string(),
                uom: ingredient.uom,
                to_picked_bulk_qty: ingredient.to_picked_bulk_qty.to_string(), // CRITICAL: Include bulk picking requirement
                picked_bulk_qty: picked_qty, // CRITICAL: Include current picked quantity for auto-switching logic
            });
        }

        // Use database DESC ordering (LineId DESC) to match search modal and ensure consistent indexing
        
        info!("✅ DEBUG: Returning {} bulk picking ingredients for run {} (should be 4 according to official BME4)", 
              results.len(), run_no);

        Ok(results)
    }

    /// Get ingredient index for a specific ItemKey in a bulk run, prioritizing first batch (lowest RowNum)
    #[instrument(skip(self))]
    pub async fn get_ingredient_index(&self, run_no: i32, item_key: &str) -> Result<usize> {
        info!("Getting ingredient index for item_key: {} in run: {} (prioritizing first batch - lowest RowNum)", item_key, run_no);
        
        let ingredients = self.database.get_bulk_run_ingredients(run_no).await?;
        // Use database DESC ordering (LineId DESC) to match search modal and ensure consistent indexing
        
        // Collect all batches for this ingredient with their array indices
        let mut ingredient_batches: Vec<(usize, &BulkPickedItem)> = ingredients
            .iter()
            .enumerate()
            .filter(|(_, ingredient)| ingredient.item_key == item_key)
            .collect();
            
        if ingredient_batches.is_empty() {
            return Err(anyhow::anyhow!("Ingredient {} not found in run {}", item_key, run_no));
        }
        
        // Sort by RowNum ASC to find the first batch (lowest RowNum)
        // This ensures we always start from the first batch when switching ingredients
        ingredient_batches.sort_by_key(|(_, ingredient)| ingredient.row_num);
        
        // Return the index of the first batch (lowest RowNum) regardless of completion status
        let (first_batch_index, first_batch) = ingredient_batches[0];
        info!("Returning first batch for ingredient {} at index {} (RowNum: {}, LineId: {}) for run {}", 
              item_key, first_batch_index, first_batch.row_num, first_batch.line_id, run_no);
        
        Ok(first_batch_index)
    }

    /// Get ingredient data by specific coordinates (RowNum, LineId)
    /// Used for pallet transitions within the same ingredient
    /// FIXED: Now uses individual batch records instead of aggregated data
    pub async fn get_ingredient_by_coordinates(
        &self,
        run_no: i32,
        item_key: &str,
        row_num: i32,
        line_id: i32,
    ) -> Result<BulkRunFormData> {
        info!(
            "Getting ingredient data by coordinates - ItemKey: {}, RowNum: {}, LineId: {} in run: {}", 
            item_key, row_num, line_id, run_no
        );
        
        // Get run information
        let run = self.database.get_bulk_run(run_no).await?
            .ok_or_else(|| anyhow::anyhow!("Run {} not found", run_no))?;
        
        // Get individual batches for the ingredient (non-aggregated)
        let batches = self.database.get_bulk_run_batches_for_ingredient(run_no, item_key).await?;
        
        if batches.is_empty() {
            return Err(anyhow::anyhow!(
                "No batches found for ingredient {} in run {}", 
                item_key, run_no
            ));
        }
        
        // Find the specific batch with the requested coordinates
        let target_batch = batches.iter()
            .find(|batch| batch.row_num == row_num && batch.line_id == line_id)
            .ok_or_else(|| anyhow::anyhow!(
                "Batch with coordinates (RowNum: {}, LineId: {}) not found for ingredient {} in run {}", 
                row_num, line_id, item_key, run_no
            ))?;
        
        info!(
            "Found batch with coordinates (RowNum: {}, LineId: {}) for ingredient {} in run {}",
            row_num, line_id, item_key, run_no
        );
        
        // Get inventory info for the ingredient
        let inventory = self
            .database
            .get_inventory_info(run_no, item_key)
            .await
            .context("Failed to get inventory info")?;

        // Calculate quantities based on the specific batch
        let calculations = self.calculate_ingredient_quantities(target_batch, &run);

        // Create ingredient view using the target batch
        let ingredient_view = IngredientView {
            ingredient: target_batch.clone(),
            inventory,
            calculations,
        };

        // Get all ingredients for navigation purposes
        let all_ingredients = self.database.get_bulk_run_ingredients(run_no).await?;
        let actual_ingredient_idx = all_ingredients
            .iter()
            .position(|ing| ing.line_id == line_id && ing.item_key == item_key)
            .unwrap_or(0) as i32;

        // Create form fields
        let form_data = self.create_form_fields(
            &run,
            &ingredient_view,
            actual_ingredient_idx,
            all_ingredients.len() as i32,
        );

        Ok(BulkRunFormData {
            run,
            current_ingredient: ingredient_view,
            form_data,
        })
    }

    /// List active bulk runs with pagination (Story 1.4)
    #[instrument(skip(self))]
    pub async fn list_active_bulk_runs_paginated(
        &self,
        page: u32,
        limit: u32,
    ) -> Result<PaginatedBulkRunResponse> {
        info!(
            "Getting paginated active bulk runs - page: {}, limit: {}",
            page, limit
        );

        let (runs, total_items) = self
            .database
            .list_active_bulk_runs_paginated(page, limit)
            .await?;

        let total_pages = if total_items == 0 {
            1
        } else {
            (total_items as f64 / limit as f64).ceil() as u32
        };
        let has_previous = page > 1;
        let has_next = page < total_pages;

        let pagination = PaginationInfo {
            current_page: page,
            total_pages,
            total_items,
            page_size: limit,
            has_previous,
            has_next,
        };

        Ok(PaginatedBulkRunResponse { runs, pagination })
    }


    /// Search lots for a specific run and item key with pagination
    #[instrument(skip(self))]
    pub async fn search_run_lots_paginated(
        &self,
        run_no: i32,
        item_key: &str,
        page: u32,
        page_size: u32,
    ) -> Result<PaginatedLotSearchResponse> {
        info!(
            "Searching lots paginated for run: {}, item_key: {}, page: {}, size: {}",
            run_no, item_key, page, page_size
        );

        // Get paginated lots from database with FEFO ordering
        let (lots, total_items) = self
            .database
            .search_lots_for_run_item_paginated(run_no, item_key, page, page_size)
            .await
            .context("Failed to search lots for run item with pagination")?;

        // Calculate pagination info
        let total_pages = ((total_items as f64) / (page_size as f64)).ceil() as u32;
        let has_previous = page > 1;
        let has_next = page < total_pages;

        let pagination_info = PaginationInfo {
            current_page: page,
            total_pages,
            total_items,
            page_size,
            has_previous,
            has_next,
        };

        info!(
            "Found {} lots on page {} of {} (total: {} lots) for run {}, item {}",
            lots.len(), page, total_pages, total_items, run_no, item_key
        );

        Ok(PaginatedLotSearchResponse {
            lots,
            pagination: pagination_info,
        })
    }

    /// Get available bins for a specific lot number within a run
    /// Used for barcode scanning workflow when user scans a lot number
    #[instrument(skip(self))]
    pub async fn get_lot_bins(
        &self,
        run_no: i32,
        lot_no: &str,
        item_key: &str,
    ) -> Result<Vec<LotSearchResult>> {
        info!(
            "Getting bins for lot: {} in run: {}, item_key: {}",
            lot_no, run_no, item_key
        );

        // Get bins from database for the specific lot
        let bins = self
            .database
            .get_bins_for_lot(run_no, lot_no, item_key)
            .await
            .context("Failed to get bins for lot")?;

        info!(
            "Found {} bins for lot {} in run {}, item {}",
            bins.len(), lot_no, run_no, item_key
        );

        // Validate that the lot exists and has available inventory
        if bins.is_empty() {
            warn!(
                "No available bins found for lot {} in run {} with item {}",
                lot_no, run_no, item_key
            );
        }

        Ok(bins)
    }

    /// Get pallet tracking data for a bulk run
    /// Returns comprehensive pallet batch tracking with picked/remaining quantities
    #[instrument(skip(self))]
    pub async fn get_pallet_tracking_data(&self, run_no: i32, item_key: Option<&str>) -> Result<PalletBatchResponse> {
        info!("Getting pallet tracking data for run: {}, item_key: {:?}", run_no, item_key);

        // Get pallet tracking data from database
        let pallets = self
            .database
            .get_pallet_tracking_data(run_no, item_key)
            .await
            .context("Failed to get pallet tracking data")?;

        // Calculate totals for summary data
        let total_pallets = pallets.len() as i32;
        let total_picked_quantity = pallets
            .iter()
            .map(|p| p.quantity_picked)
            .sum::<f64>();
        let total_remaining_quantity = pallets
            .iter()
            .map(|p| p.quantity_remaining)
            .sum::<f64>();

        info!(
            "Pallet tracking for run {}: {} pallets, {:.2} KG picked, {:.2} KG remaining",
            run_no, total_pallets, total_picked_quantity, total_remaining_quantity
        );

        Ok(PalletBatchResponse {
            run_no,
            pallets,
            total_pallets,
            total_picked_quantity,
            total_remaining_quantity,
        })
    }

    /// **BME4-Compatible Pick Confirmation** - 5-Table Atomic Transaction
    /// Validates and processes pick confirmation with official BME4 workflow
    #[instrument(skip(self))]
    pub async fn confirm_pick_transaction(
        &self,
        run_no: i32,
        request: PickConfirmationRequest,
    ) -> Result<PickConfirmationResponse> {
        info!(
            "Processing pick confirmation for run: {}, lot: {}, qty: {}",
            run_no, request.lot_no, request.picked_bulk_qty
        );

        // **Step 1: Validate pick request** - BME4 business rules
        let validation_result = self
            .database
            .validate_pick_request(run_no, &request)
            .await
            .context("Failed to validate pick request")?;

        if !validation_result.is_valid {
            let error_msg = validation_result.error_message.unwrap_or_else(|| {
                "Pick validation failed".to_string()
            });
            warn!("Pick validation failed for run {}: {}", run_no, error_msg);
            return Err(anyhow::anyhow!("Validation failed: {}", error_msg));
        }

        // Log any warnings
        for warning in &validation_result.warnings {
            warn!("Pick validation warning for run {}: {}", run_no, warning);
        }

        // **Step 2: Execute atomic transaction** - 4-table updates
        // Execute and normalize known error cases for clearer frontend messages
        let response = match self
            .database
            .confirm_pick_transaction(run_no, &request)
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("BATCH_ALREADY_COMPLETED") {
                    // Map to a clean, actionable message while logging specifics
                    warn!(
                        "BATCH_ALREADY_COMPLETED detected for run {} (lot: {}, row_num: {}, line_id: {}): {}",
                        run_no, request.lot_no, request.row_num, request.line_id, err_str
                    );
                    let user_msg = "This batch is already completed. Please refresh to load the next batch.";
                    return Err(anyhow::anyhow!("BATCH_ALREADY_COMPLETED: {}", user_msg));
                }

                // Unknown failure path; keep original context
                return Err(e).context("Failed to execute pick confirmation transaction");
            }
        };

        info!(
            "Pick confirmation completed successfully for run: {} - Transaction: {:?}, Document: {:?}",
            run_no, response.transaction_id, response.document_no
        );

        Ok(response)
    }

    /// Validate pick request before processing
    /// Returns validation result with business rule checks
    #[instrument(skip(self))]
    pub async fn validate_pick_request(
        &self,
        run_no: i32,
        request: PickConfirmationRequest,
    ) -> Result<PickValidationResult> {
        info!(
            "Validating pick request for run: {}, lot: {}",
            run_no, request.lot_no
        );

        let validation_result = self
            .database
            .validate_pick_request(run_no, &request)
            .await
            .context("Failed to validate pick request")?;

        if validation_result.is_valid {
            info!("Pick validation successful for run: {}", run_no);
        } else {
            warn!(
                "Pick validation failed for run: {} - {}",
                run_no,
                validation_result.error_message.as_deref().unwrap_or("Unknown error")
            );
        }

        Ok(validation_result)
    }

    /// Check if a specific pallet is completed
    pub async fn is_pallet_completed(
        &self,
        run_no: i32,
        row_num: i32,
        line_id: i32,
    ) -> Result<bool> {
        info!("🔍 SERVICE: Checking pallet completion for run: {}, row_num: {}, line_id: {}", 
              run_no, row_num, line_id);
              
        self.database
            .is_pallet_completed(run_no, row_num, line_id)
            .await
            .context("Failed to check pallet completion status")
    }

    /// Get next available pallet for the same ingredient
    pub async fn get_next_available_pallet(
        &self,
        run_no: i32,
        current_row_num: i32,
        line_id: i32,
    ) -> Result<Option<PalletBatchInfo>> {
        info!("🔍 SERVICE: Finding next available pallet for run: {}, current_row_num: {}, line_id: {}", 
              run_no, current_row_num, line_id);
              
        self.database
            .get_next_available_pallet(run_no, current_row_num, line_id)
            .await
            .context("Failed to get next available pallet")
    }

    /// **REVERT STATUS SERVICE** - Revert bulk run status from PRINT back to NEW
    /// Used when user wants to make changes after run completion
    /// Returns the updated status data on success, None on failure
    #[instrument(skip(self))]
    pub async fn revert_bulk_run_status(&self, run_no: i32, user_id: &str) -> Result<Option<crate::models::bulk_runs::BulkRunStatusResponse>> {
        info!("🔄 SERVICE: Starting status revert for run {} by user {}", run_no, user_id);

        // Validate that the run exists and is in PRINT status
        let current_status_option = self.database
            .get_bulk_run_status(run_no)
            .await
            .context("Failed to get current run status")?;

        let current_status = match current_status_option {
            Some(status) => status,
            None => {
                warn!("⚠️ SERVICE: Cannot revert run {} - run not found", run_no);
                return Ok(None);
            }
        };

        if current_status.status != "PRINT" {
            warn!("⚠️ SERVICE: Cannot revert run {} - current status is '{}', expected 'PRINT'",
                  run_no, current_status.status);
            return Ok(None);
        }

        // Perform the actual revert operation
        let revert_success = self.database
            .revert_run_status_to_new(run_no, user_id)
            .await
            .context("Failed to revert run status in database")?;

        if revert_success {
            info!("✅ SERVICE: Successfully reverted run {} status from PRINT to NEW", run_no);

            // Get the updated status to return
            match self.database.get_bulk_run_status(run_no).await {
                Ok(updated_status) => {
                    if updated_status.is_some() {
                        info!("✅ SERVICE: Retrieved updated status for run {}", run_no);
                    } else {
                        warn!("⚠️ SERVICE: Status reverted but run {} not found in status check", run_no);
                    }
                    Ok(updated_status)
                }
                Err(e) => {
                    warn!("⚠️ SERVICE: Failed to get updated status after revert for run {}: {}", run_no, e);
                    // Return success since the revert operation succeeded, even if we can't fetch the updated status
                    Ok(None)
                }
            }
        } else {
            warn!("⚠️ SERVICE: Failed to revert run {} status - no rows were updated", run_no);
            Ok(None)
        }
    }

    /// **NEW UNIVERSAL COMPLETION CHECK** - Get detailed run completion status
    /// Returns comprehensive information about ingredient completion for automatic status transitions
    pub async fn get_run_completion_status(
        &self,
        run_no: i32,
    ) -> Result<RunCompletionStatus, anyhow::Error> {
        info!("🔍 SERVICE: Getting detailed completion status for run {}", run_no);

        // Use database method to get completion status directly
        match self.database.get_run_completion_status(run_no).await {
            Ok(completion_status) => {
                info!("📊 SERVICE: Run {} completion status - {}/{} complete ({} incomplete)",
                      run_no, completion_status.completed_count, completion_status.total_ingredients,
                      completion_status.incomplete_count);

                Ok(completion_status)
            }
            Err(e) => {
                warn!("⚠️ SERVICE: Failed to get completion status for run {}: {}", run_no, e);
                Err(anyhow::anyhow!("Failed to get run completion status: {e}"))
            }
        }
    }

    /// **NEW AUTOMATIC STATUS UPDATE** - Update run status from NEW to PRINT
    /// Called when all ingredients are complete to finalize the run
    pub async fn complete_run_status(
        &self,
        run_no: i32,
        user_id: &str,
    ) -> Result<StatusUpdateResult, anyhow::Error> {
        use crate::models::bulk_runs::StatusUpdateResult;

        info!("🔄 SERVICE: Completing run {} status (NEW → PRINT) for user: {}", run_no, user_id);

        // First, verify that the run is actually complete
        let completion_status = self.get_run_completion_status(run_no).await
            .context("Failed to verify run completion before status update")?;

        if !completion_status.is_complete {
            let error_msg = format!(
                "Cannot complete run {} - still has {} incomplete ingredients (completed: {}/{})",
                run_no, completion_status.incomplete_count, completion_status.completed_count, completion_status.total_ingredients
            );
            warn!("⚠️ SERVICE: {}", error_msg);
            return Err(anyhow::anyhow!(error_msg));
        }

        // Get current status
        let current_status_option = self.database
            .get_bulk_run_status(run_no)
            .await
            .context("Failed to get current run status")?;

        let current_status = match current_status_option {
            Some(status) => status.status,
            None => {
                let error_msg = format!("Run {run_no} not found");
                warn!("⚠️ SERVICE: {}", error_msg);
                return Err(anyhow::anyhow!(error_msg));
            }
        };

        if current_status != "NEW" {
            let error_msg = format!(
                "Cannot complete run {run_no} - current status is '{current_status}', expected 'NEW'"
            );
            warn!("⚠️ SERVICE: {}", error_msg);
            return Err(anyhow::anyhow!(error_msg));
        }

        // Perform the status update to PRINT
        let update_success = self.database
            .update_bulk_run_status(run_no, "PRINT", user_id)
            .await
            .context("Failed to update run status to PRINT")?;

        if !update_success {
            let error_msg = format!("Failed to update run {run_no} status - no rows were updated");
            warn!("⚠️ SERVICE: {}", error_msg);
            return Err(anyhow::anyhow!(error_msg));
        }

        info!("✅ SERVICE: Successfully updated run {} status from NEW to PRINT", run_no);

        Ok(StatusUpdateResult {
            old_status: current_status,
            new_status: "PRINT".to_string(),
        })
    }

    /// Update run status to PRINT if current status is NEW
    pub async fn update_print_status(
        &self,
        run_no: i32,
        user_id: &str,
    ) -> Result<StatusUpdateResult, anyhow::Error> {
        use crate::models::bulk_runs::StatusUpdateResult;

        info!("🔄 SERVICE: Checking print status update for run {} by user: {}", run_no, user_id);

        // Get current status
        let current_status_option = self.database
            .get_bulk_run_status(run_no)
            .await
            .context("Failed to get current run status")?;

        let current_status = match current_status_option {
            Some(status) => status.status,
            None => {
                let error_msg = format!("Run {run_no} not found");
                warn!("⚠️ SERVICE: {}", error_msg);
                return Err(anyhow::anyhow!(error_msg));
            }
        };

        // Only update if status is NEW
        if current_status == "NEW" {
            info!("Run {} status is NEW, updating to PRINT", run_no);
            
            // Truncate user_id to 8 chars to fit ModifiedBy column
            let truncated_user_id = if user_id.len() > 8 {
                &user_id[0..8]
            } else {
                user_id
            };

            let update_success = self.database
                .update_bulk_run_status(run_no, "PRINT", truncated_user_id)
                .await
                .context("Failed to update run status to PRINT")?;

            if !update_success {
                let error_msg = format!("Failed to update run {run_no} status - no rows were updated");
                warn!("⚠️ SERVICE: {}", error_msg);
                return Err(anyhow::anyhow!(error_msg));
            }

            info!("✅ SERVICE: Successfully updated run {} status from NEW to PRINT", run_no);

            Ok(StatusUpdateResult {
                old_status: current_status,
                new_status: "PRINT".to_string(),
            })
        } else {
            info!("Run {} status is {}, skipping update to PRINT", run_no, current_status);
            
            Ok(StatusUpdateResult {
                old_status: current_status.clone(),
                new_status: current_status,
            })
        }
    }
}
