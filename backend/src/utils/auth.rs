use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::env;
use tracing::info;

use crate::types::{AuthToken, User};
use tiberius::Row;

/// JWT Claims structure
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Claims {
    pub sub: String,         // Subject (user ID)
    pub username: String,    // Username
    pub email: String,       // Email
    pub display_name: String,// Display name
    pub exp: i64,           // Expiration time
    pub iat: i64,           // Issued at
    pub iss: String,        // Issuer
}

#[derive(Clone)]
pub struct AuthService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    issuer: String,
    token_duration: Duration,
}

impl AuthService {
    /// Initialize JWT authentication service
    /// SEC-002 FIX: JWT_SECRET is now REQUIRED - no default fallback
    pub fn new() -> Result<Self> {
        let secret = env::var("JWT_SECRET")
            .context("JWT_SECRET environment variable is required. Please set a secure secret key.")?;
        
        // SEC-002: Enforce minimum 32-character JWT secret
        if secret.len() < 32 {
            return Err(anyhow::anyhow!(
                "JWT_SECRET must be at least 32 characters (current: {}). Generate with: openssl rand -base64 32",
                secret.len()
            ));
        }

        let encoding_key = EncodingKey::from_secret(secret.as_bytes());
        let decoding_key = DecodingKey::from_secret(secret.as_bytes());

        let issuer = env::var("JWT_ISSUER")
            .unwrap_or_else(|_| "NWFTH-BulkPicking".to_string());

        // SEC-006 FIX: Reduced default from 24h to 8h for warehouse operations
        let token_duration_hours = env::var("JWT_DURATION_HOURS")
            .unwrap_or_else(|_| "8".to_string())
            .parse::<i64>()
            .unwrap_or(8);

        let token_duration = Duration::hours(token_duration_hours);

        info!("🔐 JWT Authentication initialized with {}h token duration", token_duration_hours);

        Ok(Self {
            encoding_key,
            decoding_key,
            issuer,
            token_duration,
        })
    }


    /// Generate JWT token for authenticated user
    pub fn generate_token(&self, user: &User) -> Result<AuthToken> {
        let now = Utc::now();
        let exp = now + self.token_duration;

        let claims = Claims {
            sub: user.user_id.clone(),
            username: user.username.clone(),
            email: user.email.clone(),
            display_name: user.display_name.clone(),
            exp: exp.timestamp(),
            iat: now.timestamp(),
            iss: self.issuer.clone(),
        };

        let token_string = encode(&Header::default(), &claims, &self.encoding_key)
            .context("Failed to sign JWT token")?;

        info!("🎫 Generated JWT token for user: {}", user.username);

        Ok(AuthToken {
            access_token: token_string,
            token_type: "Bearer".to_string(),
            expires_in: self.token_duration.num_seconds(),
            expires_at: exp.timestamp(),
            user_id: user.user_id.clone(),
            username: user.username.clone(),
        })
    }

    /// Verify and decode JWT token
    pub fn verify_token(&self, token: &str) -> Result<Claims> {
        let token_data = decode::<Claims>(
            token, 
            &self.decoding_key, 
            &Validation::default()
        ).context("Invalid JWT token signature")?;

        let now = Utc::now().timestamp();
        if now > token_data.claims.exp {
            return Err(anyhow::anyhow!("Token expired"));
        }

        // Verify issuer
        if token_data.claims.iss != self.issuer {
            return Err(anyhow::anyhow!("Invalid token issuer"));
        }

        Ok(token_data.claims)
    }

    /// Extract token from Authorization header
    pub fn extract_token_from_header(auth_header: Option<&str>) -> Option<&str> {
        auth_header?
            .strip_prefix("Bearer ")
    }

    /// Create user from database row
    pub fn create_user_from_db_row(row: &Row, username: &str) -> User {
        let user_id: &str = row.get("UserID").unwrap_or(username);
        let email: &str = row.get("UserEmail").unwrap_or("");
        let display_name: &str = row.get("FullName").unwrap_or(username);
        let is_active: bool = row.get("IsActive").unwrap_or(true);

        User {
            user_id: user_id.to_string(),
            username: username.to_string(),
            email: if email.is_empty() {
                format!("{username}@nwfth.com")
            } else {
                email.to_string()
            },
            display_name: display_name.to_string(),
            is_active,
        }
    }

    /// Create user from LDAP (when no database record exists)
    pub fn create_user_from_ldap(username: &str) -> User {
        User {
            user_id: username.to_string(),
            username: username.to_string(),
            email: format!("{username}@nwfth.com"),
            display_name: username.to_string(),
            is_active: true,
        }
    }
}