use crate::database::Database;
use crate::models::bulk_runs::BulkPickedItem;
use anyhow::{Context, Result};
use tiberius::{Query as TiberiusQuery, Row};
use tracing::{info, instrument};

impl Database {
    /// Get all batches for a specific ingredient in a run
    /// Used for intelligent ingredient completion analysis
    #[instrument(skip(self))]
    pub async fn get_ingredient_batches(&self, run_no: i32, item_key: &str) -> Result<Vec<BulkPickedItem>> {
        info!("Getting all batches for ingredient {} in run {}", item_key, run_no);

        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        let query = r#"
            SELECT 
                bp.RunNo,
                bp.RowNum,
                bp.LineId,
                bp.ItemKey,
                im.Desc1 as Description,
                bp.Location,
                bp.StandardQty,
                bp.PackSize,
                bp.Unit,
                bp.TopickedStdQty,
                bp.ToPickedBulkQty,
                bp.PickedBulkQty,
                bp.PickingDate,
                bp.ItemBatchStatus as Status
            FROM cust_BulkPicked bp
            LEFT JOIN INMAST im ON bp.ItemKey = im.Itemkey
            WHERE bp.RunNo = @P1 
              AND bp.ItemKey = @P2
              AND bp.ToPickedBulkQty > 0  -- Only bulk picking batches
            ORDER BY bp.RowNum, bp.BatchNo
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(run_no);
        select.bind(item_key);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute ingredient batches query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get ingredient batches results")?;

        let mut batches = Vec::new();
        for row in rows {
            match self.row_to_bulk_picked_item(&row) {
                Ok(batch) => batches.push(batch),
                Err(e) => {
                    info!("Failed to convert row to BulkPickedItem: {}", e);
                }
            }
        }

        info!(
            "Found {} batches for ingredient {} in run {}",
            batches.len(),
            item_key,
            run_no
        );

        Ok(batches)
    }

    /// Get bulk picking ingredients with completion filtering
    /// Used for ItemKey search modal with intelligent filtering
    #[instrument(skip(self))]
    pub async fn get_available_bulk_ingredients(
        &self, 
        run_no: i32,
        hide_completed: bool,
    ) -> Result<Vec<BulkPickedItem>> {
        info!(
            "Getting available bulk ingredients for run {} (hide_completed: {})",
            run_no, hide_completed
        );

        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        let base_query = r#"
            SELECT DISTINCT
                bp.RunNo,
                MIN(bp.RowNum) as RowNum,
                bp.LineId,
                bp.ItemKey,
                im.Desc1 as Description,
                bp.Location,
                bp.StandardQty,
                bp.PackSize,
                bp.Unit,
                bp.TopickedStdQty,
                bp.ToPickedBulkQty,
                SUM(ISNULL(bp.PickedBulkQty, 0)) as PickedBulkQty,
                MAX(bp.PickingDate) as PickingDate,
                MAX(bp.ItemBatchStatus) as Status,
                -- Calculate completion status
                CASE 
                    WHEN bp.ToPickedBulkQty <= 0 THEN 'NOT_REQUIRED'
                    WHEN ISNULL(SUM(bp.PickedBulkQty), 0) >= bp.ToPickedBulkQty THEN 'COMPLETE'
                    WHEN ISNULL(SUM(bp.PickedBulkQty), 0) > 0 THEN 'IN_PROGRESS'
                    ELSE 'PENDING'
                END as CompletionStatus
            FROM cust_BulkPicked bp
            LEFT JOIN INMAST im ON bp.ItemKey = im.Itemkey
            WHERE bp.RunNo = @P1
              AND bp.ToPickedBulkQty > 0  -- Only ingredients requiring bulk picking
            GROUP BY bp.RunNo, bp.LineId, bp.ItemKey, im.Desc1, bp.Location, 
                     bp.Unit, bp.StandardQty, bp.PackSize, bp.TopickedStdQty, bp.ToPickedBulkQty
        "#;

        let filter_clause = if hide_completed {
            " HAVING ISNULL(MAX(bp.PickedBulkQty), 0) < bp.ToPickedBulkQty"
        } else {
            ""
        };

        let order_clause = " ORDER BY bp.LineId DESC";

        let full_query = format!("{base_query}{filter_clause}{order_clause}");

        let mut select = TiberiusQuery::new(&full_query);
        select.bind(run_no);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute available bulk ingredients query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get available bulk ingredients results")?;

        let mut ingredients = Vec::new();
        for row in rows {
            match self.row_to_bulk_picked_item(&row) {
                Ok(ingredient) => ingredients.push(ingredient),
                Err(e) => {
                    info!("Failed to convert row to BulkPickedItem: {}", e);
                }
            }
        }

        info!(
            "Found {} available bulk ingredients for run {} (filter: {})",
            ingredients.len(),
            run_no,
            if hide_completed { "hide completed" } else { "show all" }
        );

        Ok(ingredients)
    }

    /// Get completion statistics for all ingredients in a run
    /// Used for run-level progress tracking and dashboard metrics
    #[instrument(skip(self))]
    pub async fn get_run_completion_stats(&self, run_no: i32) -> Result<RunCompletionStats> {
        info!("Getting completion statistics for run {}", run_no);

        let mut client = self
            .get_client()
            .await
            .context("Failed to get read database client")?;

        let query = r#"
            SELECT 
                COUNT(*) as TotalIngredients,
                SUM(CASE WHEN ISNULL(MAX(bp.PickedBulkQty), 0) >= bp.ToPickedBulkQty THEN 1 ELSE 0 END) as CompletedIngredients,
                SUM(CASE WHEN ISNULL(MAX(bp.PickedBulkQty), 0) > 0 AND ISNULL(MAX(bp.PickedBulkQty), 0) < bp.ToPickedBulkQty THEN 1 ELSE 0 END) as InProgressIngredients,
                SUM(CASE WHEN ISNULL(MAX(bp.PickedBulkQty), 0) = 0 THEN 1 ELSE 0 END) as UnpickedIngredients,
                SUM(CASE WHEN bp.ToPickedBulkQty > 0 THEN 1 ELSE 0 END) as BulkPickingIngredients
            FROM cust_BulkPicked bp
            WHERE bp.RunNo = @P1
            GROUP BY bp.RunNo, bp.LineId, bp.ItemKey, bp.ToPickedBulkQty
        "#;

        let mut select = TiberiusQuery::new(query);
        select.bind(run_no);

        let stream = select
            .query(&mut client)
            .await
            .context("Failed to execute run completion stats query")?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .context("Failed to get run completion stats results")?;

        let total_ingredients: i32 = rows.iter().map(|row| row.get::<i32, _>("TotalIngredients").unwrap_or(0)).sum();
        let completed_ingredients: i32 = rows.iter().map(|row| row.get::<i32, _>("CompletedIngredients").unwrap_or(0)).sum();
        let in_progress_ingredients: i32 = rows.iter().map(|row| row.get::<i32, _>("InProgressIngredients").unwrap_or(0)).sum();
        let unpicked_ingredients: i32 = rows.iter().map(|row| row.get::<i32, _>("UnpickedIngredients").unwrap_or(0)).sum();
        let bulk_picking_ingredients: i32 = rows.iter().map(|row| row.get::<i32, _>("BulkPickingIngredients").unwrap_or(0)).sum();

        let completion_percentage = if total_ingredients > 0 {
            (completed_ingredients as f64 / total_ingredients as f64) * 100.0
        } else {
            0.0
        };

        let stats = RunCompletionStats {
            run_no,
            total_ingredients,
            completed_ingredients,
            in_progress_ingredients,
            unpicked_ingredients,
            bulk_picking_ingredients,
            completion_percentage,
        };

        info!(
            "Run {} completion: {}/{} ingredients ({}%)",
            run_no, completed_ingredients, total_ingredients, completion_percentage as i32
        );

        Ok(stats)
    }
}

/// Run completion statistics for dashboard and progress tracking
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunCompletionStats {
    pub run_no: i32,
    pub total_ingredients: i32,
    pub completed_ingredients: i32,
    pub in_progress_ingredients: i32,
    pub unpicked_ingredients: i32,
    pub bulk_picking_ingredients: i32,
    pub completion_percentage: f64,
}