use serde::Serialize;

/// Unified API response structure for all endpoints
#[derive(Serialize, Clone, Debug)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

impl<T> ApiResponse<T> {
    /// Create successful response
    pub fn success(data: T, message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: message.into(),
            error_code: None,
            warning: None,
        }
    }


    /// Create error response
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            message: message.into(),
            error_code: None,
            warning: None,
        }
    }


}

/// JWT token structure for authentication
#[derive(Serialize, Clone, Debug)]
pub struct AuthToken {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub expires_at: i64,
    pub user_id: String,
    pub username: String,
}

/// User information structure
#[derive(Serialize, Clone, Debug)]
pub struct User {
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub display_name: String,
    pub is_active: bool,
}

/// Login response structure
#[derive(Serialize, Clone, Debug)]
pub struct LoginResponse {
    pub token: AuthToken,
    pub user: User,
}

