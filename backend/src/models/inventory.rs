use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};

/// Enhanced inventory alert model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryAlert {
    pub alert_type: InventoryAlertType,
    pub item_key: String,
    pub message: String,
    pub severity: AlertSeverity,
    pub recommended_action: Option<String>,
}

/// Types of inventory alerts
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InventoryAlertType {
    OutOfStock,
    LowStock,
    InsufficientQuantity,
    ExpiredLots,
    ExpiringSoon,
    UomMismatch,
    LocationIssue,
}

/// Alert severity levels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Critical, // Blocks picking operation
    Warning,  // Should be addressed but doesn't block
    Info,     // Informational only
}

impl InventoryAlert {
    /// Create a critical out of stock alert
    /// Only available during test compilation
    #[cfg(test)]
    pub fn out_of_stock(item_key: &str) -> Self {
        Self {
            alert_type: InventoryAlertType::OutOfStock,
            item_key: item_key.to_string(),
            message: format!("Item {} is out of stock", item_key),
            severity: AlertSeverity::Critical,
            recommended_action: Some("Check alternative lots or contact purchasing".to_string()),
        }
    }

    /// Create a low stock warning
    #[allow(dead_code)]
    pub fn low_stock(item_key: &str, current_qty: &BigDecimal, uom: &str) -> Self {
        Self {
            alert_type: InventoryAlertType::LowStock,
            item_key: item_key.to_string(),
            message: format!(
                "Low stock: {item_key} has only {current_qty} {uom} remaining"
            ),
            severity: AlertSeverity::Warning,
            recommended_action: Some("Consider replenishing stock soon".to_string()),
        }
    }

    /// Create insufficient quantity alert
    #[allow(dead_code)]
    pub fn insufficient_quantity(
        item_key: &str,
        available: &BigDecimal,
        needed: &BigDecimal,
        uom: &str,
    ) -> Self {
        Self {
            alert_type: InventoryAlertType::InsufficientQuantity,
            item_key: item_key.to_string(),
            message: format!(
                "Insufficient quantity: {item_key} has {available} {uom} available, but {needed} {uom} is needed"
            ),
            severity: AlertSeverity::Critical,
            recommended_action: Some(
                "Partial picking may be required, or find alternative lots".to_string(),
            ),
        }
    }

    /// Create expired lots alert
    #[allow(dead_code)]
    pub fn expired_lots(item_key: &str, lot_count: usize, lot_numbers: &[String]) -> Self {
        let lots_text = if lot_count > 3 {
            format!("{} and {} more", lot_numbers[..3].join(", "), lot_count - 3)
        } else {
            lot_numbers.join(", ")
        };

        Self {
            alert_type: InventoryAlertType::ExpiredLots,
            item_key: item_key.to_string(),
            message: format!(
                "Item {item_key} has {lot_count} expired lot(s): {lots_text}"
            ),
            severity: AlertSeverity::Warning,
            recommended_action: Some("Avoid expired lots, use FIFO rotation".to_string()),
        }
    }
}
