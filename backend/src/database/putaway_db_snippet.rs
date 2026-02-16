
    /// Search for transactions associated with a lot
    pub async fn find_transactions_by_lot(
        &self,
        lot_no: &str,
    ) -> Result<Vec<LotTransactionItem>, PutawayError> {
        let mut client = self
            .db
            .get_client()
            .await
            .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

        // Query LotTransaction and LotMaster to get relevant details
        // We filter by TransactionType that implies commitment (e.g., Issue) or just show all for selection?
        // User screenshot shows "Transfer Available Qty" vs "Transfer Qty" (Scenario 2) vs "Commit" (Scenario 3)
        // Scenario 3 likely means moving 'Committed' stock.
        // We look for transactions where QtyIssued > 0 OR QtyForLotAssignment > 0, basically active commitments.
        // For now, let's fetch recent transactions for the lot.
        let query = r#"
            SELECT 
                lt.LotTranNo, lt.LotNo, lt.BinNo, lt.TransactionType, 
                COALESCE(lt.QtyIssued, 0) + COALESCE(lt.QtyReceived, 0) as Qty,
                COALESCE(lt.IssueDocNo, lt.ReceiptDocNo, '') as DocNo,
                lt.RecDate, lt.Processed
            FROM LotTransaction lt
            WHERE lt.LotNo = @P1
            ORDER BY lt.RecDate DESC
        "#;

        let rows = client
            .query(query, &[&lot_no])
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
                transaction_type: row.get::<u8, _>("TransactionType").unwrap_or(0),
                qty: row.get::<f64, _>("Qty").unwrap_or(0.0),
                doc_no: row.get::<&str, _>("DocNo").unwrap_or("").to_string(),
                transaction_date: row
                    .get::<NaiveDateTime, _>("RecDate")
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
                status: row.get::<&str, _>("Processed").unwrap_or("").to_string(),
            });
        }

        Ok(transactions)
    }

    /// Execute transfer of committed stock (Scenario 2 & 3)
    pub async fn execute_committed_bin_transfer(
        &self,
        lot_no: &str,
        item_key: &str,
        location: &str,
        target_bin: &str,
        transaction_ids: &[i32],
        user_id: &str,
    ) -> Result<usize, PutawayError> {
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

        let transaction_result: Result<usize, PutawayError> = async {
            let mut transferred_count = 0;
            let now = bangkok_now().naive_local();
             // Truncate user ID
            let user_id_truncated = if user_id.len() > 8 { &user_id[0..8] } else { user_id };

            // Get next BT document number (shared for all items in this request, or one per item? Usually one per request)
            let bt_number = self.get_next_bt_sequence().await?;
            let document_no = format!("BT-{bt_number:08}");

             // Validate Target Bin Exists
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


            for &tran_id in transaction_ids {
                // 1. Get Transaction Details including current Bin
                let tran_query = r#"
                    SELECT BinNo, QtyIssued, QtyReceived, TransactionType 
                    FROM LotTransaction 
                    WHERE LotTranNo = @P1 AND LotNo = @P2
                "#;
                let tran_row = client.query(tran_query, &[&tran_id, &lot_no]).await
                    .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
                    .into_row().await
                    .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;

                if let Some(row) = tran_row {
                    let current_bin: &str = row.get("BinNo").unwrap_or("");
                    // Assume QtyIssued is the committed qty we are moving? Or is it QtyForLotAssignment?
                    // For Issue transactions (Type 9, 5 etc), QtyIssued is likely the amount.
                    let qty: f64 = row.get::<f64, _>("QtyIssued").unwrap_or(0.0) + row.get::<f64, _>("QtyReceived").unwrap_or(0.0);
                    
                    if current_bin == target_bin {
                        continue; // Skip if already in target bin
                    }

                    // 2. Update LotTransaction Bin
                    let update_tran_query = "UPDATE LotTransaction SET BinNo = @P1 WHERE LotTranNo = @P2";
                    client.execute(update_tran_query, &[&target_bin, &tran_id]).await
                        .map_err(|e| PutawayError::TransactionError(format!("Failed to update LotTransaction {tran_id}: {e}")))?;

                    // 3. Update LotMaster (Source Bin) - Reduce QtyOnHand AND QtyCommitSales
                    // Since this is a committed transfer, we assume we are moving the commitment too.
                     let source_update_query = r#"
                        UPDATE LotMaster 
                        SET QtyOnHand = QtyOnHand - @P1, 
                            QtyCommitSales = QtyCommitSales - @P1,
                            RecUserId = @P2, Recdate = @P3
                        WHERE LotNo = @P4 AND ItemKey = @P5 AND LocationKey = @P6 AND BinNo = @P7
                     "#;
                     client.execute(source_update_query, &[&qty, &user_id_truncated, &now, &lot_no, &item_key, &location, &current_bin]).await
                        .map_err(|e| PutawayError::TransactionError(format!("Failed to update source LotMaster: {e}")))?;
                     
                     // Clean up source if 0? Maybe not if it's committed, wait... if we move commitment, 0 is possible.
                     // Let's leave cleanup for now to be safe, or check QtyOnHand <= 0.
                     
                     // 4. Update LotMaster (Target Bin) - Increment QtyOnHand AND QtyCommitSales
                     // Check if exists first
                     let dest_check_query = "SELECT COUNT(*) as count FROM LotMaster WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4";
                     let dest_count_row = client.query(dest_check_query, &[&lot_no, &item_key, &location, &target_bin]).await
                        .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
                        .into_row().await
                        .map_err(|e| PutawayError::DatabaseError(e.to_string()))?;
                     
                     let exists = dest_count_row.map(|r| r.get::<i32, _>("count").unwrap_or(0)).unwrap_or(0) > 0;

                     if exists {
                         let dest_update_query = r#"
                            UPDATE LotMaster 
                            SET QtyOnHand = QtyOnHand + @P1, 
                                QtyCommitSales = QtyCommitSales + @P1,
                                RecUserId = @P2, Recdate = @P3
                            WHERE LotNo = @P4 AND ItemKey = @P5 AND LocationKey = @P6 AND BinNo = @P7
                         "#;
                         client.execute(dest_update_query, &[&qty, &user_id_truncated, &now, &lot_no, &item_key, &location, &target_bin]).await
                             .map_err(|e| PutawayError::TransactionError(format!("Failed to update dest LotMaster: {e}")))?;
                     } else {
                         // Insert new record in dest bin (Copy details from source... this requires fetching source details first)
                         // For simplicity in this plan, I'll fetch source details. 
                         // Note: Ideally we should have fetched source details earlier.
                         // Let's assume we copy basic fields. Real implementation requires fetching.
                          let fetch_source_query = "SELECT DateReceived, DateExpiry, VendorKey, VendorLotNo, LotStatus FROM LotMaster WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4";
                          let source_details = client.query(fetch_source_query, &[&lot_no, &item_key, &location, &current_bin]).await
                              .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
                              .into_row().await
                              .map_err(|e| PutawayError::DatabaseError(e.to_string()))?
                              .ok_or(PutawayError::ValidationError("Source lot not found for copy".to_string()))?;

                           let date_received: NaiveDateTime = source_details.get("DateReceived").unwrap_or(now);
                           let date_expiry: NaiveDateTime = source_details.get("DateExpiry").unwrap_or(now);
                           let vendor_key: &str = source_details.get("VendorKey").unwrap_or("");
                           let vendor_lot_no: &str = source_details.get("VendorLotNo").unwrap_or("");
                           let lot_status: &str = source_details.get("LotStatus").unwrap_or("");
                           
                           let insert_query = r#"
                                INSERT INTO LotMaster (
                                    LotNo, ItemKey, LocationKey, BinNo, 
                                    QtyOnHand, QtyCommitSales, QtyIssued, QtyReceived, QtyOnOrder,
                                    DateReceived, DateExpiry, VendorKey, VendorLotNo, 
                                    LotStatus, TransactionType, DocumentNo, DocumentLineNo,
                                    RecUserId, Recdate
                                ) VALUES (
                                    @P1, @P2, @P3, @P4,
                                    @P5, @P5, 0, 0, 0,
                                    @P6, @P7, @P8, @P9,
                                    @P10, 8, @P11, 1,
                                    @P12, @P13
                                )
                           "#;
                           client.execute(insert_query, &[
                               &lot_no, &item_key, &location, &target_bin,
                               &qty,
                               &date_received, &date_expiry, &vendor_key, &vendor_lot_no,
                               &lot_status, &document_no,
                               &user_id_truncated, &now
                           ]).await.map_err(|e| PutawayError::TransactionError(format!("Failed to insert dest LotMaster: {e}")))?;
                     }
                    
                    // 5. Audit Trail (BinTransfer)
                    let bin_transfer_query = r#"
                        INSERT INTO BinTransfer (
                            ItemKey, Location, LotNo, BinNoFrom, BinNoTo, 
                            LotTranNo, QtyOnHand, TransferQty, InTransID, 
                            RecUserID, RecDate, ContainerNo
                        ) VALUES (@P1, @P2, @P3, @P4, @P5, @P6, @P7, @P7, 0, @P8, @P9, '0')
                    "#;
                    client.execute(bin_transfer_query, &[
                        &item_key, &location, &lot_no, &current_bin, &target_bin,
                        &tran_id, &qty, &user_id_truncated, &now
                    ]).await.map_err(|e| PutawayError::TransactionError(format!("Failed to log BinTransfer: {e}")))?;

                    transferred_count += 1;
                }
            }

            Ok(transferred_count)
        }.await;

        match transaction_result {
            Ok(count) => {
                client.simple_query("COMMIT").await.map_err(|e| PutawayError::DatabaseError(format!("Commit failed: {e}")))?;
                Ok(count)
            },
            Err(e) => {
                client.simple_query("ROLLBACK").await.ok();
                Err(e)
            }
        }
    }
