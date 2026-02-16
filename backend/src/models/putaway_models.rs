use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Serialize, Deserialize)]
pub struct LotSearchResult {
    pub lot_no: String,
    pub item_key: String,
    pub location: String,
    pub current_bin: String,
    pub qty_on_hand: f64,
    pub qty_commit_sales: f64,
    pub qty_available: f64,
    pub expiry_date: Option<String>,
    pub item_description: String,
    pub uom: String,
    pub lot_status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BinValidationResult {
    pub bin_no: String,
    pub location: String,
    pub is_valid: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BinTransferRequest {
    pub lot_no: String,
    pub item_key: String,
    pub location: String,
    pub bin_from: String,
    pub bin_to: String,
    pub transfer_qty: f64,
    pub user_id: String,
    pub remarks: Option<String>,
    pub referenced: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransferResult {
    pub success: bool,
    pub document_no: String,
    pub message: String,
    pub timestamp: String,
    pub source_lot_status: Option<String>,
    pub destination_lot_status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PutawayHealthResponse {
    pub status: String,
    pub service: String,
    pub timestamp: String,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LotSearchItem {
    pub lot_no: String,
    pub item_key: String,
    pub item_description: String,
    pub location: String,
    pub current_bin: String,
    pub qty_on_hand: f64,
    pub qty_commit_sales: f64,
    pub qty_available: f64,
    pub date_received: Option<String>,
    pub expiry_date: Option<String>,
    pub uom: String,
    pub lot_status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BinSearchItem {
    pub bin_no: String,
    pub location: String,
    pub description: String,
    pub aisle: String,
    pub row: String,
    pub rack: String,
    /// Lot status if this bin contains the specified lot (optional - only populated when lot context is provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lot_status: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PutawayError {
    #[error("Lot not found: {lot_no}")]
    LotNotFound { lot_no: String },

    #[error("Invalid bin: {bin_no} in location {location}")]
    InvalidBin { bin_no: String, location: String },

    #[error("Insufficient quantity: requested {requested}, available {available}")]
    InsufficientQuantity { requested: f64, available: f64 },

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Transaction error: {0}")]
    TransactionError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LotTransactionItem {
    pub lot_tran_no: i32,
    pub lot_no: String,
    pub bin_no: String,
    pub doc_no: String,
    pub issue_doc_line_no: Option<i16>,
    pub qty: f64,
    pub transaction_type: u8,
    pub tran_typ: String,  // Human readable transaction type name
    pub transaction_date: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct CommittedTransferRequest {
    pub lot_no: String,
    pub item_key: String,
    pub location: String,
    pub source_bin: String,
    pub to_bin: String,
    pub transfer_qty: f64,
    pub user_id: String,
    pub remarks: Option<String>,
    pub referenced: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CommittedTransferResult {
    pub success: bool,
    pub message: String,
    pub document_no: String,
    pub timestamp: String,
}

// Internal database models
#[derive(Debug)]
#[allow(dead_code)]
pub struct LotMasterRecord {
    pub lot_no: String,
    pub item_key: String,
    pub location_key: String,
    pub bin_no: String,
    pub qty_on_hand: f64,
    pub qty_issued: f64,
    pub qty_commit_sales: f64,
    pub date_expiry: DateTime<Utc>,
    pub vendor_key: String,
    pub vendor_lot_no: String,
    pub document_no: String,
    pub document_line_no: i16,
    pub transaction_type: u8,
    pub lot_status: String,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct ItemMasterRecord {
    pub item_key: String,
    pub desc1: String,
    pub desc2: String,
    pub stock_uom_code: String,
    pub purchase_uom_code: String,
    pub sales_uom_code: String,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct BinTransferRecord {
    pub bin_tran_id: Option<i32>,
    pub item_key: String,
    pub location: String,
    pub lot_no: String,
    pub bin_no_from: String,
    pub bin_no_to: String,
    pub lot_tran_no: Option<i32>,
    pub qty_on_hand: f64,
    pub transfer_qty: f64,
    pub rec_user_id: String,
    pub rec_date: DateTime<Utc>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct LotTransactionRecord {
    pub lot_tran_no: Option<i32>,
    pub lot_no: String,
    pub item_key: String,
    pub location_key: String,
    pub date_received: DateTime<Utc>,
    pub date_expiry: DateTime<Utc>,
    pub transaction_type: i16, // 8 = Receipt, 9 = Issue
    pub issue_doc_no: Option<String>,
    pub issue_doc_line_no: Option<i16>,
    pub issue_date: Option<DateTime<Utc>>,
    pub qty_issued: Option<f64>,
    pub receipt_doc_no: Option<String>,
    pub receipt_doc_line_no: Option<i16>,
    pub qty_received: Option<f64>,
    pub vendor_key: String,
    pub bin_no: String,
    pub rec_userid: String,
    pub rec_date: DateTime<Utc>,
    pub processed: String,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct SequenceRecord {
    pub seq_name: String,
    pub seq_num: i32,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct MintxdhRecord {
    pub in_trans_id: Option<i32>,
    pub item_key: String,
    pub location: String,
    pub to_location: String,
    pub sys_id: String,
    pub process_id: String,
    pub sys_doc_id: String,
    pub sys_lin_sq: i16,
    pub trn_typ: String,
    pub trn_sub_typ: String,
    pub doc_no: String,
    pub doc_date: DateTime<Utc>,
    pub apl_date: DateTime<Utc>,
    pub trn_desc: String,
    pub trn_qty: f64,
    pub trn_amt: f64,
    pub nl_acct: String,
    pub in_acct: String,
    pub created_serlot: String,
    pub rec_user_id: String,
    pub rec_date: DateTime<Utc>,
    pub rec_time: DateTime<Utc>,
    pub updated_fin_table: Option<bool>,
    pub sort_field: String,
    pub jrnl_btch_no: String,
    pub std_cost: f64,
    pub stdcostupdated: bool,
    pub gl_trn_amt: f64,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct InlocRecord {
    pub item_key: String,
    pub location: String,
    pub inclasskey: String,
    pub revacct: String,
    pub cogsacct: String,
    pub stdcost: f64,
}

// GL Account mapping helper
pub fn map_inclasskey_to_inacct(inclasskey: &str) -> &str {
    match inclasskey {
        "RM" => "1100",      // Raw Materials
        "PM" => "1110",      // Packaging Materials  
        "WIP" => "1120",     // Work in Progress
        "NS" => "1100",      // Non-Stock
        key if key.starts_with("FG-") => "1140", // All Finished Goods
        _ => "1100",         // Default to inventory asset account
    }
}
