use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use axum::http::HeaderMap;
use tracing::{debug, warn};

use crate::{AppState, utils::AuthService};

/// JWT authentication middleware
/// Validates JWT tokens and extracts user information for protected routes
pub async fn jwt_auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract Authorization header
    let auth_header = headers.get("authorization")
        .and_then(|h| h.to_str().ok());

    // Extract token from header
    let token = match AuthService::extract_token_from_header(auth_header) {
        Some(token) => token,
        None => {
            warn!("🚫 JWT Auth: No Authorization header provided");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Verify token
    match state.auth_service.verify_token(token) {
        Ok(claims) => {
            debug!("✅ JWT Auth: Valid token for user: {}", claims.username);
            
            // Add user information to request headers for downstream handlers
            request.headers_mut().insert(
                "x-user-id", 
                claims.sub.parse().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            );
            request.headers_mut().insert(
                "x-username", 
                claims.username.parse().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            );
            
            Ok(next.run(request).await)
        }
        Err(e) => {
            warn!("🚫 JWT Auth: Invalid token - {}", e);
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

