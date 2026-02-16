use crate::utils::bangkok_now_rfc3339;
use crate::database::{Database, putaway_db::PutawayDatabase};
use crate::models::putaway_models::{
    LotSearchResult, BinValidationResult, BinTransferRequest, 
    TransferResult, PutawayHealthResponse, LotSearchItem, BinSearchItem, PutawayError
};

pub struct PutawayService {
    db: PutawayDatabase,
}

impl PutawayService {
    pub fn new(database: Database) -> Self {
        Self {
            db: PutawayDatabase::new(database),
        }
    }

    /// Search for lot details by lot number
    pub async fn search_lot(&self, lot_no: &str) -> Result<LotSearchResult, PutawayError> {
        // Validate input
        if lot_no.trim().is_empty() {
            return Err(PutawayError::ValidationError("Lot number cannot be empty".to_string()));
        }

        // Search in database
        match self.db.find_lot_by_number(lot_no).await? {
            Some((lot_record, item_record)) => {
                // Calculate available quantity (QtyOnHand - QtyCommitSales)
                let qty_available = lot_record.qty_on_hand - lot_record.qty_commit_sales;

                // Format expiry date
                let expiry_date = lot_record.date_expiry.format("%Y-%m-%d").to_string();

                Ok(LotSearchResult {
                    lot_no: lot_record.lot_no,
                    item_key: lot_record.item_key,
                    location: lot_record.location_key,
                    current_bin: lot_record.bin_no,
                    qty_on_hand: lot_record.qty_on_hand,
                    qty_commit_sales: lot_record.qty_commit_sales,
                    qty_available,
                    expiry_date: Some(expiry_date),
                    item_description: item_record.desc1,
                    uom: item_record.stock_uom_code,
                    lot_status: lot_record.lot_status,
                })
            }
            None => Err(PutawayError::LotNotFound { lot_no: lot_no.to_string() }),
        }
    }

    /// Validate destination bin
    pub async fn validate_bin(&self, location: &str, bin_no: &str) -> Result<BinValidationResult, PutawayError> {
        // Validate input
        if location.trim().is_empty() || bin_no.trim().is_empty() {
            return Ok(BinValidationResult {
                bin_no: bin_no.to_string(),
                location: location.to_string(),
                is_valid: false,
                message: "Location and bin number cannot be empty".to_string(),
            });
        }

        // Check if bin is valid
        match self.db.validate_bin_location(location, bin_no).await {
            Ok(is_valid) => {
                let message = if is_valid { "Bin is valid and available".to_string() } else { format!("Bin '{bin_no}' not found in location '{location}'") };

                Ok(BinValidationResult {
                    bin_no: bin_no.to_string(),
                    location: location.to_string(),
                    is_valid,
                    message,
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Execute bin transfer using validated/corrected quantity
    pub async fn execute_transfer(&self, request: BinTransferRequest) -> Result<TransferResult, PutawayError> {
        // Validate request
        self.validate_transfer_request(&request)?;

        // Validate in database and get corrected transfer quantity for full transfers
        let (actual_transfer_qty, is_full_transfer) = self.db.validate_transfer_request(
            &request.lot_no,
            &request.item_key,
            &request.location,
            &request.bin_from,
            &request.bin_to,
            request.transfer_qty,
        ).await?;

        // Execute transfer with the corrected quantity (exact available qty for full transfers)
        match self.db.execute_bin_transfer_transaction(
            &request.lot_no,
            &request.item_key,
            &request.location,
            &request.bin_from,
            &request.bin_to,
            actual_transfer_qty,
            &request.user_id,
            request.remarks.as_deref().unwrap_or(""),
            request.referenced.as_deref().unwrap_or(""),
        ).await {
            Ok((document_no, source_lot_status, destination_lot_status)) => {
                Ok(TransferResult {
                    success: true,
                    document_no,
                    message: if is_full_transfer {
                        format!(
                            "Successfully transferred {} units (FULL TRANSFER) of lot {} from {} to {} - Source bin cleared",
                            actual_transfer_qty, request.lot_no, request.bin_from, request.bin_to
                        )
                    } else {
                        format!(
                            "Successfully transferred {} units of lot {} from {} to {}",
                            actual_transfer_qty, request.lot_no, request.bin_from, request.bin_to
                        )
                    },
                    timestamp: bangkok_now_rfc3339(),
                    source_lot_status,
                    destination_lot_status,
                })
            }
            Err(e) => {
                Ok(TransferResult {
                    success: false,
                    document_no: String::new(),
                    message: format!("Transfer failed: {e}"),
                    timestamp: bangkok_now_rfc3339(),
                    source_lot_status: None,
                    destination_lot_status: None,
                })
            }
        }
    }


    /// Search for lots with pagination
    pub async fn search_lots_paginated(&self, query: Option<&str>, page: i32, limit: i32) -> Result<(Vec<LotSearchItem>, i32), PutawayError> {
        // Validate inputs
        let safe_page = if page < 1 { 1 } else { page };
        let safe_limit = if limit > 100 { 100 } else if limit < 1 { 20 } else { limit };
        
        // Search in database with pagination
        self.db.search_lots_paginated(query, safe_page, safe_limit).await
    }

    /// Get service health status
    pub async fn get_health(&self) -> PutawayHealthResponse {
        PutawayHealthResponse {
            status: "healthy".to_string(),
            service: "putaway".to_string(),
            timestamp: bangkok_now_rfc3339(),
            version: "1.0.0".to_string(),
        }
    }

    /// Validate transfer request fields
    fn validate_transfer_request(&self, request: &BinTransferRequest) -> Result<(), PutawayError> {
        if request.lot_no.trim().is_empty() {
            return Err(PutawayError::ValidationError("Lot number is required".to_string()));
        }

        if request.item_key.trim().is_empty() {
            return Err(PutawayError::ValidationError("Item key is required".to_string()));
        }

        if request.location.trim().is_empty() {
            return Err(PutawayError::ValidationError("Location is required".to_string()));
        }

        if request.bin_from.trim().is_empty() {
            return Err(PutawayError::ValidationError("Source bin is required".to_string()));
        }

        if request.bin_to.trim().is_empty() {
            return Err(PutawayError::ValidationError("Destination bin is required".to_string()));
        }

        if request.bin_from == request.bin_to {
            return Err(PutawayError::ValidationError("Source and destination bins cannot be the same".to_string()));
        }

        if request.transfer_qty <= 0.0 {
            return Err(PutawayError::ValidationError("Transfer quantity must be greater than zero".to_string()));
        }

        if request.user_id.trim().is_empty() {
            return Err(PutawayError::ValidationError("User ID is required".to_string()));
        }

        Ok(())
    }

    /// Get all active putaway remarks for dropdown
    pub async fn get_active_remarks(&self) -> Result<Vec<serde_json::Value>, PutawayError> {
        self.db.get_active_remarks().await
    }

    /// Search for bins with pagination
    ///
    /// When lot_no, item_key, and location are provided, the search will LEFT JOIN with LotMaster
    /// to show if the bin contains this lot and what status it has (helps users see consolidation targets)
    pub async fn search_bins_paginated(
        &self,
        query: Option<&str>,
        page: i32,
        limit: i32,
        lot_no: Option<&str>,
        item_key: Option<&str>,
        location: Option<&str>,
    ) -> Result<(Vec<BinSearchItem>, i32), PutawayError> {
        // Validate inputs
        let safe_page = if page < 1 { 1 } else { page };
        let safe_limit = if limit > 100 { 100 } else if limit < 1 { 20 } else { limit };

        // Search in database with pagination and lot context
        self.db.search_bins_paginated(query, safe_page, safe_limit, lot_no, item_key, location).await
    }
    /// Search for transactions associated with a lot and bin
    pub async fn search_lot_transactions(&self, lot_no: &str, bin_no: &str) -> Result<Vec<crate::models::putaway_models::LotTransactionItem>, PutawayError> {
        if lot_no.trim().is_empty() {
             return Err(PutawayError::ValidationError("Lot number cannot be empty".to_string()));
        }
        if bin_no.trim().is_empty() {
             return Err(PutawayError::ValidationError("Bin number cannot be empty".to_string()));
        }
        self.db.find_transactions_by_lot_and_bin(lot_no, bin_no).await
    }

    /// Execute transfer of committed items (BME style)
    pub async fn execute_committed_transfer(&self, request: crate::models::putaway_models::CommittedTransferRequest) -> Result<crate::models::putaway_models::CommittedTransferResult, PutawayError> {
        // Validate request
        if request.to_bin.trim().is_empty() {
             return Err(PutawayError::ValidationError("Target bin is required".to_string()));
        }
        if request.user_id.trim().is_empty() {
             return Err(PutawayError::ValidationError("User ID is required".to_string()));
        }
        if request.source_bin.trim().is_empty() {
             return Err(PutawayError::ValidationError("Source bin is required".to_string()));
        }
        if request.transfer_qty <= 0.0 {
             return Err(PutawayError::ValidationError("Transfer quantity must be greater than 0".to_string()));
        }

        // Execute transfer transaction (BME style: creates 2 LotTransaction records)
        match self.db.execute_committed_bin_transfer(
            &request.lot_no,
            &request.item_key,
            &request.location,
            &request.to_bin,
            request.transfer_qty,
            &request.source_bin,
            &request.user_id,
            request.remarks.as_deref().unwrap_or(""),
            request.referenced.as_deref().unwrap_or(""),
        ).await {
            Ok(doc_no) => Ok(crate::models::putaway_models::CommittedTransferResult {
                success: true,
                message: format!("Successfully transferred {} to {} (Doc: {})", request.transfer_qty, request.to_bin, doc_no),
                document_no: doc_no,
                timestamp: bangkok_now_rfc3339(),
            }),
            Err(e) => Ok(crate::models::putaway_models::CommittedTransferResult {
                success: false,
                message: format!("Transfer failed: {e}"),
                document_no: String::new(),
                timestamp: bangkok_now_rfc3339(),
            })
        }
    }
}

