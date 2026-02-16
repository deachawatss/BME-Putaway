use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PutawayItem {
    pub lot_no: String,
    pub item_key: String,
    pub item_description: Option<String>,
    pub location_key: String,
    pub bin_no: Option<String>,
    pub qty_received: f64,
    pub qty_on_hand: f64,
    pub date_received: DateTime<Utc>,
    pub date_expiry: DateTime<Utc>,
    pub vendor_key: String,
    pub vendor_lot_no: String,
    pub document_no: String,
    pub lot_status: String,
    pub rec_user_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ScanType {
    Item,
    Location,
    Lot,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanResponse {
    pub valid: bool,
    pub scan_type: ScanType,
    pub data: Option<ScanData>,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ScanData {
    Item {
        item_key: String,
        description: String,
        unit: String,
    },
    Location {
        location_key: String,
        description: String,
        location_type: String,
    },
    Lot {
        lot_no: String,
        item_key: String,
        qty_on_hand: f64,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PutawayHistory {
    pub transaction_id: i32,
    pub lot_no: String,
    pub item_key: String,
    pub from_location: String,
    pub to_location: String,
    pub bin_no: String,
    pub qty_moved: f64,
    pub transaction_date: DateTime<Utc>,
    pub user_id: String,
}
