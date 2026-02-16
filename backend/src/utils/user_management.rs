use anyhow::Result;
use axum::http::HeaderMap;
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};
use std::env;
use tracing::{info, warn};

/// Extract authenticated user from multiple sources with priority order
/// Priority: JWT token > x-user-id header > request body user_id > default fallback
pub fn extract_authenticated_user(
    headers: &HeaderMap,
    request_user_id: Option<&String>,
) -> Option<String> {
    info!("🔐 USER_EXTRACTION: Starting user authentication extraction");

    // Priority 1: Extract from JWT token
    if let Some(jwt_user) = extract_user_from_jwt_token(headers) {
        info!("✅ USER_EXTRACTION: User extracted from JWT token: {}", jwt_user);
        return Some(jwt_user);
    }

    // Priority 2: Extract from x-user-id header  
    if let Some(header_user) = extract_user_from_header(headers) {
        info!("✅ USER_EXTRACTION: User extracted from x-user-id header: {}", header_user);
        return Some(header_user);
    }

    // Priority 3: Extract from request body
    if let Some(body_user) = request_user_id.filter(|s| !s.trim().is_empty()) {
        info!("✅ USER_EXTRACTION: User extracted from request body: {}", body_user);
        return Some(body_user.clone());
    }

    warn!("⚠️ USER_EXTRACTION: No user found in JWT, headers, or request body - will default to SYSTEM");
    None
}

/// Extract username from JWT token with proper validation
/// For production deployment - currently returns None for development
pub fn extract_user_from_jwt_token(headers: &HeaderMap) -> Option<String> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .or_else(|| headers.get("Authorization").and_then(|v| v.to_str().ok()))?;

    if !auth_header.starts_with("Bearer ") {
        return None;
    }

    let token = &auth_header[7..]; // Remove "Bearer " prefix

    // Development mode: check for dummy token
    if token == "dummy-jwt-token" {
        warn!("🔧 JWT_EXTRACTION: Development dummy token detected - skipping JWT parsing");
        return None;
    }

    // Production JWT parsing (implement when real JWT tokens are used)
    extract_username_from_jwt_payload(token)
}

/// JWT Claims structure for token validation
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,      // Subject (user ID)
    exp: usize,       // Expiration time
    iat: usize,       // Issued at
    username: Option<String>, // Username field
    user_id: Option<String>,  // Alternative user ID field
}

/// Parse JWT payload to extract username with proper validation
fn extract_username_from_jwt_payload(token: &str) -> Option<String> {
    // Skip validation for empty or obviously invalid tokens
    if token.is_empty() || token == "null" || token == "undefined" {
        return None;
    }

    // Get JWT secret from environment
    let jwt_secret = env::var("JWT_SECRET").unwrap_or_else(|_| {
        "development-secret-key".to_string()
    });

    // Configure validation
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.validate_nbf = false;
    
    // Decode and validate token
    match decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_ref()),
        &validation,
    ) {
        Ok(token_data) => {
            let claims = token_data.claims;
            
            // Extract username with priority: username field > user_id field > sub field
            let extracted_user = claims.username
                .or(claims.user_id)
                .or({
                    if !claims.sub.is_empty() {
                        Some(claims.sub)
                    } else {
                        None
                    }
                });

            if let Some(user) = &extracted_user {
                info!("✅ JWT_PARSING: Successfully extracted user from JWT: {}", user);
            } else {
                warn!("⚠️ JWT_PARSING: JWT valid but no username found in claims");
            }

            extracted_user
        }
        Err(_err) => {
            // Don't log errors for development/startup scenarios - just silently fail
            // This prevents error spam during server initialization
            None
        }
    }
}

/// Extract user from x-user-id header
pub fn extract_user_from_header(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Truncate user ID based on database field constraints with appropriate field-specific limits
pub fn truncate_user_id_for_field(user_id: &str, field_type: UserIdFieldType) -> String {
    let max_length = match field_type {
        UserIdFieldType::RecUseridVarchar => 8,   // varchar(8) - Cust_BulkLotPicked, Cust_BulkPalletLotPicked, LotTransaction
        UserIdFieldType::RecUseridNvarchar => 8,  // nvarchar(8) - cust_BulkPicked.RecUserId
        UserIdFieldType::ModifiedBy => 8,         // nvarchar(8) - All ModifiedBy fields
    };

    truncate_user_id_safely(user_id, max_length)
}

/// Field type enum for proper truncation handling
#[derive(Debug, Clone)]
pub enum UserIdFieldType {
    RecUseridVarchar,  // varchar(8) - Cust_BulkLotPicked, Cust_BulkPalletLotPicked, LotTransaction, LotMaster
    RecUseridNvarchar, // nvarchar(8) - cust_BulkPicked.RecUserId
    ModifiedBy,        // nvarchar(8) - All ModifiedBy fields
}

/// Safely truncate user ID to specified length with logging
pub fn truncate_user_id_safely(user_id: &str, max_length: usize) -> String {
    if user_id.len() <= max_length {
        user_id.to_string()
    } else {
        let truncated = &user_id[0..max_length];
        warn!(
            "⚠️ USER_TRUNCATION: User ID '{}' truncated to '{}' (max length: {})",
            user_id, truncated, max_length
        );
        truncated.to_string()
    }
}

/// Validate that user context is available for database operations
pub fn validate_user_context(user_id: Option<&String>) -> Result<String> {
    match user_id {
        Some(id) if !id.trim().is_empty() => {
            info!("✅ USER_VALIDATION: User context validated: {}", id);
            Ok(id.clone())
        }
        _ => {
            let default_user = "SYSTEM".to_string();
            warn!("⚠️ USER_VALIDATION: No valid user context found - defaulting to: {}", default_user);
            Ok(default_user)
        }
    }
}

/// Get appropriate user ID for specific database field with proper truncation
pub fn get_user_id_for_field(user_id: &str, field_type: UserIdFieldType) -> String {
    let truncated = truncate_user_id_for_field(user_id, field_type.clone());
    
    if truncated != user_id {
        info!(
            "📏 FIELD_TRUNCATION: User '{}' truncated to '{}' for field type {:?}",
            user_id, truncated, field_type
        );
    }
    
    truncated
}

/// Enhanced user extraction with comprehensive logging for debugging
pub fn extract_user_with_debug_info(
    headers: &HeaderMap,
    request_user_id: Option<&String>,
) -> (Option<String>, String) {
    let mut debug_info = String::new();
    
    // Check JWT token
    let jwt_user = extract_user_from_jwt_token(headers);
    debug_info.push_str(&format!("JWT: {jwt_user:?}, "));
    
    // Check header
    let header_user = extract_user_from_header(headers);
    debug_info.push_str(&format!("Header: {header_user:?}, "));
    
    // Check request body
    let body_user = request_user_id.filter(|s| !s.trim().is_empty()).cloned();
    debug_info.push_str(&format!("Body: {body_user:?}"));
    
    // Get final user using priority order
    let final_user = extract_authenticated_user(headers, request_user_id);
    
    info!("🔍 USER_DEBUG: Extraction summary - {}, Final: {:?}", debug_info, final_user);
    
    (final_user, debug_info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_user_id_truncation() {
        assert_eq!(truncate_user_id_safely("deachawat", 8), "deachawa");
        assert_eq!(truncate_user_id_safely("deachawat", 16), "deachawat");
        assert_eq!(truncate_user_id_safely("admin", 8), "admin");
    }

    #[test]
    fn test_field_type_truncation() {
        let user = "deachawat";

        // All fields are varchar/nvarchar(8) per SQL Server schema
        assert_eq!(
            truncate_user_id_for_field(user, UserIdFieldType::RecUseridVarchar),
            "deachawa"
        );
        assert_eq!(
            truncate_user_id_for_field(user, UserIdFieldType::RecUseridNvarchar),
            "deachawa"
        );
        assert_eq!(
            truncate_user_id_for_field(user, UserIdFieldType::ModifiedBy),
            "deachawa"
        );
    }

    #[test]
    fn test_user_validation() {
        let valid_user_str = "deachawat".to_string();
        let empty_user_str = "".to_string();
        
        let valid_user = Some(&valid_user_str);
        let empty_user = Some(&empty_user_str);
        let none_user: Option<&String> = None;

        assert_eq!(validate_user_context(valid_user).unwrap(), "deachawat");
        assert_eq!(validate_user_context(empty_user).unwrap(), "SYSTEM");
        assert_eq!(validate_user_context(none_user).unwrap(), "SYSTEM");
    }

    #[test]
    fn test_header_extraction() {
        let mut headers = HeaderMap::new();
        headers.insert("x-user-id", HeaderValue::from_static("testuser"));

        assert_eq!(extract_user_from_header(&headers), Some("testuser".to_string()));
    }
}