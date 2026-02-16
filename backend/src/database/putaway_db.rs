use crate::database::Database;
use crate::models::putaway_models::{
    map_inclasskey_to_inacct, BinSearchItem, InlocRecord, ItemMasterRecord, LotMasterRecord,
    LotSearchItem, LotTransactionItem, PutawayError,
};
use crate::utils::bangkok_now;
use anyhow::Result;
use chrono::{DateTime, NaiveDateTime, Utc};


pub struct PutawayDatabase {
    db: Database,
}

impl PutawayDatabase {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Search for lot details by lot number
    pub async fn find_lot_by_number(
        &self,
        lot_no: &str,
    ) -> Result<Option<(LotMasterRecord, ItemMasterRecord)>, PutawayError> {
        let mut client = self
            .db
            .get_client()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        let query = r#"
            SELECT
                l.LotNo, l.ItemKey, l.LocationKey, l.BinNo, l.QtyOnHand,
                l.QtyIssued, l.QtyCommitSales, l.DateExpiry, l.VendorKey, l.VendorLotNo,
                l.DocumentNo, l.DocumentLineNo, l.TransactionType, l.LotStatus,
                i.Desc1, i.Desc2, i.Stockuomcode, i.Purchaseuomcode, i.Salesuomcode
            FROM LotMaster l WITH (NOLOCK)
            JOIN INMAST i WITH (NOLOCK) ON l.ItemKey = i.Itemkey
            WHERE l.LotNo = @P1 AND l.QtyOnHand > 0
        "#;

        let result = client
            .query(query, &[&lot_no])
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        if let Some(row) = result
            .into_row()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
        {
            let lot_record = LotMasterRecord {
                lot_no: row.get::<&str, _>("LotNo").unwrap_or("").to_string(),
                item_key: row.get::<&str, _>("ItemKey").unwrap_or("").to_string(),
                location_key: row.get::<&str, _>("LocationKey").unwrap_or("").to_string(),
                bin_no: row.get::<&str, _>("BinNo").unwrap_or("").to_string(),
                qty_on_hand: row.get::<f64, _>("QtyOnHand").unwrap_or(0.0),
                qty_issued: row.get::<f64, _>("QtyIssued").unwrap_or(0.0),
                qty_commit_sales: row.get::<f64, _>("QtyCommitSales").unwrap_or(0.0),
                date_expiry: DateTime::from_naive_utc_and_offset(
                    row.get::<NaiveDateTime, _>("DateExpiry")
                        .unwrap_or_default(),
                    Utc,
                ),
                vendor_key: row.get::<&str, _>("VendorKey").unwrap_or("").to_string(),
                vendor_lot_no: row.get::<&str, _>("VendorLotNo").unwrap_or("").to_string(),
                document_no: row.get::<&str, _>("DocumentNo").unwrap_or("").to_string(),
                document_line_no: row.get::<i16, _>("DocumentLineNo").unwrap_or(0),
                transaction_type: row.get::<u8, _>("TransactionType").unwrap_or(0),
                lot_status: row.get::<&str, _>("LotStatus").unwrap_or("").to_string(),
            };

            let item_record = ItemMasterRecord {
                item_key: row.get::<&str, _>("ItemKey").unwrap_or("").to_string(),
                desc1: row.get::<&str, _>("Desc1").unwrap_or("").to_string(),
                desc2: row.get::<&str, _>("Desc2").unwrap_or("").to_string(),
                stock_uom_code: row.get::<&str, _>("Stockuomcode").unwrap_or("").to_string(),
                purchase_uom_code: row
                    .get::<&str, _>("Purchaseuomcode")
                    .unwrap_or("")
                    .to_string(),
                sales_uom_code: row.get::<&str, _>("Salesuomcode").unwrap_or("").to_string(),
            };

            Ok(Some((lot_record, item_record)))
        } else {
            Ok(None)
        }
    }

    /// Validate if a bin exists and is valid for the location
    pub async fn validate_bin_location(
        &self,
        location: &str,
        bin_no: &str,
    ) -> Result<bool, PutawayError> {
        let mut client = self
            .db
            .get_client()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        // Check if bin exists in BINMaster table
        let query = r#"
            SELECT COUNT(*) as count
            FROM BINMaster WITH (NOLOCK)
            WHERE Location = @P1 AND BinNo = @P2
        "#;

        let result = client
            .query(query, &[&location, &bin_no])
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        if let Some(row) = result
            .into_row()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
        {
            let count: i32 = row.get("count").unwrap_or(0);
            Ok(count > 0)
        } else {
            Ok(false)
        }
    }

    /// Get next sequence number for BT documents
    /// This method handles atomic sequence increment with proper transaction safety
    pub async fn get_next_bt_sequence(&self) -> Result<i32, PutawayError> {
        let mut client = self
            .db
            .get_client()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        // Use atomic increment with OUTPUT to get the new sequence number
        let query = r#"
            UPDATE Seqnum 
            SET SeqNum = SeqNum + 1 
            OUTPUT INSERTED.SeqNum 
            WHERE SeqName = 'BT'
        "#;

        let result = client.query(query, &[]).await.map_err(|e| {
            PutawayError::TransactionError(format!("Failed to increment BT sequence: {e}"))
        })?;

        if let Some(row) = result
            .into_row()
            .await
            .map_err(|e| PutawayError::TransactionError(e.to_string()))?
        {
            let next_seq: i32 = row.get("SeqNum").unwrap_or(0);
            Ok(next_seq)
        } else {
            Err(PutawayError::DatabaseError(
                "BT sequence not found or update failed".to_string(),
            ))
        }
    }

    /// Get INLOC record for GL account mapping
    pub async fn get_inloc_record(
        &self,
        item_key: &str,
        location: &str,
    ) -> Result<InlocRecord, PutawayError> {
        let mut client = self
            .db
            .get_client()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        let query = r#"
            SELECT ItemKey, Location, Inclasskey, Revacct, Cogsacct, Stdcost
            FROM INLOC WITH (NOLOCK)
            WHERE ItemKey = @P1 AND Location = @P2
        "#;

        let result = client
            .query(query, &[&item_key, &location])
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        if let Some(row) = result
            .into_row()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
        {
            Ok(InlocRecord {
                item_key: row.get::<&str, _>("ItemKey").unwrap_or("").to_string(),
                location: row.get::<&str, _>("Location").unwrap_or("").to_string(),
                inclasskey: row.get::<&str, _>("Inclasskey").unwrap_or("").to_string(),
                revacct: row.get::<&str, _>("Revacct").unwrap_or("").to_string(),
                cogsacct: row.get::<&str, _>("Cogsacct").unwrap_or("").to_string(),
                stdcost: {
                    // Handle SQL Server NUMERIC type conversion
                    // SQL Server NUMERIC types need special handling in tiberius
                    use tiberius::numeric::Numeric;

                    if let Ok(Some(numeric_val)) = row.try_get::<Numeric, _>("Stdcost") {
                        // Convert Numeric to f64
                        numeric_val.value() as f64 / 10_f64.powi(numeric_val.scale() as i32)
                    } else {
                        // Fallback: try as string
                        match row.try_get::<&str, _>("Stdcost") {
                            Ok(Some(val_str)) => val_str.parse::<f64>().unwrap_or(0.0),
                            _ => 0.0,
                        }
                    }
                },
            })
        } else {
            Err(PutawayError::DatabaseError(format!(
                "INLOC record not found for item {item_key} in location {location}"
            )))
        }
    }

    /// Execute complete bin transfer transaction with lot consolidation
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_bin_transfer_transaction(
        &self,
        lot_no: &str,
        item_key: &str,
        location: &str,
        bin_from: &str,
        bin_to: &str,
        transfer_qty: f64,
        user_id: &str,
        remarks: &str,
        referenced: &str,
    ) -> Result<(String, Option<String>, Option<String>), PutawayError> {
        // Get database client (TFCPILOT3 primary)
        let mut client = self
            .db
            .get_client()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        // **🔒 BEGIN TRANSACTION** - Ensure atomic 6-step putaway operation
        // Set REPEATABLE READ isolation level for stronger consistency
        client
            .simple_query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .await
            .map_err(|e| PutawayError::DatabaseError(format!("Failed to set isolation level: {e}")))?;

        client
            .simple_query("BEGIN TRANSACTION")
            .await
            .map_err(|e| PutawayError::DatabaseError(format!("Failed to begin transaction: {e}")))?;

        // Execute all 6 steps in a transaction
        let transaction_result: Result<String, PutawayError> = async {
        // 1. Get next BT document number
        let bt_number = self.get_next_bt_sequence().await?;
        let document_no = format!("BT-{bt_number:08}");
        let now = bangkok_now().naive_local();

        // Truncate user ID to 8 characters for database field compatibility
        let user_id_truncated = if user_id.len() > 8 {
            &user_id[0..8]
        } else {
            user_id
        };

        // **🔒 STEP 1.5: LOCK LOTMASTER FIRST** - Prevent deadlocks by acquiring locks in global order
        // Lock BOTH source and destination bins in alphabetical order to prevent circular waits
        let lock_lots_query = r#"
            SELECT LotNo, ItemKey, LocationKey, BinNo, QtyOnHand, QtyCommitSales
            FROM LotMaster WITH (UPDLOCK, ROWLOCK)
            WHERE (LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4)
               OR (LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P5)
            ORDER BY BinNo ASC
        "#;

        let locked_lots = client.query(lock_lots_query, &[&lot_no, &item_key, &location, &bin_from, &bin_to])
            .await
            .map_err(|e| PutawayError::DatabaseError(format!("Failed to lock LotMaster records: {e}")))?
            .into_results()
            .await
            .map_err(|e| PutawayError::DatabaseError(format!("Failed to get locked lots: {e}")))?;

        if locked_lots.is_empty() || locked_lots[0].is_empty() {
            return Err(PutawayError::ValidationError("Source bin not found for locking".to_string()));
        }

        // 2. Create Mintxdh record for audit trail
        let inloc_record = self.get_inloc_record(item_key, location).await?;
        let in_acct = map_inclasskey_to_inacct(&inloc_record.inclasskey);
        let std_cost = inloc_record.stdcost;
        let trn_desc = "Bin Transfer";

        let mintxdh_query = r#"
            INSERT INTO Mintxdh (
                ItemKey, Location, ToLocation, SysID, ProcessID, SysDocID, SysLinSq,
                TrnTyp, TrnSubTyp, DocNo, DocDate, AplDate, TrnDesc, TrnQty, TrnAmt,
                NLAcct, INAcct, CreatedSerlot, RecUserID, RecDate, Updated_FinTable,
                SortField, JrnlBtchNo, StdCost, Stdcostupdated, GLtrnAmt
            ) VALUES (
                @P1, @P2, '', '7', 'M', @P3, 1, 'A', '', @P4, @P5, @P5, @P6, 0, 0.000000,
                '1100', @P7, 'Y', @P8, @P9, 0, '', '', @P10, 0, 0.000000
            )
        "#;

        client
            .execute(
                mintxdh_query,
                &[
                    &item_key,
                    &location,
                    &document_no,
                    &document_no,
                    &now,
                    &trn_desc,
                    &in_acct,
                    &user_id_truncated,
                    &now,
                    &std_cost,
                ],
            )
            .await
            .map_err(|e| {
                PutawayError::TransactionError(format!("Failed to create Mintxdh record: {e}"))
            })?;

        // Get current source bin quantity for QtyOnHand field in BinTransfer
        let source_qty_result = client.query(
            "SELECT QtyOnHand FROM LotMaster WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4",
            &[&lot_no, &item_key, &location, &bin_from]
        ).await.map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        let source_qty_on_hand: f64 = if let Some(row) = source_qty_result
            .into_row()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
        {
            row.get("QtyOnHand").unwrap_or(0.0)
        } else {
            return Err(PutawayError::ValidationError(
                "Source bin record not found".to_string(),
            ));
        };

        // 3. Create Issue Transaction (Type 9 - Remove from source bin)
        let issue_transaction_query = r#"
            INSERT INTO LotTransaction (
                LotNo, ItemKey, LocationKey, TransactionType, 
                IssueDocNo, IssueDocLineNo, IssueDate, QtyIssued,
                BinNo, RecUserid, RecDate, Processed,
                DateReceived, DateExpiry, Vendorkey, VendorlotNo,
                CustomerKey, TempQty, QtyForLotAssignment, QtyUsed
            ) OUTPUT INSERTED.LotTranNo
            VALUES (@P1, @P2, @P3, 9, @P4, 1, @P5, @P6, @P7, @P8, @P9, 'Y',
                    @P10, @P11, @P12, @P13, '', 0, 0, 0)
        "#;

        // Get lot details including vendor info for transaction
        let lot_details_result = client
            .query("SELECT DateReceived, DateExpiry, VendorKey, VendorLotNo FROM LotMaster WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4",
                   &[&lot_no, &item_key, &location, &bin_from]).await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        let (date_received, date_expiry, vendor_key, vendor_lot_no) = if let Some(row) =
            lot_details_result
                .into_row()
                .await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
        {
            (
                row.get::<NaiveDateTime, _>("DateReceived")
                    .unwrap_or(now),
                row.get::<NaiveDateTime, _>("DateExpiry")
                    .unwrap_or(now),
                row.get::<&str, _>("VendorKey").unwrap_or("").to_string(),
                row.get::<&str, _>("VendorLotNo").unwrap_or("").to_string(),
            )
        } else {
            return Err(PutawayError::ValidationError(
                "Cannot get lot details for transaction".to_string(),
            ));
        };

        let issue_result = client
            .query(
                issue_transaction_query,
                &[
                    &lot_no,
                    &item_key,
                    &location,
                    &document_no,
                    &now,
                    &transfer_qty,
                    &bin_from,
                    &user_id_truncated,
                    &now,
                    &date_received,
                    &date_expiry,
                    &vendor_key,
                    &vendor_lot_no,
                ],
            )
            .await
            .map_err(|e| {
                PutawayError::TransactionError(format!("Failed to create issue transaction: {e}"))
            })?;

        let issue_lot_tran_no: i32 = if let Some(row) = issue_result
            .into_row()
            .await
            .map_err(|e| PutawayError::TransactionError(e.to_string()))?
        {
            row.get("LotTranNo").unwrap_or(0)
        } else {
            return Err(PutawayError::TransactionError(
                "Failed to get issue LotTranNo".to_string(),
            ));
        };

        // 4. Create Receipt Transaction (Type 8 - Add to destination bin)
        let receipt_transaction_query = r#"
            INSERT INTO LotTransaction (
                LotNo, ItemKey, LocationKey, TransactionType,
                ReceiptDocNo, ReceiptDocLineNo, QtyReceived,
                BinNo, RecUserid, RecDate, Processed,
                DateReceived, DateExpiry, Vendorkey, VendorlotNo,
                CustomerKey, TempQty, QtyForLotAssignment, QtyUsed
            ) VALUES (@P1, @P2, @P3, 8, @P4, 1, @P5, @P6, @P7, @P8, 'Y',
                     @P9, @P10, @P11, @P12, '', 0, 0, 0)
        "#;

        client
            .execute(
                receipt_transaction_query,
                &[
                    &lot_no,
                    &item_key,
                    &location,
                    &document_no,
                    &transfer_qty,
                    &bin_to,
                    &user_id_truncated,
                    &now,
                    &date_received,
                    &date_expiry,
                    &vendor_key,
                    &vendor_lot_no,
                ],
            )
            .await
            .map_err(|e| {
                PutawayError::TransactionError(format!(
                    "Failed to create receipt transaction: {e}"
                ))
            })?;

        // 5. Create BinTransfer record (with issue LotTranNo reference)
        let bin_transfer_query = r#"
            INSERT INTO BinTransfer (
                ItemKey, Location, LotNo, BinNoFrom, BinNoTo, 
                LotTranNo, QtyOnHand, TransferQty, InTransID, 
                RecUserID, RecDate, ContainerNo, User1, User5
            ) VALUES (@P1, @P2, @P3, @P4, @P5, @P6, @P7, @P8, 0, @P9, @P10, '0', @P11, @P12)
        "#;

        // Use truncated user_id for BinTransfer.RecUserID field

        client
            .execute(
                bin_transfer_query,
                &[
                    &item_key,
                    &location,
                    &lot_no,
                    &bin_from,
                    &bin_to,
                    &issue_lot_tran_no,
                    &source_qty_on_hand,
                    &transfer_qty,
                    &user_id_truncated,
                    &now,
                    &remarks,
                    &referenced,
                ],
            )
            .await
            .map_err(|e| {
                PutawayError::TransactionError(format!(
                    "Failed to create bin transfer record: {e}"
                ))
            })?;

        // 6. Handle LotMaster lot consolidation logic
        self.handle_lot_consolidation(
            &mut client,
            lot_no,
            item_key,
            location,
            bin_from,
            bin_to,
            transfer_qty,
            &document_no,
            user_id,
            &now,
        )
        .await?;

        Ok(document_no)
        }.await;

        // **🔒 COMMIT or ROLLBACK** - Atomic transaction handling
        match transaction_result {
            Ok(doc_no) => {
                // All 6 steps succeeded - commit the transaction
                client
                    .simple_query("COMMIT")
                    .await
                    .map_err(|e| PutawayError::DatabaseError(format!("Failed to commit transaction: {e}")))?;

                // Query source lot status (from original bin, may have been deleted if full transfer)
                let source_status = self.get_lot_status(&mut client, lot_no, item_key, location, bin_from).await;

                // Query destination lot status (should exist after transfer)
                let dest_status = self.get_lot_status(&mut client, lot_no, item_key, location, bin_to).await;

                Ok((doc_no, source_status, dest_status))
            }
            Err(e) => {
                // Any step failed - rollback the entire transaction
                let _ = client.simple_query("ROLLBACK").await;
                Err(e)
            }
        }
    }

    /// Get lot status from LotMaster for a specific bin
    async fn get_lot_status(
        &self,
        client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
        lot_no: &str,
        item_key: &str,
        location: &str,
        bin_no: &str,
    ) -> Option<String> {
        let query = "SELECT LotStatus FROM LotMaster WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4";

        match client.query(query, &[&lot_no, &item_key, &location, &bin_no]).await {
            Ok(stream) => {
                match stream.into_row().await {
                    Ok(Some(row)) => row.get::<&str, _>("LotStatus").map(|s| s.to_string()),
                    _ => None,
                }
            }
            Err(_) => None,
        }
    }

    /// Handle lot consolidation logic for LotMaster records
    #[allow(clippy::too_many_arguments)]
    async fn handle_lot_consolidation(
        &self,
        client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
        lot_no: &str,
        item_key: &str,
        location: &str,
        bin_from: &str,
        bin_to: &str,
        transfer_qty: f64,
        document_no: &str,
        user_id: &str,
        now: &NaiveDateTime,
    ) -> Result<(), PutawayError> {
        // Truncate user ID to 8 characters for database field compatibility
        let user_id_truncated = if user_id.len() > 8 {
            &user_id[0..8]
        } else {
            user_id
        };

        // **Step 0: Query source lot details FIRST (before potential deletion)**
        // CRITICAL: This must happen before Step 1 because full transfers delete the source record.
        // If we query after deletion, we'll lose lot details and the destination INSERT will fail,
        // causing lots to disappear from the database entirely.
        let source_details_result = client.query(
            "SELECT DateReceived, DateExpiry, VendorKey, VendorLotNo, QtyCommitSales, LotStatus FROM LotMaster WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4",
            &[&lot_no, &item_key, &location, &bin_from]
        ).await.map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        let source_details = source_details_result
            .into_row()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
            .ok_or_else(|| PutawayError::ValidationError("Source lot not found before transfer".to_string()))?;

        // Extract and save all source details for later use (needed for destination INSERT)
        let date_received: NaiveDateTime = source_details.get("DateReceived").unwrap_or(*now);
        let date_expiry: NaiveDateTime = source_details.get("DateExpiry").unwrap_or(*now);
        let vendor_key: String = source_details.get::<&str, _>("VendorKey").unwrap_or("").to_string();
        let vendor_lot_no: String = source_details.get::<&str, _>("VendorLotNo").unwrap_or("").to_string();
        let lot_status: Option<String> = source_details.get::<&str, _>("LotStatus").map(|s| s.to_string());

        // Step 1: Update source bin - reduce QtyOnHand or delete if becomes 0
        let source_update_result = client.query(
            "SELECT QtyOnHand FROM LotMaster WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4",
            &[&lot_no, &item_key, &location, &bin_from]
        ).await.map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        if let Some(row) = source_update_result
            .into_row()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
        {
            let current_qty: f64 = row.get("QtyOnHand").unwrap_or(0.0);
            let remaining_qty = current_qty - transfer_qty;

            if remaining_qty <= 0.0 {
                // Delete source record if quantity becomes 0
                client.execute(
                    "DELETE FROM LotMaster WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4",
                    &[&lot_no, &item_key, &location, &bin_from]
                ).await.map_err(|e| PutawayError::TransactionError(format!("Failed to delete source record: {e}")))?;
            } else {
                // Update source bin with reduced quantity
                client.execute(
                    "UPDATE LotMaster SET QtyOnHand = @P1, DocumentNo = @P2, TransactionType = 9, RecUserId = @P3, Recdate = @P4 WHERE LotNo = @P5 AND ItemKey = @P6 AND LocationKey = @P7 AND BinNo = @P8",
                    &[&remaining_qty, &document_no, &user_id_truncated, now, &lot_no, &item_key, &location, &bin_from]
                ).await.map_err(|e| PutawayError::TransactionError(format!("Failed to update source bin: {e}")))?;
            }
        }

        // Step 2: Handle destination bin - add to existing or create new record
        let dest_check_result = client.query(
            "SELECT QtyOnHand, QtyCommitSales, DateReceived, DateExpiry, VendorKey, VendorLotNo FROM LotMaster WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4",
            &[&lot_no, &item_key, &location, &bin_to]
        ).await.map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        if let Some(row) = dest_check_result
            .into_row()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
        {
            // Destination bin already has this lot - add quantities (lot consolidation)
            let current_qty: f64 = row.get("QtyOnHand").unwrap_or(0.0);
            let new_qty = current_qty + transfer_qty;

            client.execute(
                "UPDATE LotMaster SET QtyOnHand = @P1, DocumentNo = @P2, TransactionType = 8, RecUserId = @P3, Recdate = @P4 WHERE LotNo = @P5 AND ItemKey = @P6 AND LocationKey = @P7 AND BinNo = @P8",
                &[&new_qty, &document_no, &user_id_truncated, now, &lot_no, &item_key, &location, &bin_to]
            ).await.map_err(|e| PutawayError::TransactionError(format!("Failed to update destination bin: {e}")))?;
        } else {
            // Destination bin doesn't have this lot - create new record
            // Use source lot details saved in Step 0 (before potential deletion)
            let insert_query = r#"
                INSERT INTO LotMaster (
                    LotNo, ItemKey, LocationKey, DateReceived, DateExpiry,
                    QtyReceived, QtyIssued, QtyCommitSales, QtyOnHand,
                    DocumentNo, DocumentLineNo, TransactionType, VendorKey, VendorLotNo,
                    QtyOnOrder, RecUserId, Recdate, BinNo, LotStatus
                ) VALUES (
                    @P1, @P2, @P3, @P4, @P5, @P6, 0, 0, @P6, @P7, 1, 8, @P8, @P9,
                    0, @P10, @P11, @P12, @P13
                )
            "#;

            client
                .execute(
                    insert_query,
                    &[
                        &lot_no,
                        &item_key,
                        &location,
                        &date_received,
                        &date_expiry,
                        &transfer_qty,
                        &document_no,
                        &vendor_key,
                        &vendor_lot_no,
                        &user_id_truncated,
                        now,
                        &bin_to,
                        &lot_status,
                    ],
                )
                .await
                .map_err(|e| {
                    PutawayError::TransactionError(format!(
                        "Failed to create destination record: {e}"
                    ))
                })?;
        }

        Ok(())
    }

    /// Validate transfer request - checks specific bin quantities for lot consolidation
    pub async fn validate_transfer_request(
        &self,
        lot_no: &str,
        item_key: &str,
        location: &str,
        bin_from: &str,
        bin_to: &str,
        transfer_qty: f64,
    ) -> Result<(f64, bool), PutawayError> {
        let mut client = self
            .db
            .get_client()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        // Check specific bin record (not general lot record)
        let source_bin_query = r#"
            SELECT QtyOnHand, QtyCommitSales, ItemKey, LocationKey
            FROM LotMaster 
            WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4
        "#;

        let result = client
            .query(
                source_bin_query,
                &[&lot_no, &item_key, &location, &bin_from],
            )
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        if let Some(row) = result
            .into_row()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
        {
            // Verify item key matches
            let record_item_key: &str = row.get("ItemKey").unwrap_or("");
            if record_item_key != item_key {
                return Err(PutawayError::ValidationError(format!(
                    "Item key mismatch: expected {record_item_key}, got {item_key}"
                )));
            }

            // Verify location matches
            let record_location: &str = row.get("LocationKey").unwrap_or("");
            if record_location != location {
                return Err(PutawayError::ValidationError(format!(
                    "Location mismatch: expected {record_location}, got {location}"
                )));
            }

            // Calculate available quantity in THIS SPECIFIC BIN (QtyOnHand - QtyCommitSales)
            let qty_on_hand: f64 = row.get("QtyOnHand").unwrap_or(0.0);
            let qty_commit_sales: f64 = row.get("QtyCommitSales").unwrap_or(0.0);
            let available_qty = qty_on_hand - qty_commit_sales;

            // Add tolerance for floating-point precision errors (0.001 = 1 milligram tolerance)
            // This prevents false validation errors from JavaScript decimal precision issues
            const QUANTITY_TOLERANCE: f64 = 0.001;

            if transfer_qty > (available_qty + QUANTITY_TOLERANCE) {
                return Err(PutawayError::InsufficientQuantity {
                    requested: transfer_qty,
                    available: available_qty,
                });
            }

            if transfer_qty <= 0.0 {
                return Err(PutawayError::ValidationError(
                    "Transfer quantity must be greater than 0".to_string(),
                ));
            }

            // Detect full transfer: when requested quantity is within tolerance of available quantity
            // This prevents microscopic residuals that block source record deletion
            let is_full_transfer = (transfer_qty + QUANTITY_TOLERANCE) >= available_qty;
            let actual_transfer_qty = if is_full_transfer {
                // Use exact available quantity for full transfers to prevent floating-point residuals
                available_qty
            } else {
                transfer_qty
            };

            // Validate destination bin exists
            if !self.validate_bin_location(location, bin_to).await? {
                return Err(PutawayError::InvalidBin {
                    bin_no: bin_to.to_string(),
                    location: location.to_string(),
                });
            }

            // Validate source and destination bins are different
            if bin_from == bin_to {
                return Err(PutawayError::ValidationError(
                    "Source and destination bins cannot be the same".to_string(),
                ));
            }

            // Return actual transfer quantity and full transfer flag
            Ok((actual_transfer_qty, is_full_transfer))
        } else {
            Err(PutawayError::ValidationError(format!(
                "Lot {lot_no} not found in bin {bin_from} or insufficient quantity available"
            )))
        }
    }


    /// Search for lots with pagination (READ operation - uses TFCPILOT3)
    pub async fn search_lots_paginated(
        &self,
        query: Option<&str>,
        page: i32,
        limit: i32,
    ) -> Result<(Vec<LotSearchItem>, i32), PutawayError> {
        let mut client = self
            .db
            .get_client()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        let offset = (page - 1) * limit;

        // First, get total count
        let count_query = if let Some(_search_term) = query {
            r#"
                SELECT COUNT(*) as total_count
                FROM LotMaster l WITH (NOLOCK)
                JOIN INMAST i WITH (NOLOCK) ON l.ItemKey = i.Itemkey
                WHERE l.QtyOnHand > 0
                AND (l.LotNo LIKE @P1 OR i.Desc1 LIKE @P1 OR l.ItemKey LIKE @P1 OR l.BinNo LIKE @P1)
            "#
        } else {
            r#"
                SELECT COUNT(*) as total_count
                FROM LotMaster l WITH (NOLOCK)
                WHERE l.QtyOnHand > 0
            "#
        };

        let total_count = if let Some(search_term) = query {
            let search_pattern = format!("%{search_term}%");
            let count_result = client
                .query(count_query, &[&search_pattern])
                .await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;
            if let Some(row) = count_result
                .into_row()
                .await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
            {
                row.get::<i32, _>("total_count").unwrap_or(0)
            } else {
                0
            }
        } else {
            let count_result = client
                .query(count_query, &[])
                .await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;
            if let Some(row) = count_result
                .into_row()
                .await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
            {
                row.get::<i32, _>("total_count").unwrap_or(0)
            } else {
                0
            }
        };

        // Then get paginated results
        let sql_query = if let Some(_search_term) = query {
            r#"
                SELECT
                    l.LotNo, l.ItemKey, l.LocationKey, l.BinNo, l.QtyOnHand,
                    l.QtyCommitSales, l.DateReceived, l.DateExpiry, l.LotStatus,
                    i.Desc1, i.Stockuomcode
                FROM LotMaster l WITH (NOLOCK)
                JOIN INMAST i WITH (NOLOCK) ON l.ItemKey = i.Itemkey
                WHERE l.QtyOnHand > 0
                AND (l.LotNo LIKE @P1 OR i.Desc1 LIKE @P1 OR l.ItemKey LIKE @P1 OR l.BinNo LIKE @P1)
                ORDER BY l.LotNo
                OFFSET @P2 ROWS FETCH NEXT @P3 ROWS ONLY
            "#
        } else {
            r#"
                SELECT
                    l.LotNo, l.ItemKey, l.LocationKey, l.BinNo, l.QtyOnHand,
                    l.QtyCommitSales, l.DateReceived, l.DateExpiry, l.LotStatus,
                    i.Desc1, i.Stockuomcode
                FROM LotMaster l WITH (NOLOCK)
                JOIN INMAST i WITH (NOLOCK) ON l.ItemKey = i.Itemkey
                WHERE l.QtyOnHand > 0
                ORDER BY l.LotNo DESC
                OFFSET @P1 ROWS FETCH NEXT @P2 ROWS ONLY
            "#
        };

        let results = if let Some(search_term) = query {
            let search_pattern = format!("%{search_term}%");
            client
                .query(sql_query, &[&search_pattern, &offset, &limit])
                .await
        } else {
            client.query(sql_query, &[&offset, &limit]).await
        };

        match results {
            Ok(stream) => {
                let mut lots = Vec::new();
                let rows = stream
                    .into_first_result()
                    .await
                    .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

                for row in rows {
                    let qty_on_hand: f64 = row.get("QtyOnHand").unwrap_or(0.0);
                    let qty_commit_sales: f64 = row.get("QtyCommitSales").unwrap_or(0.0);
                    let qty_available = qty_on_hand - qty_commit_sales;

                    let expiry_date = row
                        .get::<NaiveDateTime, _>("DateExpiry")
                        .map(|dt| dt.format("%Y-%m-%d").to_string());

                    let date_received = row
                        .get::<NaiveDateTime, _>("DateReceived")
                        .map(|dt| dt.format("%Y-%m-%d").to_string());

                    lots.push(LotSearchItem {
                        lot_no: row.get::<&str, _>("LotNo").unwrap_or("").to_string(),
                        item_key: row.get::<&str, _>("ItemKey").unwrap_or("").to_string(),
                        item_description: row.get::<&str, _>("Desc1").unwrap_or("").to_string(),
                        location: row.get::<&str, _>("LocationKey").unwrap_or("").to_string(),
                        current_bin: row.get::<&str, _>("BinNo").unwrap_or("").to_string(),
                        qty_on_hand,
                        qty_commit_sales,
                        qty_available,
                        date_received,
                        expiry_date,
                        uom: row.get::<&str, _>("Stockuomcode").unwrap_or("").to_string(),
                        lot_status: row.get::<&str, _>("LotStatus").unwrap_or("").to_string(),
                    });
                }

                Ok((lots, total_count))
            }
            Err(e) => Err(PutawayError::DatabaseError(e.to_string())),
        }
    }

    /// Search for bins with optional query filter and pagination (READ operation - uses TFCPILOT3)
    ///
    /// When lot_no, item_key, and location are provided, LEFT JOIN with LotMaster to show
    /// if the bin contains this lot and what status it has (helps users see consolidation targets)
    pub async fn search_bins_paginated(
        &self,
        query: Option<&str>,
        page: i32,
        limit: i32,
        lot_no: Option<&str>,
        item_key: Option<&str>,
        location: Option<&str>,
    ) -> Result<(Vec<BinSearchItem>, i32), PutawayError> {
        let mut client = self
            .db
            .get_client()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        let offset = (page - 1) * limit;

        // Determine if we have lot context for LEFT JOIN
        let has_lot_context = lot_no.is_some() && item_key.is_some() && location.is_some();

        // First, get total count (count doesn't need lot join, just bin count)
        let count_query = if let Some(_search_term) = query {
            r#"
                SELECT COUNT(*) as total_count
                FROM BINMaster WITH (NOLOCK)
                WHERE BinNo LIKE @P1 OR Location LIKE @P1 OR Description LIKE @P1
            "#
        } else {
            r#"
                SELECT COUNT(*) as total_count
                FROM BINMaster WITH (NOLOCK)
            "#
        };

        let total_count = if let Some(search_term) = query {
            let search_pattern = format!("%{search_term}%");
            let count_result = client
                .query(count_query, &[&search_pattern])
                .await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;
            if let Some(row) = count_result
                .into_row()
                .await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
            {
                row.get::<i32, _>("total_count").unwrap_or(0)
            } else {
                0
            }
        } else {
            let count_result = client
                .query(count_query, &[])
                .await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;
            if let Some(row) = count_result
                .into_row()
                .await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
            {
                row.get::<i32, _>("total_count").unwrap_or(0)
            } else {
                0
            }
        };

        // Then get paginated results - conditionally LEFT JOIN with LotMaster if lot context provided
        let sql_query = match (has_lot_context, query.is_some()) {
            // Case 1: Has lot context AND search query
            (true, true) => {
                r#"
                    SELECT
                        b.Location, b.BinNo, b.Description, b.aisle, b.row, b.rack, b.RecDate,
                        l.LotStatus
                    FROM BINMaster b WITH (NOLOCK)
                    LEFT JOIN LotMaster l WITH (NOLOCK) ON
                        l.LotNo = @P1 AND
                        l.ItemKey = @P2 AND
                        l.LocationKey = @P3 AND
                        l.BinNo = b.BinNo
                    WHERE b.BinNo LIKE @P4 OR b.Location LIKE @P4 OR b.Description LIKE @P4
                    ORDER BY b.RecDate DESC
                    OFFSET @P5 ROWS FETCH NEXT @P6 ROWS ONLY
                "#
            },
            // Case 2: Has lot context but NO search query
            (true, false) => {
                r#"
                    SELECT
                        b.Location, b.BinNo, b.Description, b.aisle, b.row, b.rack, b.RecDate,
                        l.LotStatus
                    FROM BINMaster b WITH (NOLOCK)
                    LEFT JOIN LotMaster l WITH (NOLOCK) ON
                        l.LotNo = @P1 AND
                        l.ItemKey = @P2 AND
                        l.LocationKey = @P3 AND
                        l.BinNo = b.BinNo
                    ORDER BY b.RecDate DESC
                    OFFSET @P4 ROWS FETCH NEXT @P5 ROWS ONLY
                "#
            },
            // Case 3: No lot context but HAS search query
            (false, true) => {
                r#"
                    SELECT
                        Location, BinNo, Description, aisle, row, rack, RecDate
                    FROM BINMaster WITH (NOLOCK)
                    WHERE BinNo LIKE @P1 OR Location LIKE @P1 OR Description LIKE @P1
                    ORDER BY RecDate DESC
                    OFFSET @P2 ROWS FETCH NEXT @P3 ROWS ONLY
                "#
            },
            // Case 4: No lot context and NO search query
            (false, false) => {
                r#"
                    SELECT
                        Location, BinNo, Description, aisle, row, rack, RecDate
                    FROM BINMaster WITH (NOLOCK)
                    ORDER BY RecDate DESC
                    OFFSET @P1 ROWS FETCH NEXT @P2 ROWS ONLY
                "#
            },
        };

        // Execute query with appropriate parameters based on lot context and search query
        let results = match (has_lot_context, query) {
            // Case 1: Has lot context AND search query
            (true, Some(search_term)) => {
                let search_pattern = format!("%{search_term}%");
                let lot_no_param = lot_no.unwrap(); // Safe because has_lot_context is true
                let item_key_param = item_key.unwrap();
                let location_param = location.unwrap();
                client.query(
                    sql_query,
                    &[&lot_no_param, &item_key_param, &location_param, &search_pattern, &offset, &limit]
                ).await
            },
            // Case 2: Has lot context but NO search query
            (true, None) => {
                let lot_no_param = lot_no.unwrap(); // Safe because has_lot_context is true
                let item_key_param = item_key.unwrap();
                let location_param = location.unwrap();
                client.query(
                    sql_query,
                    &[&lot_no_param, &item_key_param, &location_param, &offset, &limit]
                ).await
            },
            // Case 3: No lot context but HAS search query
            (false, Some(search_term)) => {
                let search_pattern = format!("%{search_term}%");
                client.query(
                    sql_query,
                    &[&search_pattern, &offset, &limit]
                ).await
            },
            // Case 4: No lot context and NO search query
            (false, None) => {
                client.query(
                    sql_query,
                    &[&offset, &limit]
                ).await
            },
        };

        match results {
            Ok(stream) => {
                let mut bins = Vec::new();
                let rows = stream
                    .into_first_result()
                    .await
                    .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

                for row in rows {
                    // Get lot status if available (only present when lot context was provided)
                    let lot_status = if has_lot_context {
                        row.get::<&str, _>("LotStatus").map(|s| s.to_string())
                    } else {
                        None
                    };

                    bins.push(BinSearchItem {
                        bin_no: row.get::<&str, _>("BinNo").unwrap_or("").to_string(),
                        location: row.get::<&str, _>("Location").unwrap_or("").to_string(),
                        description: row.get::<&str, _>("Description").unwrap_or("").to_string(),
                        aisle: row.get::<&str, _>("aisle").unwrap_or("").to_string(),
                        row: row.get::<&str, _>("row").unwrap_or("").to_string(),
                        rack: row.get::<&str, _>("rack").unwrap_or("").to_string(),
                        lot_status,
                    });
                }

                Ok((bins, total_count))
            }
            Err(e) => Err(PutawayError::DatabaseError(e.to_string())),
        }
    }

    /// Get all active putaway remarks for dropdown
    pub async fn get_active_remarks(&self) -> Result<Vec<serde_json::Value>, PutawayError> {
        let mut client = self
            .db
            .get_client()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        let query = r#"
            SELECT id, remark_name
            FROM dbo.putawaylist WITH (NOLOCK)
            WHERE is_active = 1
            ORDER BY id
        "#;

        let result = client
            .query(query, &[])
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        let rows = result
            .into_first_result()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        let remarks: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.get::<i32, _>("id").unwrap_or(0),
                    "remark_name": row.get::<&str, _>("remark_name").unwrap_or("")
                })
            })
            .collect();

        Ok(remarks)
    }
    /// Search for transactions associated with a lot and bin
    pub async fn find_transactions_by_lot_and_bin(
        &self,
        lot_no: &str,
        bin_no: &str,
    ) -> Result<Vec<LotTransactionItem>, PutawayError> {
        let mut client = self
            .db
            .get_client()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        // Official BME query: LotTransaction UNION QCLotTransaction
        // Filters: Processed IN ('N','P'), TransactionType In (2,3,5,7,9,10,12,16,17,20,21)
        // Returns QtyIssued, IssueDocNo with human-readable transaction type names
        let query = r#"
            SELECT LotNo, BinNo, IssueDocNo as DocNo, IssueDocLineNo, QtyIssued as Qty, LotTranNo, 
                   TransactionType,
                   (CASE TransactionType  
                      When 1  Then 'Purchase Receipt' 
                      When 2  Then 'Purchase Return' 
                      When 3  Then 'Sales Issue' 
                      When 4  Then 'Sales Return' 
                      When 5  Then 'Mfg. Issue' 
                      When 6  Then 'Mfg. Return' 
                      When 7  Then 'Inventory Transfer' 
                      When 8  Then 'Inventory Adj. Positive' 
                      When 9  Then 'Inventory Adj. Negative' 
                      When 10 Then 'Damaged' 
                      When 11 Then 'Warehouse Move In' 
                      When 12 Then 'Warehouse Move Out' 
                      When 14 Then 'Physical Count' 
                      When 15 Then 'Transfer In' 
                      When 16 Then 'Transfer Out' 
                      When 17 Then 'Move' 
                      When 18 Then 'Mfg. Receipt' 
                      When 21 Then 'Sales Provisional'
                      Else 'Unknown' 
                   END) as TranTyp,
                   RecDate, Processed
            FROM LotTransaction 
            WHERE Processed IN ('N','P') 
              AND TransactionType In (2,3,5,7,9,10,12,16,17,20,21) 
              AND LotNo = @P1 AND BinNo = @P2
            UNION ALL 
            SELECT LotNo, BinNo, IssueDocNo as DocNo, IssueDocLineNo, QtyIssued as Qty, LotTranNo, 
                   TransactionType,
                   (CASE TransactionType  
                      When 1  Then 'Purchase Receipt' 
                      When 2  Then 'Purchase Return' 
                      When 3  Then 'Sales Issue' 
                      When 4  Then 'Sales Return' 
                      When 5  Then 'Mfg. Issue' 
                      When 6  Then 'Mfg. Return' 
                      When 7  Then 'Inventory Transfer' 
                      When 8  Then 'Inventory Adj. Positive' 
                      When 9  Then 'Inventory Adj. Negative' 
                      When 10 Then 'Damaged' 
                      When 11 Then 'Warehouse Move In' 
                      When 12 Then 'Warehouse Move Out' 
                      When 14 Then 'Physical Count' 
                      When 15 Then 'Transfer In' 
                      When 16 Then 'Transfer Out' 
                      When 17 Then 'Move' 
                      When 18 Then 'Mfg. Receipt' 
                      When 21 Then 'Sales Provisional'
                      Else 'Unknown' 
                   END) as TranTyp,
                   RecDate, Processed
            FROM QCLotTransaction 
            WHERE Processed IN ('N','P') 
              AND TransactionType In (2,3,5,7,9,10,12,16,17,20,21) 
              AND LotNo = @P1 AND BinNo = @P2
            ORDER BY RecDate DESC
        "#;

        let rows = client
            .query(query, &[&lot_no, &bin_no])
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
            .into_first_result()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        let mut transactions = Vec::new();
        for row in rows {
            transactions.push(LotTransactionItem {
                lot_tran_no: row.get::<i32, _>("LotTranNo").unwrap_or(0),
                lot_no: row.get::<&str, _>("LotNo").unwrap_or("").to_string(),
                bin_no: row.get::<&str, _>("BinNo").unwrap_or("").to_string(),
                doc_no: row.get::<&str, _>("DocNo").unwrap_or("").to_string(),
                issue_doc_line_no: row.get::<i16, _>("IssueDocLineNo"),
                qty: row.get::<f64, _>("Qty").unwrap_or(0.0),
                transaction_type: row.get::<u8, _>("TransactionType").unwrap_or(0),
                tran_typ: row.get::<&str, _>("TranTyp").unwrap_or("").to_string(),
                transaction_date: row
                    .get::<NaiveDateTime, _>("RecDate")
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
                status: row.get::<&str, _>("Processed").unwrap_or("").to_string(),
            });
        }

        Ok(transactions)
    }

    /// Execute transfer of committed stock (BME official behavior)
    /// 
    /// This function moves physical inventory ALONG WITH its commitment status:
    /// 1. Creates LotTransaction records: Type 9 (Issue) from source, Type 8 (Receipt) to dest
    /// 2. Reduces source LotMaster: QtyOnHand -= transfer_qty, QtyCommitSales -= transfer_qty
    /// 3. Deletes source LotMaster if QtyOnHand becomes 0
    /// 4. Creates or updates destination LotMaster with transferred quantities
    /// 5. Destination gets both QtyOnHand AND QtyCommitSales from the transfer (committed stock moves with commitment)
    pub async fn execute_committed_bin_transfer(
        &self,
        lot_no: &str,
        item_key: &str,
        location: &str,
        target_bin: &str,
        transfer_qty: f64,
        source_bin: &str,
        user_id: &str,
        remarks: &str,
        referenced: &str,
    ) -> Result<String, PutawayError> {
        let mut client = self
            .db
            .get_client()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        // Start Transaction
        client
            .simple_query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .await
            .map_err(|e| PutawayError::DatabaseError(format!("Failed to set isolation level: {e}")))?;

        client
            .simple_query("BEGIN TRANSACTION")
            .await
            .map_err(|e| PutawayError::DatabaseError(format!("Failed to begin transaction: {e}")))?;

        let transaction_result: Result<String, PutawayError> = async {
            let now = bangkok_now().naive_local();
            let user_id_truncated = if user_id.len() > 8 { &user_id[0..8] } else { user_id };

            // 1. Get next BT document number
            let bt_number = self.get_next_bt_sequence().await?;
            let document_no = format!("BT-{bt_number:08}");

            // 2. Create Mintxdh record for audit trail (same as Transfer Avail Qty)
            let inloc_record = self.get_inloc_record(item_key, location).await?;
            let in_acct = map_inclasskey_to_inacct(&inloc_record.inclasskey);
            let std_cost = inloc_record.stdcost;
            let trn_desc = "Bin Transfer";

            let mintxdh_query = r#"
                INSERT INTO Mintxdh (
                    ItemKey, Location, ToLocation, SysID, ProcessID, SysDocID, SysLinSq,
                    TrnTyp, TrnSubTyp, DocNo, DocDate, AplDate, TrnDesc, TrnQty, TrnAmt,
                    NLAcct, INAcct, CreatedSerlot, RecUserID, RecDate, Updated_FinTable,
                    SortField, JrnlBtchNo, StdCost, Stdcostupdated, GLtrnAmt
                ) VALUES (
                    @P1, @P2, '', '7', 'M', @P3, 1, 'A', '', @P4, @P5, @P5, @P6, 0, 0.000000,
                    '1100', @P7, 'Y', @P8, @P9, 0, '', '', @P10, 0, 0.000000
                )
            "#;
            client.execute(mintxdh_query, &[
                &item_key, &location, &document_no, &document_no, &now, &trn_desc,
                &in_acct, &user_id_truncated, &now, &std_cost
            ]).await.map_err(|e| PutawayError::TransactionError(format!("Failed to create Mintxdh record: {e}")))?;

            // 3. Validate Target Bin Exists
            let bin_exists_query = "SELECT COUNT(*) as count FROM BINMaster WHERE Location = @P1 AND BinNo = @P2";
            let bin_check = client.query(bin_exists_query, &[&location, &target_bin]).await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
                .into_row().await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;
            
            if let Some(row) = bin_check {
                let count: i32 = row.get("count").unwrap_or(0);
                if count == 0 {
                    return Err(PutawayError::InvalidBin { bin_no: target_bin.to_string(), location: location.to_string() });
                }
            } else {
                return Err(PutawayError::InvalidBin { bin_no: target_bin.to_string(), location: location.to_string() });
            }

            // 3. Get source LotMaster details (DateReceived, DateExpiry, VendorKey, VendorLotNo)
            let source_query = r#"
                SELECT DateReceived, DateExpiry, VendorKey, VendorLotNo
                FROM LotMaster 
                WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4
            "#;
            let source_row = client.query(source_query, &[&lot_no, &item_key, &location, &source_bin]).await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
                .into_row().await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
                .ok_or(PutawayError::ValidationError("Source lot not found".to_string()))?;

            let date_received: NaiveDateTime = source_row.get("DateReceived").unwrap_or(now);
            let date_expiry: Option<NaiveDateTime> = source_row.get("DateExpiry");
            let vendor_key: &str = source_row.get("VendorKey").unwrap_or("");
            let vendor_lot_no: &str = source_row.get("VendorLotNo").unwrap_or("");
            // Note: CustomerKey doesn't exist in LotMaster, use empty string for LotTransaction
            let customer_key: &str = "";

            // NOTE: We do NOT update LotMaster.QtyCommitSales here.
            // BME calculates commitment from LotTransaction records (Type 9 with Processed='N')
            // The Type 9 INSERT below already tracks the commitment via the standard query:
            // SELECT SUM(QtyIssued) FROM LotTransaction WHERE Processed IN ('N','P') AND TransactionType IN (2,3,5,7,9,10,12,16,17,20,21)

            // 5. INSERT LotTransaction - Type 9 (Inv Adj Negative / Issue) from source bin
            let issue_insert = r#"
                INSERT INTO LotTransaction (
                    LotNo, ItemKey, LocationKey, DateReceived, DateExpiry,
                    TransactionType, VendorlotNo, 
                    IssueDocNo, IssueDocLineNo, IssueDate, QtyIssued,
                    RecUserid, RecDate, Processed, BinNo
                ) VALUES (
                    @P1, @P2, @P3, @P4, @P5,
                    9, @P6,
                    @P7, 1, @P8, @P9,
                    @P10, @P11, 'N', @P12
                )
            "#;
            client.execute(issue_insert, &[
                &lot_no, &item_key, &location, &date_received, &date_expiry,
                &vendor_lot_no,
                &document_no, &now, &transfer_qty,
                &user_id_truncated, &now, &source_bin
            ]).await.map_err(|e| PutawayError::TransactionError(format!("Failed to insert Issue LotTransaction: {e}")))?;

            // 6. INSERT LotTransaction - Type 8 (Inv Adj Positive / Receipt) to dest bin
            // BME includes DateQuarantine field (NULL for committed transfers)
            let receipt_insert = r#"
                INSERT INTO LotTransaction (
                    LotNo, ItemKey, LocationKey, DateReceived, DateExpiry,
                    TransactionType,
                    ReceiptDocNo, ReceiptDocLineNo, QtyReceived,
                    Vendorkey, VendorlotNo, CustomerKey,
                    RecUserid, RecDate, Processed, BinNo, DateQuarantine
                ) VALUES (
                    @P1, @P2, @P3, @P4, @P5,
                    8,
                    @P6, 1, @P7,
                    @P8, @P9, @P10,
                    @P11, @P12, 'N', @P13, NULL
                )
            "#;
            client.execute(receipt_insert, &[
                &lot_no, &item_key, &location, &date_received, &date_expiry,
                &document_no, &transfer_qty,
                &vendor_key, &vendor_lot_no, &customer_key,
                &user_id_truncated, &now, &target_bin
            ]).await.map_err(|e| PutawayError::TransactionError(format!("Failed to insert Receipt LotTransaction: {e}")))?;

            // 7. Handle LotMaster movement - Move BOTH QtyOnHand AND QtyCommitSales from source to destination
            // BME "Transfer with Commit" moves physical inventory along with its commitment status
            
            // 7a. Get source LotMaster current quantities
            let source_qty_query = r#"
                SELECT QtyOnHand, QtyCommitSales, LotStatus, QtyReceived
                FROM LotMaster 
                WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4
            "#;
            let source_qty_row = client.query(source_qty_query, &[&lot_no, &item_key, &location, &source_bin]).await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
                .into_row().await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
                .ok_or(PutawayError::ValidationError("Source lot not found for qty check".to_string()))?;

            let source_qty_on_hand: f64 = source_qty_row.get("QtyOnHand").unwrap_or(0.0);
            let source_qty_commit: f64 = source_qty_row.get("QtyCommitSales").unwrap_or(0.0);
            let source_lot_status: String = source_qty_row.get::<&str, _>("LotStatus").unwrap_or("P").to_string();
            let source_qty_received: f64 = source_qty_row.get("QtyReceived").unwrap_or(0.0);
            
            // Calculate new source quantities after transfer
            let new_source_qty_on_hand = source_qty_on_hand - transfer_qty;
            let new_source_qty_commit = if source_qty_commit >= transfer_qty {
                source_qty_commit - transfer_qty
            } else {
                0.0 // Don't go negative
            };

            // 7b. Update or Delete source LotMaster
            if new_source_qty_on_hand <= 0.0 {
                // Delete source record if QtyOnHand becomes 0 or negative
                let delete_source = r#"
                    DELETE FROM LotMaster 
                    WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4
                "#;
                client.execute(delete_source, &[&lot_no, &item_key, &location, &source_bin]).await
                    .map_err(|e| PutawayError::TransactionError(format!("Failed to delete source LotMaster: {e}")))?;
            } else {
                // Update source with reduced quantities
                let update_source = r#"
                    UPDATE LotMaster 
                    SET QtyOnHand = @P1, QtyCommitSales = @P2, 
                        DocumentNo = @P3, TransactionType = 9,
                        RecUserId = @P4, Recdate = @P5
                    WHERE LotNo = @P6 AND ItemKey = @P7 AND LocationKey = @P8 AND BinNo = @P9
                "#;
                client.execute(update_source, &[
                    &new_source_qty_on_hand, &new_source_qty_commit,
                    &document_no, &user_id_truncated, &now,
                    &lot_no, &item_key, &location, &source_bin
                ]).await.map_err(|e| PutawayError::TransactionError(format!("Failed to update source LotMaster: {e}")))?;
            }

            // 7c. Check if destination LotMaster exists
            let dest_check_query = r#"
                SELECT QtyOnHand, QtyCommitSales 
                FROM LotMaster 
                WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4
            "#;
            let dest_exists = client.query(dest_check_query, &[&lot_no, &item_key, &location, &target_bin]).await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
                .into_row().await
                .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

            if let Some(dest_row) = dest_exists {
                // 7d. Destination exists - Update with added quantities
                let dest_qty_on_hand: f64 = dest_row.get("QtyOnHand").unwrap_or(0.0);
                let dest_qty_commit: f64 = dest_row.get("QtyCommitSales").unwrap_or(0.0);
                
                let new_dest_qty_on_hand = dest_qty_on_hand + transfer_qty;
                let new_dest_qty_commit = dest_qty_commit + transfer_qty; // Committed stock moves with commitment

                let update_dest = r#"
                    UPDATE LotMaster 
                    SET QtyOnHand = @P1, QtyCommitSales = @P2,
                        DocumentNo = @P3, TransactionType = 8,
                        RecUserId = @P4, Recdate = @P5
                    WHERE LotNo = @P6 AND ItemKey = @P7 AND LocationKey = @P8 AND BinNo = @P9
                "#;
                client.execute(update_dest, &[
                    &new_dest_qty_on_hand, &new_dest_qty_commit,
                    &document_no, &user_id_truncated, &now,
                    &lot_no, &item_key, &location, &target_bin
                ]).await.map_err(|e| PutawayError::TransactionError(format!("Failed to update destination LotMaster: {e}")))?;
            } else {
                // 7e. Destination doesn't exist - Create new record with committed quantities
                let insert_dest = r#"
                    INSERT INTO LotMaster (
                        LotNo, ItemKey, LocationKey, DateReceived, DateExpiry,
                        QtyReceived, QtyIssued, QtyCommitSales, QtyOnHand,
                        DocumentNo, DocumentLineNo, TransactionType, VendorKey, VendorLotNo,
                        QtyOnOrder, RecUserId, Recdate, BinNo, LotStatus
                    ) VALUES (
                        @P1, @P2, @P3, @P4, @P5,
                        @P6, 0, @P7, @P7,
                        @P8, 1, 8, @P9, @P10,
                        0, @P11, @P12, @P13, @P14
                    )
                "#;
                client.execute(insert_dest, &[
                    &lot_no, &item_key, &location, &date_received, &date_expiry,
                    &source_qty_received, &transfer_qty,
                    &document_no, &vendor_key, &vendor_lot_no,
                    &user_id_truncated, &now, &target_bin, &source_lot_status
                ]).await.map_err(|e| PutawayError::TransactionError(format!("Failed to insert destination LotMaster: {e}")))?;
            }

            // 8. Audit Trail (BinTransfer) - Include User1 (remarks) and User5 (referenced)
            let bin_transfer_query = r#"
                INSERT INTO BinTransfer (
                    ItemKey, Location, LotNo, BinNoFrom, BinNoTo, 
                    LotTranNo, QtyOnHand, TransferQty, InTransID, 
                    RecUserID, RecDate, ContainerNo, User1, User5
                ) VALUES (@P1, @P2, @P3, @P4, @P5, 0, @P6, @P7, 0, @P8, @P9, '0', @P10, @P11)
            "#;
            client.execute(bin_transfer_query, &[
                &item_key, &location, &lot_no, &source_bin, &target_bin,
                &source_qty_on_hand, &transfer_qty, &user_id_truncated, &now,
                &remarks, &referenced
            ]).await.map_err(|e| PutawayError::TransactionError(format!("Failed to log BinTransfer: {e}")))?;

            Ok(document_no)
        }.await;

        match transaction_result {
            Ok(doc_no) => {
                client.simple_query("COMMIT").await.map_err(|e| PutawayError::DatabaseError(format!("Commit failed: {e}")))?;
                Ok(doc_no)
            },
            Err(e) => {
                client.simple_query("ROLLBACK").await.ok();
                Err(e)
            }
        }
    }
}
