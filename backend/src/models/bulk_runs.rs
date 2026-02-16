use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a bulk run record from Cust_BulkRun table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkRun {
    pub run_no: i32,
    pub batch_no: String,
    pub formula_id: String,
    pub formula_desc: String,
    pub no_of_batches: i32,
    pub pallets_per_batch: Option<i32>,
    pub status: String,
    pub created_date: Option<DateTime<Utc>>,
    pub picking_date: Option<DateTime<Utc>>,
}

/// Represents an ingredient record from cust_BulkPicked table (aggregated across all batches)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkPickedItem {
    pub run_no: i32,
    pub row_num: i32,
    pub line_id: i32,
    pub item_key: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub standard_qty: BigDecimal,
    pub pack_size: BigDecimal,
    pub uom: String,
    pub to_picked_std_qty: BigDecimal,
    pub to_picked_bulk_qty: BigDecimal,  // Total required across all batches
    pub picked_bulk_qty: Option<BigDecimal>,  // Total picked across all batches
    pub picking_date: Option<DateTime<Utc>>,
    pub status: Option<String>,
    // NEW: Batch tracking fields for accurate completion detection
    pub total_batches: Option<i32>,
    pub completed_batches: Option<i32>,
    pub remaining_qty: Option<BigDecimal>,
    pub completion_status: Option<String>, // 'COMPLETE', 'IN_PROGRESS', 'PENDING', 'NOT_REQUIRED'
}

/// API response model for bulk run search
#[derive(Debug, Serialize, Deserialize)]
pub struct BulkRunSearchResponse {
    pub run: BulkRun,
    pub ingredients: Vec<BulkPickedItem>,
    pub total_ingredients: i32,
    pub completed_ingredients: i32,
}

/// API response model for bulk run status
#[derive(Debug, Serialize, Deserialize)]
pub struct BulkRunStatusResponse {
    pub run_no: i32,
    pub status: String,
    pub formula_desc: String,
    pub last_modified: Option<DateTime<Utc>>,
}

/// Inventory information for an ingredient
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryInfo {
    pub item_key: String,
    pub soh_value: BigDecimal,
    pub soh_uom: String,
    pub bulk_pack_size_value: BigDecimal,
    pub bulk_pack_size_uom: String,
    pub available_lots: Vec<LotInfo>,
}

/// Lot information for inventory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LotInfo {
    pub lot_no: String,
    pub expiry_date: Option<DateTime<Utc>>,
    pub available_qty: BigDecimal,
    pub location: String,
    pub bin: Option<String>,
}

/// Calculated quantities for an ingredient
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngredientCalculation {
    pub total_needed: BigDecimal,
    pub remaining_to_pick: BigDecimal,
    pub completion_percentage: f64,
}

/// Complete ingredient view with calculations and inventory
#[derive(Debug, Serialize, Deserialize)]
pub struct IngredientView {
    pub ingredient: BulkPickedItem,
    pub inventory: InventoryInfo,
    pub calculations: IngredientCalculation,
}

/// API model for form population
#[derive(Debug, Serialize, Deserialize)]
pub struct BulkRunFormData {
    pub run: BulkRun,
    pub current_ingredient: IngredientView,
    pub form_data: FormFields,
}

/// Form field data for frontend population
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormFields {
    pub fg_item_key: String,
    pub st_picking_date: String,
    pub item_key: String,
    pub soh_value: String,
    pub soh_uom: String,
    pub bulk_pack_size_value: String,
    pub bulk_pack_size_uom: String,
    // Total needed fields - dual display (bags and weight)
    pub total_needed_bags: String,
    pub total_needed_bags_uom: String,    // "BAGS"
    pub total_needed_kg: String,
    pub total_needed_kg_uom: String,      // "KG"
    // Remaining to pick fields - dual display (bags and weight)
    pub remaining_bags: String,
    pub remaining_bags_uom: String,       // "BAGS"
    pub remaining_kg: String, 
    pub remaining_kg_uom: String,         // "KG"
    pub ingredient_index: i32,
    pub total_ingredients: i32,
}

/// Summary data for bulk run selection modal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkRunSummary {
    pub run_no: i32,
    pub formula_id: String,
    pub formula_desc: String,
    pub status: String,
    pub batch_count: i32,
}

/// API response for bulk runs listing in modal
#[derive(Debug, Serialize, Deserialize)]
pub struct BulkRunListResponse {
    pub runs: Vec<BulkRunSummary>,
    pub total_count: i32,
}

/// Pagination information (Story 1.4)
#[derive(Debug, Serialize, Deserialize)]
pub struct PaginationInfo {
    pub current_page: u32,
    pub total_pages: u32,
    pub total_items: u64,
    pub page_size: u32,
    pub has_previous: bool,
    pub has_next: bool,
}

/// Paginated response for bulk runs (Story 1.4)
#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedBulkRunResponse {
    pub runs: Vec<BulkRunSummary>,
    pub pagination: PaginationInfo,
}

/// Search result for run item modal display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunItemSearchResult {
    pub item_key: String,
    pub description: String,
    pub location: String,
    pub line_id: i32,
    pub pack_size: String,
    pub uom: String,
    pub to_picked_bulk_qty: String, // CRITICAL: Show ToPickedBulkQty for verification
    pub picked_bulk_qty: String,    // CRITICAL: Current picked quantity for auto-switching logic
}

/// Lot search result for lot selection modal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LotSearchResult {
    pub lot_no: String,
    pub bin_no: String,
    pub date_exp: DateTime<Utc>,
    pub qty_on_hand: BigDecimal,
    pub qty_issue: BigDecimal,
    pub committed_qty: BigDecimal,
    pub available_qty: BigDecimal,  // calculated field: qty_on_hand - committed_qty
    pub available_bags: i32,        // calculated field: FLOOR(available_qty / pack_size)
    pub pack_size: BigDecimal,      // pack size for bag calculation
    pub item_key: String,
    pub location_key: String,
}

/// Pallet batch tracking data for bulk picking operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PalletBatch {
    pub pallet_number: i32,
    pub batch_number: String,
    pub row_num: i32,
    pub no_of_bags_picked: i32,
    pub quantity_picked: f64,
    pub no_of_bags_remaining: i32,
    pub quantity_remaining: f64,
}

/// Extended pallet batch with completion detection fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PalletBatchInfo {
    pub run_no: i32,
    pub row_num: i32,
    pub line_id: i32,
    pub batch_no: String,
    pub item_key: String,
    pub pallet_number: i32,
    pub pack_size: f64,
    pub to_picked_bulk_qty: f64,
    pub picked_bulk_qty: f64,
    pub description: String,
    pub is_completed: bool,
}

/// Response wrapper for pallet batch data
#[derive(Debug, Serialize, Deserialize)]
pub struct PalletBatchResponse {
    pub run_no: i32,
    pub pallets: Vec<PalletBatch>,
    pub total_pallets: i32,
    pub total_picked_quantity: f64,
    pub total_remaining_quantity: f64,
}

/// Pick confirmation request for BME4-compatible atomic transaction
#[derive(Debug, Clone, Deserialize)]
pub struct PickConfirmationRequest {
    pub row_num: i32,
    pub line_id: i32,
    pub picked_bulk_qty: BigDecimal,
    pub lot_no: String,
    pub bin_no: String,
    /// Optional user id (username). If absent, server falls back to header or SYSTEM.
    pub user_id: Option<String>,
}

/// Pick confirmation response
#[derive(Debug, Serialize)]
pub struct PickConfirmationResponse {
    pub success: bool,
    pub transaction_id: Option<i64>,
    pub document_no: Option<String>,
    pub updated_records: TransactionSummary,
}

/// Summary of records updated during pick transaction
#[derive(Debug, Serialize)]
pub struct TransactionSummary {
    pub bulk_picked_updated: bool,
    pub lot_picked_created: bool,
    pub lot_master_updated: bool,
    pub lot_transaction_created: bool,
    pub pallet_lot_picked_created: bool,
    pub total_committed_qty: BigDecimal,
}

/// Batch information for pick calculations
#[derive(Debug, Clone)]
pub struct BatchInfo {
    pub batch_no: String,
    pub item_key: String,
    pub pack_size: BigDecimal,
}

/// Paginated response for lot search
#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedLotSearchResponse {
    pub lots: Vec<LotSearchResult>,
    pub pagination: PaginationInfo,
}

/// Pick validation result with BME4-compatible business rules
#[derive(Debug, Serialize)]
pub struct PickValidationResult {
    pub is_valid: bool,
    pub error_message: Option<String>,
    pub warnings: Vec<String>,
    pub max_allowed_quantity: Option<BigDecimal>,
    pub available_inventory: Option<BigDecimal>,
}

/// Picked lot information for unpicking modal
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PickedLot {
    pub lot_tran_no: i32,        // NEW: Primary key for precise deletion
    pub lot_no: String,
    pub bin_no: String,
    pub batch_no: String,        // NEW: Batch number for table display
    pub item_key: String,        // NEW: Item key for table display
    pub location_key: String,    // NEW: Location key for table display
    pub row_num: i32,            // NEW: RowNum for correct unpick targeting
    pub line_id: i32,            // NEW: LineId for cross-ingredient unpick operations
    pub date_exp: Option<chrono::DateTime<chrono::Utc>>, // NEW: Expiry date
    pub qty_received: BigDecimal,// NEW: Quantity received
    pub alloc_lot_qty: BigDecimal,
    pub pack_size: BigDecimal,
    pub qty_on_hand: BigDecimal, // NEW: Current on-hand quantity
    pub rec_date: String,
    pub rec_userid: String,
}

/// Pending batch requirement information for pending to picked tab
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PendingBatchRequirement {
    pub batch_no: String,
    pub item_key: String,
    pub to_picked_bulk_qty: BigDecimal,
    pub pack_size: BigDecimal,
    pub total_weight_needed: BigDecimal,  // NEW: bags × pack_size (e.g., 5 × 20 = 100kg)
    pub picked_bulk_qty: BigDecimal,
    pub total_weight_picked: BigDecimal,  // NEW: picked_bags × pack_size
    pub remaining_qty: BigDecimal,
    pub row_num: i32,
    pub line_id: i32,
}

/// Response for picked lots API
#[derive(Debug, Serialize)]
pub struct PickedLotsResponse {
    pub picked_lots: Vec<PickedLot>,
    pub available_lots: Vec<LotSearchResult>, // Available lots for pending tab
    pub pending_batches: Vec<PendingBatchRequirement>, // NEW: Pending batch requirements
    pub total_picked_qty: BigDecimal,
    pub batch_no: String,
    pub item_key: String,
    pub item_description: Option<String>, // Item description for header
    pub run_no: i32,                     // Run number for header
}

/// Batch weight summary item for pending to picked modal
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BatchWeightSummaryItem {
    pub batch_no: String,
    pub item_key: String,
    pub item_description: Option<String>,
    pub to_picked_bulk_qty: BigDecimal,      // Number of bags needed
    pub picked_bulk_qty: BigDecimal,         // Number of bags picked  
    pub pack_size: BigDecimal,               // KG per bag
    pub total_weight_kg: BigDecimal,         // Total weight needed (bags × pack_size)
    pub picked_weight_kg: BigDecimal,        // Weight already picked
    pub remaining_weight_kg: BigDecimal,     // Weight remaining to pick
    pub row_num: i32,
    pub line_id: i32,
}

/// Response for batch weight summary API
#[derive(Debug, Serialize)]
pub struct BatchWeightSummaryResponse {
    pub batch_items: Vec<BatchWeightSummaryItem>,
    pub run_no: i32,
    pub total_items: i32,
    pub total_remaining_weight: BigDecimal,   // Sum of all remaining weights
}

/// Request for unpicking operations
#[derive(Debug, Deserialize)]
pub struct UnpickRequest {
    pub lot_no: Option<String>, // None for batch unpick, Some(lot) for lot unpick
    pub lot_tran_no: Option<i32>, // NEW: For precise single-record unpick operations
}

/// Individual lot picking detail for print labels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LotPickingDetail {
    pub run_no: i32,
    pub batch_no: String,
    pub line_id: i32,
    pub item_key: String,
    pub lot_no: String,
    pub bin_no: String,
    pub qty_received: BigDecimal,
    pub pack_size: BigDecimal,
    pub rec_userid: String,
    pub modified_by: String,
    pub rec_date: String,
    pub picked_bulk_qty: BigDecimal,
    pub picked_qty: BigDecimal,
}

/// **NEW UNIVERSAL RUN COMPLETION** - Detailed completion status response
#[derive(Debug, Serialize, Deserialize)]
pub struct RunCompletionStatus {
    pub is_complete: bool,
    pub incomplete_count: i32,
    pub completed_count: i32,
    pub total_ingredients: i32,
}

/// **NEW AUTOMATIC STATUS UPDATE** - Status update result response
#[derive(Debug, Serialize, Deserialize)]
pub struct StatusUpdateResult {
    pub old_status: String,
    pub new_status: String,
}
