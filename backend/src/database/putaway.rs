use super::Database;
use crate::models::putaway::*;
use crate::utils::bangkok_now;
use anyhow::Result;
use chrono::{DateTime, Utc};
use tiberius::{Query, Row};
use tracing::info;

impl Database {
    /// Get all pending putaway items (items that need to be put away)
    pub async fn get_pending_putaway_items(&self) -> Result<Vec<PutawayItem>> {
        let mut client = self.get_client().await?;

        let query = "
            SELECT DISTINCT 
                lm.LotNo,
                lm.ItemKey,
                im.Description as ItemDescription,
                lm.LocationKey,
                lm.BinNo,
                lm.QtyReceived,
                lm.QtyOnHand,
                lm.DateReceived,
                lm.DateExpiry,
                lm.VendorKey,
                lm.VendorLotNo,
                lm.DocumentNo,
                lm.LotStatus,
                lm.RecUserId
            FROM LotMaster lm
            LEFT JOIN INMAST im ON lm.ItemKey = im.ItemKey
            WHERE lm.LotStatus = 'PENDING_PUTAWAY' 
               OR (lm.LotStatus = 'RECEIVED' AND lm.QtyOnHand > 0)
            ORDER BY lm.DateReceived ASC
        ";

        let stream = Query::new(query).query(&mut client).await?;
        let rows: Vec<Row> = stream.into_first_result().await?;

        let mut items = Vec::new();
        for row in rows {
            let item = PutawayItem {
                lot_no: row.get::<&str, _>("LotNo").unwrap_or("").to_string(),
                item_key: row.get::<&str, _>("ItemKey").unwrap_or("").to_string(),
                item_description: row.get::<&str, _>("ItemDescription").map(|s| s.to_string()),
                location_key: row.get::<&str, _>("LocationKey").unwrap_or("").to_string(),
                bin_no: row.get::<&str, _>("BinNo").map(|s| s.to_string()),
                qty_received: row.get::<f64, _>("QtyReceived").unwrap_or(0.0),
                qty_on_hand: row.get::<f64, _>("QtyOnHand").unwrap_or(0.0),
                date_received: row
                    .get::<chrono::NaiveDateTime, _>("DateReceived")
                    .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
                    .unwrap_or_else(|| bangkok_now().with_timezone(&Utc)),
                date_expiry: row
                    .get::<chrono::NaiveDateTime, _>("DateExpiry")
                    .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
                    .unwrap_or_else(|| bangkok_now().with_timezone(&Utc)),
                vendor_key: row.get::<&str, _>("VendorKey").unwrap_or("").to_string(),
                vendor_lot_no: row.get::<&str, _>("VendorLotNo").unwrap_or("").to_string(),
                document_no: row.get::<&str, _>("DocumentNo").unwrap_or("").to_string(),
                lot_status: row.get::<&str, _>("LotStatus").unwrap_or("").to_string(),
                rec_user_id: row.get::<&str, _>("RecUserId").unwrap_or("").to_string(),
            };
            items.push(item);
        }

        info!("Retrieved {} pending putaway items", items.len());
        Ok(items)
    }

    /// Validate a scanned barcode
    pub async fn validate_scan(&self, barcode: &str, scan_type: &ScanType) -> Result<ScanResponse> {
        let mut client = self.get_client().await?;

        match scan_type {
            ScanType::Item => {
                let query = "SELECT ItemKey, Description, Unit FROM INMAST WHERE ItemKey = @P1 OR Barcode = @P1";
                let mut query_obj = Query::new(query);
                query_obj.bind(barcode);
                let stream = query_obj.query(&mut client).await?;
                let rows: Vec<Row> = stream.into_first_result().await?;

                if let Some(row) = rows.first() {
                    Ok(ScanResponse {
                        valid: true,
                        scan_type: ScanType::Item,
                        data: Some(ScanData::Item {
                            item_key: row.get::<&str, _>("ItemKey").unwrap_or("").to_string(),
                            description: row
                                .get::<&str, _>("Description")
                                .unwrap_or("")
                                .to_string(),
                            unit: row.get::<&str, _>("Unit").unwrap_or("").to_string(),
                        }),
                        message: "Item found".to_string(),
                    })
                } else {
                    Ok(ScanResponse {
                        valid: false,
                        scan_type: ScanType::Item,
                        data: None,
                        message: "Item not found".to_string(),
                    })
                }
            }
            ScanType::Location => {
                let query = "SELECT LocationKey, Description, LocationType FROM LOCATIONS WHERE LocationKey = @P1 OR Barcode = @P1";
                let mut query_obj = Query::new(query);
                query_obj.bind(barcode);
                let stream = query_obj.query(&mut client).await?;
                let rows: Vec<Row> = stream.into_first_result().await?;

                if let Some(row) = rows.first() {
                    Ok(ScanResponse {
                        valid: true,
                        scan_type: ScanType::Location,
                        data: Some(ScanData::Location {
                            location_key: row
                                .get::<&str, _>("LocationKey")
                                .unwrap_or("")
                                .to_string(),
                            description: row
                                .get::<&str, _>("Description")
                                .unwrap_or("")
                                .to_string(),
                            location_type: row
                                .get::<&str, _>("LocationType")
                                .unwrap_or("")
                                .to_string(),
                        }),
                        message: "Location found".to_string(),
                    })
                } else {
                    Ok(ScanResponse {
                        valid: false,
                        scan_type: ScanType::Location,
                        data: None,
                        message: "Location not found".to_string(),
                    })
                }
            }
            ScanType::Lot => {
                let query = "SELECT LotNo, ItemKey, QtyOnHand FROM LotMaster WHERE LotNo = @P1";
                let mut query_obj = Query::new(query);
                query_obj.bind(barcode);
                let stream = query_obj.query(&mut client).await?;
                let rows: Vec<Row> = stream.into_first_result().await?;

                if let Some(row) = rows.first() {
                    Ok(ScanResponse {
                        valid: true,
                        scan_type: ScanType::Lot,
                        data: Some(ScanData::Lot {
                            lot_no: row.get::<&str, _>("LotNo").unwrap_or("").to_string(),
                            item_key: row.get::<&str, _>("ItemKey").unwrap_or("").to_string(),
                            qty_on_hand: row.get::<f64, _>("QtyOnHand").unwrap_or(0.0),
                        }),
                        message: "Lot found".to_string(),
                    })
                } else {
                    Ok(ScanResponse {
                        valid: false,
                        scan_type: ScanType::Lot,
                        data: None,
                        message: "Lot not found".to_string(),
                    })
                }
            }
        }
    }

    /// Get comprehensive putaway history with transaction details
    pub async fn get_comprehensive_putaway_history(
        &self,
        limit: Option<i32>,
    ) -> Result<Vec<PutawayHistory>> {
        let mut client = self.get_client().await?;

        let limit_clause = match limit {
            Some(l) => format!("TOP {l}"),
            None => "TOP 100".to_string(),
        };

        let query = format!(
            "
            SELECT {limit_clause} 
                m.InTransID as TransactionId,
                bt.LotNo,
                m.ItemKey,
                m.Location as FromLocation,
                m.ToLocation,
                bt.BinNoTo as BinNo,
                m.TrnQty as QtyMoved,
                m.RecDate as TransactionDate,
                m.RecUserID as UserId,
                m.DocNo,
                m.TrnDesc as Description
            FROM Mintxdh m
            INNER JOIN BinTransfer bt ON m.InTransID = bt.InTransID
            WHERE m.TrnSubTyp = 'PUT'
            ORDER BY m.RecDate DESC
        "
        );

        let stream = Query::new(&query).query(&mut client).await?;
        let rows: Vec<Row> = stream.into_first_result().await?;

        let mut history = Vec::new();
        for row in rows {
            let item = PutawayHistory {
                transaction_id: row.get::<i32, _>("TransactionId").unwrap_or(0),
                lot_no: row.get::<&str, _>("LotNo").unwrap_or("").to_string(),
                item_key: row.get::<&str, _>("ItemKey").unwrap_or("").to_string(),
                from_location: row.get::<&str, _>("FromLocation").unwrap_or("").to_string(),
                to_location: row.get::<&str, _>("ToLocation").unwrap_or("").to_string(),
                bin_no: row.get::<&str, _>("BinNo").unwrap_or("").to_string(),
                qty_moved: row.get::<f64, _>("QtyMoved").unwrap_or(0.0),
                transaction_date: row
                    .get::<chrono::NaiveDateTime, _>("TransactionDate")
                    .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
                    .unwrap_or_else(|| bangkok_now().with_timezone(&Utc)),
                user_id: row.get::<&str, _>("UserId").unwrap_or("").to_string(),
            };
            history.push(item);
        }

        info!(
            "Retrieved {} comprehensive putaway history records",
            history.len()
        );
        Ok(history)
    }

    /// Legacy get putaway history (for backward compatibility)
    pub async fn get_putaway_history(&self, limit: Option<i32>) -> Result<Vec<PutawayHistory>> {
        // Use the new comprehensive method
        self.get_comprehensive_putaway_history(limit).await
    }

    /// Get transaction audit trail
    pub async fn get_transaction_audit_trail(
        &self,
        transaction_id: i32,
    ) -> Result<Vec<serde_json::Value>> {
        let mut client = self.get_client().await?;

        let query = "
            SELECT 
                'Header' as RecordType,
                m.InTransID,
                m.DocNo,
                m.TrnDesc,
                m.RecDate,
                m.RecUserID
            FROM Mintxdh m
            WHERE m.InTransID = @P1
            
            UNION ALL
            
            SELECT 
                'BinTransfer' as RecordType,
                bt.InTransID,
                bt.LotNo as DocNo,
                CONCAT('Transfer from ', bt.BinNoFrom, ' to ', bt.BinNoTo) as TrnDesc,
                bt.RecDate,
                bt.RecUserID
            FROM BinTransfer bt
            WHERE bt.InTransID = @P1
            
            UNION ALL
            
            SELECT 
                'LotTransaction' as RecordType,
                lt.LotTranNo as InTransID,
                CASE 
                    WHEN lt.TransactionType = 8 THEN lt.ReceiptDocNo
                    WHEN lt.TransactionType = 9 THEN lt.IssueDocNo
                    ELSE ''
                END as DocNo,
                CASE 
                    WHEN lt.TransactionType = 8 THEN 'Receipt'
                    WHEN lt.TransactionType = 9 THEN 'Issue'
                    ELSE 'Unknown'
                END as TrnDesc,
                lt.RecDate,
                lt.RecUserid as RecUserID
            FROM LotTransaction lt
            INNER JOIN BinTransfer bt ON lt.LotTranNo = bt.LotTranNo
            WHERE bt.InTransID = @P1
            
            ORDER BY RecDate
        ";

        let mut query_obj = Query::new(query);
        query_obj.bind(transaction_id);

        let stream = query_obj.query(&mut client).await?;
        let rows: Vec<Row> = stream.into_first_result().await?;

        let mut audit_trail = Vec::new();
        for row in rows {
            let mut record = serde_json::Map::new();
            record.insert(
                "record_type".to_string(),
                serde_json::Value::String(
                    row.get::<&str, _>("RecordType").unwrap_or("").to_string(),
                ),
            );
            record.insert(
                "transaction_id".to_string(),
                serde_json::Value::Number(serde_json::Number::from(
                    row.get::<i32, _>("InTransID").unwrap_or(0),
                )),
            );
            record.insert(
                "doc_no".to_string(),
                serde_json::Value::String(row.get::<&str, _>("DocNo").unwrap_or("").to_string()),
            );
            record.insert(
                "description".to_string(),
                serde_json::Value::String(row.get::<&str, _>("TrnDesc").unwrap_or("").to_string()),
            );
            record.insert(
                "user_id".to_string(),
                serde_json::Value::String(
                    row.get::<&str, _>("RecUserID").unwrap_or("").to_string(),
                ),
            );

            audit_trail.push(serde_json::Value::Object(record));
        }

        Ok(audit_trail)
    }
}
