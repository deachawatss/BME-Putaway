use axum::{
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use bigdecimal::BigDecimal;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info, instrument, warn};

use crate::database::Database;
use crate::models::bulk_runs::*;
use crate::models::inventory::*;
use crate::services::bulk_runs_service::BulkRunsService;

// Old JWT extraction functions moved to utils::user_management module

#[derive(Serialize)]
pub struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    message: String,
}

/// List all active bulk runs for modal selection
#[instrument(skip(database))]
pub async fn list_bulk_runs(
    State(database): State<Database>,
) -> Result<Json<ApiResponse<BulkRunListResponse>>, StatusCode> {
    info!("Bulk run list endpoint called for modal selection");

    match database.list_active_bulk_runs().await {
        Ok(runs) => {
            let total_count = runs.len() as i32;
            let response = BulkRunListResponse { runs, total_count };

            if total_count == 0 {
                Ok(Json(ApiResponse {
                    success: false,
                    data: Some(response),
                    message: "No active bulk runs found".to_string(),
                }))
            } else {
                Ok(Json(ApiResponse {
                    success: true,
                    data: Some(response),
                    message: format!("Found {total_count} active bulk runs"),
                }))
            }
        }
        Err(e) => {
            warn!("Bulk run list failed: {}", e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "Failed to retrieve bulk runs list".to_string(),
            }))
        }
    }
}

/// Search for bulk runs by query
#[instrument(skip(database))]
pub async fn search_bulk_runs(
    State(database): State<Database>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Vec<BulkRunSearchResponse>>>, StatusCode> {
    info!("Bulk run search endpoint called");

    let empty_string = String::new();
    let query = params.get("query").unwrap_or(&empty_string).trim();
    let default_mode = "partial".to_string();
    let search_mode = params.get("search_mode").unwrap_or(&default_mode).trim();

    if query.is_empty() {
        warn!("Empty search query provided");
        return Ok(Json(ApiResponse {
            success: false,
            data: None,
            message: "Search query is required".to_string(),
        }));
    }

    // SEC-009: Input validation on search parameters
    const MAX_SEARCH_QUERY_LENGTH: usize = 100;
    if query.len() > MAX_SEARCH_QUERY_LENGTH {
        warn!("Search query too long: {} characters (max: {})", query.len(), MAX_SEARCH_QUERY_LENGTH);
        return Ok(Json(ApiResponse {
            success: false,
            data: None,
            message: format!("Search query too long (max {} characters)", MAX_SEARCH_QUERY_LENGTH),
        }));
    }

    // Check for potentially dangerous characters
    if query.contains(';') || query.contains("--") || query.contains("/*") {
        warn!("Search query contains potentially dangerous characters: {}", query);
        return Ok(Json(ApiResponse {
            success: false,
            data: None,
            message: "Invalid characters in search query".to_string(),
        }));
    }

    let service = BulkRunsService::new(database);

    match service.search_bulk_runs(query, search_mode).await {
        Ok(results) => {
            let results_len = results.len();
            if results.is_empty() {
                Ok(Json(ApiResponse {
                    success: false,
                    data: Some(results),
                    message: format!("No bulk runs found for query: {query}"),
                }))
            } else {
                Ok(Json(ApiResponse {
                    success: true,
                    data: Some(results),
                    message: format!("Found {results_len} bulk runs"),
                }))
            }
        }
        Err(e) => {
            warn!("Bulk run search failed: {}", e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "Failed to search bulk runs".to_string(),
            }))
        }
    }
}

/// Get available runs for bulk picking
#[instrument(skip(database))]
pub async fn get_available_runs(
    State(database): State<Database>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Vec<BulkRun>>>, StatusCode> {
    info!("Get available runs endpoint called");

    let limit = params
        .get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(50);

    // Use the service layer to get available runs
    let service = BulkRunsService::new(database);
    match service.get_available_runs().await {
        Ok(runs) => {
            let limited_runs = if limit > 0 {
                runs.into_iter().take(limit as usize).collect()
            } else {
                runs
            };
            
            if limited_runs.is_empty() {
                Ok(Json(ApiResponse {
                    success: false,
                    data: Some(limited_runs),
                    message: "No available bulk runs found".to_string(),
                }))
            } else {
                Ok(Json(ApiResponse {
                    success: true,
                    data: Some(limited_runs.clone()),
                    message: format!("Found {} available runs", limited_runs.len()),
                }))
            }
        }
        Err(e) => {
            warn!("Get available runs failed: {}", e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "Failed to get available runs".to_string(),
            }))
        }
    }
}

/// List active bulk runs with pagination
#[instrument(skip(database))]
pub async fn list_active_bulk_runs_paginated(
    State(database): State<Database>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<PaginatedBulkRunResponse>>, StatusCode> {
    info!("Paginated bulk runs endpoint called");

    let page = params
        .get("page")
        .and_then(|p| p.parse().ok())
        .unwrap_or(1);
    let page_size = params
        .get("page_size")
        .and_then(|ps| ps.parse().ok())
        .unwrap_or(20);

    // Use the service layer 
    let service = BulkRunsService::new(database);
    match service.list_active_bulk_runs_paginated(page, page_size).await {
        Ok(response) => {
            if response.runs.is_empty() {
                Ok(Json(ApiResponse {
                    success: false,
                    data: Some(response),
                    message: "No bulk runs found".to_string(),
                }))
            } else {
                let total_items = response.pagination.total_items;
                Ok(Json(ApiResponse {
                    success: true,
                    data: Some(response),
                    message: format!("Found {total_items} bulk runs"),
                }))
            }
        }
        Err(e) => {
            warn!("Paginated bulk runs failed: {}", e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "Failed to retrieve paginated runs".to_string(),
            }))
        }
    }
}

/// Get bulk run form data for specific run
#[instrument(skip(database))]
pub async fn get_bulk_run_form_data(
    Path(run_no): Path<i32>,
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(database): State<Database>,
) -> Result<Json<ApiResponse<BulkRunFormData>>, StatusCode> {
    let ingredient_index = params.get("ingredient_index")
        .and_then(|s| s.parse::<i32>().ok());
    
    info!("Form data endpoint called for run: {} with ingredient_index: {:?}", run_no, ingredient_index);

    let service = BulkRunsService::new(database);

    match service.get_bulk_run_form_data(run_no, ingredient_index).await {
        Ok(Some(form_data)) => {
            info!("Successfully retrieved form data for run {}", run_no);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(form_data),
                message: format!("Form data retrieved for run {run_no}"),
            }))
        }
        Ok(None) => {
            // Run exists but has no bulk picking requirements (ToPickedBulkQty = 0 for all ingredients)
            warn!("Run {} has no bulk picking requirements", run_no);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Run {run_no} has no bulk picking requirements. All ingredients have ToPickedBulkQty = 0."),
            }))
        }
        Err(e) => {
            warn!("Failed to get form data for run {}: {}", run_no, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Failed to retrieve form data: {e}"),
            }))
        }
    }
}

/// Get next ingredient to pick for a run
#[instrument(skip(database))]
pub async fn get_next_ingredient(
    Path(run_no): Path<i32>,
    State(database): State<Database>,
) -> Result<Json<ApiResponse<IngredientView>>, StatusCode> {
    info!("Next ingredient endpoint called for run: {}", run_no);

    let service = BulkRunsService::new(database);

    match service.get_next_ingredient(run_no, 0).await {
        Ok(Some(ingredient)) => {
            info!("Found next ingredient for run {}", run_no);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(ingredient.current_ingredient),
                message: "Next ingredient found".to_string(),
            }))
        }
        Ok(None) => {
            info!("No more ingredients to pick for run {}", run_no);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "All ingredients completed for this run".to_string(),
            }))
        }
        Err(e) => {
            warn!("Failed to get next ingredient for run {}: {}", run_no, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "Failed to get next ingredient".to_string(),
            }))
        }
    }
}

/// Check run completion status
#[instrument(skip(database))]
pub async fn check_run_completion(
    Path(run_no): Path<i32>,
    State(database): State<Database>,
) -> Result<Json<ApiResponse<bool>>, StatusCode> {
    info!("Run completion check for: {}", run_no);

    let service = BulkRunsService::new(database);
    match service.is_run_complete(run_no).await {
        Ok(is_completed) => {
            let message = if is_completed {
                format!("Run {run_no} is completed")
            } else {
                format!("Run {run_no} is not yet complete")
            };
            Ok(Json(ApiResponse {
                success: true,
                data: Some(is_completed),
                message,
            }))
        }
        Err(e) => {
            warn!("Failed to check completion for run {}: {}", run_no, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "Failed to check run completion".to_string(),
            }))
        }
    }
}

/// **NEW UNIVERSAL RUN COMPLETION ENDPOINT** - Check detailed run completion status
/// Returns comprehensive completion information for automatic status transitions
#[instrument(skip(database))]
pub async fn check_run_completion_status(
    Path(run_no): Path<i32>,
    State(database): State<Database>,
) -> Result<Json<ApiResponse<RunCompletionStatus>>, StatusCode> {
    info!("🔍 UNIVERSAL_COMPLETION: Checking detailed completion status for run: {}", run_no);

    let service = BulkRunsService::new(database);

    // Get comprehensive completion status
    match service.get_run_completion_status(run_no).await {
        Ok(completion_status) => {
            info!("📊 UNIVERSAL_COMPLETION: Run {} - {}/{} complete, {} incomplete",
                  run_no, completion_status.completed_count, completion_status.total_ingredients,
                  completion_status.incomplete_count);

            if completion_status.is_complete {
                info!("🎉 UNIVERSAL_COMPLETION: Run {} is COMPLETE! All ingredients finished.", run_no);
            }

            Ok(Json(ApiResponse {
                success: true,
                data: Some(completion_status),
                message: format!("Completion status retrieved for run {run_no}"),
            }))
        }
        Err(e) => {
            warn!("❌ UNIVERSAL_COMPLETION: Failed to check completion status for run {}: {}", run_no, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Failed to check run completion status: {e}"),
            }))
        }
    }
}

/// **NEW AUTOMATIC STATUS UPDATE ENDPOINT** - Update run status from NEW to PRINT
/// Triggered when all ingredients are complete for automatic workflow
#[instrument(skip(database))]
pub async fn complete_run_status(
    Path(run_no): Path<i32>,
    State(database): State<Database>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<StatusUpdateResult>>, StatusCode> {
    info!("🔄 AUTO_COMPLETE: Attempting to complete run {} status (NEW → PRINT)", run_no);

    // Extract user information from headers
    let user_id = headers.get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("SYSTEM");

    info!("👤 AUTO_COMPLETE: User triggering auto-completion for run {}: {}", run_no, user_id);

    let service = BulkRunsService::new(database);

    // Update run status from NEW to PRINT
    match service.complete_run_status(run_no, user_id).await {
        Ok(status_result) => {
            info!("✅ AUTO_COMPLETE: Successfully updated run {} status: {} → {}",
                  run_no, status_result.old_status, status_result.new_status);

            Ok(Json(ApiResponse {
                success: true,
                data: Some(status_result),
                message: format!("Run {run_no} status successfully updated to PRINT"),
            }))
        }
        Err(e) => {
            warn!("❌ AUTO_COMPLETE: Failed to update run {} status: {}", run_no, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Failed to update run status: {e}"),
            }))
        }
    }
}

/// Get bulk run status
#[instrument(skip(database))]
pub async fn get_run_status(
    Path(run_no): Path<i32>,
    State(database): State<Database>,
) -> Result<Json<ApiResponse<BulkRunStatusResponse>>, StatusCode> {
    info!("Run status endpoint called for: {}", run_no);

    match database.get_bulk_run_status(run_no).await {
        Ok(Some(status_response)) => {
            info!("Run status found: {} - {}", run_no, status_response.status);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(status_response),
                message: format!("Run {run_no} status retrieved"),
            }))
        }
        Ok(None) => {
            warn!("Run status not found: {}", run_no);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Run {run_no} not found"),
            }))
        }
        Err(e) => {
            error!("Failed to get run status {}: {}", run_no, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "Failed to retrieve run status".to_string(),
            }))
        }
    }
}

/// Search for items in a bulk run
#[instrument(skip(database))]
pub async fn search_run_items(
    Path(run_no): Path<i32>,
    State(database): State<Database>,
) -> Result<Json<ApiResponse<Vec<RunItemSearchResult>>>, StatusCode> {
    info!("Item search endpoint called for run: {}", run_no);

    let service = BulkRunsService::new(database);

    match service.search_run_items(run_no).await {
        Ok(items) => {
            let item_count = items.len();
            if items.is_empty() {
                Ok(Json(ApiResponse {
                    success: false,
                    data: Some(items),
                    message: format!("No items found for run {run_no}"),
                }))
            } else {
                Ok(Json(ApiResponse {
                    success: true,
                    data: Some(items),
                    message: format!("Found {item_count} items for run {run_no}"),
                }))
            }
        }
        Err(e) => {
            warn!("Item search failed for run {}: {}", run_no, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "Failed to search run items".to_string(),
            }))
        }
    }
}

/// Get ingredient index for a specific ItemKey in a bulk run
#[instrument(skip(database))]
pub async fn get_ingredient_index(
    Path(run_no): Path<i32>,
    Query(params): Query<HashMap<String, String>>,
    State(database): State<Database>,
) -> Result<Json<ApiResponse<usize>>, StatusCode> {
    let item_key = params.get("item_key").cloned().unwrap_or_default();
    info!("Ingredient index endpoint called for run: {} with item_key: {}", run_no, item_key);

    if item_key.is_empty() {
        return Ok(Json(ApiResponse {
            success: false,
            data: None,
            message: "item_key parameter is required".to_string(),
        }));
    }

    let service = BulkRunsService::new(database);

    match service.get_ingredient_index(run_no, &item_key).await {
        Ok(index) => {
            info!("Found ingredient {} at index {} for run {}", item_key, index, run_no);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(index),
                message: format!("Ingredient {item_key} found at index {index}"),
            }))
        }
        Err(err) => {
            error!("Failed to get ingredient index for run {}: {:?}", run_no, err);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Ingredient {item_key} not found in run {run_no}: {err}"),
            }))
        }
    }
}

/// Get ingredient data by specific coordinates (RowNum, LineId)
/// Used for pallet transitions within the same ingredient
#[instrument(skip(database))]
pub async fn get_ingredient_by_coordinates(
    Path(run_no): Path<i32>,
    Query(params): Query<HashMap<String, String>>,
    State(database): State<Database>,
) -> Result<Json<ApiResponse<BulkRunFormData>>, StatusCode> {
    let item_key = params.get("item_key").cloned().unwrap_or_default();
    let row_num_str = params.get("row_num").cloned().unwrap_or_default();
    let line_id_str = params.get("line_id").cloned().unwrap_or_default();
    
    info!(
        "Get ingredient by coordinates endpoint called for run: {}, item_key: {}, row_num: {}, line_id: {}", 
        run_no, item_key, row_num_str, line_id_str
    );
    
    // Validate required parameters
    if item_key.is_empty() || row_num_str.is_empty() || line_id_str.is_empty() {
        return Ok(Json(ApiResponse {
            success: false,
            data: None,
            message: "Parameters item_key, row_num, and line_id are all required".to_string(),
        }));
    }
    
    // Parse coordinates
    let row_num: i32 = match row_num_str.parse() {
        Ok(num) => num,
        Err(_) => {
            return Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "row_num must be a valid integer".to_string(),
            }));
        }
    };
    
    let line_id: i32 = match line_id_str.parse() {
        Ok(num) => num,
        Err(_) => {
            return Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "line_id must be a valid integer".to_string(),
            }));
        }
    };

    let service = BulkRunsService::new(database);
    
    match service.get_ingredient_by_coordinates(run_no, &item_key, row_num, line_id).await {
        Ok(form_data) => {
            info!(
                "Successfully loaded ingredient {} with coordinates (RowNum: {}, LineId: {}) for run {}", 
                item_key, row_num, line_id, run_no
            );
            Ok(Json(ApiResponse {
                success: true,
                data: Some(form_data),
                message: format!(
                    "Ingredient {item_key} loaded with coordinates RowNum: {row_num}, LineId: {line_id}"
                ),
            }))
        }
        Err(err) => {
            error!(
                "Failed to get ingredient {} by coordinates (RowNum: {}, LineId: {}) for run {}: {:?}", 
                item_key, row_num, line_id, run_no, err
            );
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!(
                    "Failed to load ingredient {item_key} with coordinates (RowNum: {row_num}, LineId: {line_id}): {err}"
                ),
            }))
        }
    }
}

/// Search for lots in a bulk run with pagination support
#[instrument(skip(database))]
pub async fn search_run_lots(
    Path(run_no): Path<i32>,
    Query(params): Query<HashMap<String, String>>,
    State(database): State<Database>,
) -> Result<Json<ApiResponse<PaginatedLotSearchResponse>>, StatusCode> {
    info!("Lot search endpoint called for run: {}", run_no);

    let item_key = params.get("item_key").cloned().unwrap_or_default();
    let page: u32 = params
        .get("page")
        .and_then(|p| p.parse().ok())
        .unwrap_or(1);
    let page_size: u32 = params
        .get("page_size")
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);

    if item_key.is_empty() {
        return Ok(Json(ApiResponse {
            success: false,
            data: None,
            message: "Item key is required for lot search".to_string(),
        }));
    }

    // Validate page parameters
    if page < 1 || !(1..=100).contains(&page_size) {
        return Ok(Json(ApiResponse {
            success: false,
            data: None,
            message: "Invalid pagination parameters".to_string(),
        }));
    }

    // Use service layer for proper business logic and error handling
    let service = BulkRunsService::new(database);
    match service.search_run_lots_paginated(run_no, &item_key, page, page_size).await {
        Ok(paginated_result) => {
            let lot_count = paginated_result.lots.len();
            let total_count = paginated_result.pagination.total_items;
            
            if lot_count == 0 {
                Ok(Json(ApiResponse {
                    success: false,
                    data: Some(paginated_result),
                    message: format!("No lots found for item {item_key} in run {run_no}"),
                }))
            } else {
                Ok(Json(ApiResponse {
                    success: true,
                    data: Some(paginated_result),
                    message: format!("Found {lot_count} of {total_count} total lots for item {item_key}"),
                }))
            }
        }
        Err(e) => {
            warn!("Lot search failed for run {}, item {}: {}", run_no, item_key, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "Failed to search lots".to_string(),
            }))
        }
    }
}

/// Get bins for a specific lot
#[instrument(skip(database))]
pub async fn get_lot_bins(
    Path((run_no, lot_no)): Path<(i32, String)>,
    Query(params): Query<HashMap<String, String>>,
    State(database): State<Database>,
) -> Result<Json<ApiResponse<Vec<LotSearchResult>>>, StatusCode> {
    info!("Lot bins endpoint called for run: {}, lot: {}", run_no, lot_no);

    let item_key = params.get("item_key").cloned().unwrap_or_default();

    if item_key.is_empty() {
        return Ok(Json(ApiResponse {
            success: false,
            data: None,
            message: "Item key is required for bin search".to_string(),
        }));
    }

    // Use service layer for proper business logic and error handling
    let service = BulkRunsService::new(database);
    match service.get_lot_bins(run_no, &lot_no, &item_key).await {
        Ok(bins) => {
            let bin_count = bins.len();
            if bins.is_empty() {
                Ok(Json(ApiResponse {
                    success: false,
                    data: Some(bins),
                    message: format!("No bins found for lot {lot_no} in run {run_no}"),
                }))
            } else {
                Ok(Json(ApiResponse {
                    success: true,
                    data: Some(bins),
                    message: format!("Found {bin_count} bins for lot {lot_no}"),
                }))
            }
        }
        Err(e) => {
            warn!("Bin search failed for lot {}: {}", lot_no, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "Failed to get bins for lot".to_string(),
            }))
        }
    }
}

/// Get pallet tracking data for a run
#[instrument(skip(database))]
pub async fn get_pallet_tracking_data(
    Path(run_no): Path<i32>,
    Query(params): Query<HashMap<String, String>>,
    State(database): State<Database>,
) -> Result<Json<ApiResponse<PalletBatchResponse>>, StatusCode> {
    let item_key = params.get("item_key").cloned();
    info!("Pallet tracking endpoint called for run: {}, item_key: {:?}", run_no, item_key);

    // Use service layer for proper business logic and error handling
    let service = BulkRunsService::new(database);
    match service.get_pallet_tracking_data(run_no, item_key.as_deref()).await {
        Ok(response) => {
            Ok(Json(ApiResponse {
                success: true,
                data: Some(response),
                message: format!("Pallet tracking data retrieved for run {run_no}"),
            }))
        }
        Err(e) => {
            warn!("Failed to get pallet tracking for run {}: {}", run_no, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "Failed to retrieve pallet tracking data".to_string(),
            }))
        }
    }
}

/// Get inventory alerts for an item
#[instrument(skip(database))]
pub async fn get_inventory_alerts(
    Path(item_key): Path<String>,
    State(database): State<Database>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<ApiResponse<Vec<InventoryAlert>>>, StatusCode> {
    info!("Inventory alerts endpoint called for item: {}", item_key);

    // Get run_no from query params or use a default
    let run_no = params
        .get("run_no")
        .and_then(|r| r.parse::<i32>().ok())
        .unwrap_or(0);

    // Get real inventory alerts from the database
    match database.get_inventory_alerts(run_no, &item_key).await {
        Ok(alerts) => {
            info!("Found {} alerts for item {}", alerts.len(), item_key);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(alerts),
                message: format!("Inventory alerts retrieved for item {item_key}"),
            }))
        }
        Err(e) => {
            error!("Failed to get inventory alerts: {:?}", e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Failed to get inventory alerts: {e}"),
            }))
        }
    }
}

/// Confirm pick transaction - BME4-compatible 5-table atomic transaction
#[instrument(skip(database))]
pub async fn confirm_pick(
    Path(run_no): Path<i32>,
    State(database): State<Database>,
    headers: HeaderMap,
    Json(request): Json<PickConfirmationRequest>,
) -> Result<Json<ApiResponse<PickConfirmationResponse>>, StatusCode> {
    info!("Pick confirmation endpoint called for run: {} with lot: {}", run_no, request.lot_no);

    let service = BulkRunsService::new(database);

    // **ENHANCED USER EXTRACTION**: Use new comprehensive user management system
    use crate::utils::user_management::{extract_user_with_debug_info, validate_user_context};
    
    let mut req = request.clone();
    
    // Extract user with comprehensive debug information
    let (extracted_user, debug_info) = extract_user_with_debug_info(&headers, req.user_id.as_ref());
    
    // Set the extracted user ID on the request
    req.user_id = extracted_user.clone();
    
    // Pre-validate user context before passing to service layer  
    let final_user = match validate_user_context(req.user_id.as_ref()) {
        Ok(user) => user,
        Err(e) => {
            warn!("User context validation failed: {}", e);
            return Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "Failed to validate user context for transaction".to_string(),
            }));
        }
    };
    
    info!(
        "🔐 ENHANCED_USER_EXTRACTION: Pick confirmation for run {} - Debug: [{}], Final user: '{}'", 
        run_no, debug_info, final_user
    );

    match service.confirm_pick_transaction(run_no, req).await {
        Ok(response) => {
            info!("Pick confirmation completed successfully for run {}", run_no);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(response),
                message: format!("Pick confirmed successfully for run {run_no}"),
            }))
        }
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("BATCH_ALREADY_COMPLETED") {
                let user_msg = "This batch is already completed. Please refresh to load the next batch.".to_string();
                warn!(
                    "Pick confirmation failed (BATCH_ALREADY_COMPLETED) for run {}: {}",
                    run_no, err_str
                );
                return Ok(Json(ApiResponse {
                    success: false,
                    data: None,
                    message: user_msg,
                }));
            }

            warn!("Pick confirmation failed for run {}: {}", run_no, err_str);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Failed to confirm pick operation: {err_str}"),
            }))
        }
    }
}

/// Health check endpoint for bulk runs API
#[instrument]
pub async fn bulk_runs_health() -> Json<ApiResponse<String>> {
    Json(ApiResponse {
        success: true,
        data: Some("Bulk Runs API with BME4 Pick Transaction Support".to_string()),
        message: "Service healthy".to_string(),
    })
}

/// Debug endpoint to test validation without actual transaction
#[instrument(skip(database))]
pub async fn debug_validation(
    Path(run_no): Path<i32>,
    State(database): State<Database>,
    headers: HeaderMap,
    Json(request): Json<PickConfirmationRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    info!("Debug validation endpoint called for run: {} with lot: {}", run_no, request.lot_no);

    let service = BulkRunsService::new(database);

    let header_user = headers
        .get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let mut req = request.clone();
    if req.user_id.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
        req.user_id = header_user;
    }

    match service.validate_pick_request(run_no, req.clone()).await {
        Ok(validation_result) => {
            let debug_info = serde_json::json!({
                "validation_result": {
                    "is_valid": validation_result.is_valid,
                    "error_message": validation_result.error_message,
                    "warnings": validation_result.warnings,
                    "max_allowed_quantity": validation_result.max_allowed_quantity,
                    "available_inventory": validation_result.available_inventory
                },
                "request": {
                    "row_num": request.row_num,
                    "line_id": request.line_id,
                    "picked_bulk_qty": request.picked_bulk_qty.to_string(),
                    "lot_no": request.lot_no,
                    "bin_no": req.bin_no
                }
            });
            
            Ok(Json(ApiResponse {
                success: true,
                data: Some(debug_info),
                message: "Validation debug completed".to_string(),
            }))
        }
        Err(e) => {
            let error_info = serde_json::json!({
                "validation_error": e.to_string(),
                "request": {
                    "row_num": req.row_num,
                    "line_id": req.line_id,
                    "picked_bulk_qty": req.picked_bulk_qty.to_string(),
                    "lot_no": req.lot_no,
                    "bin_no": req.bin_no
                }
            });
            
            Ok(Json(ApiResponse {
                success: false,
                data: Some(error_info),
                message: format!("Validation failed: {e}"),
            }))
        }
    }
}

/// Check if a specific pallet is completed
pub async fn check_pallet_completion(
    Path((run_no, row_num, line_id)): Path<(i32, i32, i32)>,
    Extension(service): Extension<Arc<BulkRunsService>>,
) -> Json<ApiResponse<serde_json::Value>> {
    info!("API: Checking pallet completion for run: {}, row_num: {}, line_id: {}", 
          run_no, row_num, line_id);
    
    match service.is_pallet_completed(run_no, row_num, line_id).await {
        Ok(is_completed) => {
            let result = serde_json::json!({
                "is_completed": is_completed,
                "pallet_info": {
                    "run_no": run_no,
                    "row_num": row_num,
                    "line_id": line_id
                }
            });
            
            Json(ApiResponse {
                success: true,
                data: Some(result),
                message: format!("Pallet completion check completed - Status: {}", 
                               if is_completed { "COMPLETED" } else { "IN_PROGRESS" }),
            })
        }
        Err(e) => {
            let error_info = serde_json::json!({
                "error": e.to_string(),
                "pallet_info": {
                    "run_no": run_no,
                    "row_num": row_num,
                    "line_id": line_id
                }
            });
            
            Json(ApiResponse {
                success: false,
                data: Some(error_info),
                message: format!("Failed to check pallet completion: {e}"),
            })
        }
    }
}

/// Get next available pallet for the same ingredient
pub async fn get_next_pallet(
    Path((run_no, current_row_num, line_id)): Path<(i32, i32, i32)>,
    Extension(service): Extension<Arc<BulkRunsService>>,
) -> Json<ApiResponse<serde_json::Value>> {
    info!("API: Finding next available pallet for run: {}, current_row_num: {}, line_id: {}", 
          run_no, current_row_num, line_id);
    
    match service.get_next_available_pallet(run_no, current_row_num, line_id).await {
        Ok(maybe_pallet) => {
            match maybe_pallet {
                Some(pallet) => {
                    let result = serde_json::json!({
                        "has_next_pallet": true,
                        "next_pallet": {
                            "run_no": pallet.run_no,
                            "row_num": pallet.row_num,
                            "line_id": pallet.line_id,
                            "batch_no": pallet.batch_no,
                            "item_key": pallet.item_key,
                            "pallet_number": pallet.pallet_number,
                            "pack_size": pallet.pack_size,
                            "to_picked_bulk_qty": pallet.to_picked_bulk_qty,
                            "picked_bulk_qty": pallet.picked_bulk_qty,
                            "description": pallet.description
                        }
                    });
                    
                    Json(ApiResponse {
                        success: true,
                        data: Some(result),
                        message: format!("Next pallet found: Pallet #{} (RowNum: {})", 
                                       pallet.pallet_number, pallet.row_num),
                    })
                }
                None => {
                    let result = serde_json::json!({
                        "has_next_pallet": false,
                        "message": "No more unpicked pallets available for this ingredient"
                    });
                    
                    Json(ApiResponse {
                        success: true,
                        data: Some(result),
                        message: "No more pallets available for this ingredient".to_string(),
                    })
                }
            }
        }
        Err(e) => {
            let error_info = serde_json::json!({
                "error": e.to_string(),
                "search_params": {
                    "run_no": run_no,
                    "current_row_num": current_row_num,
                    "line_id": line_id
                }
            });
            
            Json(ApiResponse {
                success: false,
                data: Some(error_info),
                message: format!("Failed to find next pallet: {e}"),
            })
        }
    }
}

/// Get picked lots for a specific batch and ingredient (for unpicking modal)
#[instrument(skip(database))]
pub async fn get_picked_lots(
    State(database): State<Database>,
    Path((run_no, row_num, line_id)): Path<(i32, i32, i32)>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<PickedLotsResponse>>, StatusCode> {
    info!("🔍 Get picked lots endpoint called for run: {}, row: {}, line: {}", run_no, row_num, line_id);

    // Extract user information from headers
    let user_id = headers.get("x-user-id").and_then(|v| v.to_str().ok()).unwrap_or("SYSTEM");
    info!("👤 User requesting picked lots: {}", user_id);

    match database.get_picked_lots_for_ingredient(run_no, row_num, line_id).await {
        Ok(picked_lots_response) => {
            let lot_count = picked_lots_response.picked_lots.len();
            info!("✅ Retrieved {} picked lots for run: {}, row: {}, line: {}", lot_count, run_no, row_num, line_id);
            
            // Always return success: true for successful database operations
            // Frontend will handle empty arrays appropriately
            Ok(Json(ApiResponse {
                success: true,
                data: Some(picked_lots_response),
                message: if lot_count == 0 {
                    "No picked lots found for this ingredient".to_string()
                } else {
                    format!("Found {lot_count} picked lot(s)")
                },
            }))
        }
        Err(e) => {
            error!("❌ Failed to get picked lots: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get ALL picked lots for an entire run (across all ingredients)
#[instrument(skip(database))]
pub async fn get_all_picked_lots_for_run(
    State(database): State<Database>,
    Path(run_no): Path<i32>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<PickedLotsResponse>>, StatusCode> {
    info!("🔍 Get ALL picked lots endpoint called for run: {}", run_no);
    
    // Extract user information from headers
    let user_id = headers.get("x-user-id").and_then(|v| v.to_str().ok()).unwrap_or("SYSTEM");
    info!("👤 User requesting all picked lots for run: {} by user: {}", run_no, user_id);
    
    match database.get_all_picked_lots_for_run(run_no).await {
        Ok(picked_lots_response) => {
            let lot_count = picked_lots_response.picked_lots.len();
            info!("✅ Retrieved {} picked lots for entire run: {}", lot_count, run_no);
            
            // Always return success: true for successful database operations
            // Frontend will handle empty arrays appropriately
            Ok(Json(ApiResponse {
                success: true,
                data: Some(picked_lots_response),
                message: if lot_count == 0 {
                    "No picked lots found for this run".to_string()
                } else {
                    format!("Found {lot_count} picked lot(s) across all ingredients")
                },
            }))
        }
        Err(e) => {
            error!("❌ Failed to get all picked lots for run {}: {}", run_no, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Unpick a batch or specific lot (replicate official app unpicking pattern)
#[instrument(skip(database))]
pub async fn unpick_ingredient(
    State(database): State<Database>,
    Path((run_no, row_num, line_id)): Path<(i32, i32, i32)>,
    headers: HeaderMap,
    Json(unpick_request): Json<UnpickRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    info!("🔄 Unpick endpoint called for run: {}, row: {}, line: {}", run_no, row_num, line_id);
    
    // Extract user information from headers  
    let user_id = headers.get("x-user-id").and_then(|v| v.to_str().ok()).unwrap_or("SYSTEM");
    info!("👤 User requesting unpick: {}", user_id);

    // NEW: Priority order for unpick operations
    // 1. If lot_tran_no is provided, use precise unpick (highest priority)
    // 2. If lot_no is provided, use legacy lot-based unpick
    // 3. Otherwise, use batch unpick
    
    if let Some(lot_tran_no) = unpick_request.lot_tran_no {
        info!("🎯 Precise unpick using LotTranNo: {}", lot_tran_no);
        match database.unpick_by_lot_tran_no(lot_tran_no, user_id).await {
            Ok(result) => Ok(Json(ApiResponse {
                success: true,
                data: Some(result),
                message: format!("Successfully unpicked record with LotTranNo {lot_tran_no}"),
            })),
            Err(e) => {
                error!("❌ Failed to unpick LotTranNo {}: {}", lot_tran_no, e);
                Ok(Json(ApiResponse {
                    success: false,
                    data: Some(serde_json::json!({"error": e.to_string()})),
                    message: format!("Failed to unpick LotTranNo {lot_tran_no}: {e}"),
                }))
            }
        }
    } else {
        match unpick_request.lot_no {
            Some(lot_no) => {
                info!("🎯 Unpicking specific lot: {}", lot_no);
                match database.unpick_specific_lot(run_no, row_num, line_id, &lot_no, user_id).await {
                    Ok(result) => Ok(Json(ApiResponse {
                        success: true,
                        data: Some(result),
                        message: format!("Successfully unpicked lot {lot_no}"),
                    })),
                    Err(e) => {
                        error!("❌ Failed to unpick lot {}: {}", lot_no, e);
                        Ok(Json(ApiResponse {
                            success: false,
                            data: Some(serde_json::json!({"error": e.to_string()})),
                            message: format!("Failed to unpick lot {lot_no}: {e}"),
                        }))
                    }
                }
            }
            None => {
                info!("🎯 Unpicking entire batch");
                match database.unpick_entire_batch(run_no, row_num, line_id, user_id).await {
                    Ok(result) => Ok(Json(ApiResponse {
                        success: true,
                        data: Some(result),
                        message: "Successfully unpicked entire batch".to_string(),
                    })),
                    Err(e) => {
                        error!("❌ Failed to unpick batch: {}", e);
                        Ok(Json(ApiResponse {
                            success: false,
                            data: Some(serde_json::json!({"error": e.to_string()})),
                            message: format!("Failed to unpick batch: {e}"),
                        }))
                    }
                }
            }
        }
    }
}

/// Unpick all lots from all ingredients in a run
#[instrument(skip(database))]
pub async fn unpick_all_run_lots(
    State(database): State<Database>,
    Path(run_no): Path<i32>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    info!("🔄 Unpick ALL run lots endpoint called for run: {}", run_no);
    
    // Extract user information from headers  
    let user_id = headers.get("x-user-id").and_then(|v| v.to_str().ok()).unwrap_or("SYSTEM");
    info!("👤 User requesting run-wide unpick: {}", user_id);

    match database.unpick_all_run_lots(run_no, user_id).await {
        Ok(result) => Ok(Json(ApiResponse {
            success: true,
            data: Some(result),
            message: "Successfully unpicked all lots from entire run".to_string(),
        })),
        Err(e) => {
            error!("❌ Failed to unpick all run lots: {}", e);
            Ok(Json(ApiResponse {
                success: false,
                data: Some(serde_json::json!({"error": e.to_string()})),
                message: format!("Failed to unpick all run lots: {e}"),
            }))
        }
    }
}

/// Get batch weight summary for pending to picked modal
#[instrument(skip(database))]
pub async fn get_batch_weight_summary(
    State(database): State<Database>,
    Path(run_no): Path<i32>,
) -> Result<Json<ApiResponse<BatchWeightSummaryResponse>>, StatusCode> {
    info!("📊 Get batch weight summary endpoint called for run: {}", run_no);

    match database.get_batch_weight_summary(run_no).await {
        Ok(batch_items) => {
            // Calculate total remaining weight
            let total_remaining_weight = batch_items
                .iter()
                .fold(BigDecimal::from(0), |acc, item| acc + &item.remaining_weight_kg);

            let total_items = batch_items.len() as i32;

            let response = BatchWeightSummaryResponse {
                batch_items,
                run_no,
                total_items,
                total_remaining_weight,
            };

            info!("📊 Found {} batch items with total remaining weight: {}", total_items, response.total_remaining_weight);

            Ok(Json(ApiResponse {
                success: true,
                data: Some(response),
                message: format!("Found {total_items} batch items for run {run_no}"),
            }))
        }
        Err(e) => {
            error!("❌ Failed to get batch weight summary for run {}: {}", run_no, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: "Failed to get batch weight summary".to_string(),
            }))
        }
    }
}

/// Get lot picking details for print labels (individual bin picks)
#[instrument(skip(database))]
pub async fn get_run_lot_details(
    State(database): State<Database>,
    Path(run_no): Path<i32>,
) -> Result<Json<ApiResponse<Vec<LotPickingDetail>>>, StatusCode> {
    info!("🏷️ Get lot picking details endpoint called for run: {}", run_no);

    match database.get_lot_picking_details_for_run(run_no).await {
        Ok(lot_details) => {
            let count = lot_details.len();
            info!("🏷️ Found {} lot picking details for run {}", count, run_no);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(lot_details),
                message: format!("Found {count} lot picking details for run {run_no}"),
            }))
        }
        Err(e) => {
            error!("❌ Failed to get lot picking details for run {}: {}", run_no, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Failed to get lot picking details: {e}"),
            }))
        }
    }
}

/// **REVERT STATUS ENDPOINT** - Revert bulk run status from PRINT back to NEW
/// Used when user wants to make changes after run completion
#[instrument(skip(database))]
pub async fn revert_run_status(
    State(database): State<Database>,
    Path(run_no): Path<i32>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<BulkRunStatusResponse>>, StatusCode> {
    info!("🔄 REVERT: Status revert endpoint called for run: {}", run_no);

    // Extract user information from headers
    let user_id = headers.get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("SYSTEM");

    info!("👤 REVERT: User requesting status revert for run {}: {}", run_no, user_id);

    // Validate that user_id is not empty or just whitespace
    let user_id = user_id.trim();
    if user_id.is_empty() || user_id == "SYSTEM" {
        warn!("⚠️ REVERT: Invalid or missing user ID for run {} revert", run_no);
        return Ok(Json(ApiResponse {
            success: false,
            data: None,
            message: "Valid user authentication required for status revert operation".to_string(),
        }));
    }

    // Initialize service and delegate all business logic to service layer
    let service = BulkRunsService::new(database);

    match service.revert_bulk_run_status(run_no, user_id).await {
        Ok(Some(updated_status)) => {
            info!("✅ REVERT: Successfully reverted run {} status from PRINT to NEW", run_no);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(updated_status),
                message: format!("Run {run_no} status successfully reverted from PRINT to NEW"),
            }))
        }
        Ok(None) => {
            warn!("⚠️ REVERT: Failed to revert run {} - validation failed or no changes made", run_no);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Cannot revert run {run_no} - run may not exist, not in PRINT status, or already reverted"),
            }))
        }
        Err(e) => {
            error!("❌ REVERT: Service error during status revert for run {}: {}", run_no, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Database error during status revert: {e}"),
            }))
        }
    }
}

/// **NEW PRINT STATUS UPDATE ENDPOINT** - Update run status to PRINT
/// Triggered when printing labels
#[instrument(skip(database))]
pub async fn update_print_status(
    Path(run_no): Path<i32>,
    State(database): State<Database>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<StatusUpdateResult>>, StatusCode> {
    info!("🔄 PRINT_STATUS: Attempting to update run {} status to PRINT", run_no);

    // Extract user information from headers
    let user_id = headers.get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("SYSTEM");

    info!("👤 PRINT_STATUS: User triggering print status update for run {}: {}", run_no, user_id);

    let service = BulkRunsService::new(database);

    // Update run status to PRINT if currently NEW
    match service.update_print_status(run_no, user_id).await {
        Ok(status_result) => {
            info!("✅ PRINT_STATUS: Successfully processed print status update for run {}", run_no);

            Ok(Json(ApiResponse {
                success: true,
                data: Some(status_result),
                message: format!("Run {run_no} print status update processed"),
            }))
        }
        Err(e) => {
            warn!("❌ PRINT_STATUS: Failed to update run {} status: {}", run_no, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Failed to update run status: {e}"),
            }))
        }
    }
}
