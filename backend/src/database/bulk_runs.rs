use crate::database::Database;
use crate::models::bulk_runs::*;
use crate::models::inventory::{AlertSeverity, InventoryAlert, InventoryAlertType};
use crate::utils::timezone::convert_to_utc;
use anyhow::{Context, Result};
use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive};
use chrono::{NaiveDateTime, Utc};
use tiberius::{Query as TiberiusQuery, Row};
use tracing::{error, info, instrument, warn};

impl Database {
    /// List active bulk runs with pagination (Story 1.4 T1.4.3)
    #[instrument(skip(self))]
    pub async fn list_active_bulk_runs_paginated(
        &self,
        page: u32,
        limit: u32,
    ) -> Result<(Vec<BulkRunSummary>, u64)> {
        info!(
            "Listing active bulk runs with pagination - page: {}, limit: {}",
            page, limit
        );

        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        // Calculate offset
        let offset = (page.saturating_sub(1)) * limit;

        // Get total count first
        let count_query = r#"
            SELECT COUNT(*) as TotalCount
            FROM (
                SELECT DISTINCT RunNo
                FROM Cust_BulkRun 
                WHERE Status IN ('NEW', 'PRINT')
            ) as UniqueRuns
        "#;

        let count_select = TiberiusQuery::new(count_query);
        let count_stream = count_select
            .query(&mut client)
            .await
            .context("Failed to execute bulk runs count query")?;
        let count_rows: Vec<Row> = count_stream
            .into_first_result()
            .await
            .context("Failed to get bulk runs count results")?;

        let total_count: i32 = count_rows
            .first()
            .and_then(|row| row.get::<i32, _>("TotalCount"))
            .unwrap_or(0);

        // Get paginated results - SEC-001: Use parameterized queries
        let query = r#"
            SELECT DISTINCT
                RunNo,
                FormulaId,
                FormulaDesc,
                Status,
                COUNT(*) as BatchCount
            FROM Cust_BulkRun
            WHERE Status IN ('NEW', 'PRINT')
            GROUP BY RunNo, FormulaId, FormulaDesc, Status
            ORDER BY RunNo DESC
            OFFSET @P1 ROWS FETCH NEXT @P2 ROWS ONLY
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(offset as i32);
        select.bind(limit as i32);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute paginated bulk runs list query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get paginated bulk runs list results")?;

        let mut runs = Vec::new();
        for row in &rows {
            let run_summary = self.row_to_bulk_run_summary(row)?;
            runs.push(run_summary);
        }

        info!(
            "Found {} runs (page {} of {})",
            runs.len(),
            page,
            (total_count as f64 / limit as f64).ceil() as u64
        );
        Ok((runs, total_count as u64))
    }

    /// List all active bulk runs for modal selection (backward compatibility)
    #[instrument(skip(self))]
    pub async fn list_active_bulk_runs(&self) -> Result<Vec<BulkRunSummary>> {
        info!("Listing all active bulk runs for modal selection");

        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        let query = r#"
            SELECT DISTINCT 
                RunNo,
                FormulaId,
                FormulaDesc,
                Status,
                COUNT(*) as BatchCount
            FROM Cust_BulkRun 
            WHERE Status IN ('NEW', 'PRINT')
            GROUP BY RunNo, FormulaId, FormulaDesc, Status
            ORDER BY RunNo DESC
        "#;

        let select = TiberiusQuery::new(query);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute bulk runs list query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get bulk runs list results")?;

        let mut runs = Vec::new();
        for row in &rows {
            let run_summary = self.row_to_bulk_run_summary(row)?;
            runs.push(run_summary);
        }

        info!("Found {} active bulk runs for modal selection", runs.len());
        Ok(runs)
    }

    /// Search for bulk runs by run number
    #[instrument(skip(self))]
    pub async fn search_bulk_runs(&self, search_query: &str, search_mode: &str) -> Result<Vec<BulkRun>> {
        info!("Searching bulk runs with query: {} (mode: {})", search_query, search_mode);

        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        // Different query based on search mode
        let (_query, select) = if search_mode == "exact" {
            // Exact matching: only search by RunNo
            let query = r#"
                SELECT
                    RunNo,
                    BatchNo,
                    FormulaId,
                    FormulaDesc,
                    NoOfBatches,
                    PalletsPerBatch,
                    Status,
                    RecDate
                FROM Cust_BulkRun
                WHERE RunNo = @P1
                ORDER BY RunNo DESC
            "#;
            let mut select = TiberiusQuery::new(query);

            // For exact mode, only accept numeric run numbers
            if let Ok(run_no) = search_query.parse::<i32>() {
                select.bind(run_no);
            } else {
                // If not a number, bind -1 to ensure no results for exact mode
                select.bind(-1);
            }
            (query, select)
        } else {
            // Partial matching: search across RunNo, BatchNo, and FormulaId
            let query = r#"
                SELECT
                    RunNo,
                    BatchNo,
                    FormulaId,
                    FormulaDesc,
                    NoOfBatches,
                    PalletsPerBatch,
                    Status,
                    RecDate
                FROM Cust_BulkRun
                WHERE RunNo = @P1
                   OR BatchNo LIKE '%' + @P2 + '%'
                   OR FormulaId LIKE '%' + @P3 + '%'
                ORDER BY RunNo DESC
            "#;
            let mut select = TiberiusQuery::new(query);

            // Try to parse as run number, otherwise use as string search
            if let Ok(run_no) = search_query.parse::<i32>() {
                select.bind(run_no);
                select.bind(search_query);
                select.bind(search_query);
            } else {
                select.bind(-1); // Invalid run number to force string searches
                select.bind(search_query);
                select.bind(search_query);
            }
            (query, select)
        };

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute bulk run search query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get bulk run search results")?;

        let mut runs = Vec::new();
        for row in &rows {
            let run = self.row_to_bulk_run(row)?;
            runs.push(run);
        }

        info!(
            "Found {} bulk runs matching query: {}",
            runs.len(),
            search_query
        );
        Ok(runs)
    }

    /// Get bulk run by specific run number
    #[instrument(skip(self))]
    pub async fn get_bulk_run(&self, run_no: i32) -> Result<Option<BulkRun>> {
        info!("Getting bulk run: {}", run_no);

        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        let query = r#"
            SELECT 
                RunNo,
                BatchNo,
                FormulaId,
                FormulaDesc,
                NoOfBatches,
                PalletsPerBatch,
                Status,
                RecDate
            FROM Cust_BulkRun 
            WHERE RunNo = @P1
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(run_no);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute bulk run query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get bulk run results")?;

        if let Some(row) = rows.first() {
            let run = self.row_to_bulk_run(row)?;
            Ok(Some(run))
        } else {
            info!("Bulk run not found: {}", run_no);
            Ok(None)
        }
    }

    /// Get bulk run status information
    #[instrument(skip(self))]
    pub async fn get_bulk_run_status(&self, run_no: i32) -> Result<Option<BulkRunStatusResponse>> {
        info!("Getting bulk run status: {}", run_no);

        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        let query = r#"
            SELECT 
                RunNo,
                Status,
                FormulaDesc,
                ModifiedDate
            FROM Cust_BulkRun 
            WHERE RunNo = @P1
            ORDER BY RowNum DESC
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(run_no);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to query bulk run status")?;

        if let Some(row) = stream.into_first_result().await?.first() {
            // Handle SQL Server datetime - can be null
            let last_modified = row.get::<NaiveDateTime, _>("ModifiedDate")
                .map(|naive_dt| naive_dt.and_utc());
            
            let status_response = BulkRunStatusResponse {
                run_no: row.get::<i32, _>("RunNo").unwrap_or(run_no),
                status: row.get::<&str, _>("Status").unwrap_or("UNKNOWN").to_string(),
                formula_desc: row.get::<&str, _>("FormulaDesc").unwrap_or("").to_string(),
                last_modified,
            };
            
            info!("Found bulk run status: {} - {}", run_no, status_response.status);
            Ok(Some(status_response))
        } else {
            info!("Bulk run status not found: {}", run_no);
            Ok(None)
        }
    }

    /// Get individual batch records for finding the next unpicked batch
    /// Uses primary client to ensure read-after-write consistency
    #[instrument(skip(self))]
    pub async fn get_bulk_run_batches(&self, run_no: i32) -> Result<Vec<BulkPickedItem>> {
        info!("Getting individual batch records for bulk run: {}", run_no);

        // **CRITICAL FIX**: Use primary client to ensure read-after-write consistency
        // This prevents stale data reads immediately after pick confirmation writes
        let mut client = self
            .get_client()
            .await
            .context("Failed to get primary database client for consistent reads")?;
        
        info!("✅ Using primary database connection for read-after-write consistency");

        // **ENHANCED FIX**: Ensure we read most recent committed data
        // This prevents reading stale data from other transaction sessions
        let isolation_query = "SET TRANSACTION ISOLATION LEVEL READ COMMITTED;";
        let isolation_stmt = tiberius::Query::new(isolation_query);
        isolation_stmt.execute(&mut client).await
            .context("Failed to set transaction isolation level")?;
        
        info!("✅ Set READ COMMITTED isolation level for fresh data reads");

        // Individual batch query - returns each batch separately for proper unpicked batch detection
        let query = r#"
            SELECT 
                bp.RunNo,
                bp.RowNum,  -- Individual batch row number for accurate targeting
                bp.LineId,
                bp.ItemKey,
                im.Desc1 as Description,
                bp.Location,
                bp.Unit,
                bp.StandardQty,
                bp.PackSize,
                bp.TopickedStdQty,
                bp.ToPickedBulkQty,
                ISNULL(bp.PickedBulkQty, 0) as PickedBulkQty,
                ISNULL(bp.PickedQty, 0) as PickedQty,
                bp.PickingDate,
                bp.ItemBatchStatus,
                -- Individual batch remaining quantity calculation
                CASE 
                    WHEN bp.ToPickedBulkQty <= 0 THEN 0
                    ELSE bp.ToPickedBulkQty - ISNULL(bp.PickedBulkQty, 0)
                END as RemainingQty,
                -- Individual batch completion status
                CASE 
                    WHEN bp.ToPickedBulkQty <= 0 THEN 'NOT_REQUIRED'
                    WHEN ISNULL(bp.PickedBulkQty, 0) >= bp.ToPickedBulkQty THEN 'COMPLETE'
                    WHEN ISNULL(bp.PickedBulkQty, 0) > 0 THEN 'IN_PROGRESS'
                    ELSE 'PENDING'
                END as CompletionStatus,
                1 as TotalBatches,      -- Each record is one batch
                CASE WHEN bp.PickedBulkQty IS NOT NULL AND bp.PickedBulkQty > 0 THEN 1 ELSE 0 END as CompletedBatches
            FROM cust_BulkPicked bp
            LEFT JOIN INMAST im ON bp.ItemKey = im.Itemkey
            WHERE bp.RunNo = @P1
              AND bp.ToPickedBulkQty > 0  -- Only batches that need picking
            ORDER BY 
                bp.LineId DESC   -- Process ingredients in priority order (matches search modal sorting)
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(run_no);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute batch query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get batch results")?;

        let mut batches = Vec::new();
        let mut parse_errors = 0;
        
        for row in &rows {
            match self.row_to_bulk_picked_item(row) {
                Ok(batch) => {
                    batches.push(batch);
                }
                Err(e) => {
                    parse_errors += 1;
                    warn!(
                        "Failed to parse batch row for run {}: {}. Skipping row.",
                        run_no, e
                    );
                }
            }
        }

        if parse_errors > 0 {
            warn!(
                "Encountered {} parse errors while processing batches for run {}. {} batches successfully parsed.",
                parse_errors, run_no, batches.len()
            );
        }

        info!(
            "Found {} individual batches for run: {} (skipped {} invalid rows)",
            batches.len(),
            run_no,
            parse_errors
        );
        Ok(batches)
    }

    /// Get unique ingredients for ingredient search modal (grouped by ItemKey)
    /// Returns only unique ingredients, aggregating across all batches
    #[instrument(skip(self))]
    pub async fn get_unique_ingredients_for_search(&self, run_no: i32) -> Result<Vec<BulkPickedItem>> {
        info!("Getting unique ingredients for search in bulk run: {}", run_no);

        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        // Aggregate query to get unique ingredients for search modal
        let query = r#"
            SELECT 
                bp.RunNo,
                MIN(bp.RowNum) as RowNum,
                MIN(bp.LineId) as LineId,
                bp.ItemKey,
                MIN(im.Desc1) as Description,
                MIN(bp.Location) as Location,
                MIN(bp.Unit) as Unit,
                MIN(bp.StandardQty) as StandardQty,
                MIN(bp.PackSize) as PackSize,
                MIN(bp.TopickedStdQty) as TopickedStdQty,
                SUM(bp.ToPickedBulkQty) as ToPickedBulkQty,  -- Total bags needed across all batches
                SUM(ISNULL(bp.PickedBulkQty, 0)) as PickedBulkQty,  -- Total bags picked across all batches
                MAX(bp.PickingDate) as PickingDate,
                MIN(bp.ItemBatchStatus) as ItemBatchStatus,
                -- Aggregated remaining calculation (in bags)
                CASE
                    WHEN SUM(bp.ToPickedBulkQty) <= 0 THEN 0
                    ELSE SUM(bp.ToPickedBulkQty) - SUM(ISNULL(bp.PickedBulkQty, 0))
                END as RemainingQty,
                -- Aggregated completion status
                CASE 
                    WHEN SUM(bp.ToPickedBulkQty) <= 0 THEN 'NOT_REQUIRED'
                    WHEN SUM(ISNULL(bp.PickedBulkQty, 0)) >= SUM(bp.ToPickedBulkQty) THEN 'COMPLETE'
                    WHEN SUM(ISNULL(bp.PickedBulkQty, 0)) > 0 THEN 'IN_PROGRESS'
                    ELSE 'PENDING'
                END as CompletionStatus,
                COUNT(*) as TotalBatches,
                SUM(CASE WHEN bp.PickedBulkQty IS NOT NULL AND bp.PickedBulkQty > 0 THEN 1 ELSE 0 END) as CompletedBatches
            FROM cust_BulkPicked bp
            LEFT JOIN INMAST im ON bp.ItemKey = im.Itemkey
            WHERE bp.RunNo = @P1
              AND bp.ToPickedBulkQty > 0  -- Only ingredients that need picking
            GROUP BY bp.ItemKey, bp.RunNo  -- Group by ItemKey to get unique ingredients
            ORDER BY MIN(bp.LineId) DESC  -- Sort by LineId in descending order - CFTOMCH1 first
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(run_no);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute unique ingredients query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get unique ingredients results")?;

        let mut ingredients = Vec::new();
        let mut parse_errors = 0;
        
        for row in &rows {
            match self.row_to_bulk_picked_item(row) {
                Ok(ingredient) => {
                    ingredients.push(ingredient);
                }
                Err(e) => {
                    parse_errors += 1;
                    warn!(
                        "Failed to parse ingredient row for run {}: {}. Skipping row.",
                        run_no, e
                    );
                }
            }
        }

        if parse_errors > 0 {
            warn!("Encountered {} parse errors out of {} total rows", parse_errors, rows.len());
        }

        info!("Found {} unique ingredients for run: {} (skipped {} invalid rows)", ingredients.len(), run_no, parse_errors);
        Ok(ingredients)
    }

    /// Get ingredients for a bulk run from cust_BulkPicked table
    #[instrument(skip(self))]
    pub async fn get_bulk_run_ingredients(&self, run_no: i32) -> Result<Vec<BulkPickedItem>> {
        info!("Getting ingredients for bulk run: {}", run_no);

        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        // FIXED: Individual batch query to show actual ToPickedBulkQty values per batch
        // This resolves the issue where 100.000 quantities were showing as 5.00 due to aggregation
        let query = r#"
            SELECT 
                bp.RunNo,
                bp.RowNum,
                bp.LineId,
                bp.ItemKey,
                im.Desc1 as Description,
                bp.Location,
                bp.Unit,
                bp.StandardQty,
                bp.PackSize,
                bp.TopickedStdQty,
                (bp.ToPickedBulkQty * bp.PackSize) as ToPickedBulkQty,  -- FIXED: Show total weight (bags × pack size) in KG
                (ISNULL(bp.PickedBulkQty, 0) * bp.PackSize) as PickedBulkQty,  -- Also convert picked quantity to weight
                bp.PickingDate,
                bp.ItemBatchStatus,
                -- Individual batch remaining weight calculation
                CASE 
                    WHEN bp.ToPickedBulkQty <= 0 THEN 0
                    ELSE (bp.ToPickedBulkQty * bp.PackSize) - (ISNULL(bp.PickedBulkQty, 0) * bp.PackSize)
                END as RemainingQty,
                -- Individual batch completion status
                CASE 
                    WHEN bp.ToPickedBulkQty <= 0 THEN 'NOT_REQUIRED'
                    WHEN ISNULL(bp.PickedBulkQty, 0) >= bp.ToPickedBulkQty THEN 'COMPLETE'
                    WHEN ISNULL(bp.PickedBulkQty, 0) > 0 THEN 'IN_PROGRESS'
                    ELSE 'PENDING'
                END as CompletionStatus,
                1 as TotalBatches,      -- Each record is one batch
                CASE WHEN bp.PickedBulkQty IS NOT NULL AND bp.PickedBulkQty > 0 THEN 1 ELSE 0 END as CompletedBatches
            FROM cust_BulkPicked bp
            LEFT JOIN INMAST im ON bp.ItemKey = im.Itemkey
            WHERE bp.RunNo = @P1
              AND bp.ToPickedBulkQty > 0  -- Only batches that need picking
            ORDER BY 
                bp.LineId DESC   -- Process ingredients in priority order (matches search modal sorting)
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(run_no);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute ingredients query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get ingredients results")?;

        let mut ingredients = Vec::new();
        let mut parse_errors = 0;
        
        for row in &rows {
            match self.row_to_bulk_picked_item(row) {
                Ok(ingredient) => {
                    ingredients.push(ingredient);
                }
                Err(e) => {
                    parse_errors += 1;
                    warn!(
                        "Failed to parse ingredient row for run {}: {}. Skipping row.",
                        run_no, e
                    );
                    // Continue processing other rows instead of failing completely
                }
            }
        }

        if parse_errors > 0 {
            warn!(
                "Encountered {} parse errors while processing ingredients for run {}. {} ingredients successfully parsed.",
                parse_errors, run_no, ingredients.len()
            );
        }

        info!(
            "Found {} ingredients for run: {} (skipped {} invalid rows)",
            ingredients.len(),
            run_no,
            parse_errors
        );
        Ok(ingredients)
    }

    /// Get individual batch records for a bulk run (non-aggregated)
    /// Used for pallet navigation when we need specific RowNum data
    #[instrument(skip(self))]
    pub async fn get_bulk_run_batches_for_ingredient(
        &self, 
        run_no: i32, 
        item_key: &str
    ) -> Result<Vec<BulkPickedItem>> {
        info!("Getting individual batch records for run: {}, item: {}", run_no, item_key);
        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        // Query for individual batch records without GROUP BY
        let query = r#"
            SELECT 
                bp.RunNo,
                bp.RowNum,
                bp.BatchNo,
                bp.LineId,
                bp.ItemKey,
                im.Desc1 as Description,
                bp.Location,
                bp.Unit,
                bp.StandardQty,
                bp.PackSize,
                bp.TopickedStdQty,
                bp.ToPickedBulkQty,
                ISNULL(bp.PickedBulkQty, 0) as PickedBulkQty,
                bp.PickingDate,
                bp.ItemBatchStatus,
                -- Calculate remaining quantity for this specific batch
                CASE 
                    WHEN bp.ToPickedBulkQty > ISNULL(bp.PickedBulkQty, 0) THEN 
                        bp.ToPickedBulkQty - ISNULL(bp.PickedBulkQty, 0)
                    ELSE 0.0
                END as RemainingQty,
                -- Batch completion status
                CASE 
                    WHEN bp.ToPickedBulkQty <= 0 THEN 'NOT_REQUIRED'
                    WHEN ISNULL(bp.PickedBulkQty, 0) >= bp.ToPickedBulkQty THEN 'COMPLETE'
                    WHEN ISNULL(bp.PickedBulkQty, 0) > 0 THEN 'IN_PROGRESS'
                    ELSE 'PENDING'
                END as CompletionStatus,
                1 as TotalBatches,
                CASE WHEN ISNULL(bp.PickedBulkQty, 0) >= bp.ToPickedBulkQty THEN 1 ELSE 0 END as CompletedBatches
            FROM cust_BulkPicked bp
            LEFT JOIN INMAST im ON bp.ItemKey = im.Itemkey
            WHERE bp.RunNo = @P1
              AND bp.ItemKey = @P2
              AND bp.ToPickedBulkQty > 0  -- Only batches requiring bulk picking
            ORDER BY bp.BatchNo
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(run_no);
        select.bind(item_key);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute batch query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get batch results")?;

        let mut batches = Vec::new();
        let mut parse_errors = 0;
        
        for row in &rows {
            match self.row_to_bulk_picked_item(row) {
                Ok(batch) => {
                    batches.push(batch);
                }
                Err(e) => {
                    parse_errors += 1;
                    warn!("Failed to parse batch row for run {}, item {}: {}", run_no, item_key, e);
                }
            }
        }

        if parse_errors > 0 {
            warn!(
                "Encountered {} parse errors while processing batches for run {}, item {}. {} batches successfully parsed.",
                parse_errors, run_no, item_key, batches.len()
            );
        }

        info!(
            "Found {} individual batches for run: {}, item: {} (skipped {} invalid rows)",
            batches.len(),
            run_no,
            item_key,
            parse_errors
        );
        Ok(batches)
    }

    /// Get comprehensive inventory information for an item from INMAST and INLOC tables
    #[instrument(skip(self))]
    pub async fn get_inventory_info(&self, run_no: i32, item_key: &str) -> Result<InventoryInfo> {
        info!(
            "Getting comprehensive inventory info for run: {}, item: {}",
            run_no, item_key
        );

        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        // Enhanced query with more detailed inventory information using correct column names
        let inventory_query = r#"
            SELECT 
                m.Itemkey as ItemKey,
                m.Desc1 as Description,
                m.Stockuomcode as BaseUOM,
                m.Abckey as Category,
                m.User7 as BulkPackSize,
                m.User8 as BulkPackUOM,
                ISNULL(SUM(l.Qtyonhand), 0) as TotalSOH,
                COUNT(DISTINCT l.Location) as LocationCount,
                MAX(l.ModifyDate) as LatestReceived,
                MIN(l.CreateDate) as EarliestReceived,
                AVG(l.Stdcost) as StandardCost,
                AVG(l.Lstcost) as LastCost,
                AVG(m.Safetystockqty) as SafetyStock,
                CASE 
                    WHEN ISNULL(SUM(l.Qtyonhand), 0) <= ISNULL(AVG(m.Safetystockqty), 0) THEN 'LOW'
                    WHEN ISNULL(SUM(l.Qtyonhand), 0) = 0 THEN 'OUT_OF_STOCK'
                    ELSE 'NORMAL'
                END as StockStatus
            FROM INMAST m
            LEFT JOIN INLOC l ON m.Itemkey = l.Itemkey
            WHERE m.Itemkey = @P1
            GROUP BY m.Itemkey, m.Desc1, m.Stockuomcode, m.Abckey, m.User7, m.User8, m.Safetystockqty
        "#;

        let mut select = TiberiusQuery::new(inventory_query);
        select.bind(item_key);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute inventory query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get inventory results")?;

        if let Some(row) = rows.first() {
            let soh_value: f64 = row.get("TotalSOH").unwrap_or(0.0);
            let base_uom: &str = row.get("BaseUOM").unwrap_or("KG");
            let bulk_pack_size: f64 = row.get("BulkPackSize").unwrap_or(0.0);
            let _bulk_pack_uom_value: f64 = row.get("BulkPackUOM").unwrap_or(0.0);
            let stock_status: &str = row.get("StockStatus").unwrap_or("UNKNOWN");
            let location_count: i32 = row.get("LocationCount").unwrap_or(0);
            let _description: Option<&str> = row.get("Description");

            // Use the Unit field from cust_BulkPicked for bulk pack UOM
            let bulk_pack_uom = base_uom; // Using base UOM as the standard unit

            // Validate and standardize UOM
            let standardized_soh_uom = self.standardize_uom(base_uom);
            let standardized_bulk_uom = self.standardize_uom(bulk_pack_uom);

            // Get available lots with enhanced information
            let lots = self.get_available_lots(run_no, item_key).await?;

            // Log inventory status for monitoring
            if stock_status == "LOW" || stock_status == "OUT_OF_STOCK" {
                warn!(
                    "Inventory alert for {}: {} (SOH: {}, Locations: {})",
                    item_key, stock_status, soh_value, location_count
                );
            }

            Ok(InventoryInfo {
                item_key: item_key.to_string(),
                soh_value: BigDecimal::from_f64(soh_value).unwrap_or_default(),
                soh_uom: standardized_soh_uom,
                bulk_pack_size_value: BigDecimal::from_f64(bulk_pack_size).unwrap_or_default(),
                bulk_pack_size_uom: standardized_bulk_uom,
                available_lots: lots,
            })
        } else {
            warn!("Item not found in inventory: {}", item_key);
            // Return default inventory info with proper error indication
            Ok(InventoryInfo {
                item_key: item_key.to_string(),
                soh_value: BigDecimal::from_i32(0).unwrap(),
                soh_uom: "N/A".to_string(),
                bulk_pack_size_value: BigDecimal::from_i32(0).unwrap(),
                bulk_pack_size_uom: "N/A".to_string(),
                available_lots: vec![],
            })
        }
    }




    /// Get available lots for an item with basic information
    #[instrument(skip(self))]
    pub async fn get_available_lots(&self, run_no: i32, item_key: &str) -> Result<Vec<LotInfo>> {
        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        // Official BME4-compatible FEFO logic (First Expired, First Out) with pack size validation
        // CRITICAL: Uses same PARTIAL bin exclusion logic as lot search modal for consistency
        let lots_query = r#"
            SELECT 
                l.LotNo,
                l.DateExpiry,
                l.QtyOnHand,
                l.QtyCommitSales,
                l.LocationKey,
                l.BinNo,
                l.DateReceived,
                l.VendorLotNo,
                (l.QtyOnHand - l.QtyCommitSales) as AvailableQty,
                CASE 
                    WHEN l.DateExpiry IS NULL THEN 'NO_EXPIRY'
                    WHEN l.DateExpiry < GETDATE() THEN 'EXPIRED'
                    WHEN l.DateExpiry < DATEADD(day, 30, GETDATE()) THEN 'EXPIRING_SOON'
                    ELSE 'GOOD'
                END as LotStatus,
                DATEDIFF(day, GETDATE(), l.DateExpiry) as DaysUntilExpiry
            FROM LotMaster l
            INNER JOIN cust_BulkPicked bp ON l.ItemKey = bp.ItemKey
            INNER JOIN Cust_BulkRun cbr ON bp.RunNo = cbr.RunNo
            INNER JOIN BINMaster b ON l.BinNo = b.BinNo AND l.LocationKey = b.Location  -- Join for PARTIAL filter
            WHERE l.ItemKey = @P1 
              AND cbr.RunNo = @P2
              AND bp.ToPickedBulkQty > 0
              AND l.LocationKey = 'TFC1'
              AND l.QtyOnHand > 0
              AND l.BinNo IS NOT NULL
              AND l.BinNo != ''
              AND (l.LotStatus IN ('P', 'C') OR l.LotStatus IS NULL OR l.LotStatus = '')  -- Include only P (Production), C (Current), NULL, or blank statuses
              AND (l.QtyOnHand - l.QtyCommitSales) >= bp.PackSize  -- Pack size validation (replaces hardcoded 25.0)
              AND b.Nettable = 1  -- Only nettable bins for bulk picking (excludes special purpose bins)
              AND (b.User4 IS NULL OR b.User4 != 'PARTIAL')  -- CRITICAL: Exclude PARTIAL bins (PWBF-04 fix)
              AND (l.DateExpiry IS NULL OR l.DateExpiry >= GETDATE())  -- CRITICAL: Exclude expired lots completely
            ORDER BY 
                -- 1. FEFO: First Expired, First Out (official BME4 behavior)
                l.DateExpiry ASC,
                -- 2. For same expiry date, smallest quantity first (complete FEFO)
                l.QtyOnHand ASC,
                -- 3. Lot number for consistency
                l.LotNo ASC
        "#;

        let mut select = TiberiusQuery::new(lots_query);
        select.bind(item_key);
        select.bind(run_no);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute lots query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get lots results")?;

        let mut lots = Vec::new();
        for row in &rows {
            let lot_no: &str = row.get("LotNo").unwrap_or("");
            let exp_date: Option<tiberius::time::chrono::NaiveDateTime> = row.get("DateExpiry");
            let available_qty: f64 = row.get("AvailableQty").unwrap_or(0.0);
            let location: &str = row.get("LocationKey").unwrap_or("");
            let bin: Option<&str> = row.get("BinNo");
            let lot_status: &str = row.get("LotStatus").unwrap_or("UNKNOWN");
            let days_until_expiry: Option<i32> = row.get("DaysUntilExpiry");


            let expiry_date = exp_date.map(|dt| convert_to_utc(dt.and_utc()));

            // Log lot status warnings
            if lot_status == "EXPIRED" || lot_status == "EXPIRING_SOON" {
                let days_msg = days_until_expiry.map_or("unknown".to_string(), |d| d.to_string());
                warn!(
                    "Lot {} for item {} status: {} (days until expiry: {})",
                    lot_no, location, lot_status, days_msg
                );
            }

            // Use actual available quantity (QtyOnHand - QtyCommitSales) for accurate lot suggestions
            lots.push(LotInfo {
                lot_no: lot_no.to_string(),
                expiry_date,
                available_qty: BigDecimal::from_f64(available_qty).unwrap_or_default(),
                location: location.to_string(),
                bin: bin.map(|s| s.to_string()).or_else(|| Some("".to_string())),
            });
        }

        Ok(lots)
    }

    /// Standardize UOM values to consistent format
    fn standardize_uom(&self, uom: &str) -> String {
        match uom.to_uppercase().trim() {
            "KG" | "KILOGRAM" | "KILOGRAMS" => "KG".to_string(),
            "LB" | "LBS" | "POUND" | "POUNDS" => "LB".to_string(),
            "G" | "GRAM" | "GRAMS" => "G".to_string(),
            "OZ" | "OUNCE" | "OUNCES" => "OZ".to_string(),
            "ROLL" | "ROLLS" => "Roll".to_string(),
            "CASE" | "CASES" => "Case".to_string(),
            "PALLET" | "PALLETS" | "PLT" => "Pallet".to_string(),
            "EACH" | "EA" | "PC" | "PIECE" | "PIECES" => "Each".to_string(),
            "BOX" | "BOXES" => "Box".to_string(),
            "BAG" | "BAGS" => "Bag".to_string(),
            "CARTON" | "CARTONS" => "Carton".to_string(),
            "" | " " => "N/A".to_string(),
            other => other.to_string(),
        }
    }


    /// Convert database row to BulkRun model
    fn row_to_bulk_run(&self, row: &Row) -> Result<BulkRun> {
        let run_no: i32 = row.get("RunNo").unwrap_or(0);
        let batch_no: &str = row.get("BatchNo").unwrap_or("");
        let formula_id: &str = row.get("FormulaId").unwrap_or("");
        let formula_desc: &str = row.get("FormulaDesc").unwrap_or("");
        let no_of_batches: i32 = row.get("NoOfBatches").unwrap_or(1);
        let pallets_per_batch: Option<i32> = row.get("PalletsPerBatch");
        let status: &str = row.get("Status").unwrap_or("UNKNOWN");
        let rec_date: Option<tiberius::time::chrono::NaiveDateTime> = row.get("RecDate");

        let created_date_utc = rec_date.map(|dt| convert_to_utc(dt.and_utc()));

        Ok(BulkRun {
            run_no,
            batch_no: batch_no.to_string(),
            formula_id: formula_id.to_string(),
            formula_desc: formula_desc.to_string(),
            no_of_batches,
            pallets_per_batch,
            status: status.to_string(),
            created_date: created_date_utc,
            picking_date: None, // No picking date in Cust_BulkRun table
        })
    }

    /// Convert database row to BulkPickedItem model
    pub fn row_to_bulk_picked_item(&self, row: &Row) -> Result<BulkPickedItem> {
        // Enhanced error handling with specific context for debugging
        let run_no: i32 = match row.try_get::<i64, _>("RunNo") {
            Ok(Some(val)) => val as i32,
            _ => match row.try_get::<i32, _>("RunNo") {
                Ok(Some(val)) => val,
                _ => {
                    warn!("Missing RunNo field in bulk picked item row");
                    0
                }
            }
        };
        let row_num: i32 = match row.try_get::<i64, _>("RowNum") {
            Ok(Some(val)) => val as i32,
            _ => match row.try_get::<i32, _>("RowNum") {
                Ok(Some(val)) => val,
                _ => {
                    warn!("Missing RowNum field for run {}", run_no);
                    0
                }
            }
        };
        let line_id: i32 = match row.try_get::<i64, _>("LineId") {
            Ok(Some(val)) => val as i32,
            _ => match row.try_get::<i32, _>("LineId") {
                Ok(Some(val)) => val,
                _ => {
                    warn!("Missing LineId field for run {}", run_no);
                    0
                }
            }
        };
        let item_key: &str = row.get("ItemKey").unwrap_or_else(|| {
            warn!("Missing ItemKey field for run {}", run_no);
            ""
        });
        
        // Handle optional fields more carefully
        let description: Option<&str> = row.try_get("Description").ok().flatten();
        let location: Option<&str> = row.try_get("Location").ok().flatten();
        let unit: &str = row.get("Unit").unwrap_or("KG");
        
        // Robust handling of numeric fields with error context
        let standard_qty: f64 = match row.try_get("StandardQty") {
            Ok(val) => val.unwrap_or(0.0),
            Err(e) => {
                warn!("Error parsing StandardQty for run {} item {}: {}", run_no, item_key, e);
                0.0
            }
        };
        let pack_size: f64 = match row.try_get("PackSize") {
            Ok(val) => val.unwrap_or(1.0),
            Err(e) => {
                warn!("Error parsing PackSize for run {} item {}: {}", run_no, item_key, e);
                1.0
            }
        };
        let topicked_std_qty: f64 = match row.try_get("TopickedStdQty") {
            Ok(val) => val.unwrap_or(0.0),
            Err(e) => {
                warn!("Error parsing TopickedStdQty for run {} item {}: {}", run_no, item_key, e);
                0.0
            }
        };
        let to_picked_bulk_qty: f64 = match row.try_get("ToPickedBulkQty") {
            Ok(val) => val.unwrap_or(0.0),
            Err(e) => {
                warn!("Error parsing ToPickedBulkQty for run {} item {}: {}", run_no, item_key, e);
                0.0
            }
        };
        
        // Handle nullable fields gracefully
        let picked_bulk_qty: Option<f64> = row.try_get("PickedBulkQty").ok().flatten();
        let picking_date: Option<tiberius::time::chrono::NaiveDateTime> = 
            row.try_get("PickingDate").ok().flatten();
        let status: Option<&str> = row.try_get("ItemBatchStatus").ok().flatten();

        let picking_date_utc = picking_date.map(|dt| convert_to_utc(dt.and_utc()));

        // NEW: Extract batch tracking fields for completion logic
        let total_batches: Option<i32> = row.try_get("TotalBatches").ok().flatten();
        let completed_batches: Option<i32> = row.try_get("CompletedBatches").ok().flatten();
        let remaining_qty: Option<f64> = row.try_get("RemainingQty").ok().flatten();
        let completion_status: Option<&str> = row.try_get("CompletionStatus").ok().flatten();

        Ok(BulkPickedItem {
            run_no,
            row_num,
            line_id,
            item_key: item_key.to_string(),
            description: description.map(|s| s.to_string()),
            location: location.map(|s| s.to_string()),
            standard_qty: BigDecimal::from_f64(standard_qty).unwrap_or_default(),
            pack_size: BigDecimal::from_f64(pack_size).unwrap_or_default(),
            uom: unit.to_string(),
            to_picked_std_qty: BigDecimal::from_f64(topicked_std_qty).unwrap_or_default(),
            to_picked_bulk_qty: BigDecimal::from_f64(to_picked_bulk_qty).unwrap_or_default(),
            picked_bulk_qty: picked_bulk_qty
                .map(|qty| BigDecimal::from_f64(qty).unwrap_or_default()),
            picking_date: picking_date_utc,
            status: status.map(|s| s.to_string()),
            // NEW: Batch tracking fields for accurate completion detection
            total_batches,
            completed_batches,
            remaining_qty: remaining_qty.map(|qty| BigDecimal::from_f64(qty).unwrap_or_default()),
            completion_status: completion_status.map(|s| s.to_string()),
        })
    }

    /// Convert database row to BulkRunSummary model for modal display
    fn row_to_bulk_run_summary(&self, row: &Row) -> Result<BulkRunSummary> {
        let run_no: i32 = row.get("RunNo").unwrap_or(0);
        let formula_id: &str = row.get("FormulaId").unwrap_or("");
        let formula_desc: &str = row.get("FormulaDesc").unwrap_or("");
        let status: &str = row.get("Status").unwrap_or("UNKNOWN");
        let batch_count: i32 = row.get("BatchCount").unwrap_or(0);

        Ok(BulkRunSummary {
            run_no,
            formula_id: formula_id.to_string(),
            formula_desc: formula_desc.to_string(),
            status: status.to_string(),
            batch_count,
        })
    }

    /// Search lots for a specific run and item key for lot selection modal
    #[instrument(skip(self))]
    pub async fn search_lots_for_run_item(
        &self,
        run_no: i32,
        item_key: &str,
    ) -> Result<Vec<LotSearchResult>> {
        info!(
            "Searching lots for run: {}, item_key: {}",
            run_no, item_key
        );

        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        // Enhanced query with PackSize from cust_BulkPicked - matches paginated version
        let query = format!(
            r#"
            SELECT DISTINCT
                l.LotNo,
                l.BinNo,
                l.DateExpiry,
                l.QtyOnHand,
                l.QtyIssued,
                l.QtyCommitSales as CommitedQty,
                (l.QtyOnHand - l.QtyCommitSales) as AvailableQty,
                FLOOR((l.QtyOnHand - l.QtyCommitSales) / bp.PackSize) as AvailableBags,
                bp.PackSize,
                l.ItemKey,
                l.LocationKey,
                CASE
                    WHEN l.BinNo LIKE 'A%-%' THEN 5    -- A-zone (highest priority)
                    WHEN l.BinNo LIKE 'K%-%' THEN 4    -- K-zone storage
                    WHEN l.BinNo LIKE 'TPJS%' THEN 3   -- Special picking
                    ELSE 2                             -- Other bins
                END as BinPriority
            FROM LotMaster l
            INNER JOIN BINMaster b ON l.BinNo = b.BinNo AND l.LocationKey = b.Location
            INNER JOIN cust_BulkPicked bp ON l.ItemKey = bp.ItemKey AND bp.RunNo = {run_no}
            WHERE l.ItemKey = '{item_key}'
                AND l.LocationKey = 'TFC1'
                AND l.QtyOnHand > 0
                AND (l.LotStatus IN ('P', 'C') OR l.LotStatus IS NULL OR l.LotStatus = '')  -- Include only P (Production), C (Current), NULL, or blank statuses
                AND (l.QtyOnHand - l.QtyCommitSales) > 0                    -- Available inventory only
                AND l.QtyOnHand >= bp.PackSize                              -- PackSize minimum threshold validation
                AND FLOOR((l.QtyOnHand - l.QtyCommitSales) / bp.PackSize) >= 1  -- Must have at least 1 available bag
                AND (b.User4 IS NULL OR b.User4 != 'PARTIAL')              -- Exclude partial picking bins
                AND l.BinNo NOT LIKE '%Variance'                           -- Exclude variance bins
                AND b.User1 NOT LIKE '%WHTIP8%'                            -- Exclude special bins
                AND (l.DateExpiry IS NULL OR l.DateExpiry >= GETDATE())    -- CRITICAL: Exclude expired lots completely
            ORDER BY
                l.DateExpiry ASC,              -- FEFO: First Expired, First Out
                BinPriority DESC,              -- A-zone first, then K-zone
                (l.QtyOnHand - l.QtyCommitSales) ASC,    -- Smaller available quantity first
                l.LotNo ASC                    -- Consistent ordering
        "#
        );

        let select = TiberiusQuery::new(&query);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute lot search query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get lot search results")?;

        info!("Found {} lots for run {}, item {}", rows.len(), run_no, item_key);

        let mut lots = Vec::new();
        for row in rows {
            match self.row_to_lot_search_result(&row) {
                Ok(lot) => lots.push(lot),
                Err(e) => {
                    warn!("Failed to convert row to lot result: {}", e);
                    continue;
                }
            }
        }

        Ok(lots)
    }

    /// Search lots for a specific run and item key with pagination support
    #[instrument(skip(self))]
    pub async fn search_lots_for_run_item_paginated(
        &self,
        run_no: i32,
        item_key: &str,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<LotSearchResult>, u64)> {
        info!(
            "Searching lots paginated for run: {}, item_key: {}, page: {}, size: {}",
            run_no, item_key, page, page_size
        );

        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        // Enhanced count query with available bags validation
        let count_query = r#"
            SELECT COUNT(DISTINCT CONCAT(l.LotNo, '|', l.BinNo)) as total_count
            FROM LotMaster l
            INNER JOIN BINMaster b ON l.BinNo = b.BinNo AND l.LocationKey = b.Location
            INNER JOIN cust_BulkPicked bp ON l.ItemKey = bp.ItemKey AND bp.RunNo = @P2
            WHERE l.ItemKey = @P1
                AND l.LocationKey = 'TFC1'
                AND l.QtyOnHand > 0
                AND (l.LotStatus IN ('P', 'C') OR l.LotStatus IS NULL OR l.LotStatus = '')  -- Include only P (Production), C (Current), NULL, or blank statuses
                AND (l.QtyOnHand - l.QtyCommitSales) > 0                    -- Available inventory only
                AND l.QtyOnHand >= bp.PackSize                              -- PackSize minimum threshold validation
                AND FLOOR((l.QtyOnHand - l.QtyCommitSales) / bp.PackSize) >= 1  -- Must have at least 1 available bag
                AND (b.User4 IS NULL OR b.User4 != 'PARTIAL')              -- Exclude partial picking bins
                AND l.BinNo NOT LIKE '%Variance'                           -- Exclude variance bins
                AND b.User1 NOT LIKE '%WHTIP8%'                            -- Exclude special bins
                AND (l.DateExpiry IS NULL OR l.DateExpiry >= GETDATE())    -- CRITICAL: Exclude expired lots completely
        "#;

        let mut count_select = TiberiusQuery::new(count_query);
        count_select.bind(item_key);
        count_select.bind(run_no);
        let count_stream = count_select
            .query(&mut client)
            .await
            .context("Failed to execute count query")?;

        let count_rows: Vec<Row> = count_stream
            .into_first_result()
            .await
            .context("Failed to get count results")?;

        let total_count: i32 = count_rows
            .first()
            .and_then(|row| row.get("total_count"))
            .unwrap_or(0);
        let total_count_u64 = total_count as u64;

        // Calculate offset for pagination
        let offset = (page.saturating_sub(1)) * page_size;

        // Enhanced paginated query with available bags calculation and filtering
        let query = r#"
            SELECT DISTINCT
                l.LotNo,
                l.BinNo,
                l.DateExpiry,
                l.QtyOnHand,
                l.QtyIssued,
                l.QtyCommitSales as CommitedQty,
                (l.QtyOnHand - l.QtyCommitSales) as AvailableQty,
                FLOOR((l.QtyOnHand - l.QtyCommitSales) / bp.PackSize) as AvailableBags,
                bp.PackSize,
                l.ItemKey,
                l.LocationKey,
                CASE 
                    WHEN l.BinNo LIKE 'A%-%' THEN 5    -- A-zone (highest priority)
                    WHEN l.BinNo LIKE 'K%-%' THEN 4    -- K-zone storage
                    WHEN l.BinNo LIKE 'TPJS%' THEN 3   -- Special picking
                    ELSE 2                             -- Other bins
                END as BinPriority
            FROM LotMaster l
            INNER JOIN BINMaster b ON l.BinNo = b.BinNo AND l.LocationKey = b.Location
            INNER JOIN cust_BulkPicked bp ON l.ItemKey = bp.ItemKey AND bp.RunNo = @P4
            WHERE l.ItemKey = @P1
                AND l.LocationKey = 'TFC1'
                AND l.QtyOnHand > 0
                AND (l.LotStatus IN ('P', 'C') OR l.LotStatus IS NULL OR l.LotStatus = '')  -- Include only P (Production), C (Current), NULL, or blank statuses
                AND (l.QtyOnHand - l.QtyCommitSales) > 0                    -- Available inventory only
                AND l.QtyOnHand >= bp.PackSize                              -- PackSize minimum threshold validation
                AND FLOOR((l.QtyOnHand - l.QtyCommitSales) / bp.PackSize) >= 1  -- Must have at least 1 available bag
                AND (b.User4 IS NULL OR b.User4 != 'PARTIAL')              -- Exclude partial picking bins
                AND l.BinNo NOT LIKE '%Variance'                           -- Exclude variance bins
                AND b.User1 NOT LIKE '%WHTIP8%'                            -- Exclude special bins
                AND (l.DateExpiry IS NULL OR l.DateExpiry >= GETDATE())    -- CRITICAL: Exclude expired lots completely
            ORDER BY 
                l.DateExpiry ASC,              -- FEFO: First Expired, First Out
                BinPriority DESC,              -- A-zone first, then K-zone
                l.QtyOnHand DESC,              -- BME4 FEFO: Larger quantity first
                l.LotNo ASC                    -- Consistent ordering
            OFFSET @P2 ROWS
            FETCH NEXT @P3 ROWS ONLY
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(item_key);       // @P1
        select.bind(offset as i32);  // @P2
        select.bind(page_size as i32); // @P3
        select.bind(run_no);         // @P4
        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute paginated lot search query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get paginated lot search results")?;

        info!("Found {} lots on page {} for run {}, item {}", rows.len(), page, run_no, item_key);

        let mut lots = Vec::new();
        for row in rows {
            match self.row_to_lot_search_result(&row) {
                Ok(lot) => lots.push(lot),
                Err(e) => {
                    warn!("Failed to convert row to lot result: {}", e);
                    continue;
                }
            }
        }

        Ok((lots, total_count_u64))
    }

    /// Get available bins for a specific lot number within a run
    /// Used for barcode scanning workflow when user scans a lot number
    #[instrument(skip(self))]
    pub async fn get_bins_for_lot(
        &self,
        run_no: i32,
        lot_no: &str,
        item_key: &str,
    ) -> Result<Vec<LotSearchResult>> {
        info!(
            "Getting bins for lot: {} in run: {}, item_key: {}",
            lot_no, run_no, item_key
        );

        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        // Query to get all bins for the specific lot number with consistent pack size validation
        // Validates lot exists for the run's ingredient and returns bin information
        // Use DISTINCT to remove duplicates from formula joins while preserving A-zone priority order
        let query = r#"
            SELECT DISTINCT
                l.LotNo,
                l.BinNo,
                l.DateExpiry,
                l.QtyOnHand,
                l.QtyIssued,
                l.QtyCommitSales as CommitedQty,
                (l.QtyOnHand - l.QtyCommitSales) as AvailableQty,
                FLOOR((l.QtyOnHand - l.QtyCommitSales) / bp.PackSize) as AvailableBags,
                bp.PackSize,
                l.ItemKey,
                l.LocationKey,
                -- Add bin priority as a field so it can be used in ORDER BY with DISTINCT
                CASE 
                    WHEN l.BinNo = 'WH Variance' THEN 0                    -- Avoid variance bins
                    WHEN l.BinNo LIKE 'A%-%' THEN 5                        -- A-zone picking bins (highest priority)
                    WHEN l.QtyOnHand >= 500 AND l.BinNo LIKE 'K%-%' THEN 3    -- K-zone storage with high quantity
                    WHEN l.QtyOnHand >= 100 THEN 2                         -- Medium quantity bins  
                    WHEN l.QtyOnHand >= 25 THEN 1                          -- Low quantity bins
                    ELSE 0                                                  -- Very low quantity bins
                END as BinPriority,
                -- Add FEFO sorting fields for DISTINCT compatibility
                CASE 
                    WHEN l.DateExpiry IS NULL THEN 1
                    ELSE 0
                END as ExpiryNullSort
            FROM LotMaster l
            INNER JOIN FMItem fm ON l.ItemKey = fm.ItemKey
            INNER JOIN Cust_BulkRun cbr ON fm.FormulaID = cbr.FormulaId
            INNER JOIN cust_BulkPicked bp ON cbr.RunNo = bp.RunNo AND l.ItemKey = bp.ItemKey AND bp.ToPickedBulkQty > 0
            INNER JOIN BINMaster b ON l.BinNo = b.BinNo AND l.LocationKey = b.Location
            WHERE cbr.RunNo = @P1 
                AND l.LotNo = @P2
                AND l.ItemKey = @P3
                AND l.LocationKey = 'TFC1'
                AND l.QtyOnHand > 0
                AND l.LotStatus IN ('P', 'B', 'C')
                AND (b.Nettable = 0 OR l.BinNo LIKE 'K%-%')  -- Include physical bins and K-zone storage
                AND (l.QtyOnHand - l.QtyCommitSales) > 0
                AND (l.DateExpiry IS NULL OR l.DateExpiry >= GETDATE())    -- CRITICAL: Exclude expired lots completely
                AND (l.QtyOnHand - l.QtyCommitSales) >= bp.PackSize  -- Pack size validation (consistent with lot search)
                AND FLOOR((l.QtyOnHand - l.QtyCommitSales) / bp.PackSize) >= 1  -- Must have at least 1 available bag
                AND l.BinNo NOT LIKE '%Variance'            -- Exclude variance bins from bulk operations
                AND l.BinNo NOT LIKE 'PWBB-%'               -- Exclude PWBB staging/replenishment areas
                AND l.BinNo NOT LIKE 'PWBA-%'               -- Exclude PWBA staging/replenishment areas  
                AND l.BinNo NOT LIKE 'PWBE-%'               -- Exclude PWBE staging/replenishment areas (consistent with lot search)
                AND b.User1 NOT LIKE '%WHTIP8%'             -- Exclude WHTIP8 satellite warehouse for bulk picking
            ORDER BY 
                -- 1. FEFO: First Expired, First Out (earliest expiry first)
                CASE 
                    WHEN l.DateExpiry IS NULL THEN 1
                    ELSE 0
                END,
                l.DateExpiry ASC,
                -- 2. Smallest quantity first for same expiry date (proper FEFO)
                l.QtyOnHand ASC,
                -- 3. Bin priority for storage efficiency (A-zone > I-zone > K-zone)
                BinPriority DESC,
                -- 4. Bin number for consistency
                l.BinNo ASC
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(run_no);
        select.bind(lot_no);
        select.bind(item_key);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute lot bins query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get lot bins results")?;

        let mut lots = Vec::new();
        for row in &rows {
            match self.row_to_lot_search_result(row) {
                Ok(lot) => lots.push(lot),
                Err(e) => {
                    warn!("Failed to convert row to LotSearchResult: {}", e);
                }
            }
        }

        info!(
            "Found {} available bins for lot: {} in run: {}",
            lots.len(), lot_no, run_no
        );

        Ok(lots)
    }

    /// Convert database row to LotSearchResult model
    fn row_to_lot_search_result(&self, row: &Row) -> Result<LotSearchResult> {
        let lot_no: &str = row.get("LotNo").unwrap_or("");
        let bin_no: &str = row.get("BinNo").unwrap_or("");
        let qty_on_hand: f64 = row.get("QtyOnHand").unwrap_or(0.0);
        let date_exp: Option<tiberius::time::chrono::NaiveDateTime> = row.get("DateExpiry");
        let qty_issue: f64 = row.get("QtyIssued").unwrap_or(0.0);
        let committed_qty: f64 = row.get("CommitedQty").unwrap_or(0.0);
        let available_qty: f64 = row.get("AvailableQty").unwrap_or(0.0);
        let available_bags: i32 = row.get::<f64, _>("AvailableBags").unwrap_or(0.0) as i32;
        let pack_size: f64 = row.get("PackSize").unwrap_or(1.0);
        let item_key: &str = row.get("ItemKey").unwrap_or("");
        let location_key: &str = row.get("LocationKey").unwrap_or("");

        // Convert NaiveDateTime to UTC DateTime
        let date_exp_utc = date_exp.map(|dt| convert_to_utc(dt.and_utc()));

        Ok(LotSearchResult {
            lot_no: lot_no.to_string(),
            bin_no: bin_no.to_string(),
            qty_on_hand: BigDecimal::from_f64(qty_on_hand).unwrap_or_default(),
            date_exp: date_exp_utc.unwrap_or_else(|| convert_to_utc(chrono::Utc::now())),
            qty_issue: BigDecimal::from_f64(qty_issue).unwrap_or_default(),
            committed_qty: BigDecimal::from_f64(committed_qty).unwrap_or_default(),
            available_qty: BigDecimal::from_f64(available_qty).unwrap_or_default(),
            available_bags,
            pack_size: BigDecimal::from_f64(pack_size).unwrap_or_default(),
            item_key: item_key.to_string(),
            location_key: location_key.to_string(),
        })
    }

    /// Get current ingredient for a bulk run (used for pallet calculation)
    /// Returns the first ingredient with remaining quantity to pick
    async fn get_current_ingredient_for_run(&self, run_no: i32) -> Result<BulkPickedItem> {
        let ingredients = self.get_bulk_run_ingredients(run_no).await
            .context("Failed to get ingredients for current ingredient lookup")?;

        // Find first ingredient with ToPickedBulkQty > 0 that needs picking
        let current_ingredient = ingredients.iter()
            .find(|ingredient| {
                ingredient.to_picked_bulk_qty > BigDecimal::from(0) &&
                ingredient.to_picked_bulk_qty > *ingredient.picked_bulk_qty.as_ref().unwrap_or(&BigDecimal::from(0))
            })
            .or_else(|| ingredients.first())
            .ok_or_else(|| anyhow::anyhow!("No ingredients available for run {}", run_no))?;

        Ok(current_ingredient.clone())
    }

    /// Get pallet tracking data for a bulk run
    /// Uses read database (TFCPILOT3) for real-time production data
    #[instrument(skip(self))]
    pub async fn get_pallet_tracking_data(&self, run_no: i32, item_key: Option<&str>) -> Result<Vec<PalletBatch>> {
        info!("Getting pallet tracking data for run: {}, item_key: {:?}", run_no, item_key);

        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        // Use provided item_key or fall back to current ingredient
        let target_item_key = if let Some(key) = item_key {
            key.to_string()
        } else {
            let current_ingredient = self.get_current_ingredient_for_run(run_no).await
                .context("Failed to get current ingredient for pallet calculation")?;
            current_ingredient.item_key
        };

        // Query cust_BulkPicked with actual picked quantities aggregated from Cust_BulkLotPicked
        let query = r#"
            SELECT 
                ROW_NUMBER() OVER (ORDER BY bp.BatchNo) as PalletNumber,
                bp.BatchNo as BatchNumber,
                bp.RowNum as RowNum,
                -- Bags picked per batch (aggregate from actual lot picks)
                COALESCE(picked.ActualPickedBags, 0) as NoOfBagsPicked,
                -- Quantity picked per batch (actual picked bags × pack size)
                COALESCE(picked.ActualPickedBags, 0) * bp.PackSize as QuantityPicked,
                -- Bags remaining per batch (ToPickedBulkQty - actual picked)
                bp.ToPickedBulkQty - COALESCE(picked.ActualPickedBags, 0) as NoOfBagsRemaining,
                -- Quantity remaining per batch (remaining bags × pack size)
                (bp.ToPickedBulkQty - COALESCE(picked.ActualPickedBags, 0)) * bp.PackSize as QuantityRemaining
            FROM cust_BulkPicked bp
            LEFT JOIN (
                -- Aggregate actual picked quantities from individual bin records
                SELECT 
                    blp.RunNo,
                    blp.RowNum,
                    blp.LineId,
                    SUM(COALESCE(blp.QtyReceived, 0) / bp2.PackSize) as ActualPickedBags
                FROM Cust_BulkLotPicked blp
                INNER JOIN cust_BulkPicked bp2 ON bp2.RunNo = blp.RunNo 
                                              AND bp2.RowNum = blp.RowNum 
                                              AND bp2.LineId = blp.LineId
                WHERE blp.RunNo = @P1
                  AND bp2.ItemKey = @P2
                GROUP BY blp.RunNo, blp.RowNum, blp.LineId
            ) picked ON picked.RunNo = bp.RunNo 
                     AND picked.RowNum = bp.RowNum 
                     AND picked.LineId = bp.LineId
            WHERE bp.RunNo = @P1
              AND bp.ItemKey = @P2
            ORDER BY bp.BatchNo
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(run_no);
        select.bind(&target_item_key);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute pallet tracking query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get pallet tracking results")?;

        let mut pallets = Vec::new();
        for row in &rows {
            match self.row_to_pallet_batch(row) {
                Ok(pallet) => pallets.push(pallet),
                Err(e) => {
                    warn!("Failed to convert row to PalletBatch: {}", e);
                }
            }
        }

        info!(
            "Found {} pallet batches for run: {}",
            pallets.len(),
            run_no
        );

        Ok(pallets)
    }

    /// **TASK 2: 5-Table Atomic Transaction Pattern** 
    /// Confirm pick operation matching official BME4 workflow exactly
    /// Updates: cust_BulkPicked, Cust_BulkLotPicked, LotMaster, LotTransaction, Cust_BulkPalletLotPicked
    /// Execute a transaction with retry mechanism for concurrency conflicts
    async fn execute_with_retry<F, Fut, T>(&self, operation: F, max_retries: u32, operation_name: &str) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut attempt = 0;
        let mut last_error = None;
        
        while attempt <= max_retries {
            if attempt > 0 {
                let delay_ms = 2u64.pow(attempt.min(6)); // Exponential backoff: 2, 4, 8, 16, 32, 64 ms max
                info!("🔄 RETRY: Attempt {} for {} after {}ms delay", attempt + 1, operation_name, delay_ms);
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }
            
            match operation().await {
                Ok(result) => {
                    if attempt > 0 {
                        info!("✅ RETRY_SUCCESS: {} succeeded on attempt {}", operation_name, attempt + 1);
                    }
                    return Ok(result);
                }
                Err(e) => {
                    let error_msg = e.to_string().to_lowercase();
                    let is_retryable = error_msg.contains("deadlock") 
                        || error_msg.contains("timeout") 
                        || error_msg.contains("lock") 
                        || error_msg.contains("primary key") 
                        || error_msg.contains("duplicate key")
                        || error_msg.contains("constraint violation");
                        
                    if !is_retryable || attempt >= max_retries {
                        warn!("❌ RETRY_EXHAUSTED: {} failed after {} attempts: {}", operation_name, attempt + 1, e);
                        return Err(e);
                    }
                    
                    warn!("⚠️ RETRY: {} failed on attempt {}, will retry: {}", operation_name, attempt + 1, e);
                    last_error = Some(e);
                    attempt += 1;
                }
            }
        }
        
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Max retries exceeded for {}", operation_name)))
    }

    #[instrument(skip(self))]
    pub async fn confirm_pick_transaction(
        &self,
        run_no: i32,
        request: &PickConfirmationRequest,
    ) -> Result<PickConfirmationResponse> {
        info!(
            "🚀 TRANSACTION_START: BME4-compatible pick confirmation with retry mechanism for run: {}, lot: {}, qty: {}",
            run_no, request.lot_no, request.picked_bulk_qty
        );
        
        // **CONCURRENCY-SAFE TRANSACTION** - Execute with retry mechanism for database conflicts
        let operation = || async {
            self.execute_pick_transaction_internal(run_no, request).await
        };
        
        self.execute_with_retry(operation, 3, "pick_confirmation_transaction").await
    }
    
    #[instrument(skip(self))]
    async fn execute_pick_transaction_internal(
        &self,
        run_no: i32,
        request: &PickConfirmationRequest,
    ) -> Result<PickConfirmationResponse> {
        info!(
            "🚀 TRANSACTION_START: BME4-compatible pick confirmation for run: {}, lot: {}, qty: {}, row_num: {}, line_id: {}, bin_no: {}",
            run_no, request.lot_no, request.picked_bulk_qty, request.row_num, request.line_id, request.bin_no
        );

        // **CRITICAL FIX**: Use separate connections for sequence generation vs main transaction
        // This prevents SQL Server error 266 caused by implicit transaction contexts
        info!("🔍 CONNECTION_SEPARATION: Creating separate connections for sequences vs transaction");
        
        // Connection 1: For sequence generation (auto-commit, disposable)
        let mut sequence_client = self.get_client().await.map_err(|e| {
            let error_msg = format!("Failed to connect to TFCPILOT3 for sequence generation: {e}");
            error!("🔌 SEQUENCE_CONNECTION_FAILED: {}", error_msg);
            anyhow::anyhow!("SEQUENCE_DATABASE_CONNECTION_FAILED: {}", error_msg)
        })?;
        
        info!("✅ CONNECTION_SEPARATION: Sequence generation client connected");
        
        info!(
            "🚀 TRANSACTION_VALIDATED: Starting BME4-compatible pick confirmation for run: {}, lot: {}, qty: {}, row_num: {}, line_id: {}, bin_no: {}",
            run_no, request.lot_no, request.picked_bulk_qty, request.row_num, request.line_id, request.bin_no
        );

        // **ENHANCED ERROR RECOVERY**: Pre-validation to catch issues early
        info!("🔍 DEBUG: Pre-validation - Checking request parameters");
        if request.lot_no.is_empty() || request.bin_no.is_empty() {
            let error_msg = format!("Invalid request parameters - lot_no: '{}', bin_no: '{}'", request.lot_no, request.bin_no);
            warn!("❌ DEBUG: {}", error_msg);
            return Err(anyhow::anyhow!("VALIDATION_ERROR: {}", error_msg));
        }
        if request.picked_bulk_qty <= BigDecimal::from(0) {
            let error_msg = format!("Invalid picked quantity: {}", request.picked_bulk_qty);
            warn!("❌ DEBUG: {}", error_msg);
            return Err(anyhow::anyhow!("VALIDATION_ERROR: {}", error_msg));
        }

        // **BATCH COMPLETION VALIDATION**: Check if this batch is already completed  
        info!("🔍 BATCH_VALIDATION: Checking if batch is already completed for run: {}, row_num: {}, line_id: {}", 
              run_no, request.row_num, request.line_id);
        
        // Use separate connection for validation to avoid transaction conflicts
        let mut validation_client = self.get_client().await.map_err(|e| {
            let error_msg = format!("Failed to connect for validation: {e}");
            error!("🔌 VALIDATION_CONNECTION_FAILED: {}", error_msg);
            anyhow::anyhow!("VALIDATION_DATABASE_CONNECTION_FAILED: {}", error_msg)
        })?;
        
        let validation_query = r#"
            SELECT 
                ToPickedBulkQty, 
                ISNULL(PickedBulkQty, 0) as PickedBulkQty,
                ToPickedBulkQty - ISNULL(PickedBulkQty, 0) as RemainingQty,
                ItemKey
            FROM cust_BulkPicked 
            WHERE RunNo = @P1 AND RowNum = @P2 AND LineId = @P3
        "#;
        
        let mut validation_select = TiberiusQuery::new(validation_query);
        validation_select.bind(run_no);
        validation_select.bind(request.row_num);
        validation_select.bind(request.line_id);
        
        let validation_stream = validation_select.query(&mut validation_client).await
            .context("Failed to execute batch validation query")?;
        
        let validation_rows: Vec<Row> = validation_stream.into_first_result().await
            .context("Failed to get batch validation results")?;
        
        if let Some(row) = validation_rows.first() {
            let to_picked_qty: f64 = row.get("ToPickedBulkQty").unwrap_or(0.0);
            let picked_qty: f64 = row.get("PickedBulkQty").unwrap_or(0.0);
            let remaining_qty: f64 = row.get("RemainingQty").unwrap_or(0.0);
            let item_key: &str = row.get("ItemKey").unwrap_or("");
            
            // Enhanced debugging with raw column values
            let raw_to_picked: Option<f64> = row.try_get("ToPickedBulkQty").ok().flatten();
            let raw_picked: Option<f64> = row.try_get("PickedBulkQty").ok().flatten();
            let raw_remaining: Option<f64> = row.try_get("RemainingQty").ok().flatten();
            
            info!("📊 BATCH_VALIDATION: Enhanced debugging for run={}, row_num={}, line_id={}", 
                  run_no, request.row_num, request.line_id);
            info!("📊 BATCH_VALIDATION: Raw values - ToPickedBulkQty: {:?}, PickedBulkQty: {:?}, RemainingQty: {:?}",
                  raw_to_picked, raw_picked, raw_remaining);
            info!("📊 BATCH_VALIDATION: Calculated values - ItemKey: {}, ToPickedQty: {}, PickedQty: {}, RemainingQty: {}",
                  item_key, to_picked_qty, picked_qty, remaining_qty);
            
            // CRITICAL FIX: Handle NULL PickedBulkQty properly in remaining quantity calculation
            let actual_remaining_qty = if raw_picked.is_none() {
                // If PickedBulkQty is NULL, this means the batch is unpicked
                // So remaining should equal ToPickedBulkQty
                to_picked_qty
            } else {
                // If PickedBulkQty has a value, use the calculated remaining
                remaining_qty
            };
            
            info!("📊 BATCH_VALIDATION: CORRECTED remaining quantity: {} (was {})", 
                  actual_remaining_qty, remaining_qty);
            
            if actual_remaining_qty <= 0.0 {
                // Log detailed context, but return a concise, user-friendly error
                warn!(
                    "❌ BATCH_ALREADY_COMPLETED: run={}, row_num={}, line_id={}, item_key={}, to_picked={}, picked={}, remaining={}, corrected_remaining={}",
                    run_no, request.row_num, request.line_id, item_key, to_picked_qty, picked_qty, remaining_qty, actual_remaining_qty
                );
                let user_friendly = "This batch is already completed. Please refresh to load the next batch.";
                return Err(anyhow::anyhow!("BATCH_ALREADY_COMPLETED: {}", user_friendly));
            }
            
            // Update remaining quantity for subsequent validation
            let remaining_for_validation = actual_remaining_qty;
            
            // Validate picked quantity doesn't exceed remaining
            let requested_qty = self.safe_bigdecimal_to_f64(&request.picked_bulk_qty, "validation_picked_qty")?;
            // ROUNDING FIX: Use 3-decimal rounding to prevent floating-point precision issues
            let requested_qty_rounded = self.round_to_3_decimals(requested_qty);
            let remaining_rounded = self.round_to_3_decimals(remaining_for_validation);
            if self.qty_greater_than(requested_qty_rounded, remaining_rounded) {
                let error_msg = format!(
                    "Cannot pick {} bags. Only {} bags remaining in this batch (ItemKey: {}, Batch: RowNum={}, LineId={})",
                    requested_qty_rounded, remaining_rounded, item_key, request.row_num, request.line_id
                );
                warn!("❌ BATCH_VALIDATION: {}", error_msg);
                return Err(anyhow::anyhow!("INSUFFICIENT_BATCH_QUANTITY: {}", error_msg));
            }
            
            info!("✅ BATCH_VALIDATION: Batch validation passed - can pick {} bags from {} remaining (corrected from {})", 
                  requested_qty_rounded, remaining_rounded, remaining_qty);
        } else {
            let error_msg = format!("Batch not found: run={}, row_num={}, line_id={}", 
                                    run_no, request.row_num, request.line_id);
            warn!("❌ BATCH_VALIDATION: {}", error_msg);
            return Err(anyhow::anyhow!("BATCH_NOT_FOUND: {}", error_msg));
        }

        // With new architecture using TFCPILOT3 as primary, sync is no longer needed
        info!("✅ DEBUG: Using TFCPILOT3 as primary database - no sync required");

        // Client already obtained above for validation, reuse it for transaction
        info!("✅ DEBUG: Reusing primary database connection for transaction");

        let mut summary = TransactionSummary {
            bulk_picked_updated: false,
            lot_picked_created: false,
            lot_master_updated: false,
            lot_transaction_created: false,
            pallet_lot_picked_created: false,
            total_committed_qty: BigDecimal::from(0),
        };

        // Get current batch information for calculations
        info!("🔍 DEBUG: Getting batch info for run: {}, row_num: {}, line_id: {}", run_no, request.row_num, request.line_id);
        let batch_info = self.get_batch_info_for_pick(&mut validation_client, run_no, request).await
            .context("CRITICAL: Failed to get batch info for pick calculation")?;
        info!("✅ DEBUG: Batch info retrieved - batch_no: {}, item_key: {}, pack_size: {}", 
              batch_info.batch_no, batch_info.item_key, batch_info.pack_size);
        
        let picked_qty = &request.picked_bulk_qty * &batch_info.pack_size;
        info!("📊 DEBUG: Calculated picked_qty: {} (bulk_qty: {} * pack_size: {})", 
              picked_qty, request.picked_bulk_qty, batch_info.pack_size);
        
        // Convert pack_size to f64 for database insertion
        let pack_size_f64 = self.safe_bigdecimal_to_f64(&batch_info.pack_size, "pack_size_conversion")?;
        info!("🔧 DEBUG: Converted pack_size to f64: {} for database insertion", pack_size_f64);

        // **BME4-COMPATIBLE QUANTITY VALIDATION**: Check picked quantity against requirement
        info!("🔍 DEBUG: BME4 quantity validation - Getting current requirements");
        let required_qty = self.get_required_quantity_for_batch(&mut validation_client, run_no, request.row_num, request.line_id).await
            .context("Failed to get required quantity for validation")?;
        
        let picked_qty_f64 = self.safe_bigdecimal_to_f64(&picked_qty, "picked_qty_validation")?;
        let required_qty_f64 = self.safe_bigdecimal_to_f64(&required_qty, "required_qty_validation")?;
        
        // ROUNDING FIX: Use 3-decimal rounding to prevent floating-point precision issues
        let picked_qty_f64_rounded = self.round_to_3_decimals(picked_qty_f64);
        let required_qty_f64_rounded = self.round_to_3_decimals(required_qty_f64);
        
        info!("📊 DEBUG: Quantity validation - picked: {} KG (rounded: {}), required: {} KG (rounded: {})", 
              picked_qty_f64, picked_qty_f64_rounded, required_qty_f64, required_qty_f64_rounded);
        
        if self.qty_greater_than(picked_qty_f64_rounded, required_qty_f64_rounded) {
            let error_msg = format!("Quantity picked is more than Qty Required {} KG", required_qty_f64_rounded);
            warn!("❌ DEBUG: BME4 validation failed - {}", error_msg);
            return Err(anyhow::anyhow!("QUANTITY_VALIDATION_FAILED: {}", error_msg));
        }
        info!("✅ DEBUG: BME4 quantity validation passed");
        
        let bangkok_now = crate::utils::timezone::bangkok_now_sql_server();
        info!("⏰ DEBUG: Bangkok timestamp (SQL Server format): {}", bangkok_now);

        // **CRITICAL FIX**: Pre-generate BT document sequence using separate connection
        // This prevents implicit transaction contexts from interfering with main transaction
        info!("🔢 SEQUENCE_PRE_GEN: Generating BT document sequence using separate connection");
        // NOTE: LotTranNo is auto-generated by IDENTITY column, no manual generation needed
        
        let bt_document = self.generate_bt_document_number(&mut sequence_client).await
            .context("CRITICAL: Failed to pre-generate BT document using separate connection")?;
        info!("✅ SEQUENCE_PRE_GEN: Generated BT Document: {}", bt_document);
        
        // **CRITICAL FIX**: Close validation connection to prevent implicit transaction state
        // The validation_client was used for multiple queries and may have implicit transaction context
        drop(validation_client);
        info!("🔒 CONNECTION_SEPARATION: Validation connection closed to prevent transaction interference");
        
        // Close sequence generation connection to ensure no transaction state carries over
        drop(sequence_client);
        info!("🔒 CONNECTION_SEPARATION: Sequence generation connection closed");
        
        // Connection 2: Fresh connection for main transaction (clean transaction state)
        info!("🔍 CONNECTION_FRESH: Creating fresh connection for main transaction");
        let mut client = self.get_client().await.map_err(|e| {
            let error_msg = format!("Failed to create fresh connection for main transaction: {e}");
            error!("🔌 TRANSACTION_CONNECTION_FAILED: {}", error_msg);
            anyhow::anyhow!("TRANSACTION_DATABASE_CONNECTION_FAILED: {}", error_msg)
        })?;
        
        info!("✅ CONNECTION_FRESH: Main transaction client connected with clean state");

        // **🔒 TRANSACTION ATOMICITY FIX** - Use explicit BEGIN TRANSACTION for data integrity
        // **CRITICAL**: This ensures all 6 steps are atomic - either ALL succeed or ALL are rolled back
        // This prevents data corruption from partial transactions (e.g., step 3 fails but steps 1-2 already committed)
        info!("🔒 BEGIN_TRANSACTION: Starting explicit transaction for atomic 6-step bulk picking");

        // Set REPEATABLE READ isolation level for stronger consistency
        client.simple_query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ").await
            .context("Failed to set isolation level")?;
        info!("✅ ISOLATION_LEVEL: Set to REPEATABLE READ for data consistency");

        client.simple_query("BEGIN TRANSACTION").await
            .context("Failed to begin transaction")?;
        info!("✅ TRANSACTION_STARTED: All 6 steps will be atomic (all-or-nothing)");

        // Transaction wrapper: Execute all 6 steps and handle rollback on error
        let transaction_result: Result<PickConfirmationResponse> = async {

        // **🔒 STEP 0: LOCK LOTMASTER FIRST** - Global lock order to prevent deadlocks
        // This ensures LotMaster is locked before any application tables
        info!("🔒 STEP_0_LOCK: Acquiring exclusive lock on LotMaster to prevent deadlocks");

        let lock_lot_query = r#"
            SELECT LotNo, ItemKey, QtyOnHand, QtyCommitSales
            FROM LotMaster WITH (UPDLOCK, ROWLOCK)
            WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4
        "#;

        let mut lock_stmt = TiberiusQuery::new(lock_lot_query);
        lock_stmt.bind(request.lot_no.clone());
        lock_stmt.bind(batch_info.item_key.clone());
        lock_stmt.bind("TFC1");
        lock_stmt.bind(request.bin_no.clone());

        let lock_result = lock_stmt.query(&mut client).await
            .context("Failed to lock LotMaster record")?;

        let lot_row = lock_result.into_row().await?
            .ok_or_else(|| anyhow::anyhow!("Lot not found for locking"))?;

        let current_qty_on_hand: f64 = lot_row.get("QtyOnHand").unwrap_or(0.0);
        let current_qty_commit: f64 = lot_row.get("QtyCommitSales").unwrap_or(0.0);
        let available_qty = current_qty_on_hand - current_qty_commit;

        // Validate sufficient quantity BEFORE any updates
        if picked_qty_f64 > available_qty {
            return Err(anyhow::anyhow!(
                "Insufficient quantity: requested {}, available {}",
                picked_qty_f64, available_qty
            ));
        }

        info!("✅ STEP_0_LOCKED: LotMaster locked successfully - Lot: {}, Available: {} KG",
              request.lot_no, available_qty);

        // **STEP 1: UPDATE cust_BulkPicked** - Set picked quantities and timestamps
        info!("🔄 DEBUG: STEP 1 - Updating cust_BulkPicked table");
        
        // **ENHANCED USER ID HANDLING**: Use new smart truncation system
        use crate::utils::user_management::{validate_user_context, get_user_id_for_field, UserIdFieldType};
        
        let validated_user = validate_user_context(request.user_id.as_ref())
            .context("Failed to validate user context for transaction")?;
        
        // Get properly truncated user IDs for different field types
        let user_id_for_rec_user = get_user_id_for_field(&validated_user, UserIdFieldType::RecUseridNvarchar);
        let user_id_for_modified_by = get_user_id_for_field(&validated_user, UserIdFieldType::ModifiedBy);
        
        info!("👤 USER_IDS: Original: '{}', RecUserId: '{}', ModifiedBy: '{}'", 
              validated_user, user_id_for_rec_user, user_id_for_modified_by);
        let update_bulk_picked_query = r#"
            UPDATE cust_BulkPicked 
            SET PickedBulkQty = ISNULL(PickedBulkQty, 0) + @P1,
                PickedQty = ISNULL(PickedQty, 0) + @P2,
                PickingDate = @P3,
                ModifiedBy = @P4,
                ModifiedDate = @P5,
                ItemBatchStatus = 'Allocated'
            WHERE RunNo = @P6 AND RowNum = @P7 AND LineId = @P8
        "#;

        let mut update_stmt = TiberiusQuery::new(update_bulk_picked_query);
        // **ENHANCED TYPE CONVERSION**: More robust BigDecimal to f64 conversion
        let bulk_qty_f64 = self.safe_bigdecimal_to_f64(&request.picked_bulk_qty, "picked_bulk_qty")?;
        let picked_qty_f64 = self.safe_bigdecimal_to_f64(&picked_qty, "picked_qty")?;
            
        info!("📝 DEBUG: Binding parameters - bulk_qty: {}, picked_qty: {}, run_no: {}, row_num: {}, line_id: {}", 
              bulk_qty_f64, picked_qty_f64, run_no, request.row_num, request.line_id);
              
        update_stmt.bind(bulk_qty_f64);
        update_stmt.bind(picked_qty_f64);
        update_stmt.bind(bangkok_now.clone());
        update_stmt.bind(user_id_for_modified_by.clone()); // cust_BulkPicked.ModifiedBy (nvarchar(16))
        update_stmt.bind(bangkok_now.clone());
        update_stmt.bind(run_no);
        update_stmt.bind(request.row_num);
        update_stmt.bind(request.line_id);

        let update_result = match update_stmt.execute(&mut client).await {
            Ok(result) => {
                info!("✅ STEP_1_SUCCESS: UPDATE cust_BulkPicked executed successfully with parameters: bulk_qty={}, picked_qty={}, run_no={}, row_num={}, line_id={}", 
                      bulk_qty_f64, picked_qty_f64, run_no, request.row_num, request.line_id);
                result
            }
            Err(e) => {
                let error_msg = format!("STEP 1 FAILED: UPDATE cust_BulkPicked failed for run: {}, row: {}, line: {} with parameters bulk_qty={}, picked_qty={} - Database error: {}", 
                               run_no, request.row_num, request.line_id, bulk_qty_f64, picked_qty_f64, e);
                error!("❌ STEP_1_ERROR: {}", error_msg);
                return Err(anyhow::anyhow!("STEP_1_UPDATE_FAILED: {}", error_msg));
            }
        };
        
        let affected_rows = update_result.rows_affected().iter().sum::<u64>();
        if affected_rows == 0 {
            // Check if the record exists in the primary database
            let check_query = r#"
                SELECT COUNT(*) as record_count
                FROM cust_BulkPicked 
                WHERE RunNo = @P1 AND RowNum = @P2 AND LineId = @P3
            "#;
            
            let mut check_stmt = TiberiusQuery::new(check_query);
            check_stmt.bind(run_no);
            check_stmt.bind(request.row_num);
            check_stmt.bind(request.line_id);
            
            let check_result = check_stmt.query(&mut client).await;
            let record_exists = if let Ok(stream) = check_result {
                let rows: Vec<Row> = stream.into_first_result().await.unwrap_or_default();
                rows.first().and_then(|row| row.get::<i32, _>("record_count")).unwrap_or(0) > 0
            } else {
                false
            };
            
            let error_msg = if record_exists {
                format!("STEP 1 FAILED: Record exists but UPDATE failed. Possible database permission or constraint issue. Run: {}, RowNum: {}, LineId: {}", 
                       run_no, request.row_num, request.line_id)
            } else {
                format!("STEP 1 FAILED: No cust_BulkPicked record found in TFCPILOT3 primary database. Run: {}, RowNum: {}, LineId: {} - Record may need to be created first", 
                       run_no, request.row_num, request.line_id)
            };
            
            warn!("❌ DEBUG: {}", error_msg);
            return Err(anyhow::anyhow!("DATABASE_RECORD_NOT_FOUND: {}", error_msg));
        }
        info!("✅ DEBUG: Step 1 completed - Rows affected: {}", affected_rows);
        summary.bulk_picked_updated = true;

        // **STEP 2: INSERT Cust_BulkLotPicked** - Create lot allocation using pre-generated LotTranNo
        info!("🔄 DEBUG: STEP 2 - Creating Cust_BulkLotPicked record");
        // LotTranNo will be auto-generated by IDENTITY column (no manual generation needed)
        let pallet_no = format!("Pallet {}", request.row_num); // Official PalletNo format

        // **BME4 COMPATIBILITY FIX** - Use existing PalletID for same pallet or generate new one
        info!("🔍 STEP2_PALLET_CHECK: Starting PalletID lookup/generation for STEP 2 (Cust_BulkLotPicked)");
        let pallet_id = match self.get_existing_pallet_id(&mut client, run_no, request.row_num, request.line_id).await? {
            Some(existing_id) => {
                info!("🔄 STEP2_REUSE_PALLET: Using existing PalletID {} for run: {}, row_num: {}, line_id: {} (PREVENTING DUPLICATE)",
                      existing_id, run_no, request.row_num, request.line_id);
                existing_id
            }
            None => {
                let new_id = self.generate_next_pallet_id(&mut client).await
                    .context("CRITICAL: Failed to generate PalletId for Step 2")?;
                info!("🆕 STEP2_NEW_PALLET: Generated new PalletID {} for run: {}, row_num: {}, line_id: {} (FIRST TIME)",
                      new_id, run_no, request.row_num, request.line_id);
                new_id
            }
        };
        info!("🔢 DEBUG: LotTranNo auto-generated by IDENTITY column, PalletNo: {}, PalletId: {}", pallet_no, pallet_id);

        let insert_lot_picked_query = r#"
            INSERT INTO Cust_BulkLotPicked
            (RunNo, RowNum, BatchNo, LineId, LotNo, SuggestedLotNo,
             ItemKey, LocationKey, BinNo, QtyReceived, AllocLotQty, PalletNo,
             LotStatus, TransactionType, RecUserid, RecDate,
             DateReceived, DateExpiry, ReceiptDocNo, ReceiptDocLineNo,
             Vendorkey, VendorlotNo, IssueDocNo, IssueDocLineNo, IssueDate,
             CustomerKey, ModifiedBy, ModifiedDate, Processed, TempQty,
             QtyForLotAssignment, QtyUsed, QtyIssued, PackSize, QtyOnHand,
             PalletId, User1, User2, User3, User4, User5, User6, User7, User8,
             User9, User10, User11, User12, CUSTOM1, CUSTOM2, CUSTOM3, CUSTOM4,
             CUSTOM5, CUSTOM6, CUSTOM7, CUSTOM8, CUSTOM9, CUSTOM10,
             ESG_REASON, ESG_APPROVER)
            VALUES
            (@P1, @P2, @P3, @P4, @P5, @P6,
             @P7, @P8, @P9, @P10, @P11, @P12,
             @P13, @P14, @P15, @P16,
             @P17, @P18, @P19, @P20,
             @P21, @P22, @P23, @P24, @P25,
             @P26, @P27, @P28, @P29, @P30,
             @P31, @P32, @P33, @P34, @P35,
             @P36, @P37, @P38, @P39, @P40, @P41, @P42, @P43, @P44,
             @P45, @P46, @P47, @P48, @P49, @P50, @P51, @P52,
             @P53, @P54, @P55, @P56,
             @P57, @P58, @P59, @P60)
        "#;

        let mut insert_stmt = TiberiusQuery::new(insert_lot_picked_query);
        info!("📝 DEBUG: Binding Cust_BulkLotPicked parameters (60 total) to TFCPILOT3 - AUTO-GEN LotTranNo, run_no: {}, batch_no: {}, item_key: {}, lot_no: {}, bin_no: {}", 
              run_no, batch_info.batch_no, batch_info.item_key, request.lot_no, request.bin_no);
              
        // Complete parameter binding for TFCPILOT3 schema compliance - LotTranNo will be auto-generated by database
        insert_stmt.bind(run_no);                             // @P1 - RunNo
        insert_stmt.bind(request.row_num);                    // @P2 - RowNum  
        insert_stmt.bind(batch_info.batch_no.clone());        // @P3 - BatchNo
        insert_stmt.bind(request.line_id);                    // @P4 - LineId
        insert_stmt.bind(request.lot_no.clone());             // @P5 - LotNo
        insert_stmt.bind(request.lot_no.clone());             // @P6 - SuggestedLotNo (same as selected lot)
        insert_stmt.bind(batch_info.item_key.clone());        // @P7 - ItemKey
        insert_stmt.bind("TFC1");                             // @P8 - LocationKey
        insert_stmt.bind(request.bin_no.clone());             // @P9 - BinNo
        insert_stmt.bind(picked_qty_f64);                     // @P10 - QtyReceived
        insert_stmt.bind(picked_qty_f64);                     // @P11 - AllocLotQty (same)
        insert_stmt.bind(pallet_no.clone());                  // @P12 - PalletNo
        insert_stmt.bind("Allocated");                        // @P13 - LotStatus
        insert_stmt.bind(5u8);                                // @P14 - TransactionType (tinyint)
        insert_stmt.bind(get_user_id_for_field(&validated_user, UserIdFieldType::RecUseridVarchar)); // @P15 - RecUserid (varchar(8))
        insert_stmt.bind(bangkok_now.clone());                // @P16 - RecDate
        
        // Additional required parameters for TFCPILOT3 schema (P17-P60) - LotTranNo removed from parameters
        insert_stmt.bind(bangkok_now.clone());                // @P17 - DateReceived
        insert_stmt.bind(bangkok_now.clone());                // @P18 - DateExpiry (placeholder)
        insert_stmt.bind("");                                 // @P19 - ReceiptDocNo
        insert_stmt.bind(0i16);                               // @P20 - ReceiptDocLineNo (smallint)
        insert_stmt.bind("");                                 // @P21 - Vendorkey
        insert_stmt.bind("");                                 // @P22 - VendorlotNo  
        insert_stmt.bind("");                                 // @P23 - IssueDocNo
        insert_stmt.bind(0i16);                               // @P24 - IssueDocLineNo (smallint)
        insert_stmt.bind(None::<&str>);                       // @P25 - IssueDate (null - allocation record, not issue)
        insert_stmt.bind("");                                 // @P26 - CustomerKey
        insert_stmt.bind("");                                 // @P27 - ModifiedBy (empty string per official app)
        insert_stmt.bind(bangkok_now.clone());                // @P28 - ModifiedDate
        insert_stmt.bind("N");                                // @P29 - Processed (char)
        insert_stmt.bind(0.0f64);                             // @P30 - TempQty
        insert_stmt.bind(0.0f64);                             // @P31 - QtyForLotAssignment
        insert_stmt.bind(0.0f64);                             // @P32 - QtyUsed
        insert_stmt.bind(0.0f64);                             // @P33 - QtyIssued (0 - allocation record, not actual issue)
        insert_stmt.bind(pack_size_f64);                      // @P34 - PackSize (from batch_info, not hardcoded)
        insert_stmt.bind(picked_qty_f64);                     // @P35 - QtyOnHand
        insert_stmt.bind(pallet_id.clone());                  // @P36 - PalletId (6-digit sequential)
        insert_stmt.bind("");                                 // @P37 - User1
        insert_stmt.bind("");                                 // @P38 - User2  
        insert_stmt.bind("");                                 // @P39 - User3
        insert_stmt.bind("");                                 // @P40 - User4
        insert_stmt.bind("");                                 // @P41 - User5
        insert_stmt.bind(bangkok_now.clone());                // @P42 - User6 (datetime)
        insert_stmt.bind(0.0f64);                             // @P43 - User7 (float)
        insert_stmt.bind(0.0f64);                             // @P44 - User8 (float)
        insert_stmt.bind(0.0f64);                             // @P45 - User9 (decimal as f64)
        insert_stmt.bind(0.0f64);                             // @P46 - User10 (decimal as f64)
        insert_stmt.bind(0i32);                               // @P47 - User11 (int)
        insert_stmt.bind(0i32);                               // @P48 - User12 (int)
        insert_stmt.bind(false);                              // @P49 - CUSTOM1 (bit)
        insert_stmt.bind(false);                              // @P50 - CUSTOM2 (bit)
        insert_stmt.bind(false);                              // @P51 - CUSTOM3 (bit)
        insert_stmt.bind(false);                              // @P52 - CUSTOM4 (bit)
        insert_stmt.bind(false);                              // @P53 - CUSTOM5 (bit)
        insert_stmt.bind(false);                              // @P54 - CUSTOM6 (bit)
        insert_stmt.bind(false);                              // @P55 - CUSTOM7 (bit)
        insert_stmt.bind(false);                              // @P56 - CUSTOM8 (bit)
        insert_stmt.bind(false);                              // @P57 - CUSTOM9 (bit)
        insert_stmt.bind(false);                              // @P58 - CUSTOM10 (bit)
        insert_stmt.bind("");                                 // @P59 - ESG_REASON
        insert_stmt.bind("");                                 // @P60 - ESG_APPROVER

        let insert_result = match insert_stmt.execute(&mut client).await {
            Ok(result) => {
                info!("✅ STEP_2_SUCCESS: INSERT Cust_BulkLotPicked executed successfully with key parameters (LotTranNo auto-generated): run_no={}, batch_no={}, lot_no={}, item_key={}, bin_no={}, qty_received={}, pallet_no={}, pallet_id={}", 
                      run_no, batch_info.batch_no, request.lot_no, batch_info.item_key, request.bin_no, picked_qty_f64, pallet_no, pallet_id);
                result
            }
            Err(e) => {
                let error_msg = format!("STEP 2 FAILED: INSERT Cust_BulkLotPicked failed with key parameters (LotTranNo auto-generated): run_no={}, batch_no={}, lot_no={}, item_key={}, bin_no={}, qty_received={}, pallet_no={}, pallet_id={} - Database error: {} - Check table schema, constraints, or data types", 
                               run_no, batch_info.batch_no, request.lot_no, batch_info.item_key, request.bin_no, picked_qty_f64, pallet_no, pallet_id, e);
                error!("❌ STEP_2_ERROR: {}", error_msg);
                return Err(anyhow::anyhow!("STEP_2_INSERT_FAILED: {}", error_msg));
            }
        };

        let insert_affected_rows = insert_result.rows_affected().iter().sum::<u64>();
        if insert_affected_rows == 0 {
            let error_msg = "STEP 2 FAILED: No rows inserted into Cust_BulkLotPicked. Auto-generated LotTranNo or table constraints may have failed";
            warn!("❌ DEBUG: {}", error_msg);
            return Err(anyhow::anyhow!("DATABASE_INSERT_FAILED: {}", error_msg));
        }
        info!("✅ DEBUG: Step 2 completed - Rows affected: {}", insert_affected_rows);
        summary.lot_picked_created = true;

        // **STEP 3: UPDATE LotMaster** - Add to QtyCommitSales for inventory commitment
        info!("🔄 DEBUG: STEP 3 - Updating LotMaster QtyCommitSales");
        let update_lot_master_query = r#"
            UPDATE LotMaster
            SET QtyCommitSales = QtyCommitSales + @P1
            WHERE LotNo = @P2 
              AND ItemKey = @P3 
              AND LocationKey = @P4 
              AND BinNo = @P5
        "#;

        let mut lot_master_stmt = TiberiusQuery::new(update_lot_master_query);
        info!("📝 DEBUG: Binding LotMaster parameters - picked_qty: {} (converted from {}), lot_no: {}, item_key: {}, location: TFC1, bin_no: {}", 
              picked_qty_f64, picked_qty, request.lot_no, batch_info.item_key, request.bin_no);
              
        lot_master_stmt.bind(picked_qty_f64);
        lot_master_stmt.bind(request.lot_no.clone());
        lot_master_stmt.bind(batch_info.item_key.clone());
        lot_master_stmt.bind(crate::database::DEFAULT_LOCATION_KEY);
        lot_master_stmt.bind(request.bin_no.clone());

        let lot_update_result = match lot_master_stmt.execute(&mut client).await {
            Ok(result) => {
                info!("✅ STEP_3_SUCCESS: UPDATE LotMaster QtyCommitSales executed successfully with parameters: picked_qty={}, lot_no={}, item_key={}, location=TFC1, bin_no={}", 
                      picked_qty_f64, request.lot_no, batch_info.item_key, request.bin_no);
                result
            }
            Err(e) => {
                let error_msg = format!("STEP 3 FAILED: UPDATE LotMaster failed with parameters: picked_qty={}, lot_no={}, item_key={}, location=TFC1, bin_no={} using TFCPILOT3 primary database - Database error: {}", 
                               picked_qty_f64, request.lot_no, batch_info.item_key, request.bin_no, e);
                error!("❌ STEP_3_ERROR: {}", error_msg);
                return Err(anyhow::anyhow!("STEP_3_UPDATE_FAILED: {}", error_msg));
            }
        };

        let lot_affected_rows = lot_update_result.rows_affected().iter().sum::<u64>();
        if lot_affected_rows == 0 {
            // Enhanced debugging: Check if record exists before failing
            let check_query = r#"
                SELECT COUNT(*) as record_count, LotNo, ItemKey, LocationKey, BinNo, QtyOnHand, QtyCommitSales
                FROM LotMaster 
                WHERE LotNo = @P1 AND ItemKey = @P2 AND LocationKey = @P3 AND BinNo = @P4
                GROUP BY LotNo, ItemKey, LocationKey, BinNo, QtyOnHand, QtyCommitSales
            "#;
            
            let mut check_stmt = TiberiusQuery::new(check_query);
            check_stmt.bind(request.lot_no.clone());
            check_stmt.bind(batch_info.item_key.clone());
            check_stmt.bind("TFC1");
            check_stmt.bind(request.bin_no.clone());
            
            let debug_info = if let Ok(stream) = check_stmt.query(&mut client).await {
                let rows: Vec<Row> = stream.into_first_result().await.unwrap_or_default();
                if let Some(row) = rows.first() {
                    format!("LotMaster record exists: count={}, lot={}, item={}, location={}, bin={}, qty_on_hand={}, qty_commit_sales={}", 
                        row.get::<i32, _>("record_count").unwrap_or(0),
                        row.get::<&str, _>("LotNo").unwrap_or("N/A"),
                        row.get::<&str, _>("ItemKey").unwrap_or("N/A"),
                        row.get::<&str, _>("LocationKey").unwrap_or("N/A"),
                        row.get::<&str, _>("BinNo").unwrap_or("N/A"),
                        row.get::<f64, _>("QtyOnHand").unwrap_or(0.0),
                        row.get::<f64, _>("QtyCommitSales").unwrap_or(0.0)
                    )
                } else {
                    "LotMaster record NOT FOUND - lot/bin/item combination doesn't exist".to_string()
                }
            } else {
                "Failed to query LotMaster for debugging".to_string()
            };

            let error_msg = format!("STEP 3 FAILED: No rows updated in LotMaster using TFCPILOT3 primary database. Lot: {}, Item: {}, Bin: {}. Debug: {}", 
                request.lot_no, batch_info.item_key, request.bin_no, debug_info);
            error!("❌ STEP_3_DETAILED_ERROR: {}", error_msg);
            return Err(anyhow::anyhow!("LOT_UPDATE_FAILED: {}", error_msg));
        }
        info!("✅ DEBUG: Step 3 completed - Rows affected: {}", lot_affected_rows);
        summary.lot_master_updated = true;
        summary.total_committed_qty = picked_qty.clone();

        // **STEP 4: INSERT LotTransaction** - Create audit trail using pre-generated BT document
        info!("🔄 DEBUG: STEP 4 - Creating LotTransaction audit trail with pre-generated BT document: {}", bt_document);

        let insert_lot_transaction_query = r#"
            INSERT INTO LotTransaction
            (LotNo, ItemKey, LocationKey, TransactionType,
             QtyIssued, IssueDocNo, IssueDocLineNo, IssueDate, 
             ReceiptDocNo, RecUserid, RecDate, BinNo, CustomerKey, User5)
            OUTPUT INSERTED.LotTranNo
            VALUES
            (@P1, @P2, @P3, @P4,
             @P5, @P6, @P7, @P8, 
             @P9, @P10, @P11, @P12, @P13, @P14)
        "#;

        let mut transaction_stmt = TiberiusQuery::new(insert_lot_transaction_query);
        info!("📝 DEBUG: Binding LotTransaction parameters - lot_no: {}, item_key: {}, qty_issued: {}, batch_no: {}, bt_document: {}", 
              request.lot_no, batch_info.item_key, picked_qty_f64, batch_info.batch_no, bt_document);
              
        transaction_stmt.bind(request.lot_no.clone());
        transaction_stmt.bind(batch_info.item_key.clone());
        transaction_stmt.bind("TFC1");
        transaction_stmt.bind(5); // TransactionType for picking operation
        transaction_stmt.bind(picked_qty_f64);
        transaction_stmt.bind(batch_info.batch_no.clone()); // IssueDocNo links to batch
        transaction_stmt.bind(request.line_id); // IssueDocLineNo
        transaction_stmt.bind(bangkok_now.clone()); // IssueDate (pick timestamp)
        transaction_stmt.bind(bt_document.clone()); // ReceiptDocNo (BT-XXXXXXXX)
        transaction_stmt.bind(get_user_id_for_field(&validated_user, UserIdFieldType::RecUseridVarchar)); // RecUserid (varchar(8))
        transaction_stmt.bind(bangkok_now.clone()); // RecDate (transaction timestamp)
        transaction_stmt.bind(request.bin_no.clone());
        transaction_stmt.bind(""); // CustomerKey - empty string for bulk production picking
        transaction_stmt.bind("Picking Customization"); // Transaction source

        let transaction_result = match transaction_stmt.query(&mut client).await {
            Ok(result) => {
                info!("✅ STEP_4_SUCCESS: INSERT LotTransaction executed successfully");
                result
            }
            Err(e) => {
                let error_msg = format!("STEP 4 FAILED: INSERT LotTransaction failed with document: {bt_document} - Database error: {e}");
                error!("❌ STEP_4_ERROR: {}", error_msg);
                return Err(anyhow::anyhow!("STEP_4_INSERT_FAILED: {}", error_msg));
            }
        };

        // Get the auto-generated LotTranNo from the OUTPUT clause
        let generated_lot_tran_no = if let Some(row) = transaction_result.into_row().await? {
            row.get::<i32, _>(0).unwrap_or(0)
        } else {
            let error_msg = "STEP 4 FAILED: No LotTranNo returned from LotTransaction INSERT";
            warn!("❌ DEBUG: {}", error_msg);
            return Err(anyhow::anyhow!("TRANSACTION_INSERT_FAILED: {}", error_msg));
        };

        info!("✅ DEBUG: Step 4 completed - Generated LotTranNo: {}", generated_lot_tran_no);
        summary.lot_transaction_created = true;

        // **STEP 5: UPSERT pallet-lot traceability (CRITICAL FIX)**
        // Use MERGE to UPDATE existing record or INSERT new one for proper duplicate handling
        let pallet_upsert_query = r#"
            MERGE Cust_BulkPalletLotPicked AS target
            USING (SELECT @P1 as RunNo, @P2 as RowNum, @P4 as LineId) AS source
            ON (target.RunNo = source.RunNo AND target.RowNum = source.RowNum AND target.LineId = source.LineId)
            WHEN MATCHED THEN
                UPDATE SET 
                    PalletID = @P5,
                    ModifiedBy = @P8,
                    ModifiedDate = @P9
            WHEN NOT MATCHED THEN
                INSERT (RunNo, RowNum, BatchNo, LineId, PalletID, RecUserid, RecDate, ModifiedBy, ModifiedDate)
                VALUES (@P1, @P2, @P3, @P4, @P5, @P6, @P7, @P8, @P9);
        "#;

        // **BME4 COMPATIBILITY FIX** - Use existing PalletID for same pallet or generate new one
        info!("🔍 STEP5_PALLET_CHECK: Starting PalletID lookup/generation for STEP 5 (Cust_BulkPalletLotPicked)");
        let pallet_id = match self.get_existing_pallet_id(&mut client, run_no, request.row_num, request.line_id).await? {
            Some(existing_id) => {
                info!("🔄 STEP5_REUSE_PALLET: Using existing PalletID {} for run: {}, row_num: {}, line_id: {} (CONSISTENCY CHECK)",
                      existing_id, run_no, request.row_num, request.line_id);
                existing_id
            }
            None => {
                let new_id = self.generate_next_pallet_id(&mut client).await
                    .context("Failed to generate sequential PalletID")?;
                info!("🆕 STEP5_NEW_PALLET: Generated new PalletID {} for run: {}, row_num: {}, line_id: {} (ERROR: Should reuse from Step 2!)",
                      new_id, run_no, request.row_num, request.line_id);
                new_id
            }
        };
        
        let mut pallet_stmt = TiberiusQuery::new(pallet_upsert_query);
        info!("📝 DEBUG: Binding Cust_BulkPalletLotPicked UPSERT parameters - run_no: {}, batch_no: {}, pallet_id: {} (sequential)", 
              run_no, batch_info.batch_no, pallet_id);
              
        pallet_stmt.bind(run_no);                                 // @P1 - RunNo
        pallet_stmt.bind(request.row_num);                        // @P2 - RowNum  
        pallet_stmt.bind(batch_info.batch_no.clone());            // @P3 - BatchNo
        pallet_stmt.bind(request.line_id);                        // @P4 - LineId
        pallet_stmt.bind(pallet_id.clone());                      // @P5 - PalletID
        pallet_stmt.bind(get_user_id_for_field(&validated_user, UserIdFieldType::RecUseridVarchar)); // @P6 - RecUserid (varchar(8))
        pallet_stmt.bind(bangkok_now.clone());                    // @P7 - RecDate (also used as ModifiedDate for INSERT)
        pallet_stmt.bind(user_id_for_modified_by.clone());        // @P8 - ModifiedBy (nvarchar(16), only for UPDATE)
        pallet_stmt.bind(bangkok_now.clone());                    // @P9 - ModifiedDate (only for UPDATE)

        let pallet_result = match pallet_stmt.execute(&mut client).await {
            Ok(result) => {
                info!("✅ STEP_5_SUCCESS: UPSERT Cust_BulkPalletLotPicked executed successfully (handles duplicates)");
                result
            }
            Err(e) => {
                let error_msg = format!("STEP 5 FAILED: UPSERT Cust_BulkPalletLotPicked failed with PalletID: {pallet_id} - Database error: {e}");
                error!("❌ STEP_5_ERROR: {}", error_msg);
                return Err(anyhow::anyhow!("STEP_5_UPSERT_FAILED: {}", error_msg));
            }
        };

        if pallet_result.rows_affected().first() == Some(&1u64) {
            info!("✅ DEBUG: Step 5 completed - Cust_BulkPalletLotPicked record upserted with PalletID: {}", pallet_id);
            summary.pallet_lot_picked_created = true;
        } else {
            return Err(anyhow::anyhow!("CRITICAL: Cust_BulkPalletLotPicked upsert affected {} rows (expected 1)", 
                       pallet_result.rows_affected().first().unwrap_or(&0u64)));
        }

        // Operations completed (note: without explicit transaction, each operation commits individually)

        info!(
            "🎉 BME4 5-table pick confirmation completed successfully - Run: {}, Document: {}, Summary: cust_BulkPicked={}, Cust_BulkLotPicked={} (LotTranNo auto-generated), LotMaster={}, LotTransaction={}, Cust_BulkPalletLotPicked={}",
            run_no, bt_document, summary.bulk_picked_updated, summary.lot_picked_created, summary.lot_master_updated, summary.lot_transaction_created, summary.pallet_lot_picked_created
        );

        // **UNIFIED PATTERN**: All operations now use TFCPILOT3 directly
        info!("✅ All operations completed successfully on TFCPILOT3 primary database");

        // **STEP 6: UPDATE PNITEM** - Update PNITEM table with picked quantities
        // Logic mirrors execute_phase5_pnitem_update in transaction_service.rs
        info!("🔄 DEBUG: STEP 6 - Updating PNITEM table");

        // 1. Check current User11 status
        let check_pnitem_query = r#"
            SELECT User11
            FROM PNITEM WITH (UPDLOCK, ROWLOCK)
            WHERE BatchNo = @P1 AND Lineid = @P2
        "#;

        let mut check_stmt = TiberiusQuery::new(check_pnitem_query);
        check_stmt.bind(batch_info.batch_no.clone());
        check_stmt.bind(request.line_id);

        let user11_value = match check_stmt.query(&mut client).await {
            Ok(stream) => {
                let rows: Vec<Row> = stream.into_first_result().await.unwrap_or_default();
                if let Some(row) = rows.first() {
                    row.get::<i32, _>(0)
                } else {
                    // Log warning but don't fail transaction if PNITEM record missing (legacy data support)
                    warn!("⚠️ PNITEM record not found for BatchNo={}, LineId={}. Skipping PNITEM update.", 
                          batch_info.batch_no, request.line_id);
                    None
                }
            },
            Err(e) => {
                warn!("⚠️ Failed to check PNITEM User11: {}. Skipping PNITEM update.", e);
                None
            }
        };

        // 2. Conditional Update based on User11
        // If User11 is NULL -> First Pick (Override values)
        // If User11 is NOT NULL -> Subsequent Pick (Accumulate values)
        
        // Only proceed if we found the record (or if we want to be strict, we could fail here)
        // For now, we'll assume if check failed/empty, we skip update to avoid breaking flow
        
        let pnitem_update_sql = if user11_value.is_none() {
            // First time (User11 is NULL) -> Override values
            info!("🆕 PNITEM: First pick detected (User11 is NULL) - Overriding values");
            r#"
            UPDATE PNITEM
            SET
                ActualQty = @P1,
                AllocQty = @P1,
                ActualQtyDispUom = @P1,
                SerLotQty = @P1,
                Status = 'A',
                User11 = 1
            WHERE BatchNo = @P2 AND Lineid = @P3
            "#
        } else {
            // Subsequent times (User11 is NOT NULL) -> Accumulate values
            info!("➕ PNITEM: Subsequent pick detected (User11={:?}) - Accumulating values", user11_value);
            r#"
            UPDATE PNITEM
            SET
                ActualQty = ActualQty + @P1,
                AllocQty = AllocQty + @P1,
                ActualQtyDispUom = ActualQtyDispUom + @P1,
                SerLotQty = SerLotQty + @P1,
                Status = 'A'
            WHERE BatchNo = @P2 AND Lineid = @P3
            "#
        };

        let mut pnitem_stmt = TiberiusQuery::new(pnitem_update_sql);
        pnitem_stmt.bind(picked_qty_f64); // @P1 - Weight in KG
        pnitem_stmt.bind(batch_info.batch_no.clone()); // @P2
        pnitem_stmt.bind(request.line_id); // @P3

        match pnitem_stmt.execute(&mut client).await {
            Ok(_) => {
                info!("✅ STEP_6_SUCCESS: PNITEM updated successfully");
            },
            Err(e) => {
                // Log error but decide if it should be critical
                // For consistency with transaction_service, this SHOULD be critical
                let error_msg = format!("STEP 6 FAILED: PNITEM update failed - Database error: {e}");
                error!("❌ STEP_6_ERROR: {}", error_msg);
                return Err(anyhow::anyhow!("STEP_6_UPDATE_FAILED: {}", error_msg));
            }
        }

        // **STEP 6b: UPDATE PNMAST STATUS** - Update PNMAST from 'R' to 'A' on first pick
        // Only update if status is currently 'R' (Ready), leave 'A' (Active/Allocated) unchanged
        info!("🔄 DEBUG: STEP 6b - Updating PNMAST status 'R' → 'A' for batch: {}", batch_info.batch_no);
        
        let update_pnmast_query = r#"
            UPDATE PNMAST
            SET Status = 'A'
            WHERE BatchNo = @P1 AND Status = 'R'
        "#;

        let mut pnmast_stmt = TiberiusQuery::new(update_pnmast_query);
        pnmast_stmt.bind(batch_info.batch_no.clone());

        match pnmast_stmt.execute(&mut client).await {
            Ok(result) => {
                let rows_affected = result.rows_affected().iter().sum::<u64>();
                if rows_affected > 0 {
                    info!("✅ STEP_6b_SUCCESS: PNMAST status updated 'R' → 'A' for batch: {}", batch_info.batch_no);
                } else {
                    info!("ℹ️ STEP_6b_SKIP: PNMAST status already 'A' for batch: {}", batch_info.batch_no);
                }
            },
            Err(e) => {
                warn!("⚠️ STEP_6b_WARNING: Failed to update PNMAST status: {}. Continuing...", e);
                // Non-critical: Continue even if PNMAST update fails
            }
        }

        // **SMART COMPLETION**: Step 7 completion check - Only triggers when ALL item keys are completely picked
        // Status changes NEW → PRINT automatically only when the FINAL pick of the LAST ingredient is completed
        // This ensures status change happens once when truly all ingredients are done, not after each individual pick
        info!("🔄 DEBUG: STEP 7 - Smart completion check: Verifying if ALL ingredients are now complete");
        
        let completion_check_result = self.check_and_update_run_completion(
            &mut client,
            run_no,
            &get_user_id_for_field(&validated_user, UserIdFieldType::ModifiedBy),
            &bangkok_now,
        ).await;
        
        match completion_check_result {
            Ok(true) => {
                info!("🎉 DEBUG: Step 7 SMART COMPLETION - Run {} status updated NEW → PRINT (ALL bulk ingredients now completely picked)", run_no);
            },
            Ok(false) => {
                info!("📋 DEBUG: Step 7 - Run {} still has incomplete ingredients (Status remains NEW, more picking needed)", run_no);
            },
            Err(e) => {
                warn!("⚠️ DEBUG: Step 7 non-critical error - Smart completion check failed: {}. Pick operation succeeded but status not updated.", e);
                // Continue - this is not critical for the main pick operation
            }
        }

            // **🎉 OPERATIONS COMPLETE** - All 7 steps completed successfully
            info!("🎉 TRANSACTION_COMPLETE: All 7-step bulk picking operations completed successfully");
            info!("✅ ALL_STEPS_SUCCEEDED: Ready to commit transaction");
            info!("🔄 TRANSACTION_TIMESTAMP: Pick transaction completed at {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f UTC"));

            Ok(PickConfirmationResponse {
                success: true,
                transaction_id: Some(generated_lot_tran_no as i64),
                document_no: Some(bt_document),
                updated_records: summary,
            })
        }.await;

        // **🔒 ATOMIC TRANSACTION HANDLING** - COMMIT on success, ROLLBACK on error
        match transaction_result {
            Ok(response) => {
                // All 6 steps succeeded - commit the transaction
                client.simple_query("COMMIT").await
                    .context("Failed to commit bulk picking transaction")?;

                info!("✅ TRANSACTION_COMMITTED: All 7 steps permanently saved to database");
                info!("🎯 SUCCESS: Bulk picking transaction completed atomically for run {}", run_no);
                Ok(response)
            },
            Err(e) => {
                // Any step failed - rollback the entire transaction
                let _ = client.simple_query("ROLLBACK").await;

                error!("💥 TRANSACTION_ROLLED_BACK: Error during bulk picking - all changes reverted: {}", e);
                error!("🔄 DATA_INTEGRITY: No partial state - database remains consistent");
                Err(anyhow::anyhow!("BULK_PICKING_FAILED_AND_ROLLED_BACK: {}", e))
            }
        }
    }

    /// Get batch information for pick calculations
    /// CRITICAL FIX: Use READ database (TFCPILOT3) for batch info lookup, same as form data
    async fn get_batch_info_for_pick(
        &self,
        client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
        run_no: i32,
        request: &PickConfirmationRequest,
    ) -> Result<BatchInfo> {
        info!("🔍 DEBUG: Getting batch info for run: {}, row_num: {}, line_id: {}", run_no, request.row_num, request.line_id);
        info!("🔧 DEBUG: Using passed client connection to avoid transaction context issues");
        
        // CRITICAL FIX: Use the passed client to avoid creating new connections that cause transaction issues
        
        let query = r#"
            SELECT bp.BatchNo, bp.ItemKey, bp.PackSize
            FROM cust_BulkPicked bp
            WHERE bp.RunNo = @P1 AND bp.RowNum = @P2 AND bp.LineId = @P3
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(run_no);
        select.bind(request.row_num);
        select.bind(request.line_id);

        info!("🔎 DEBUG: Executing batch info query using passed client for run: {}, row: {}, line: {}", run_no, request.row_num, request.line_id);
        let stream = select
            .query(client)
            .await
            .context("Failed to execute batch info query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get batch info results")?;

        info!("📊 DEBUG: Batch info query returned {} rows", rows.len());

        let row = rows.first().ok_or_else(|| {
            warn!("❌ DEBUG: No batch found for run: {}, row: {}, line: {}", 
                  run_no, request.row_num, request.line_id);
            anyhow::anyhow!("CRITICAL: Batch info not found for run: {}, row: {}, line: {}", 
                            run_no, request.row_num, request.line_id)
        })?;

        let batch_no: &str = row.get("BatchNo").unwrap_or("");
        let item_key: &str = row.get("ItemKey").unwrap_or("");
        let pack_size: f64 = row.get("PackSize").unwrap_or(25.0);

        info!("📋 DEBUG: Found batch info - batch_no: '{}', item_key: '{}', pack_size: {}", batch_no, item_key, pack_size);

        if batch_no.is_empty() || item_key.is_empty() {
            warn!("⚠️ DEBUG: Batch info has empty fields - batch_no: '{}', item_key: '{}'", batch_no, item_key);
        }

        Ok(BatchInfo {
            batch_no: batch_no.to_string(),
            item_key: item_key.to_string(),
            pack_size: BigDecimal::from_f64(pack_size).unwrap_or_default(),
        })
    }

    /// Get required quantity for BME4-compatible validation
    /// Calculates remaining quantity needed: (ToPickedBulkQty - PickedBulkQty) * PackSize
    async fn get_required_quantity_for_batch(
        &self,
        client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
        run_no: i32,
        row_num: i32,
        line_id: i32,
    ) -> Result<BigDecimal> {
        info!("🔍 DEBUG: Getting required quantity for run: {}, row: {}, line: {}", run_no, row_num, line_id);
        
        let query = r#"
            SELECT 
                bp.ToPickedBulkQty,
                COALESCE(bp.PickedBulkQty, 0) as PickedBulkQty,
                bp.PackSize,
                -- Calculate remaining bags needed
                (bp.ToPickedBulkQty - COALESCE(bp.PickedBulkQty, 0)) as RemainingBags,
                -- Calculate remaining quantity in KG
                (bp.ToPickedBulkQty - COALESCE(bp.PickedBulkQty, 0)) * bp.PackSize as RequiredQty
            FROM cust_BulkPicked bp
            WHERE bp.RunNo = @P1 AND bp.RowNum = @P2 AND bp.LineId = @P3
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(run_no);
        select.bind(row_num);
        select.bind(line_id);

        let stream = select
            .query(client)
            .await
            .context("Failed to execute required quantity query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get required quantity results")?;

        let row = rows.first().ok_or_else(|| {
            anyhow::anyhow!("No batch found for quantity validation - run: {}, row: {}, line: {}", 
                           run_no, row_num, line_id)
        })?;

        let to_picked: f64 = row.get("ToPickedBulkQty").unwrap_or(0.0);
        let picked: f64 = row.get("PickedBulkQty").unwrap_or(0.0);
        let pack_size: f64 = row.get("PackSize").unwrap_or(25.0);
        let required_qty: f64 = row.get("RequiredQty").unwrap_or(0.0);

        info!("📊 DEBUG: Quantity breakdown - ToPickedBulkQty: {}, PickedBulkQty: {}, PackSize: {}, RequiredQty: {} KG", 
              to_picked, picked, pack_size, required_qty);

        // **CRITICAL DEBUG**: Manual calculation verification
        let manual_remaining_bags = to_picked - picked;
        let manual_required_qty = manual_remaining_bags * pack_size;
        info!("🔍 MANUAL_CALC: Remaining bags: {} ({}–{}), Manual RequiredQty: {} KG", 
              manual_remaining_bags, to_picked, picked, manual_required_qty);

        // **BUG FIX VALIDATION**: Ensure we return the correct remaining quantity
        if (required_qty - manual_required_qty).abs() > 0.01 {
            warn!("⚠️ QUANTITY_MISMATCH: SQL RequiredQty ({}) != Manual calc ({})", required_qty, manual_required_qty);
        }

        Ok(BigDecimal::from_f64(required_qty).unwrap_or_default())
    }


    /// Generate BT-XXXXXXXX document number using official BME4 Seqnum pattern
    pub(crate) async fn generate_bt_document_number(&self, client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>) -> Result<String> {
        info!("🔢 BT_DOC_GEN: Starting BME4-compatible BT document generation using Seqnum table");
        
        // **ATOMIC SEQUENCE INCREMENT PATTERN** - Use OUTPUT clause to increment and return value in single query
        // This ensures each pick operation gets a unique document number for audit trail integrity
        let atomic_query = r#"
            UPDATE Seqnum 
            SET SeqNum = SeqNum + 1 
            OUTPUT INSERTED.SeqNum as NextSeq
            WHERE SeqName = 'BT'
        "#;

        info!("🔄 BT_DOC_GEN: Atomically incrementing BT sequence number for unique document generation");

        // Execute atomic increment and retrieve in single operation
        let query = TiberiusQuery::new(atomic_query);
        let stream = query
            .query(client)
            .await
            .context("Failed to increment BT sequence number atomically")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get BT sequence from Seqnum table")?;

        let row = rows.first().ok_or_else(|| anyhow::anyhow!("No BT sequence returned from Seqnum table"))?;
        
        let next_seq: i32 = row.get::<i32, _>("NextSeq").unwrap_or(26019202);
        
        info!("✅ BT_DOC_GEN: Generated unique BT sequence {} (atomic increment)", next_seq);

        Ok(format!("BT-{next_seq:08}"))
    }


    /// **TASK 3: Multi-Batch Inventory Logic** - Validation rules for BME4 compatibility
    /// Validates pick request against business rules and available inventory
    #[instrument(skip(self))]
    pub async fn validate_pick_request(
        &self,
        run_no: i32,
        request: &PickConfirmationRequest,
    ) -> Result<PickValidationResult> {
        info!(
            "Validating pick request for run: {}, lot: {}, requested: {}",
            run_no, request.lot_no, request.picked_bulk_qty
        );

        // Use primary database (TFCPILOT3) for transaction validation
        let mut client = self
            .get_client()
            .await
            .context("Failed to get primary database client for validation")?;

        // STEP 1: Get ItemKey from the specific batch first
        let item_key_query = r#"
            SELECT ItemKey 
            FROM cust_BulkPicked 
            WHERE RunNo = @P1 AND RowNum = @P2 AND LineId = @P3
        "#;

        let mut item_key_select = TiberiusQuery::new(item_key_query);
        item_key_select.bind(run_no);
        item_key_select.bind(request.row_num);
        item_key_select.bind(request.line_id);

        let item_key_stream = item_key_select
            .query(&mut client)
            .await
            .context("Failed to get ItemKey for validation")?;

        let item_key_rows: Vec<Row> = item_key_stream
            .into_first_result()
            .await
            .context("Failed to get ItemKey results")?;

        let item_key_row = item_key_rows.first().ok_or_else(|| {
            anyhow::anyhow!("Batch not found for run: {}, row_num: {}, line_id: {}", 
                           run_no, request.row_num, request.line_id)
        })?;

        let item_key: &str = item_key_row.get("ItemKey").unwrap_or("");

        // STEP 2: Get INGREDIENT TOTALS for proper completion validation
        // FIXED: Corrected JOIN logic to include ALL batches for the ingredient (remove LineId constraint)
        // This prevents "Qty Required 0" validation errors during pallet advancement
        let ingredient_validation_query = r#"
            SELECT 
                -- Single batch info (for batch-specific validation)
                bp_batch.ToPickedBulkQty as BatchToPickedBulkQty,
                ISNULL(bp_batch.PickedBulkQty, 0) as BatchCurrentlyPicked,
                bp_batch.PackSize,
                bp_batch.ItemKey,
                bp_batch.BatchNo,
                bp_batch.RowNum,
                bp_batch.LineId,
                -- Ingredient totals (for completion validation across ALL batches)
                -- CRITICAL FIX: Remove LineId constraint to include all ingredient batches
                SUM(bp_all.ToPickedBulkQty) as IngredientTotalRequired,
                SUM(ISNULL(bp_all.PickedBulkQty, 0)) as IngredientTotalPicked
            FROM cust_BulkPicked bp_batch
            INNER JOIN cust_BulkPicked bp_all 
                ON bp_batch.RunNo = bp_all.RunNo 
                AND bp_batch.ItemKey = bp_all.ItemKey
                -- REMOVED: AND bp_batch.LineId = bp_all.LineId (this was incorrect)
                -- Different pallets of same ingredient can have different LineIds
                AND bp_all.ToPickedBulkQty > 0  -- Only include batches that require picking
            WHERE bp_batch.RunNo = @P1 
              AND bp_batch.ItemKey = @P2 
              AND bp_batch.RowNum = @P3
            GROUP BY 
                bp_batch.ToPickedBulkQty, bp_batch.PickedBulkQty, bp_batch.PackSize,
                bp_batch.ItemKey, bp_batch.BatchNo, bp_batch.RowNum, bp_batch.LineId
        "#;

        let mut select = TiberiusQuery::new(ingredient_validation_query);
        select.bind(run_no);
        select.bind(item_key);
        select.bind(request.row_num);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute batch validation query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get batch validation results")?;

        let row = rows.first().ok_or_else(|| {
            anyhow::anyhow!("Ingredient not found for run: {}, line_id: {}", 
                            run_no, request.line_id)
        })?;

        // Get batch-specific values (for individual batch validation)
        let batch_to_picked_qty: f64 = row.get("BatchToPickedBulkQty").unwrap_or(0.0);
        let batch_currently_picked: f64 = row.get("BatchCurrentlyPicked").unwrap_or(0.0);
        let pack_size: f64 = row.get("PackSize").unwrap_or(25.0);
        let item_key: &str = row.get("ItemKey").unwrap_or("");
        let _batch_no: &str = row.get("BatchNo").unwrap_or("");
        
        // Get ingredient-level totals (for completion validation)
        let ingredient_total_required: f64 = row.get("IngredientTotalRequired").unwrap_or(0.0);
        let ingredient_total_picked: f64 = row.get("IngredientTotalPicked").unwrap_or(0.0);

        info!(
            "🔍 BATCH VALIDATION DATA: run={}, line_id={}, row_num={}, BatchToPickedQty={}, BatchCurrentlyPicked={}, PackSize={}, ItemKey={}", 
            run_no, request.line_id, request.row_num, batch_to_picked_qty, batch_currently_picked, pack_size, item_key
        );
        
        info!(
            "🔍 INGREDIENT TOTALS (ALL BATCHES): ItemKey={}, TotalRequired={}, TotalPicked={}, RemainingForIngredient={}",
            item_key, ingredient_total_required, ingredient_total_picked, 
            ingredient_total_required - ingredient_total_picked
        );
        
        // Additional debugging for pallet advancement issues
        info!(
            "🎯 PALLET ADVANCEMENT DEBUG: Current pallet (Row{}): {:.1}/{:.1} bags, Ingredient overall: {:.1}/{:.1} bags",
            request.row_num, batch_currently_picked, batch_to_picked_qty,
            ingredient_total_picked, ingredient_total_required
        );

        // **Validation Rule 1A**: Batch-level validation (per-batch over-picking prevention)
        let requested_qty = self.safe_bigdecimal_to_f64(&request.picked_bulk_qty, "validation_picked_bulk_qty")?;
        let batch_remaining = batch_to_picked_qty - batch_currently_picked;
        
        // ROUNDING FIX: Use 3-decimal rounding to prevent floating-point precision issues
        let requested_qty_rounded = self.round_to_3_decimals(requested_qty);
        let batch_remaining_rounded = self.round_to_3_decimals(batch_remaining);
        
        // **Validation Rule 1B**: Ingredient-level completion validation (total across all batches)  
        let ingredient_remaining = ingredient_total_required - ingredient_total_picked;
        let ingredient_remaining_rounded = self.round_to_3_decimals(ingredient_remaining);
        
        info!(
            "📊 BATCH VALIDATION: requested_qty={} (rounded: {}), batch_remaining={} (rounded: {}) (batch: {:.1}/{:.1})",
            requested_qty, requested_qty_rounded, batch_remaining, batch_remaining_rounded, 
            batch_currently_picked, batch_to_picked_qty
        );
        
        info!(
            "📊 INGREDIENT VALIDATION: ingredient_remaining={} (rounded: {}) (total: {:.1}/{:.1} bags across all batches)",
            ingredient_remaining, ingredient_remaining_rounded, ingredient_total_picked, ingredient_total_required
        );
        
        // First check: Validate against the specific batch being picked  
        if self.qty_greater_than(requested_qty_rounded, batch_remaining_rounded) {
            let required_kg = self.round_to_3_decimals(batch_remaining * pack_size);
            warn!(
                "❌ BATCH OVER-PICKING: requested {} bags > batch remaining {} bags ({}KG) for Row{}",
                requested_qty_rounded, batch_remaining_rounded, required_kg, request.row_num
            );
            
            let error_msg = if batch_remaining_rounded <= 0.0 {
                // Enhanced error for completed batches during pallet advancement
                let remaining_ingredient = self.round_to_3_decimals(ingredient_total_required - ingredient_total_picked);
                if remaining_ingredient > 0.0 {
                    format!("Pallet Row{} is completed ({batch_currently_picked:.1}/{batch_to_picked_qty:.1} bags). Ingredient has {remaining_ingredient:.1} bags remaining in other pallets.", request.row_num)
                } else {
                    format!("This batch is already completed ({batch_currently_picked:.1}/{batch_to_picked_qty:.1} bags). Please refresh to load the next available batch.")
                }
            } else {
                format!("Cannot pick {} bags from pallet Row{}. Only {} bags remaining ({}KG)", requested_qty_rounded, request.row_num, batch_remaining_rounded, required_kg)
            };
            
            return Ok(PickValidationResult {
                is_valid: false,
                error_message: Some(error_msg),
                warnings: vec![],
                max_allowed_quantity: Some(BigDecimal::from_f64(batch_remaining_rounded.max(0.0)).unwrap_or_default()),
                available_inventory: None,
            });
        }
        
        // ENHANCED: Validate that batch has actual requirements (prevents "Qty Required 0" errors)
        if batch_to_picked_qty <= 0.0 {
            warn!(
                "❌ INVALID BATCH: Row{} has ToPickedBulkQty={}, should not be available for picking",
                request.row_num, batch_to_picked_qty
            );
            
            let error_msg = format!(
                "Pallet Row{} has no picking requirements (ToPickedBulkQty={}). This indicates a data synchronization issue.",
                request.row_num, batch_to_picked_qty
            );
            
            return Ok(PickValidationResult {
                is_valid: false,
                error_message: Some(error_msg),
                warnings: vec!["Data synchronization issue detected. Please refresh and try again.".to_string()],
                max_allowed_quantity: Some(BigDecimal::from(0)),
                available_inventory: None,
            });
        }
        
        // Second check: Validate against ingredient totals for completion detection 
        // ROUNDING FIX: Use rounded value from earlier calculation
        if ingredient_remaining_rounded <= 0.0 {
            warn!(
                "❌ INGREDIENT COMPLETED: Total ingredient already picked ({:.1}/{:.1} bags across all batches)",
                ingredient_total_picked, ingredient_total_required
            );
            
            let error_msg = format!(
                "Ingredient already fully picked ({ingredient_total_picked:.1}/{ingredient_total_required:.1} bags) across all batches"
            );
            
            return Ok(PickValidationResult {
                is_valid: false,
                error_message: Some(error_msg),
                warnings: vec![],
                max_allowed_quantity: Some(BigDecimal::from(0)),
                available_inventory: None,
            });
        }

        // **Validation Rule 2**: Lot availability check
        let lot_availability_query = r#"
            SELECT 
                l.QtyOnHand,
                l.QtyCommitSales,
                (l.QtyOnHand - l.QtyCommitSales) as AvailableQty
            FROM LotMaster l
            WHERE l.LotNo = @P1 
              AND l.ItemKey = @P2 
              AND l.LocationKey = 'TFC1' 
              AND l.BinNo = @P3
        "#;

        let mut lot_select = TiberiusQuery::new(lot_availability_query);
        lot_select.bind(request.lot_no.clone());
        lot_select.bind(item_key);
        lot_select.bind(request.bin_no.clone());

        let lot_stream = lot_select
            .query(&mut client)
            .await
            .context("Failed to execute lot availability query")?;

        let lot_rows: Vec<Row> = lot_stream
            .into_first_result()
            .await
            .context("Failed to get lot availability results")?;

        if let Some(lot_row) = lot_rows.first() {
            let available_qty: f64 = lot_row.get("AvailableQty").unwrap_or(0.0);
            let requested_kg = requested_qty * pack_size;
            
            // ROUNDING FIX: Use 3-decimal rounding for lot availability comparison
            let requested_kg_rounded = self.round_to_3_decimals(requested_kg);
            let available_qty_rounded = self.round_to_3_decimals(available_qty);

            if self.qty_greater_than(requested_kg_rounded, available_qty_rounded) {
                return Ok(PickValidationResult {
                    is_valid: false,
                    error_message: Some(format!(
                        "Insufficient lot availability. Requested: {:.3} KG, Available: {:.3} KG", 
                        requested_kg_rounded, available_qty_rounded
                    )),
                    warnings: vec![],
                    max_allowed_quantity: Some(BigDecimal::from_f64(available_qty_rounded / pack_size).unwrap_or_default()),
                    available_inventory: Some(BigDecimal::from_f64(available_qty_rounded).unwrap_or_default()),
                });
            }

            // Generate warnings for low inventory
            let mut warnings = Vec::new();
            if available_qty < (requested_kg * 2.0) {
                warnings.push(format!(
                    "Low inventory warning: Only {:.1} KG remaining after this pick",
                    available_qty - requested_kg
                ));
            }

            Ok(PickValidationResult {
                is_valid: true,
                error_message: None,
                warnings,
                max_allowed_quantity: Some(BigDecimal::from_f64(batch_remaining).unwrap_or_default()),
                available_inventory: Some(BigDecimal::from_f64(available_qty).unwrap_or_default()),
            })
        } else {
            Ok(PickValidationResult {
                is_valid: false,
                error_message: Some(format!(
                    "Lot {} not found or not available in bin {} for item {}",
                    request.lot_no, request.bin_no, item_key
                )),
                warnings: vec![],
                max_allowed_quantity: None,
                available_inventory: None,
            })
        }
    }

    /// Check if a specific pallet/batch is completed (picked quantity meets requirement)
    #[instrument(skip(self))]
    pub async fn is_pallet_completed(
        &self,
        run_no: i32,
        row_num: i32,
        line_id: i32,
    ) -> Result<bool> {
        info!("🔍 PALLET_CHECK: Checking completion status for run: {}, row_num: {}, line_id: {}", 
              run_no, row_num, line_id);
        
        let mut client = self.get_client().await
            .context("Failed to get primary database client for pallet completion check")?;
        
        let check_query = r#"
            SELECT 
                ToPickedBulkQty,
                ISNULL(PickedBulkQty, 0) as PickedBulkQty,
                CASE 
                    WHEN ISNULL(PickedBulkQty, 0) >= ToPickedBulkQty THEN 1 
                    ELSE 0 
                END as IsCompleted
            FROM cust_BulkPicked
            WHERE RunNo = @P1 AND RowNum = @P2 AND LineId = @P3
        "#;
        
        let mut query = TiberiusQuery::new(check_query);
        query.bind(run_no);
        query.bind(row_num);
        query.bind(line_id);
        
        let stream = query.query(&mut client).await
            .context("Failed to execute pallet completion check query")?;
        
        let rows: Vec<Row> = stream.into_first_result().await
            .context("Failed to get pallet completion results")?;
        
        if let Some(row) = rows.first() {
            let to_picked: f64 = row.get("ToPickedBulkQty").unwrap_or(0.0);
            let picked: f64 = row.get("PickedBulkQty").unwrap_or(0.0);
            let is_completed: i32 = row.get("IsCompleted").unwrap_or(0);
            
            info!("📊 PALLET_STATUS: run: {}, row_num: {}, line_id: {} - picked: {}/{} bags, completed: {}", 
                  run_no, row_num, line_id, picked, to_picked, is_completed > 0);
            
            Ok(is_completed > 0)
        } else {
            warn!("❌ PALLET_NOT_FOUND: No batch record found for run: {}, row_num: {}, line_id: {}", 
                  run_no, row_num, line_id);
            Ok(false)
        }
    }
    
    /// Find the next unpicked pallet for the same ingredient (ItemKey/LineId)
    #[instrument(skip(self))]
    pub async fn get_next_available_pallet(
        &self,
        run_no: i32,
        current_row_num: i32,
        line_id: i32,
    ) -> Result<Option<PalletBatchInfo>> {
        info!("🔍 NEXT_PALLET: Finding next available pallet for run: {}, current_row_num: {}, line_id: {}", 
              run_no, current_row_num, line_id);
        
        let mut client = self.get_client().await
            .context("Failed to get primary database client for next pallet search")?;
        
        let next_pallet_query = r#"
            SELECT 
                bp.RunNo,
                bp.RowNum,
                bp.LineId,
                bp.BatchNo,
                bp.ItemKey,
                bp.PackSize,
                bp.ToPickedBulkQty,
                ISNULL(bp.PickedBulkQty, 0) as PickedBulkQty,
                CASE 
                    WHEN ISNULL(bp.PickedBulkQty, 0) >= bp.ToPickedBulkQty THEN 1 
                    ELSE 0 
                END as IsCompleted,
                im.Description,
                ROW_NUMBER() OVER (ORDER BY bp.BatchNo) as PalletNumber
            FROM cust_BulkPicked bp
            INNER JOIN INMAST im ON bp.ItemKey = im.ItemKey
            WHERE bp.RunNo = @P1 
              AND bp.LineId = @P2 
              AND bp.RowNum != @P3
              AND bp.ToPickedBulkQty > 0
              AND ISNULL(bp.PickedBulkQty, 0) < bp.ToPickedBulkQty  -- Only unpicked pallets
            ORDER BY bp.BatchNo ASC  -- Get next pallet in sequence
        "#;
        
        let mut query = TiberiusQuery::new(next_pallet_query);
        query.bind(run_no);
        query.bind(line_id);
        query.bind(current_row_num);
        
        let stream = query.query(&mut client).await
            .context("Failed to execute next pallet search query")?;
        
        let rows: Vec<Row> = stream.into_first_result().await
            .context("Failed to get next pallet results")?;
        
        if let Some(row) = rows.first() {
            match self.row_to_pallet_batch_info(row) {
                Ok(pallet) => {
                    info!("✅ NEXT_PALLET_FOUND: run: {}, row_num: {}, line_id: {} - Next pallet: {} (RowNum: {})", 
                          run_no, current_row_num, line_id, pallet.pallet_number, pallet.row_num);
                    Ok(Some(pallet))
                }
                Err(e) => {
                    warn!("❌ PALLET_CONVERSION_ERROR: Failed to convert next pallet row: {}", e);
                    Ok(None)
                }
            }
        } else {
            info!("🔚 NO_MORE_PALLETS: No more unpicked pallets found for run: {}, line_id: {} after row_num: {}", 
                  run_no, line_id, current_row_num);
            Ok(None)
        }
    }

    /// Convert database row to PalletBatchInfo model for completion detection
    /// Handles SQL Server numeric types for direct cust_BulkPicked values
    fn row_to_pallet_batch_info(&self, row: &Row) -> Result<PalletBatchInfo> {
        // ROW_NUMBER() returns i64, convert to i32
        let pallet_number_i64: i64 = row.get::<i64, _>("PalletNumber").unwrap_or(0);
        let pallet_number: i32 = pallet_number_i64 as i32;
        
        let run_no: i32 = row.get("RunNo").unwrap_or(0);
        let row_num: i32 = row.get("RowNum").unwrap_or(0);
        let line_id: i32 = row.get("LineId").unwrap_or(0);
        let batch_no: &str = row.get("BatchNo").unwrap_or("");
        let item_key: &str = row.get("ItemKey").unwrap_or("");
        let description: &str = row.get("Description").unwrap_or("");
        let pack_size: f64 = row.get("PackSize").unwrap_or(25.0);
        let to_picked_bulk_qty: f64 = row.get("ToPickedBulkQty").unwrap_or(0.0);
        let picked_bulk_qty: f64 = row.get("PickedBulkQty").unwrap_or(0.0);
        let is_completed: i32 = row.get("IsCompleted").unwrap_or(0);

        Ok(PalletBatchInfo {
            run_no,
            row_num,
            line_id,
            batch_no: batch_no.to_string(),
            item_key: item_key.to_string(),
            pallet_number,
            pack_size,
            to_picked_bulk_qty,
            picked_bulk_qty,
            description: description.to_string(),
            is_completed: is_completed > 0,
        })
    }

    /// Convert database row to PalletBatch model
    /// Handles SQL Server numeric types for direct cust_BulkPicked values
    fn row_to_pallet_batch(&self, row: &Row) -> Result<PalletBatch> {
        // ROW_NUMBER() returns i64, convert to i32
        let pallet_number_i64: i64 = row.get::<i64, _>("PalletNumber").unwrap_or(0);
        let pallet_number: i32 = pallet_number_i64 as i32;
        let batch_number: &str = row.get("BatchNumber").unwrap_or("");
        let row_num: i32 = row.get("RowNum").unwrap_or(0);
        
        // Handle SQL Server numeric types - SQL Server returns DECIMAL calculations as f64
        let no_of_bags_picked: i32 = row.get::<f64, _>("NoOfBagsPicked").unwrap_or(0.0) as i32;
        let quantity_picked: f64 = row.get::<f64, _>("QuantityPicked").unwrap_or(0.0);
        let no_of_bags_remaining: i32 = row.get::<f64, _>("NoOfBagsRemaining").unwrap_or(0.0) as i32;
        
        let quantity_remaining: f64 = row.get::<f64, _>("QuantityRemaining").unwrap_or(0.0);

        Ok(PalletBatch {
            pallet_number,
            batch_number: batch_number.to_string(),
            row_num,
            no_of_bags_picked,
            quantity_picked,
            no_of_bags_remaining,
            quantity_remaining,
        })
    }


    /// **FIXED FUNCTION** - Check if a PalletID already exists for this pallet (RunNo + RowNum + LineId)
    /// Returns existing PalletID to maintain consistency across multiple picks for same logical pallet
    /// This fixes the BME4 compatibility issue where same pallet should have same PalletID
    /// **CRITICAL FIX**: Check BOTH tables since PalletID is created in Cust_BulkLotPicked first
    async fn get_existing_pallet_id(
        &self,
        client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
        run_no: i32,
        row_num: i32,
        line_id: i32,
    ) -> Result<Option<String>> {
        info!("🔍 PALLET_LOOKUP: Checking existing PalletID for run: {}, row_num: {}, line_id: {} in BOTH tables",
              run_no, row_num, line_id);

        // **FIX**: Check Cust_BulkLotPicked first since that's where PalletID is initially created (Step 2)
        let lot_picked_query = r#"
            SELECT TOP 1 PalletId
            FROM Cust_BulkLotPicked
            WHERE RunNo = @P1 AND RowNum = @P2 AND LineId = @P3
            ORDER BY RecDate DESC
        "#;

        let mut query = TiberiusQuery::new(lot_picked_query);
        query.bind(run_no);
        query.bind(row_num);
        query.bind(line_id);

        let stream = query.query(client).await
            .context("Failed to execute PalletID lookup in Cust_BulkLotPicked")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get PalletID lookup results from Cust_BulkLotPicked")?;

        if let Some(row) = rows.first() {
            let existing_pallet_id: Option<&str> = row.get("PalletId");
            if let Some(pallet_id) = existing_pallet_id {
                info!("✅ PALLET_FOUND_LOT: Existing PalletID {} found in Cust_BulkLotPicked for run: {}, row_num: {}, line_id: {}",
                      pallet_id, run_no, row_num, line_id);
                return Ok(Some(pallet_id.to_string()));
            }
        }

        // **FALLBACK**: Check Cust_BulkPalletLotPicked as secondary source
        let pallet_lot_query = r#"
            SELECT TOP 1 PalletID
            FROM Cust_BulkPalletLotPicked
            WHERE RunNo = @P1 AND RowNum = @P2 AND LineId = @P3
            ORDER BY RecDate DESC
        "#;

        let mut pallet_query = TiberiusQuery::new(pallet_lot_query);
        pallet_query.bind(run_no);
        pallet_query.bind(row_num);
        pallet_query.bind(line_id);

        let pallet_stream = pallet_query.query(client).await
            .context("Failed to execute PalletID lookup in Cust_BulkPalletLotPicked")?;

        let pallet_rows: Vec<Row> = pallet_stream
            .into_first_result()
            .await
            .context("Failed to get PalletID lookup results from Cust_BulkPalletLotPicked")?;

        if let Some(row) = pallet_rows.first() {
            let existing_pallet_id: Option<&str> = row.get("PalletID");
            if let Some(pallet_id) = existing_pallet_id {
                info!("✅ PALLET_FOUND_PALLET: Existing PalletID {} found in Cust_BulkPalletLotPicked for run: {}, row_num: {}, line_id: {}",
                      pallet_id, run_no, row_num, line_id);
                return Ok(Some(pallet_id.to_string()));
            }
        }

        info!("🆕 PALLET_NEW: No existing PalletID found in either table, will generate new one for run: {}, row_num: {}, line_id: {}",
              run_no, row_num, line_id);
        Ok(None)
    }

    /// Generate the next sequential PalletId using official Seqnum table pattern with dual database support
    /// Uses atomic UPDATE with OUTPUT clause for concurrency safety, matching BT document generation
    async fn generate_next_pallet_id(
        &self,
        client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    ) -> Result<String> {
        info!("🏷️ PALLET_GEN: Starting official PT sequence generation using Seqnum table (dual database support)");

        // **ATOMIC SEQUENCE INCREMENT PATTERN** - Use OUTPUT clause to increment and return value in single query
        // This ensures each pick operation gets a unique PalletId for traceability integrity
        let atomic_query = r#"
            UPDATE Seqnum
            SET SeqNum = SeqNum + 1
            OUTPUT INSERTED.SeqNum as NextSeq
            WHERE SeqName = 'PT'
        "#;

        info!("🔄 PALLET_GEN: Atomically incrementing PT sequence number for unique PalletId generation");

        // Execute atomic increment and retrieve in single operation on current database
        let query = TiberiusQuery::new(atomic_query);
        let stream = query
            .query(client)
            .await
            .context("Failed to increment PT sequence number atomically")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get PT sequence from Seqnum table")?;

        let row = rows.first().ok_or_else(|| anyhow::anyhow!("No PT sequence returned from Seqnum table"))?;

        let next_seq: i32 = row.get::<i32, _>("NextSeq").unwrap_or(623611);

        // Single database - no synchronization needed
        info!("✅ PALLET_GEN: Generated unique PalletId {} (atomic increment from Seqnum)", next_seq);

        Ok(format!("{next_seq}"))
    }
    
    /// Safe BigDecimal to f64 conversion with enhanced error handling
    /// **BME4 RUN COMPLETION DETECTION** - Step 6 of bulk picking workflow
    /// Check if all bulk ingredients are completed and update Cust_BulkRun status from NEW → PRINT
    /// Returns Ok(true) if status was updated, Ok(false) if not yet complete, Err for database errors
    /// NOTE: Available for manual completion trigger via API endpoints
    pub async fn check_and_update_run_completion(
        &self,
        client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
        run_no: i32,
        user_id: &str,
        bangkok_now: &str,
    ) -> Result<bool> {
        info!("🔍 DEBUG: Checking completion status for run {}", run_no);
        
        // **CRITICAL FIX**: Check pallet-level completion using actual PickedBulkQty values
        // Each pallet (RowNum) must be individually completed, not just ingredient aggregates
        // This fixes the premature status change bug by validating every single pallet
        let completion_check_query = r#"
            SELECT COUNT(*) as IncompleteCount
            FROM cust_BulkPicked bp
            WHERE bp.RunNo = @P1
              AND bp.ToPickedBulkQty > 0
              AND (bp.PickedBulkQty IS NULL OR bp.PickedBulkQty < bp.ToPickedBulkQty)
        "#;
        
        let mut check_stmt = TiberiusQuery::new(completion_check_query);
        check_stmt.bind(run_no);
        
        let check_result = check_stmt
            .query(client)
            .await
            .context("Failed to execute completion check query")?;
        
        let incomplete_count: i32 = if let Some(row) = check_result.into_row().await? {
            row.get::<i32, _>("IncompleteCount").unwrap_or(1)
        } else {
            return Err(anyhow::anyhow!("No result returned from completion check query"));
        };
        
        info!("📊 PALLET_CHECK: Run {} has {} incomplete pallets (pallet-level validation)", run_no, incomplete_count);

        // If there are incomplete pallets, run is not ready for completion
        if incomplete_count > 0 {
            info!("⏳ COMPLETION_CHECK: Run {} has {} incomplete pallets - remaining in NEW status",
                  run_no, incomplete_count);
            return Ok(false);
        }

        // **ALL PALLETS COMPLETED** - Update run status to PRINT
        info!("🎯 COMPLETION_SUCCESS: All pallets completed for run {} - Updating status NEW → PRINT", run_no);
        
        let update_run_status_query = r#"
            UPDATE Cust_BulkRun 
            SET Status = 'PRINT', 
                ModifiedDate = @P1,
                ModifiedBy = @P2
            WHERE RunNo = @P3 AND Status = 'NEW'
        "#;
        
        let mut update_stmt = TiberiusQuery::new(update_run_status_query);
        update_stmt.bind(bangkok_now);
        update_stmt.bind(user_id);
        update_stmt.bind(run_no);
        
        let update_result = update_stmt
            .execute(client)
            .await
            .context("Failed to update Cust_BulkRun status")?;
            
        let affected_rows = update_result.rows_affected().iter().sum::<u64>();
        
        if affected_rows == 0 {
            info!("📋 DEBUG: No rows updated - Run {} may already be PRINT status or not found", run_no);
            return Ok(false);
        }
        
        info!("✅ DEBUG: Successfully updated run {} status from NEW → PRINT (affected rows: {})", run_no, affected_rows);
        
        // Run completion status successfully updated
        
        Ok(true)
    }

    /// **REVERT STATUS OPERATION** - Revert bulk run status from PRINT back to NEW
    /// Used when user wants to make changes after run completion
    /// Updates all batch records for the specified run
    pub async fn revert_run_status_to_new(
        &self,
        run_no: i32,
        user_id: &str,
    ) -> Result<bool> {
        let mut client = self.get_client().await
            .context("Failed to get database connection for status revert")?;

        // Get current Bangkok timestamp
        let bangkok_now = crate::utils::timezone::bangkok_now_sql_server();

        info!("🔄 REVERT: Starting status revert for run {} (PRINT → NEW) by user {}", run_no, user_id);

        // Update all batch records for this run from PRINT to NEW
        let revert_query = r#"
            UPDATE Cust_BulkRun
            SET Status = 'NEW',
                ModifiedBy = @P1,
                ModifiedDate = @P2
            WHERE RunNo = @P3
              AND Status = 'PRINT'
        "#;

        // Truncate user_id to fit database field constraints (ModifiedBy is limited to 8 characters)
        let user_id_truncated = if user_id.len() > 8 {
            &user_id[..8]
        } else {
            user_id
        };

        let mut revert_stmt = tiberius::Query::new(revert_query);
        revert_stmt.bind(user_id_truncated);
        revert_stmt.bind(&bangkok_now);
        revert_stmt.bind(run_no);

        let revert_result = revert_stmt.execute(&mut client)
            .await
            .context("Failed to revert run status")?;

        let affected_rows = revert_result.rows_affected().iter().sum::<u64>();

        if affected_rows == 0 {
            // No rows were updated - either run doesn't exist or not in PRINT status
            warn!("⚠️ REVERT: No rows updated for run {} - run may not exist or not in PRINT status", run_no);
            return Ok(false);
        }

        info!("✅ REVERT: Successfully reverted run {} status PRINT → NEW (affected {} batch records) by user {}",
              run_no, affected_rows, user_id);

        // Status revert completed successfully

        Ok(true)
    }

    /// Prevents type conversion issues that could cause SQL Server binding errors
    fn safe_bigdecimal_to_f64(&self, value: &BigDecimal, field_name: &str) -> Result<f64> {
        // Method 1: Direct to_f64 conversion (preferred)
        if let Some(f64_value) = value.to_f64() {
            if f64_value.is_finite() && f64_value >= 0.0 {
                info!("🔢 DEBUG: {} converted directly: {} -> {}", field_name, value, f64_value);
                return Ok(f64_value);
            } else {
                warn!("⚠️ DEBUG: {} direct conversion resulted in invalid value: {}", field_name, f64_value);
            }
        }

        // Method 2: String parsing fallback
        let value_str = value.to_string();
        match value_str.parse::<f64>() {
            Ok(parsed_value) => {
                if parsed_value.is_finite() && parsed_value >= 0.0 {
                    info!("🔢 DEBUG: {} converted via string parsing: {} -> {}", field_name, value, parsed_value);
                    Ok(parsed_value)
                } else {
                    let error_msg = format!("Invalid {field_name} value after string parsing: {parsed_value}");
                    warn!("❌ DEBUG: {}", error_msg);
                    Err(anyhow::anyhow!("TYPE_CONVERSION_ERROR: {}", error_msg))
                }
            }
            Err(e) => {
                let error_msg = format!("Failed to parse {field_name} '{value_str}' as f64: {e}");
                warn!("❌ DEBUG: {}", error_msg);
                Err(anyhow::anyhow!("TYPE_CONVERSION_ERROR: {}", error_msg))
            }
        }
    }

    /// Round to 3 decimal places for consistent quantity comparisons
    /// This prevents floating-point precision issues like 76.10999999999999 vs 76.11
    fn round_to_3_decimals(&self, value: f64) -> f64 {
        (value * 1000.0).round() / 1000.0
    }

    /// Tolerance-based "greater than" comparison for floating-point quantities
    /// Returns true if `a > b + EPSILON` (where EPSILON = 0.0005 for 3 decimal precision)
    /// This handles cases where 76.10999999999999 should be considered equal to 76.11
    fn qty_greater_than(&self, a: f64, b: f64) -> bool {
        const EPSILON: f64 = 0.0005; // Half of 0.001 for 3-decimal tolerance
        a > b + EPSILON
    }

    /// Get inventory alerts for an item based on current stock levels and conditions
    #[instrument(skip(self))]
    pub async fn get_inventory_alerts(
        &self,
        run_no: i32,
        item_key: &str,
    ) -> Result<Vec<InventoryAlert>> {
        let mut alerts = Vec::new();

        // Get current inventory info
        let inventory = self.get_inventory_info(run_no, item_key).await?;
        
        let current_soh = inventory
            .soh_value
            .to_string()
            .parse::<f64>()
            .unwrap_or(0.0);
        
        // Check for out of stock
        if current_soh == 0.0 {
            alerts.push(InventoryAlert {
                alert_type: InventoryAlertType::OutOfStock,
                item_key: item_key.to_string(),
                message: format!("Item {item_key} is out of stock"),
                severity: AlertSeverity::Critical,
                recommended_action: Some("Check alternative items or contact purchasing".to_string()),
            });
        }
        // Check for low stock (less than 100 units)
        else if current_soh < 100.0 {
            alerts.push(InventoryAlert {
                alert_type: InventoryAlertType::LowStock,
                item_key: item_key.to_string(),
                message: format!(
                    "Low stock warning: {} has only {:.2} {} remaining",
                    item_key, current_soh, inventory.soh_uom
                ),
                severity: AlertSeverity::Warning,
                recommended_action: Some("Plan for replenishment soon".to_string()),
            });
        }
        
        // Check for expired lots
        let expired_lots: Vec<&LotInfo> = inventory
            .available_lots
            .iter()
            .filter(|lot| {
                lot.expiry_date
                    .map(|exp| exp < Utc::now())
                    .unwrap_or(false)
            })
            .collect();
        
        if !expired_lots.is_empty() {
            let lot_numbers: Vec<String> = expired_lots
                .iter()
                .map(|lot| lot.lot_no.clone())
                .collect();
            
            alerts.push(InventoryAlert {
                alert_type: InventoryAlertType::ExpiredLots,
                item_key: item_key.to_string(),
                message: format!(
                    "Warning: {} expired lots found: {}",
                    expired_lots.len(),
                    lot_numbers.join(", ")
                ),
                severity: AlertSeverity::Warning,
                recommended_action: Some("Remove expired lots from inventory".to_string()),
            });
        }
        
        // Check for lots expiring soon (within 30 days)
        let expiring_soon: Vec<&LotInfo> = inventory
            .available_lots
            .iter()
            .filter(|lot| {
                lot.expiry_date
                    .map(|exp| exp > Utc::now() && exp < Utc::now() + chrono::Duration::days(30))
                    .unwrap_or(false)
            })
            .collect();
        
        if !expiring_soon.is_empty() {
            let lot_numbers: Vec<String> = expiring_soon
                .iter()
                .map(|lot| lot.lot_no.clone())
                .collect();
            
            alerts.push(InventoryAlert {
                alert_type: InventoryAlertType::ExpiringSoon,
                item_key: item_key.to_string(),
                message: format!(
                    "{} lots expiring within 30 days: {}",
                    expiring_soon.len(),
                    lot_numbers.join(", ")
                ),
                severity: AlertSeverity::Info,
                recommended_action: Some("Prioritize these lots for picking".to_string()),
            });
        }
        
        Ok(alerts)
    }

    /// Get picked lots for a specific ingredient (for unpicking modal)
    pub async fn get_picked_lots_for_ingredient(
        &self,
        run_no: i32,
        row_num: i32,
        line_id: i32,
    ) -> Result<PickedLotsResponse, anyhow::Error> {
        let mut client = self.get_client().await
            .with_context(|| format!("Failed to connect to database for picked lots query - run: {run_no}, row: {row_num}, line: {line_id}"))?;

        info!("🔍 DEBUG: Getting picked lots for run: {}, row: {}, line: {}", run_no, row_num, line_id);

        // First get batch and item info
        let batch_query = r#"
            SELECT BatchNo, ItemKey 
            FROM cust_BulkPicked 
            WHERE RunNo = @P1 AND RowNum = @P2 AND LineId = @P3
        "#;

        info!("🔍 DEBUG: Executing batch query for run: {}, row: {}, line: {}", run_no, row_num, line_id);
        let mut batch_stmt = tiberius::Query::new(batch_query);
        batch_stmt.bind(run_no);
        batch_stmt.bind(row_num);
        batch_stmt.bind(line_id);

        let batch_result = batch_stmt.query(&mut client).await
            .context("Failed to execute batch query")?
            .into_row().await
            .context("Failed to get batch row")?;

        let (batch_no, item_key) = if let Some(row) = batch_result {
            let batch_no: &str = row.get("BatchNo").unwrap_or("");
            let item_key: &str = row.get("ItemKey").unwrap_or("");
            info!("🔍 DEBUG: Found batch info - BatchNo: '{}', ItemKey: '{}'", batch_no, item_key);
            (batch_no.to_string(), item_key.to_string())
        } else {
            info!("❌ DEBUG: No batch found in cust_BulkPicked for run: {}, row: {}, line: {}", run_no, row_num, line_id);
            return Err(anyhow::anyhow!("No batch found for run: {}, row: {}, line: {}", run_no, row_num, line_id));
        };

        // Get individual picked lot records (one record per lot-bin combination)
        // This shows separate records for each bin pick instead of aggregated batches
        // Each unique LotTranNo represents a separate pick operation that can be deleted individually
        let query = r#"
            SELECT 
                blp.LotTranNo,
                bp.BatchNo,
                blp.LotNo,
                blp.BinNo,
                bp.ItemKey,
                bp.Location as LocationKey,
                bp.RowNum,
                blp.RecDate,
                blp.RecUserid,
                blp.AllocLotQty,
                blp.PackSize,
                lm.DateExpiry,
                blp.QtyOnHand,
                blp.QtyReceived
            FROM Cust_BulkLotPicked blp
            INNER JOIN cust_BulkPicked bp ON bp.RunNo = blp.RunNo AND bp.RowNum = blp.RowNum AND bp.LineId = blp.LineId
            LEFT JOIN LotMaster lm ON lm.LotNo = blp.LotNo AND lm.ItemKey = bp.ItemKey AND lm.LocationKey = bp.Location AND lm.BinNo = blp.BinNo
            WHERE blp.RunNo = @P1 AND bp.ItemKey = @P2
            ORDER BY blp.RecDate ASC, blp.LotTranNo ASC
        "#;

        info!("🔍 DEBUG: Executing picked lots query for run: {}, ItemKey: '{}' (all batches)", run_no, item_key);
        let mut stmt = tiberius::Query::new(query);
        stmt.bind(run_no);
        stmt.bind(&item_key);

        let stream = stmt.query(&mut client).await
            .context("Failed to execute picked lots query")?;

        let mut picked_lots = Vec::new();
        let mut total_picked_qty = BigDecimal::from(0);

        let rows: Vec<Row> = stream.into_first_result().await
            .context("Failed to get picked lots result")?;

        info!("🔍 DEBUG: Found {} rows in Cust_BulkLotPicked table", rows.len());

        for row in rows {
            let lot_tran_no: i32 = row.get("LotTranNo").unwrap_or(0);
            let lot_no: &str = row.get("LotNo").unwrap_or("");
            let bin_no: &str = row.get("BinNo").unwrap_or("");
            let alloc_lot_qty: f64 = row.get("AllocLotQty").unwrap_or(0.0);
            let pack_size: f64 = row.get("PackSize").unwrap_or(20.0);
            let rec_userid: &str = row.get("RecUserid").unwrap_or("");
            let rec_date: chrono::NaiveDateTime = row.get("RecDate").unwrap_or_default();
            
            // Extract fields from joins
            let batch_no: &str = row.get("BatchNo").unwrap_or("");
            let item_key: &str = row.get("ItemKey").unwrap_or("");
            let location_key: &str = row.get("LocationKey").unwrap_or("");
            let row_num: i32 = row.get("RowNum").unwrap_or(0);
            let date_exp: Option<chrono::NaiveDateTime> = row.get("DateExpiry");
            let qty_on_hand: f64 = row.get("QtyOnHand").unwrap_or(0.0);
            let qty_received: f64 = row.get("QtyReceived").unwrap_or(0.0);

            info!("🔍 DEBUG: Processing individual picked lot - LotTranNo: {}, LotNo: '{}', BinNo: '{}', BatchNo: '{}', ItemKey: '{}', LocationKey: '{}', AllocLotQty: {}", 
                  lot_tran_no, lot_no, bin_no, batch_no, item_key, location_key, alloc_lot_qty);

            let picked_lot = PickedLot {
                lot_tran_no,
                lot_no: lot_no.to_string(),
                bin_no: bin_no.to_string(),
                batch_no: batch_no.to_string(),
                item_key: item_key.to_string(),
                location_key: location_key.to_string(),
                row_num,
                line_id,
                date_exp: date_exp.map(|dt| chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc)),
                qty_received: BigDecimal::from_f64(qty_received).unwrap_or_default(),
                alloc_lot_qty: BigDecimal::from_f64(alloc_lot_qty).unwrap_or_default(),
                pack_size: BigDecimal::from_f64(pack_size).unwrap_or_default(),
                qty_on_hand: BigDecimal::from_f64(qty_on_hand).unwrap_or_default(),
                rec_date: rec_date.format("%Y-%m-%d %H:%M:%S").to_string(),
                rec_userid: rec_userid.to_string(),
            };

            total_picked_qty += &picked_lot.alloc_lot_qty;
            picked_lots.push(picked_lot);
        }

        info!("✅ Found {} picked lots with total qty: {} for run: {}, ItemKey: '{}' (all batches)", picked_lots.len(), total_picked_qty, run_no, item_key);

        // Get item description for header
        let item_description_query = r#"
            SELECT Description
            FROM ItemMaster
            WHERE ItemKey = @P1
        "#;
        
        let mut desc_stmt = tiberius::Query::new(item_description_query);
        desc_stmt.bind(&item_key);
        
        let item_description = match desc_stmt.query(&mut client).await {
            Ok(stream) => {
                match stream.into_row().await {
                    Ok(Some(row)) => {
                        let desc = row.get::<&str, _>("Description").map(|s| s.to_string());
                        info!("✅ Found item description for {}: {:?}", item_key, desc);
                        desc
                    }
                    Ok(None) => {
                        info!("⚠️ No description found for item: {}", item_key);
                        None
                    }
                    Err(e) => {
                        error!("❌ Failed to parse ItemMaster row for {}: {}", item_key, e);
                        None
                    }
                }
            },
            Err(e) => {
                error!("❌ Failed to query ItemMaster for {}: {}", item_key, e);
                None
            }
        };

        // Get available lots for pending tab (reuse existing logic)
        let (available_lots, _) = match self.search_lots_for_run_item_paginated(run_no, &item_key, 1, 50).await {
            Ok(result) => {
                info!("✅ Successfully retrieved available lots for item: {}", item_key);
                result
            }
            Err(e) => {
                error!("❌ Failed to get available lots for item '{}' in run {}: {}", item_key, run_no, e);
                // Return empty results instead of failing the entire operation
                (Vec::new(), 0)
            }
        };

        // Get ALL batch requirements for "Pending to Picked" tab summary (official app format)
        // This shows ALL batches with WEIGHT calculations (bags × pack_size)
        let pending_batch_query = r#"
            SELECT DISTINCT
                bp.BatchNo, bp.ItemKey, bp.ToPickedBulkQty, bp.PackSize,
                (bp.ToPickedBulkQty * bp.PackSize) as TotalWeightNeeded,
                ISNULL(bp.PickedBulkQty, 0) as PickedBulkQty,
                (ISNULL(bp.PickedBulkQty, 0) * bp.PackSize) as TotalWeightPicked,
                (bp.ToPickedBulkQty - ISNULL(bp.PickedBulkQty, 0)) as RemainingQty,
                bp.RowNum, bp.LineId
            FROM cust_BulkPicked bp
            WHERE bp.RunNo = @P1 
              AND bp.ToPickedBulkQty > 0
            ORDER BY bp.ItemKey ASC, bp.BatchNo ASC
        "#;

        info!("🔍 DEBUG: Executing ALL batch requirements query for run: {} (all items for Pending to Picked tab)", run_no);
        let mut pending_stmt = tiberius::Query::new(pending_batch_query);
        pending_stmt.bind(run_no);

        let pending_batches = match pending_stmt.query(&mut client).await {
            Ok(stream) => {
                let rows: Vec<Row> = stream.into_first_result().await
                    .context("Failed to get pending batch requirements result")?;
                
                info!("🔍 DEBUG: Found {} pending batch requirements", rows.len());
                
                let mut batches = Vec::new();
                for row in rows {
                    let batch_no: &str = row.get("BatchNo").unwrap_or("");
                    let item_key_db: &str = row.get("ItemKey").unwrap_or("");
                    let to_picked_bulk_qty: f64 = row.get("ToPickedBulkQty").unwrap_or(0.0);
                    let pack_size: f64 = row.get("PackSize").unwrap_or(20.0);
                    let total_weight_needed: f64 = row.get("TotalWeightNeeded").unwrap_or(0.0);
                    let picked_bulk_qty: f64 = row.get("PickedBulkQty").unwrap_or(0.0);
                    let total_weight_picked: f64 = row.get("TotalWeightPicked").unwrap_or(0.0);
                    let remaining_qty: f64 = row.get("RemainingQty").unwrap_or(0.0);
                    let row_num: i32 = row.get("RowNum").unwrap_or(0);
                    let line_id: i32 = row.get("LineId").unwrap_or(0);

                    batches.push(crate::models::bulk_runs::PendingBatchRequirement {
                        batch_no: batch_no.to_string(),
                        item_key: item_key_db.to_string(),
                        to_picked_bulk_qty: BigDecimal::from_f64(to_picked_bulk_qty).unwrap_or_default(),
                        pack_size: BigDecimal::from_f64(pack_size).unwrap_or_default(),
                        total_weight_needed: BigDecimal::from_f64(total_weight_needed).unwrap_or_default(),
                        picked_bulk_qty: BigDecimal::from_f64(picked_bulk_qty).unwrap_or_default(),
                        total_weight_picked: BigDecimal::from_f64(total_weight_picked).unwrap_or_default(),
                        remaining_qty: BigDecimal::from_f64(remaining_qty).unwrap_or_default(),
                        row_num,
                        line_id,
                    });
                }
                batches
            }
            Err(e) => {
                error!("❌ Failed to get pending batch requirements for item '{}' in run {}: {}", item_key, run_no, e);
                Vec::new()
            }
        };

        let response = PickedLotsResponse {
            picked_lots,
            available_lots,
            pending_batches, // NEW: Add pending batch requirements
            total_picked_qty,
            batch_no,
            item_key,
            item_description,
            run_no,
        };
        
        info!("🎯 FINAL RESPONSE: Returning PickedLotsResponse for run: {}, row: {}, line: {} - picked_lots: {}, available_lots: {}, pending_batches: {}", 
              run_no, row_num, line_id, response.picked_lots.len(), response.available_lots.len(), response.pending_batches.len());
        
        Ok(response)
    }

    /// Get ALL picked lots for an entire run (across all ingredients)
    pub async fn get_all_picked_lots_for_run(
        &self,
        run_no: i32,
    ) -> Result<PickedLotsResponse, anyhow::Error> {
        let mut client = self.get_client().await
            .with_context(|| format!("Failed to connect to database for all picked lots query - run: {run_no}"))?;

        info!("🔍 DEBUG: Getting ALL picked lots for entire run: {}", run_no);

        // Get ALL individual picked lot records across ALL ingredients in the run
        // This shows separate records for each bin pick instead of aggregated batches
        // Each unique LotTranNo represents a separate pick operation that can be deleted individually
        let query = r#"
            SELECT 
                blp.LotTranNo,
                bp.BatchNo,
                blp.LotNo,
                blp.BinNo,
                bp.ItemKey,
                bp.Location as LocationKey,
                bp.RowNum,
                bp.LineId,
                blp.RecDate,
                blp.RecUserid,
                blp.AllocLotQty,
                blp.PackSize,
                lm.DateExpiry,
                blp.QtyOnHand,
                blp.QtyReceived
            FROM Cust_BulkLotPicked blp
            INNER JOIN cust_BulkPicked bp ON bp.RunNo = blp.RunNo AND bp.RowNum = blp.RowNum AND bp.LineId = blp.LineId
            LEFT JOIN LotMaster lm ON lm.LotNo = blp.LotNo AND lm.ItemKey = bp.ItemKey AND lm.LocationKey = bp.Location AND lm.BinNo = blp.BinNo
            WHERE blp.RunNo = @P1
            ORDER BY bp.ItemKey ASC, bp.BatchNo ASC, blp.RecDate ASC, blp.LotTranNo ASC
        "#;

        info!("🔍 DEBUG: Executing all picked lots query for run: {} (ALL ingredients)", run_no);
        let mut stmt = tiberius::Query::new(query);
        stmt.bind(run_no);

        let stream = stmt.query(&mut client).await
            .context("Failed to execute all picked lots query")?;

        let mut picked_lots = Vec::new();
        let mut total_picked_qty = BigDecimal::from(0);

        let rows: Vec<Row> = stream.into_first_result().await
            .context("Failed to get all picked lots result")?;

        info!("🔍 DEBUG: Found {} rows in Cust_BulkLotPicked table for all ingredients", rows.len());

        for row in rows {
            let lot_tran_no: i32 = row.get("LotTranNo").unwrap_or(0);
            let lot_no: &str = row.get("LotNo").unwrap_or("");
            let bin_no: &str = row.get("BinNo").unwrap_or("");
            let alloc_lot_qty: f64 = row.get("AllocLotQty").unwrap_or(0.0);
            let pack_size: f64 = row.get("PackSize").unwrap_or(20.0);
            let rec_userid: &str = row.get("RecUserid").unwrap_or("");
            let rec_date: chrono::NaiveDateTime = row.get("RecDate").unwrap_or_default();
            
            // Extract fields from joins
            let batch_no: &str = row.get("BatchNo").unwrap_or("");
            let item_key: &str = row.get("ItemKey").unwrap_or("");
            let location_key: &str = row.get("LocationKey").unwrap_or("");
            let row_num: i32 = row.get("RowNum").unwrap_or(0);
            let line_id: i32 = row.get("LineId").unwrap_or(0);
            let date_exp: Option<chrono::NaiveDateTime> = row.get("DateExpiry");
            let qty_on_hand: f64 = row.get("QtyOnHand").unwrap_or(0.0);
            let qty_received: f64 = row.get("QtyReceived").unwrap_or(0.0);

            info!("🔍 DEBUG: Processing individual picked lot - LotTranNo: {}, LotNo: '{}', BinNo: '{}', BatchNo: '{}', ItemKey: '{}', LocationKey: '{}', AllocLotQty: {}", 
                  lot_tran_no, lot_no, bin_no, batch_no, item_key, location_key, alloc_lot_qty);

            let picked_lot = PickedLot {
                lot_tran_no,
                lot_no: lot_no.to_string(),
                bin_no: bin_no.to_string(),
                batch_no: batch_no.to_string(),
                item_key: item_key.to_string(),
                location_key: location_key.to_string(),
                row_num,
                line_id,
                date_exp: date_exp.map(|dt| chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc)),
                qty_received: BigDecimal::from_f64(qty_received).unwrap_or_default(),
                alloc_lot_qty: BigDecimal::from_f64(alloc_lot_qty).unwrap_or_default(),
                pack_size: BigDecimal::from_f64(pack_size).unwrap_or_default(),
                qty_on_hand: BigDecimal::from_f64(qty_on_hand).unwrap_or_default(),
                rec_date: rec_date.format("%Y-%m-%d %H:%M:%S").to_string(),
                rec_userid: rec_userid.to_string(),
            };

            total_picked_qty += &picked_lot.alloc_lot_qty;
            picked_lots.push(picked_lot);
        }

        info!("✅ Found {} picked lots with total qty: {} for run: {} (ALL ingredients)", picked_lots.len(), total_picked_qty, run_no);

        // For all-run view, we don't need specific item details or available lots
        // Return a summary response with all picked lots
        let response = PickedLotsResponse {
            picked_lots,
            available_lots: Vec::new(), // Empty for all-run view
            pending_batches: Vec::new(), // Empty for all-run view
            total_picked_qty,
            batch_no: "".to_string(), // N/A for all-run view
            item_key: "ALL".to_string(), // Indicates all ingredients
            item_description: Some("All Run Picked Lots".to_string()),
            run_no,
        };
        
        info!("🎯 FINAL RESPONSE: Returning ALL PickedLotsResponse for run: {} - picked_lots: {}", 
              run_no, response.picked_lots.len());
        
        Ok(response)
    }

    /// Unpick entire batch (replicate official app pattern)
    pub async fn unpick_entire_batch(
        &self,
        run_no: i32,
        row_num: i32,
        line_id: i32,
        user_id: &str,
    ) -> Result<serde_json::Value, anyhow::Error> {
        let mut client = self.get_client().await
            .context("Failed to connect to database for unpick operation")?;

        info!("🔄 Starting unpick entire batch: run={}, row={}, line={}", run_no, row_num, line_id);

        // Start transaction
        client.simple_query("BEGIN TRANSACTION").await
            .context("Failed to start transaction")?;

        let result = self.execute_unpick_operations(&mut client, run_no, row_num, line_id, None, user_id).await;

        match result {
            Ok(data) => {
                client.simple_query("COMMIT").await
                    .context("Failed to commit unpick transaction")?;
                info!("✅ Successfully committed unpick batch transaction");
                Ok(data)
            }
            Err(e) => {
                let _ = client.simple_query("ROLLBACK").await;
                error!("❌ Unpick batch failed, rolled back: {}", e);
                Err(e)
            }
        }
    }

    /// Unpick specific lot (replicate official app pattern)
    pub async fn unpick_specific_lot(
        &self,
        run_no: i32,
        row_num: i32,
        line_id: i32,
        lot_no: &str,
        user_id: &str,
    ) -> Result<serde_json::Value, anyhow::Error> {
        let mut client = self.get_client().await
            .context("Failed to connect to database for unpick lot operation")?;

        info!("🔄 Starting unpick specific lot: run={}, row={}, line={}, lot={}", run_no, row_num, line_id, lot_no);

        // Start transaction
        client.simple_query("BEGIN TRANSACTION").await
            .context("Failed to start transaction")?;

        let result = self.execute_unpick_operations(&mut client, run_no, row_num, line_id, Some(lot_no), user_id).await;

        match result {
            Ok(data) => {
                client.simple_query("COMMIT").await
                    .context("Failed to commit unpick lot transaction")?;
                info!("✅ Successfully committed unpick lot transaction");
                Ok(data)
            }
            Err(e) => {
                let _ = client.simple_query("ROLLBACK").await;
                error!("❌ Unpick lot failed, rolled back: {}", e);
                Err(e)
            }
        }
    }

    /// Execute unpicking operations following official app pattern
    async fn execute_unpick_operations(
        &self,
        client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
        run_no: i32,
        row_num: i32,
        line_id: i32,
        specific_lot: Option<&str>,
        _user_id: &str,
    ) -> Result<serde_json::Value, anyhow::Error> {
        
        // Step 1: Validate that allocation records exist before attempting unpick operations
        // Check Cust_BulkLotPicked records directly to ensure unpick is valid
        let check_allocations_query = if let Some(lot) = specific_lot {
            format!(r#"
                SELECT COUNT(*) as AllocationCount
                FROM Cust_BulkLotPicked
                WHERE RunNo = {run_no} AND RowNum = {row_num} AND LineId = {line_id} AND LotNo = '{lot}'
            "#)
        } else {
            // For batch unpick, check ALL allocation records for this specific batch (RowNum)
            format!(r#"
                SELECT COUNT(*) as AllocationCount
                FROM Cust_BulkLotPicked
                WHERE RunNo = {run_no} AND RowNum = {row_num} AND LineId = {line_id}
            "#)
        };

        let stream = client.simple_query(&check_allocations_query).await
            .context("Failed to check allocation records")?;
        
        let rows: Vec<Row> = stream.into_first_result().await
            .context("Failed to process allocation count")?;

        if let Some(row) = rows.first() {
            let allocation_count: i32 = row.get("AllocationCount").unwrap_or(0);
            if allocation_count == 0 {
                info!("✅ No allocations found - ingredient already unpicked (success)");
                return Ok(serde_json::json!({
                    "message": "No allocations found - ingredient already unpicked",
                    "status": "success",
                    "operation_type": "unpick_already_clean",
                    "allocation_count": 0
                }));
            }
            info!("✅ Found {} allocation records to unpick", allocation_count);
        } else {
            info!("✅ No allocation records exist - ingredient already unpicked (success)");
            return Ok(serde_json::json!({
                "message": "No allocation records exist - ingredient already unpicked",
                "status": "success",
                "operation_type": "unpick_already_clean",
                "allocation_count": 0
            }));
        }

        // Step 1a: Fetch BatchNo for PNITEM update
        // We need BatchNo to update the PNITEM table later
        let get_batch_no_query = format!(r#"
            SELECT TOP 1 BatchNo
            FROM cust_BulkPicked
            WHERE RunNo = {run_no} AND RowNum = {row_num} AND LineId = {line_id}
        "#);

        let batch_stream = client.simple_query(&get_batch_no_query).await
            .context("Failed to get BatchNo")?;
        
        let batch_rows: Vec<Row> = batch_stream.into_first_result().await
            .context("Failed to process BatchNo rows")?;

        let batch_no = if let Some(row) = batch_rows.first() {
            row.get::<&str, _>("BatchNo").unwrap_or("").to_string()
        } else {
            String::new()
        };

        if batch_no.is_empty() {
            warn!("⚠️ BatchNo not found for run={}, row={}, line={}. PNITEM update will be skipped.", run_no, row_num, line_id);
        } else {
            info!("✅ Found BatchNo: {} for PNITEM update", batch_no);
            
            // Step 1a.1: Check Batch Status in PNMAST
            // Prevent deletion if batch is already closed
            self.check_batch_status(client, &batch_no).await?;
        }

        // Step 1b: Get actual issued quantities from LotTransaction for rollback calculations
        // Use direct LotTransaction query with batch number matching - collect LotTranNo values for cleanup
        let get_issued_quantities_query = if let Some(lot) = specific_lot {
            format!(r#"
                SELECT lt.LotNo, lt.ItemKey, lt.BinNo,
                       SUM(lt.QtyIssued) as ActualIssued,
                       STRING_AGG(CAST(lt.LotTranNo AS VARCHAR), ',') as LotTranNos
                FROM LotTransaction lt
                WHERE EXISTS (
                    SELECT 1 FROM Cust_BulkLotPicked blp
                    WHERE blp.RunNo = {run_no} AND blp.RowNum = {row_num} AND blp.LineId = {line_id}
                    AND blp.LotNo = '{lot}' AND lt.IssueDocNo = blp.BatchNo
                ) AND lt.LotNo = '{lot}' AND lt.TransactionType = 5 AND lt.User5 = 'Picking Customization'
                GROUP BY lt.LotNo, lt.ItemKey, lt.BinNo
            "#)
        } else {
            // For batch unpick, get actual issued quantities for this specific batch (RowNum)
            format!(r#"
                SELECT lt.LotNo, lt.ItemKey, lt.BinNo,
                       SUM(lt.QtyIssued) as ActualIssued,
                       STRING_AGG(CAST(lt.LotTranNo AS VARCHAR), ',') as LotTranNos
                FROM LotTransaction lt
                WHERE EXISTS (
                    SELECT 1 FROM Cust_BulkLotPicked blp
                    WHERE blp.RunNo = {run_no} AND blp.RowNum = {row_num} AND blp.LineId = {line_id}
                    AND lt.IssueDocNo = blp.BatchNo
                ) AND lt.TransactionType = 5 AND lt.User5 = 'Picking Customization'
                GROUP BY lt.LotNo, lt.ItemKey, lt.BinNo
            "#)
        };

        let stream = client.simple_query(&get_issued_quantities_query).await
            .context("Failed to get actual issued quantities")?;
        
        let rows: Vec<Row> = stream.into_first_result().await
            .context("Failed to process issued quantity rows")?;

        let mut total_qty_to_rollback = 0.0;
        let mut lot_rollbacks = Vec::new();
        let mut all_lot_tran_nos = Vec::new();

        // Process LotTransaction records if found (for QtyCommitSales rollback and cleanup)
        // Note: It's possible no LotTransaction records are found if they were created differently
        // but the unpick should still proceed with allocation record deletion
        for row in rows {
            let lot_no: &str = row.get("LotNo").unwrap_or("");
            let item_key: &str = row.get("ItemKey").unwrap_or("");
            let bin_no: &str = row.get("BinNo").unwrap_or("");
            let actual_issued: f64 = row.get("ActualIssued").unwrap_or(0.0);
            let lot_tran_nos_str: &str = row.get("LotTranNos").unwrap_or("");
            
            // Parse comma-separated LotTranNo values
            if !lot_tran_nos_str.is_empty() {
                for lot_tran_no_str in lot_tran_nos_str.split(',') {
                    if let Ok(lot_tran_no) = lot_tran_no_str.trim().parse::<i64>() {
                        all_lot_tran_nos.push(lot_tran_no);
                    }
                }
            }
            
            total_qty_to_rollback += actual_issued;
            lot_rollbacks.push((lot_no.to_string(), item_key.to_string(), bin_no.to_string(), actual_issued));
        }

        info!("✅ Found {} LotTransaction records for rollback calculations (total qty: {}, LotTranNos: {})", 
              lot_rollbacks.len(), total_qty_to_rollback, all_lot_tran_nos.len());

        // Step 2: Reset picked quantities in cust_BulkPicked (inventory integrity first)
        let reset_picked_query = if specific_lot.is_some() {
            // For specific lot unpick, use partial reduction with rollback quantity
            if total_qty_to_rollback > 0.0 {
                // Use calculated rollback quantity from LotTransaction records
                format!(r#"
                    UPDATE cust_BulkPicked
                    SET PickedBulkQty = CASE
                        WHEN PickedBulkQty - {total_qty_to_rollback} <= 0 THEN 0
                        ELSE PickedBulkQty - {total_qty_to_rollback}
                    END,
                    PickedQty = CASE
                        WHEN PickedBulkQty - {total_qty_to_rollback} <= 0 THEN 0
                        ELSE (PickedBulkQty - {total_qty_to_rollback}) * PackSize
                    END,
                    ItemBatchStatus = CASE
                        WHEN PickedBulkQty - {total_qty_to_rollback} <= 0 THEN NULL
                        ELSE 'Allocated'
                    END,
                    PickingDate = CASE
                        WHEN PickedBulkQty - {total_qty_to_rollback} <= 0 THEN NULL
                        ELSE PickingDate
                    END,
                    ModifiedBy = CASE
                        WHEN PickedBulkQty - {total_qty_to_rollback} <= 0 THEN NULL
                        ELSE ModifiedBy
                    END
                    WHERE RunNo = {run_no} AND RowNum = {row_num} AND LineId = {line_id}
                "#)
            } else {
                // Fallback: Reset specific row only when no rollback quantity found
                format!(r#"
                    UPDATE cust_BulkPicked
                    SET PickedBulkQty = 0, PickedQty = 0,
                        ItemBatchStatus = NULL, PickingDate = NULL, ModifiedBy = NULL
                    WHERE RunNo = {run_no} AND RowNum = {row_num} AND LineId = {line_id}
                "#)
            }
        } else {
            // For entire batch unpick, reset ONLY this specific batch
            format!(r#"
                UPDATE cust_BulkPicked
                SET PickedBulkQty = 0, PickedQty = 0,
                    ItemBatchStatus = NULL, PickingDate = NULL, ModifiedBy = NULL
                WHERE RunNo = {run_no} AND RowNum = {row_num} AND LineId = {line_id}
            "#)
        };

        client.simple_query(&reset_picked_query).await
            .context("Failed to reset picked quantities")?;
        info!("✅ Reset picked quantities in cust_BulkPicked (Step 2 - inventory integrity first)");

        // Step 3: Rollback inventory commitments in LotMaster with safety checks
        // Only subtract what WE actually added, preserving pre-existing commitments
        if !lot_rollbacks.is_empty() {
            for (lot_no, item_key, bin_no, actual_issued) in &lot_rollbacks {
                let rollback_query = format!(r#"
                    UPDATE LotMaster
                    SET QtyCommitSales = CASE
                        WHEN QtyCommitSales - {actual_issued} < 0 THEN 0
                        ELSE QtyCommitSales - {actual_issued}
                    END
                    WHERE ItemKey = '{item_key}' AND LotNo = '{lot_no}' AND BinNo = '{bin_no}'
                "#);

                client.simple_query(&rollback_query).await
                    .with_context(|| format!("Failed to rollback inventory for lot {lot_no} bin {bin_no}"))?;
                info!("✅ Rolled back {} qty for lot {} item {} bin {} (Step 3 - inventory rollback)", actual_issued, lot_no, item_key, bin_no);
            }
        } else {
            info!("⚠️ No LotTransaction records found for QtyCommitSales rollback - deletion will still proceed");
        }

        // Step 4: Delete corresponding LotTransaction records for complete audit trail cleanup
        // This now works because Cust_BulkLotPicked records still exist for the EXISTS check
        if !all_lot_tran_nos.is_empty() {
            let lot_tran_nos_str = all_lot_tran_nos.iter()
                .map(|n| n.to_string())
                .collect::<Vec<String>>()
                .join(",");
                
            let delete_lot_transaction_query = if let Some(lot) = specific_lot {
                format!(r#"
                    DELETE FROM LotTransaction
                    WHERE TransactionType = 5
                      AND User5 = 'Picking Customization'
                      AND LotTranNo IN ({lot_tran_nos_str})
                      AND EXISTS (
                          SELECT 1 FROM Cust_BulkLotPicked blp
                          WHERE blp.RunNo = {run_no} AND blp.RowNum = {row_num} AND blp.LineId = {line_id}
                          AND blp.LotNo = '{lot}' AND LotTransaction.IssueDocNo = blp.BatchNo
                      )
                "#)
            } else {
                format!(r#"
                    DELETE FROM LotTransaction
                    WHERE TransactionType = 5
                      AND User5 = 'Picking Customization'
                      AND LotTranNo IN ({lot_tran_nos_str})
                      AND EXISTS (
                          SELECT 1 FROM Cust_BulkLotPicked blp
                          WHERE blp.RunNo = {run_no} AND blp.RowNum = {row_num} AND blp.LineId = {line_id}
                          AND LotTransaction.IssueDocNo = blp.BatchNo
                      )
                "#)
            };

            // Execute the DELETE operation to clean up LotTransaction audit records
            client.simple_query(&delete_lot_transaction_query).await
                .context("Failed to delete LotTransaction audit records")?;
                
            info!("✅ Deleted {} LotTransaction records (Step 4 - audit cleanup)", all_lot_tran_nos.len());
        } else {
            info!("⚠️ No LotTransaction records found to delete (this is normal if records were created differently)");
        }

        // Step 5: Delete allocation records (after audit cleanup)
        let delete_allocations_query = if let Some(lot) = specific_lot {
            format!(r#"
                DELETE FROM Cust_BulkLotPicked
                WHERE RunNo = {run_no} AND RowNum = {row_num} AND LineId = {line_id} AND LotNo = '{lot}'
            "#)
        } else {
            // For batch unpick, delete allocations for this specific batch (RowNum)
            format!(r#"
                DELETE FROM Cust_BulkLotPicked
                WHERE RunNo = {run_no} AND RowNum = {row_num} AND LineId = {line_id}
            "#)
        };

        client.simple_query(&delete_allocations_query).await
            .context("Failed to delete allocation records")?;
        info!("✅ Deleted allocation records from Cust_BulkLotPicked (Step 5)");

        // Step 6: Clean up pallet traceability records (mobile app only)
        let delete_pallet_query = if let Some(_lot) = specific_lot {
            format!(r#"
                DELETE FROM Cust_BulkPalletLotPicked
                WHERE RunNo = {run_no} AND RowNum = {row_num} AND LineId = {line_id}
            "#) // Note: no lot-specific filter in this table
        } else {
            // For batch unpick, delete pallet records for this specific batch (RowNum)
            format!(r#"
                DELETE FROM Cust_BulkPalletLotPicked
                WHERE RunNo = {run_no} AND RowNum = {row_num} AND LineId = {line_id}
            "#)
        };

        client.simple_query(&delete_pallet_query).await
            .context("Failed to delete pallet traceability records")?;
        info!("✅ Deleted pallet traceability records (Step 6)");

        // Step 7: Revert PNITEM table quantities
        // Logic mirrors Phase 6 in picking_service.rs and unpick_by_lot_tran_no
        if total_qty_to_rollback > 0.0 && !batch_no.is_empty() {
            info!("🔄 DEBUG: STEP 7 - Reverting PNITEM table quantities (Qty: {} KG)", total_qty_to_rollback);

            let revert_pnitem_query = r#"
                DECLARE @Remainder float;

                -- Calculate remainder first (what the qty WOULD be after decrement)
                SELECT @Remainder = ActualQty - @P1
                FROM PNITEM WITH (UPDLOCK, ROWLOCK)
                WHERE BatchNo = @P2 AND Lineid = @P3;

                -- Apply updates based on Remainder
                -- If Remainder <= 0.001 (Fully Unpicked), reset to StdQty and Status 'R'
                -- Else (Partially Unpicked), set to Remainder and keep Status 'A'
                UPDATE PNITEM
                SET ActualQty = CASE WHEN @Remainder <= 0.001 THEN StdQty ELSE @Remainder END,
                    AllocQty = CASE WHEN @Remainder <= 0.001 THEN 0 ELSE AllocQty - @P1 END,
                    ActualQtyDispUom = CASE WHEN @Remainder <= 0.001 THEN StdQty ELSE @Remainder END,
                    SerLotQty = CASE WHEN @Remainder <= 0.001 THEN 0 ELSE SerLotQty - @P1 END,
                    Status = CASE WHEN @Remainder <= 0.001 THEN 'R' ELSE Status END,
                    User11 = CASE WHEN @Remainder <= 0.001 THEN NULL ELSE User11 END
                WHERE BatchNo = @P2 AND Lineid = @P3
            "#;

            let mut pnitem_stmt = TiberiusQuery::new(revert_pnitem_query);
            pnitem_stmt.bind(total_qty_to_rollback); // @P1 - Quantity being removed (KG)
            pnitem_stmt.bind(batch_no.as_str());     // @P2
            pnitem_stmt.bind(line_id);               // @P3

            match pnitem_stmt.execute(client).await {
                Ok(_) => {
                    info!("✅ STEP_7_SUCCESS: PNITEM reverted successfully");
                },
                Err(e) => {
                    let error_msg = format!("STEP 7 FAILED: PNITEM revert failed - Database error: {e}");
                    error!("❌ STEP_7_ERROR: {}", error_msg);
                    return Err(anyhow::anyhow!("STEP_7_REVERT_FAILED: {}", error_msg));
                }
            }

            // **STEP 7b: CHECK AND REVERT PNMAST STATUS**
            // If all PNITEM.User11 for this batch are NULL, revert PNMAST status to 'R'
            info!("🔄 DEBUG: STEP 7b - Checking PNMAST status reversion for batch: {}", batch_no);
            
            let check_pnitem_query = r#"
                SELECT COUNT(*) as active_count
                FROM PNITEM
                WHERE BatchNo = @P1 AND User11 IS NOT NULL
            "#;
            
            let mut check_stmt = TiberiusQuery::new(check_pnitem_query);
            check_stmt.bind(batch_no.as_str());
            
            // Extract value first to release the borrow on client
            let active_count: Option<i32> = if let Ok(stream) = check_stmt.query(client).await {
                if let Ok(Some(r)) = stream.into_row().await {
                    r.get::<i32, _>(0)
                } else {
                    None
                }
            } else {
                None
            };
            
            // Now safe to use client again
            if let Some(count) = active_count {
                if count == 0 {
                    // All PNITEM.User11 are NULL - revert PNMAST to 'R'
                    let revert_pnmast_query = r#"
                        UPDATE PNMAST
                        SET Status = 'R'
                        WHERE BatchNo = @P1 AND Status = 'A'
                    "#;
                    
                    let mut revert_stmt = TiberiusQuery::new(revert_pnmast_query);
                    revert_stmt.bind(batch_no.as_str());
                    
                    match revert_stmt.execute(client).await {
                        Ok(result) => {
                            let rows_affected = result.rows_affected().iter().sum::<u64>();
                            if rows_affected > 0 {
                                info!("✅ STEP_7b_SUCCESS: PNMAST status reverted 'A' → 'R' for batch: {} (all User11 NULL)", batch_no);
                            } else {
                                info!("ℹ️ STEP_7b_SKIP: PNMAST status already 'R' for batch: {}", batch_no);
                            }
                        },
                        Err(e) => {
                            warn!("⚠️ STEP_7b_WARNING: Failed to revert PNMAST status: {}. Continuing...", e);
                        }
                    }
                } else {
                    info!("ℹ️ STEP_7b_SKIP: PNMAST status remains 'A' for batch: {} ({} active PNITEM rows)", batch_no, count);
                }
            }
        } else {
            info!("ℹ️ Skipping PNITEM revert (Qty: {}, BatchNo: '{}')", total_qty_to_rollback, batch_no);
        }

        // Ensure transaction commit completion
        info!("🔄 Transaction completed for unpick operation - database state now consistent");


        // Return summary
        Ok(serde_json::json!({
            "unpicked_lots": lot_rollbacks.len(),
            "total_qty_rolled_back": total_qty_to_rollback,
            "lot_transaction_cleanup": "completed",
            "operation_type": if specific_lot.is_some() { "lot" } else { "batch" },
            "run_no": run_no,
            "row_num": row_num,
            "line_id": line_id
        }))
    }

    /// Unpick all lots from all ingredients in a run
    pub async fn unpick_all_run_lots(
        &self,
        run_no: i32,
        user_id: &str,
    ) -> Result<serde_json::Value, anyhow::Error> {
        info!("🔄 Starting unpick ALL lots for entire run: {}", run_no);

        // First, get all ingredients that have picked lots
        let mut client = self.get_client().await
            .context("Failed to connect to database for unpick all operation")?;

        // Get all ingredients with picked lots in this run
        let query = r#"
            SELECT DISTINCT bp.RowNum, bp.LineId, bp.ItemKey, bp.BatchNo
            FROM cust_BulkPicked bp
            INNER JOIN Cust_BulkLotPicked blp ON bp.RunNo = blp.RunNo AND bp.RowNum = blp.RowNum AND bp.LineId = blp.LineId
            WHERE bp.RunNo = @P1
            ORDER BY bp.ItemKey ASC, bp.BatchNo ASC
        "#;

        info!("🔍 Finding all ingredients with picked lots for run: {}", run_no);
        let mut stmt = tiberius::Query::new(query);
        stmt.bind(run_no);

        let stream = stmt.query(&mut client).await
            .context("Failed to get ingredients with picked lots")?;

        let rows: Vec<Row> = stream.into_first_result().await
            .context("Failed to process ingredients with picked lots")?;

        info!("📋 Found {} ingredients with picked lots to unpick", rows.len());

        if rows.is_empty() {
            return Ok(serde_json::json!({
                "message": "No picked lots found to unpick",
                "ingredients_processed": 0,
                "total_lots_removed": 0,
                "run_no": run_no
            }));
        }

        let mut total_ingredients_processed = 0;
        let mut total_errors = Vec::new();
        let total_ingredients = rows.len();

        // Collect unique batch numbers for PNMAST status check
        let mut unique_batches: std::collections::HashSet<String> = std::collections::HashSet::new();
        
        // Process each ingredient
        for row in rows {
            let row_num: i32 = row.get("RowNum").unwrap_or(0);
            let line_id: i32 = row.get("LineId").unwrap_or(0);
            let item_key: &str = row.get("ItemKey").unwrap_or("");
            let batch_no: &str = row.get("BatchNo").unwrap_or("");
            
            // Collect batch number for later PNMAST check
            if !batch_no.is_empty() {
                unique_batches.insert(batch_no.to_string());
            }

            info!("🎯 Unpicking ingredient: {} (row: {}, line: {})", item_key, row_num, line_id);

            // Use existing unpick_entire_batch function for each ingredient
            match self.unpick_entire_batch(run_no, row_num, line_id, user_id).await {
                Ok(response) => {
                    total_ingredients_processed += 1;
                    // Check if this was an "already clean" response
                    if let Some(operation_type) = response.get("operation_type") {
                        if operation_type == "unpick_already_clean" {
                            info!("✅ Ingredient {} already unpicked (no work needed)", item_key);
                        } else {
                            info!("✅ Successfully unpicked ingredient: {}", item_key);
                        }
                    } else {
                        info!("✅ Successfully unpicked ingredient: {}", item_key);
                    }
                },
                Err(e) => {
                    error!("❌ Failed to unpick ingredient {}: {}", item_key, e);
                    total_errors.push(format!("Failed to unpick {item_key}: {e}"));
                }
            }
        }

        // **FINAL PNMAST STATUS CHECK** - Check and revert PNMAST for all unique batches
        // This is a final pass to ensure all batch statuses are correctly reverted
        info!("🔄 DEBUG: Final PNMAST status check for {} unique batches", unique_batches.len());
        
        for batch_no in &unique_batches {
            let check_pnitem_query = r#"
                SELECT COUNT(*) as active_count
                FROM PNITEM
                WHERE BatchNo = @P1 AND User11 IS NOT NULL
            "#;
            
            let mut check_stmt = tiberius::Query::new(check_pnitem_query);
            check_stmt.bind(batch_no.as_str());
            
            // Extract value first to release the borrow on client
            let active_count: Option<i32> = if let Ok(stream) = check_stmt.query(&mut client).await {
                if let Ok(Some(r)) = stream.into_row().await {
                    r.get::<i32, _>(0)
                } else {
                    None
                }
            } else {
                None
            };
            
            // Now safe to use client again
            if let Some(count) = active_count {
                if count == 0 {
                    // All PNITEM.User11 are NULL - revert PNMAST to 'R'
                    let revert_pnmast_query = r#"
                        UPDATE PNMAST
                        SET Status = 'R'
                        WHERE BatchNo = @P1 AND Status = 'A'
                    "#;
                    
                    let mut revert_stmt = tiberius::Query::new(revert_pnmast_query);
                    revert_stmt.bind(batch_no.as_str());
                    
                    match revert_stmt.execute(&mut client).await {
                        Ok(result) => {
                            let rows_affected = result.rows_affected().iter().sum::<u64>();
                            if rows_affected > 0 {
                                info!("✅ PNMAST_REVERT_SUCCESS: Status reverted 'A' → 'R' for batch: {} (all User11 NULL)", batch_no);
                            }
                        },
                        Err(e) => {
                            warn!("⚠️ PNMAST_REVERT_WARNING: Failed to revert status for batch {}: {}", batch_no, e);
                        }
                    }
                }
            }
        }

        let result = serde_json::json!({
            "message": if total_errors.is_empty() {
                format!("Successfully unpicked all lots from {total_ingredients_processed} ingredients")
            } else {
                format!("Unpicked {} ingredients with {} errors", total_ingredients_processed, total_errors.len())
            },
            "ingredients_processed": total_ingredients_processed,
            "total_ingredients": total_ingredients,
            "errors": total_errors,
            "run_no": run_no,
            "operation_type": "run_wide_unpick"
        });

        if total_errors.is_empty() {
            info!("✅ Successfully completed run-wide unpick for run: {} - processed {} ingredients", 
                  run_no, total_ingredients_processed);
        } else {
            warn!("⚠️ Completed run-wide unpick with errors for run: {} - processed {}/{} ingredients", 
                  run_no, total_ingredients_processed, total_ingredients);
        }

        Ok(result)
    }

    /// Unpick specific picked lot using LotTranNo for precise targeting
    /// This is the new precise unpick function that targets exactly one record
    pub async fn unpick_by_lot_tran_no(
        &self,
        lot_tran_no: i32,
        user_id: &str,
    ) -> Result<serde_json::Value, anyhow::Error> {
        let mut client = self.get_client().await
            .context("Failed to connect to database for precise unpick operation")?;

        info!("🎯 Starting precise unpick using LotTranNo: {}", lot_tran_no);

        // Start transaction
        client.simple_query("BEGIN TRANSACTION").await
            .context("Failed to start transaction")?;

        let result = self.execute_precise_unpick_operations(&mut client, lot_tran_no, user_id).await;

        match result {
            Ok(data) => {
                client.simple_query("COMMIT").await
                    .context("Failed to commit precise unpick transaction")?;
                info!("✅ Successfully committed precise unpick transaction for LotTranNo: {}", lot_tran_no);
                Ok(data)
            }
            Err(e) => {
                let _ = client.simple_query("ROLLBACK").await;
                error!("❌ Precise unpick failed for LotTranNo: {}, rolled back: {}", lot_tran_no, e);
                Err(e)
            }
        }
    }

    /// Execute precise unpicking operations using LotTranNo for exact targeting
    async fn execute_precise_unpick_operations(
        &self,
        client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
        lot_tran_no: i32,
        _user_id: &str,
    ) -> Result<serde_json::Value, anyhow::Error> {
        
        // Step 1: Get the specific allocation record using LotTranNo
        let get_allocation_query = r#"
            SELECT blp.RunNo, blp.RowNum, blp.LineId, blp.LotNo, blp.BinNo,
                   blp.ItemKey, blp.AllocLotQty, blp.BatchNo,
                   bp.PackSize, bp.ItemKey as BulkPickedItemKey
            FROM Cust_BulkLotPicked blp
            INNER JOIN cust_BulkPicked bp ON bp.RunNo = blp.RunNo AND bp.RowNum = blp.RowNum AND bp.LineId = blp.LineId
            WHERE blp.LotTranNo = @P1
        "#;

        info!("🔍 Getting allocation record for LotTranNo: {}", lot_tran_no);
        let mut stmt = tiberius::Query::new(get_allocation_query);
        stmt.bind(lot_tran_no);

        let stream = stmt.query(client).await
            .context("Failed to get allocation record")?;
        
        let rows: Vec<Row> = stream.into_first_result().await
            .context("Failed to process allocation record")?;

        if rows.is_empty() {
            return Err(anyhow::anyhow!("No allocation record found for LotTranNo: {}", lot_tran_no));
        }

        let row = &rows[0];
        let run_no: i32 = row.get("RunNo").unwrap_or(0);
        let row_num: i32 = row.get("RowNum").unwrap_or(0);
        let line_id: i32 = row.get("LineId").unwrap_or(0);
        let lot_no: &str = row.get("LotNo").unwrap_or("");
        let bin_no: &str = row.get("BinNo").unwrap_or("");
        let item_key: &str = row.get("ItemKey").unwrap_or("");
        let alloc_lot_qty: f64 = row.get("AllocLotQty").unwrap_or(0.0);
        let pack_size: f64 = row.get("PackSize").context("PackSize must be available from cust_BulkPicked table")?;
        let batch_no: &str = row.get("BatchNo").unwrap_or("");

        info!("✅ Found allocation record - Run: {}, Row: {}, Line: {}, Lot: {}, Bin: {}, Item: {}, AllocLotQty: {} KG, PackSize: {} KG/bag",
              run_no, row_num, line_id, lot_no, bin_no, item_key, alloc_lot_qty, pack_size);

        // Step 1a: Check Batch Status in PNMAST
        // Prevent deletion if batch is already closed
        if !batch_no.is_empty() {
            self.check_batch_status(client, batch_no).await?;
        }

        // Step 2: Get specific LotTransaction record for the exact allocation being deleted
        // FIXED: Instead of SUM() which aggregates all records, get the specific record tied to this LotTranNo
        let get_transaction_query = r#"
            SELECT lt.QtyIssued as ActualIssued,
                   CAST(lt.LotTranNo AS VARCHAR) as LotTranNos
            FROM LotTransaction lt
            INNER JOIN Cust_BulkLotPicked blp ON blp.LotTranNo = @P1
            WHERE lt.LotNo = blp.LotNo
              AND lt.ItemKey = blp.ItemKey
              AND lt.BinNo = blp.BinNo
              AND lt.IssueDocNo = blp.BatchNo
              AND lt.TransactionType = 5
              AND lt.User5 = 'Picking Customization'
              AND lt.QtyIssued = blp.AllocLotQty  -- Match specific quantity
        "#;

        let mut stmt = tiberius::Query::new(get_transaction_query);
        stmt.bind(lot_tran_no);

        let stream = stmt.query(client).await
            .context("Failed to get transaction record")?;
        
        let rows: Vec<Row> = stream.into_first_result().await
            .context("Failed to process transaction record")?;

        let (actual_issued, lot_tran_nos_str) = if let Some(row) = rows.first() {
            let actual_issued = row.get("ActualIssued").unwrap_or(0.0);
            let lot_tran_nos_str = row.get("LotTranNos").unwrap_or("").to_string();
            (actual_issued, lot_tran_nos_str)
        } else {
            (0.0, String::new())
        };

        info!("✅ Found specific issued quantity: {} for LotTranNo: {}, lot: {}, bin: {}", actual_issued, lot_tran_no, lot_no, bin_no);

        // Step 3: Rollback inventory commitment in LotMaster (inventory integrity first)
        if actual_issued > 0.0 {
            let rollback_query = r#"
                UPDATE LotMaster
                SET QtyCommitSales = CASE
                    WHEN QtyCommitSales - @P1 < 0 THEN 0
                    ELSE QtyCommitSales - @P1
                END
                WHERE ItemKey = @P2 AND LotNo = @P3 AND BinNo = @P4
            "#;

            let mut stmt = tiberius::Query::new(rollback_query);
            stmt.bind(actual_issued);
            stmt.bind(item_key);
            stmt.bind(lot_no);
            stmt.bind(bin_no);

            stmt.execute(client).await
                .with_context(|| format!("Failed to rollback inventory for lot {lot_no} bin {bin_no}"))?;
            info!("✅ Rolled back {} qty for lot {} item {} bin {} (Step 3 - inventory rollback)", actual_issued, lot_no, item_key, bin_no);
        } else {
            info!("ℹ️ No LotTransaction record found - skipping inventory rollback");
        }

        // Step 4: Update picked quantities in cust_BulkPicked (subtract the specific quantity)
        // FIXED: Use separate variables to avoid calculation inconsistencies
        let update_picked_query = r#"
            UPDATE cust_BulkPicked
            SET PickedBulkQty = CASE
                WHEN PickedBulkQty - @P1 <= 0 THEN 0
                ELSE PickedBulkQty - @P1
            END,
            PickedQty = CASE
                WHEN PickedBulkQty - @P1 <= 0 THEN 0
                ELSE PickedQty - @P5
            END,
            ItemBatchStatus = CASE
                WHEN PickedBulkQty - @P1 <= 0 THEN NULL
                ELSE 'Allocated'
            END,
            PickingDate = CASE
                WHEN PickedBulkQty - @P1 <= 0 THEN NULL
                ELSE PickingDate
            END,
            ModifiedBy = CASE
                WHEN PickedBulkQty - @P1 <= 0 THEN NULL
                ELSE ModifiedBy
            END
            WHERE RunNo = @P2 AND RowNum = @P3 AND LineId = @P4
        "#;

        // CRITICAL FIX: Convert AllocLotQty (KG) to bags by dividing by PackSize
        let qty_in_bags = alloc_lot_qty / pack_size;
        let qty_in_kg = alloc_lot_qty; // The actual KG amount to subtract from PickedQty
        info!("🔧 UNIT_CONVERSION: AllocLotQty: {} KG ÷ PackSize: {} KG/bag = {} bags to subtract",
              alloc_lot_qty, pack_size, qty_in_bags);
        let mut stmt = tiberius::Query::new(update_picked_query);
        stmt.bind(qty_in_bags);    // @P1 - bags to subtract from PickedBulkQty
        stmt.bind(run_no);         // @P2
        stmt.bind(row_num);        // @P3
        stmt.bind(line_id);        // @P4
        stmt.bind(qty_in_kg);      // @P5 - KG to subtract from PickedQty

        stmt.execute(client).await
            .context("Failed to update picked quantities")?;
        info!("✅ Updated picked quantities in cust_BulkPicked (Step 4 - subtracted {} bags = {} KG)", qty_in_bags, alloc_lot_qty);

        // Step 5: Delete the specific LotTransaction record tied to this allocation
        if !lot_tran_nos_str.is_empty() {
            let delete_lot_transaction_query = r#"
                DELETE FROM LotTransaction
                WHERE TransactionType = 5
                  AND User5 = 'Picking Customization'
                  AND LotTranNo = @P1
            "#;

            let mut lt_stmt = tiberius::Query::new(delete_lot_transaction_query);
            lt_stmt.bind(lot_tran_nos_str.parse::<i32>().unwrap_or(0));

            lt_stmt.execute(client).await
                .context("Failed to delete specific LotTransaction audit record in precise unpick")?;

            info!("✅ Deleted specific LotTransaction audit record (Step 5 - audit cleanup) - LotTranNo: {}", lot_tran_nos_str);
        } else {
            info!("⚠️ No specific LotTransaction record found to delete for precise unpick - LotTranNo: {}", lot_tran_no);
        }

        // Step 6: Delete the specific allocation record using LotTranNo
        let delete_allocation_query = r#"
            DELETE FROM Cust_BulkLotPicked
            WHERE LotTranNo = @P1
        "#;

        let mut stmt = tiberius::Query::new(delete_allocation_query);
        stmt.bind(lot_tran_no);

        stmt.execute(client).await
            .context("Failed to delete specific allocation record")?;
        info!("✅ Deleted allocation record for LotTranNo: {} (Step 6)", lot_tran_no);

        // Step 7: Clean up pallet traceability record if this was the only record for this ingredient
        // Check if there are other allocation records for this ingredient
        let check_remaining_query = r#"
            SELECT COUNT(*) as RemainingCount
            FROM Cust_BulkLotPicked 
            WHERE RunNo = @P1 AND RowNum = @P2 AND LineId = @P3
        "#;

        let mut stmt = tiberius::Query::new(check_remaining_query);
        stmt.bind(run_no);
        stmt.bind(row_num);
        stmt.bind(line_id);

        let stream = stmt.query(client).await
            .context("Failed to check remaining allocations")?;
        
        let rows: Vec<Row> = stream.into_first_result().await
            .context("Failed to process remaining count")?;

        let remaining_count: i32 = if let Some(row) = rows.first() {
            row.get("RemainingCount").unwrap_or(0)
        } else {
            0
        };

        // Only delete pallet record if no more allocations remain for this ingredient
        if remaining_count == 0 {
            let delete_pallet_query = r#"
                DELETE FROM Cust_BulkPalletLotPicked 
                WHERE RunNo = @P1 AND RowNum = @P2 AND LineId = @P3
            "#;

            let mut stmt = tiberius::Query::new(delete_pallet_query);
            stmt.bind(run_no);
            stmt.bind(row_num);
            stmt.bind(line_id);

            stmt.execute(client).await
                .context("Failed to delete pallet traceability record")?;
            info!("✅ Deleted pallet traceability record (Step 7 - last allocation for ingredient)");
        } else {
            info!("ℹ️ Keeping pallet traceability record ({} allocations remaining)", remaining_count);
        }

        // **STEP 8: REVERT PNITEM** - Revert PNITEM table quantities
        // Logic mirrors Phase 6 in picking_service.rs
        info!("🔄 DEBUG: STEP 8 - Reverting PNITEM table quantities");

        let revert_pnitem_query = r#"
            DECLARE @Remainder float;

            -- Calculate remainder first (what the qty WOULD be after decrement)
            SELECT @Remainder = ActualQty - @P1
            FROM PNITEM WITH (UPDLOCK, ROWLOCK)
            WHERE BatchNo = @P2 AND Lineid = @P3;

            -- Apply updates based on Remainder
            -- If Remainder <= 0.001 (Fully Unpicked), reset to StdQty and Status 'R'
            -- Else (Partially Unpicked), set to Remainder and keep Status 'A'
            UPDATE PNITEM
            SET ActualQty = CASE WHEN @Remainder <= 0.001 THEN StdQty ELSE @Remainder END,
                AllocQty = CASE WHEN @Remainder <= 0.001 THEN 0 ELSE AllocQty - @P1 END,
                ActualQtyDispUom = CASE WHEN @Remainder <= 0.001 THEN StdQty ELSE @Remainder END,
                SerLotQty = CASE WHEN @Remainder <= 0.001 THEN 0 ELSE SerLotQty - @P1 END,
                Status = CASE WHEN @Remainder <= 0.001 THEN 'R' ELSE Status END,
                User11 = CASE WHEN @Remainder <= 0.001 THEN NULL ELSE User11 END
            WHERE BatchNo = @P2 AND Lineid = @P3
        "#;

        let mut pnitem_stmt = TiberiusQuery::new(revert_pnitem_query);
        pnitem_stmt.bind(alloc_lot_qty); // @P1 - Quantity being removed (KG)
        pnitem_stmt.bind(batch_no);      // @P2
        pnitem_stmt.bind(line_id);       // @P3

        match pnitem_stmt.execute(client).await {
            Ok(_) => {
                info!("✅ STEP_8_SUCCESS: PNITEM reverted successfully (Qty: {} KG)", alloc_lot_qty);
            },
            Err(e) => {
                let error_msg = format!("STEP 8 FAILED: PNITEM revert failed - Database error: {e}");
                error!("❌ STEP_8_ERROR: {}", error_msg);
                return Err(anyhow::anyhow!("STEP_8_REVERT_FAILED: {}", error_msg));
            }
        }

        // **STEP 8b: CHECK AND REVERT PNMAST STATUS**
        // If all PNITEM.User11 for this batch are NULL, revert PNMAST status to 'R'
        info!("🔄 DEBUG: STEP 8b - Checking PNMAST status reversion for batch: {}", batch_no);
        
        let check_pnitem_query = r#"
            SELECT COUNT(*) as active_count
            FROM PNITEM
            WHERE BatchNo = @P1 AND User11 IS NOT NULL
        "#;
        
        let mut check_stmt = TiberiusQuery::new(check_pnitem_query);
        check_stmt.bind(batch_no);
        
        // Extract value first to release the borrow on client
        let active_count: Option<i32> = if let Ok(stream) = check_stmt.query(client).await {
            if let Ok(Some(r)) = stream.into_row().await {
                r.get::<i32, _>(0)
            } else {
                None
            }
        } else {
            None
        };
        
        // Now safe to use client again
        if let Some(count) = active_count {
            if count == 0 {
                // All PNITEM.User11 are NULL - revert PNMAST to 'R'
                let revert_pnmast_query = r#"
                    UPDATE PNMAST
                    SET Status = 'R'
                    WHERE BatchNo = @P1 AND Status = 'A'
                "#;
                
                let mut revert_stmt = TiberiusQuery::new(revert_pnmast_query);
                revert_stmt.bind(batch_no);
                
                match revert_stmt.execute(client).await {
                    Ok(result) => {
                        let rows_affected = result.rows_affected().iter().sum::<u64>();
                        if rows_affected > 0 {
                            info!("✅ STEP_8b_SUCCESS: PNMAST status reverted 'A' → 'R' for batch: {} (all User11 NULL)", batch_no);
                        } else {
                            info!("ℹ️ STEP_8b_SKIP: PNMAST status already 'R' for batch: {}", batch_no);
                        }
                    },
                    Err(e) => {
                        warn!("⚠️ STEP_8b_WARNING: Failed to revert PNMAST status: {}. Continuing...", e);
                    }
                }
            } else {
                info!("ℹ️ STEP_8b_SKIP: PNMAST status remains 'A' for batch: {} ({} active PNITEM rows)", batch_no, count);
            }
        }

        // Return summary
        Ok(serde_json::json!({
            "unpicked_lot_tran_no": lot_tran_no,
            "lot_no": lot_no,
            "bin_no": bin_no,
            "item_key": item_key,
            "quantity_removed": alloc_lot_qty,
            "actual_issued_rollback": actual_issued,
            "operation_type": "precise_unpick",
            "run_no": run_no,
            "row_num": row_num,
            "line_id": line_id
        }))
    }

    /// Get batch weight summary for pending to picked modal
    #[instrument(skip(self))]
    pub async fn get_batch_weight_summary(&self, run_no: i32) -> Result<Vec<BatchWeightSummaryItem>> {
        info!("📊 Getting batch weight summary for run: {}", run_no);

        let mut client = self
            .get_client()
            .await
            .context("Failed to get database client")?;

        let query = r#"
            SELECT
                bp.BatchNo,
                bp.ItemKey,
                im.Desc1 as ItemDescription,
                bp.ToPickedBulkQty,
                ISNULL(bp.PickedBulkQty, 0) as PickedBulkQty,
                bp.PackSize,
                -- Calculate actual picked quantity from lot allocation records
                ISNULL(actual_picked.ActualPickedQty, 0) as ActualPickedQty,
                -- Calculate weight values using actual picked quantities
                (bp.ToPickedBulkQty * bp.PackSize) as TotalWeightKG,
                (ISNULL(actual_picked.ActualPickedQty, 0) * bp.PackSize) as PickedWeightKG,
                ((bp.ToPickedBulkQty - ISNULL(actual_picked.ActualPickedQty, 0)) * bp.PackSize) as RemainingWeightKG,
                bp.RowNum,
                bp.LineId
            FROM cust_BulkPicked bp
            LEFT JOIN INMAST im ON bp.ItemKey = im.Itemkey
            LEFT JOIN (
                -- Calculate actual picked quantities from lot allocation records
                SELECT
                    blp.RunNo,
                    blp.BatchNo,
                    blp.ItemKey,
                    SUM(ISNULL(blp.QtyReceived, 0) / blp.PackSize) as ActualPickedQty
                FROM Cust_BulkLotPicked blp
                WHERE blp.RunNo = @P1
                GROUP BY blp.RunNo, blp.BatchNo, blp.ItemKey
            ) actual_picked ON bp.RunNo = actual_picked.RunNo
                AND bp.BatchNo = actual_picked.BatchNo
                AND bp.ItemKey = actual_picked.ItemKey
            WHERE bp.RunNo = @P1
              AND bp.ToPickedBulkQty > 0
            ORDER BY bp.ItemKey ASC, bp.BatchNo ASC, bp.LineId ASC
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(run_no);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute batch weight summary query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get batch weight summary results")?;

        let mut batch_items = Vec::new();
        for row in &rows {
            let batch_no: &str = row.get("BatchNo").unwrap_or("");
            let item_key: &str = row.get("ItemKey").unwrap_or("");
            let item_description: Option<&str> = row.get("ItemDescription");
            let to_picked_bulk_qty: f64 = row.get("ToPickedBulkQty").unwrap_or(0.0);
            let picked_bulk_qty: f64 = row.get("ActualPickedQty").unwrap_or(0.0); // Use actual picked from allocations
            let pack_size: f64 = row.get("PackSize").unwrap_or(0.0);
            let total_weight_kg: f64 = row.get("TotalWeightKG").unwrap_or(0.0);
            let picked_weight_kg: f64 = row.get("PickedWeightKG").unwrap_or(0.0);
            let remaining_weight_kg: f64 = row.get("RemainingWeightKG").unwrap_or(0.0);
            let row_num: i32 = row.get("RowNum").unwrap_or(0);
            let line_id: i32 = row.get("LineId").unwrap_or(0);

            let batch_item = BatchWeightSummaryItem {
                batch_no: batch_no.to_string(),
                item_key: item_key.to_string(),
                item_description: item_description.map(|s| s.to_string()),
                to_picked_bulk_qty: BigDecimal::from_f64(to_picked_bulk_qty).unwrap_or_default(),
                picked_bulk_qty: BigDecimal::from_f64(picked_bulk_qty).unwrap_or_default(),
                pack_size: BigDecimal::from_f64(pack_size).unwrap_or_default(),
                total_weight_kg: BigDecimal::from_f64(total_weight_kg).unwrap_or_default(),
                picked_weight_kg: BigDecimal::from_f64(picked_weight_kg).unwrap_or_default(),
                remaining_weight_kg: BigDecimal::from_f64(remaining_weight_kg).unwrap_or_default(),
                row_num,
                line_id,
            };

            batch_items.push(batch_item);
        }

        info!(
            "📊 Found {} batch items with weight calculations for run: {}",
            batch_items.len(), run_no
        );

        Ok(batch_items)
    }

    /// Get lot picking details for print labels showing individual bin picks
    #[instrument(skip(self))]
    pub async fn get_lot_picking_details_for_run(&self, run_no: i32) -> Result<Vec<LotPickingDetail>> {
        info!("🏷️ Getting lot picking details for run: {}", run_no);
        
        let mut client = self
            .get_client()
            .await
            .context("Failed to get database client")?;

        let query = r#"
            SELECT
                blp.RunNo,
                blp.BatchNo,
                blp.LineId,
                blp.ItemKey,
                blp.LotNo,
                blp.BinNo,
                blp.QtyReceived,
                blp.PackSize,
                blp.RecUserid,
                ISNULL(blp.ModifiedBy, '') as ModifiedBy,
                CONVERT(varchar, blp.RecDate, 120) as RecDate,
                ISNULL(bp.PickedBulkQty, 0) as PickedBulkQty,
                ISNULL(bp.PickedQty, 0) as PickedQty
            FROM Cust_BulkLotPicked blp
            INNER JOIN cust_BulkPicked bp ON blp.RunNo = bp.RunNo
                AND blp.BatchNo = bp.BatchNo
                AND blp.ItemKey = bp.ItemKey
            WHERE blp.RunNo = @P1
                AND blp.QtyReceived > 0
            ORDER BY blp.BatchNo, blp.LineId, blp.LotNo, blp.BinNo
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(run_no);
        let stream = select.query(&mut client)
            .await
            .context("Failed to execute lot picking details query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get lot picking details results")?;

        let mut lot_details = Vec::new();

        for row in rows {
            let run_no: i32 = row.get("RunNo").unwrap_or(0);
            let batch_no: &str = row.get("BatchNo").unwrap_or("");
            let line_id: i32 = row.get("LineId").unwrap_or(0);
            let item_key: &str = row.get("ItemKey").unwrap_or("");
            let lot_no: &str = row.get("LotNo").unwrap_or("");
            let bin_no: &str = row.get("BinNo").unwrap_or("");
            let qty_received_f64: f64 = row.get("QtyReceived").unwrap_or(0.0);
            let pack_size_f64: f64 = row.get("PackSize").unwrap_or(0.0);
            let rec_userid: &str = row.get("RecUserid").unwrap_or("");
            let modified_by: &str = row.get("ModifiedBy").unwrap_or("");
            let rec_date: &str = row.get("RecDate").unwrap_or("");
            let picked_bulk_qty_f64: f64 = row.get("PickedBulkQty").unwrap_or(0.0);
            let picked_qty_f64: f64 = row.get("PickedQty").unwrap_or(0.0);

            // Convert f64 to BigDecimal
            let qty_received = BigDecimal::from_f64(qty_received_f64)
                .unwrap_or_else(|| BigDecimal::from(0));
            let pack_size = BigDecimal::from_f64(pack_size_f64)
                .unwrap_or_else(|| BigDecimal::from(0));
            let picked_bulk_qty = BigDecimal::from_f64(picked_bulk_qty_f64)
                .unwrap_or_else(|| BigDecimal::from(0));
            let picked_qty = BigDecimal::from_f64(picked_qty_f64)
                .unwrap_or_else(|| BigDecimal::from(0));

            let lot_detail = LotPickingDetail {
                run_no,
                batch_no: batch_no.to_string(),
                line_id,
                item_key: item_key.to_string(),
                lot_no: lot_no.to_string(),
                bin_no: bin_no.to_string(),
                qty_received,
                pack_size,
                rec_userid: rec_userid.to_string(),
                modified_by: modified_by.to_string(),
                rec_date: rec_date.to_string(),
                picked_bulk_qty,
                picked_qty,
            };

            lot_details.push(lot_detail);
        }

        info!("🏷️ Found {} lot picking details for run {}", lot_details.len(), run_no);
        Ok(lot_details)
    }

    /// **NEW AUTOMATIC STATUS UPDATE** - Update bulk run status with user audit
    pub async fn update_bulk_run_status(
        &self,
        run_no: i32,
        new_status: &str,
        user_id: &str,
    ) -> Result<bool, anyhow::Error> {
        info!("🔄 DATABASE: Updating run {} status to '{}' by user: {}", run_no, new_status, user_id);

        let sql = r#"
            UPDATE Cust_BulkRun
            SET Status = @P1,
                ModifiedBy = @P2,
                ModifiedDate = GETDATE()
            WHERE RunNo = @P3
        "#;

        let mut client = self.get_client().await?;
        let mut stmt = tiberius::Query::new(sql);
        stmt.bind(new_status);
        stmt.bind(user_id);
        stmt.bind(run_no);

        let stream = stmt.execute(&mut client)
            .await
            .context("Failed to execute status update query")?;

        let rows_affected = stream.rows_affected().iter().sum::<u64>();

        let success = rows_affected > 0;

        if success {
            info!("✅ DATABASE: Successfully updated run {} status to '{}' ({} rows affected)",
                  run_no, new_status, rows_affected);
        } else {
            warn!("⚠️ DATABASE: No rows updated for run {} status change to '{}'", run_no, new_status);
        }

        Ok(success)
    }

    /// **NEW UNIVERSAL COMPLETION CHECK** - Get detailed run completion status
    pub async fn get_run_completion_status(
        &self,
        run_no: i32,
    ) -> Result<crate::models::bulk_runs::RunCompletionStatus, anyhow::Error> {
        info!("🔍 DATABASE: Getting detailed completion status for run {} at {}", run_no, chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f UTC"));

        let query = r#"
            WITH IngredientCompletion AS (
                SELECT
                    bp.RunNo,
                    bp.LineId,
                    bp.ItemKey,
                    SUM(bp.ToPickedBulkQty) as TotalRequired,
                    -- Use allocation-based calculation instead of unreliable PickedBulkQty
                    SUM(ISNULL(actual_picked.ActualPickedQty, 0)) as TotalPicked,
                    CASE
                        WHEN SUM(ISNULL(actual_picked.ActualPickedQty, 0)) >= SUM(bp.ToPickedBulkQty)
                        THEN 1 ELSE 0
                    END as IsIngredientComplete
                FROM cust_BulkPicked bp
                LEFT JOIN (
                    SELECT RunNo, ItemKey,
                           SUM(ISNULL(QtyReceived, 0) / PackSize) as ActualPickedQty
                    FROM Cust_BulkLotPicked
                    WHERE RunNo = @P1
                    GROUP BY RunNo, ItemKey
                ) actual_picked ON bp.RunNo = actual_picked.RunNo
                                AND bp.ItemKey = actual_picked.ItemKey
                WHERE bp.RunNo = @P1
                  AND bp.ToPickedBulkQty > 0
                GROUP BY bp.RunNo, bp.LineId, bp.ItemKey
            )
            SELECT
                COUNT(*) as TotalIngredients,
                SUM(IsIngredientComplete) as CompletedIngredients,
                SUM(CASE WHEN IsIngredientComplete = 0 THEN 1 ELSE 0 END) as IncompleteIngredients
            FROM IngredientCompletion
            GROUP BY RunNo
        "#;

        let mut client = self.get_client().await?;
        let mut stmt = tiberius::Query::new(query);
        stmt.bind(run_no);

        let stream = stmt.query(&mut client).await
            .context("Failed to execute completion status query")?;

        let rows: Vec<Row> = stream.into_first_result().await
            .context("Failed to get completion status results")?;

        if let Some(row) = rows.first() {
            let total_ingredients: i32 = row.get("TotalIngredients").unwrap_or(0);
            let completed_count: i32 = row.get("CompletedIngredients").unwrap_or(0);
            let incomplete_count: i32 = row.get("IncompleteIngredients").unwrap_or(0);
            let is_complete = incomplete_count == 0 && total_ingredients > 0;

            info!("📊 DATABASE: Run {} completion - {}/{} complete, {} incomplete, is_complete: {}",
                  run_no, completed_count, total_ingredients, incomplete_count, is_complete);

            // **DEBUG: Show individual ingredient completion details**
            let debug_query = r#"
                SELECT
                    bp.LineId,
                    bp.ItemKey,
                    SUM(bp.ToPickedBulkQty) as TotalRequired,
                    -- Use allocation-based calculation instead of unreliable PickedBulkQty
                    SUM(ISNULL(actual_picked.ActualPickedQty, 0)) as TotalPicked,
                    CASE
                        WHEN SUM(ISNULL(actual_picked.ActualPickedQty, 0)) >= SUM(bp.ToPickedBulkQty)
                        THEN 'COMPLETE' ELSE 'INCOMPLETE'
                    END as Status
                FROM cust_BulkPicked bp
                LEFT JOIN (
                    SELECT RunNo, ItemKey,
                           SUM(ISNULL(QtyReceived, 0) / PackSize) as ActualPickedQty
                    FROM Cust_BulkLotPicked
                    WHERE RunNo = @P1
                    GROUP BY RunNo, ItemKey
                ) actual_picked ON bp.RunNo = actual_picked.RunNo
                                AND bp.ItemKey = actual_picked.ItemKey
                WHERE bp.RunNo = @P1
                  AND bp.ToPickedBulkQty > 0
                GROUP BY bp.LineId, bp.ItemKey
                ORDER BY bp.LineId
            "#;
            let mut debug_client = self.get_client().await?;
            let mut debug_stmt = tiberius::Query::new(debug_query);
            debug_stmt.bind(run_no);
            if let Ok(debug_stream) = debug_stmt.query(&mut debug_client).await {
                if let Ok(debug_rows) = debug_stream.into_first_result().await {
                    for debug_row in debug_rows {
                        let line_id: i32 = debug_row.get("LineId").unwrap_or(0);
                        let item_key: &str = debug_row.get("ItemKey").unwrap_or("");
                        let total_required: i32 = debug_row.get::<f64, _>("TotalRequired").map(|v| v as i32).unwrap_or(0);
                        let total_picked: i32 = debug_row.get::<f64, _>("TotalPicked").map(|v| v as i32).unwrap_or(0);
                        let status: &str = debug_row.get("Status").unwrap_or("");
                        info!("🔍 INGREDIENT_DEBUG: LineId {} {} - Required: {}, Picked: {}, Status: {}",
                              line_id, item_key, total_required, total_picked, status);
                    }
                }
            }

            Ok(crate::models::bulk_runs::RunCompletionStatus {
                is_complete,
                incomplete_count,
                completed_count,
                total_ingredients,
            })
        } else {
            warn!("⚠️ DATABASE: No completion data found for run {} - treating as incomplete", run_no);
            Ok(crate::models::bulk_runs::RunCompletionStatus {
                is_complete: false,
                incomplete_count: 0,
                completed_count: 0,
                total_ingredients: 0,
            })
        }
    }

    /// Check if a batch is closed in PNMAST
    async fn check_batch_status(
        &self,
        client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
        batch_no: &str,
    ) -> Result<(), anyhow::Error> {
        let query = "SELECT Status FROM PNMAST WHERE BatchNo = @P1";
        let mut stmt = tiberius::Query::new(query);
        stmt.bind(batch_no);

        let stream = stmt.query(client).await?;
        let row = stream.into_row().await?;

        if let Some(r) = row {
            let status: &str = r.get("Status").unwrap_or("");
            if status == "C" {
                return Err(anyhow::anyhow!(
                    "Cannot delete lot from batch {} because it is already closed",
                    batch_no
                ));
            }
        }
        Ok(())
    }
}
