use axum::{
    extract::State,
    http::{header, Method, StatusCode, HeaderMap},
    middleware::from_fn_with_state,
    response::{Html, IntoResponse, Json},
    routing::{get, post, put},
    Router,
};
use ldap3::{LdapConnAsync, LdapConnSettings, Scope, SearchEntry};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tiberius::{Query as TiberiusQuery, Row};
use tokio::time;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::{debug, error, info, instrument, warn};

// Security libraries
use bcrypt;
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

mod constants;
mod database;
mod handlers;
mod middleware;
mod models;
mod services;
mod types;
mod utils;

use handlers::putaway;
use middleware::auth::jwt_auth_middleware;
use types::{ApiResponse, LoginResponse, User};
use utils::AuthService;

#[derive(Clone)]
pub struct AppState {
    pub database: database::Database,
    pub ldap_config: LdapConfig,
    pub auth_service: AuthService,
    pub static_assets_path: String,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("database", &self.database)
            .field("ldap_enabled", &self.ldap_config.enabled)
            .finish()
    }
}


#[derive(Clone, Debug)]
pub struct LdapConfig {
    pub url: String,
    pub base_dn: String,
    pub enabled: bool,
    pub use_ssl: bool,
    pub skip_verify: bool,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginData {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_at: i64,
    pub expires_in: i64,
    pub user: User,
}

// LoginResponse and User are defined in types/mod.rs

#[derive(Serialize)]
pub struct HealthResponse {
    pub success: bool,
    pub status: String,
    pub message: String,
    pub timestamp: String,
    pub version: String,
}

#[derive(Serialize)]
pub struct DatabaseStatusResponse {
    pub success: bool,
    pub database: String,
    pub timestamp: String,
}

#[derive(Serialize)]
pub struct AuthHealthResponse {
    pub success: bool,
    pub status: String,
    pub message: String,
    pub primary_database: String,
    pub tbl_user_exists: bool,
    pub ldap_enabled: bool,
    pub issues: Vec<String>,
    pub timestamp: String,
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Health check endpoint
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        success: true,
        status: "healthy".to_string(),
        message: "Putaway backend is running".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        version: VERSION.to_string(),
    })
}

/// Database status endpoint - shows current database configuration
async fn database_status(State(state): State<AppState>) -> Json<DatabaseStatusResponse> {
    Json(DatabaseStatusResponse {
        success: true,
        database: state.database.get_database_name().to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

/// Authentication health check endpoint - validates authentication dependencies
async fn auth_health(State(state): State<AppState>) -> Json<AuthHealthResponse> {
    let mut issues = Vec::new();
    let database_name = state.database.get_database_name().to_string();
    let ldap_enabled = state.ldap_config.enabled;

    // Check if tbl_user table exists
    let tbl_user_exists = match state.database.table_exists("tbl_user").await {
        Ok(exists) => {
            if !exists {
                issues.push("Authentication table 'tbl_user' not found in database".to_string());
            }
            exists
        }
        Err(e) => {
            issues.push(format!("Failed to check authentication table: {e}"));
            false
        }
    };

    // Determine overall status
    let status = if issues.is_empty() {
        "healthy"
    } else {
        "degraded"
    };

    let message = if issues.is_empty() {
        "All authentication dependencies are available"
    } else {
        "Authentication service has configuration issues"
    };

    Json(AuthHealthResponse {
        success: issues.is_empty(),
        status: status.to_string(),
        message: message.to_string(),
        primary_database: database_name,
        tbl_user_exists,
        ldap_enabled,
        issues,
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

/// Authentication status check endpoint
async fn auth_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<ApiResponse<bool>> {
    // Check for Authorization header
    if let Some(auth_header) = headers.get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                // Validate the JWT token
                match state.auth_service.verify_token(token) {
                    Ok(_) => return Json(ApiResponse::success(true, "User is authenticated")),
                    Err(_) => return Json(ApiResponse::success(false, "Invalid token")),
                }
            }
        }
    }

    // No valid token found
    Json(ApiResponse::success(false, "No authentication token provided"))
}

/// Authentication endpoint with proper JWT tokens
#[instrument(skip(state, request))]
async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<ApiResponse<LoginResponse>>, StatusCode> {
    debug!("🔐 Login attempt for username: {}", request.username);
    info!("🔐 Login attempt received");

    if !state.ldap_config.enabled {
        warn!("⚠️ LDAP authentication is disabled");
        return Ok(Json(ApiResponse::error("Authentication is currently disabled")));
    }

    // Try both domain formats for LDAP authentication
    let user_formats = vec![
        format!("{}@NWFTH.com", request.username),
        format!("{}@newlywedsfoods.co.th", request.username),
        request.username.clone(),
    ];

    for user_format in user_formats {
        info!("🔍 Attempting LDAP authentication for: {}", user_format);

        match authenticate_ldap(&state.ldap_config, &user_format, &request.password).await {
            Ok(user) => {
                info!("✅ LDAP authentication successful for: {}", user_format);

                // Generate proper JWT token
                match state.auth_service.generate_token(&user) {
                    Ok(token) => {
                        let login_response = LoginResponse { token, user };
                        return Ok(Json(ApiResponse::success(login_response, "Authentication successful")));
                    }
                    Err(e) => {
                        error!("❌ Failed to generate JWT token: {}", e);
                        return Ok(Json(ApiResponse::error("Failed to generate authentication token")));
                    }
                }
            }
            Err(e) => {
                info!("❌ LDAP authentication failed for {}: {}", user_format, e);
                continue;
            }
        }
    }

    // Try SQL fallback authentication
    debug!("🔄 LDAP authentication failed, attempting SQL fallback");
    match authenticate_sql(&state, &request.username, &request.password).await {
        Ok(user) => {
            info!("✅ SQL authentication successful for: {}", request.username);

            // Generate proper JWT token
            match state.auth_service.generate_token(&user) {
                Ok(token) => {
                    let login_response = LoginResponse { token, user };
                    Ok(Json(ApiResponse::success(login_response, "Authentication successful")))
                }
                Err(e) => {
                    error!("❌ Failed to generate JWT token: {}", e);
                    Ok(Json(ApiResponse::error("Failed to generate authentication token")))
                }
            }
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("Authentication table 'tbl_user' not found") {
                error!("🚨 Database configuration error: {}", error_msg);
                Ok(Json(ApiResponse::error("Authentication service unavailable. Please contact system administrator.")))
            } else if error_msg.contains("Invalid object name 'tbl_user'") {
                error!("🚨 Database table missing: tbl_user table not found in current database");
                Ok(Json(ApiResponse::error("Authentication service unavailable. Please contact system administrator.")))
            } else {
                warn!("❌ Authentication failed for user {}: {}", request.username, e);
                Ok(Json(ApiResponse::error("Invalid username or password")))
            }
        }
    }
}

async fn authenticate_ldap(
    config: &LdapConfig,
    username: &str,
    password: &str,
) -> Result<User, Box<dyn std::error::Error + Send + Sync>> {
    // Configure LDAP connection with SSL/TLS support
    let mut settings = LdapConnSettings::new()
        .set_conn_timeout(Duration::from_secs(5));

    if config.skip_verify {
        // SEC-008: LDAP skip_verify only allowed in development
        if std::env::var("RUST_ENV").unwrap_or_default() != "development" {
            return Err("LDAP_SKIP_VERIFY is only allowed in development environment".into());
        }
        tracing::warn!("⚠️ LDAP SSL certificate verification is DISABLED - development only!");
        settings = settings.set_no_tls_verify(true);
    }

    let (conn, mut ldap) = if config.use_ssl {
        // Use LDAPS (LDAP over SSL/TLS)
        LdapConnAsync::with_settings(settings, &config.url).await?
    } else {
        // Use plain LDAP (not recommended for production)
        LdapConnAsync::with_settings(settings, &config.url).await?
    };
    ldap3::drive!(conn);

    // Bind with user credentials
    ldap.simple_bind(username, password).await?.success()?;

    // Search for user information with enhanced debugging
    let search_filter = if username.contains('@') {
        format!("(userPrincipalName={username})")
    } else {
        format!("(sAMAccountName={username})")
    };

    info!("🔍 LDAP search starting - Base DN: '{}', Filter: '{}'", config.base_dn, search_filter);

    let (results, _res) = ldap
        .search(&config.base_dn, Scope::Subtree, &search_filter, vec![
            "cn", "department", "displayName", "givenName", "sAMAccountName",
            "company", "title", "organizationalUnit", "ou", "description",
            "physicalDeliveryOfficeName", "division", "departmentNumber"
        ])
        .await?
        .success()?;

    info!("📊 LDAP search completed - Found {} results", results.len());

    // If primary search failed, try alternative search strategies
    let final_results = if results.is_empty() {
        info!("⚠️  Primary search failed, trying alternative search strategies...");

        // Try searching with just the username part (before @)
        let alt_username = if username.contains('@') {
            username.split('@').next().unwrap_or(username)
        } else {
            username
        };

        let alt_filter = format!("(sAMAccountName={alt_username})");
        info!("🔄 Trying alternative search - Filter: '{}'", alt_filter);

        let (alt_results, _) = ldap
            .search(&config.base_dn, Scope::Subtree, &alt_filter, vec![
                "cn", "department", "displayName", "givenName", "sAMAccountName",
                "company", "title", "organizationalUnit", "ou", "description",
                "physicalDeliveryOfficeName", "division", "departmentNumber"
            ])
            .await?
            .success()?;

        info!("🔄 Alternative search completed - Found {} results", alt_results.len());
        alt_results
    } else {
        results
    };

    let user = if let Some(entry) = final_results.into_iter().next() {
        let search_entry = SearchEntry::construct(entry);

        // Debug logging: see what LDAP returns
        info!("LDAP attributes for {}: {:?}", username, search_entry.attrs);

        // Extract clean username from email format
        let clean_username = if username.contains('@') {
            username.split('@').next().unwrap_or(username).to_string()
        } else {
            username.to_string()
        };

        // Get display name with improved fallback logic
        let display_name = search_entry
            .attrs
            .get("displayName")
            .or_else(|| search_entry.attrs.get("cn"))
            .and_then(|v| v.first())
            .cloned()
            .unwrap_or_else(|| {
                // Better fallback: capitalize first letter of clean username
                let mut chars = clean_username.chars();
                match chars.next() {
                    None => clean_username.clone(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            });

        let first_name = search_entry
            .attrs
            .get("givenName")
            .and_then(|v| v.first())
            .cloned();

        // Enhanced department extraction with multiple fallback options
        info!("🔍 All LDAP attributes for {}: {:#?}", username, search_entry.attrs);

        let department = search_entry.attrs.get("department")
            .and_then(|v| v.first())
            .cloned()
            .or_else(|| {
                info!("📋 'department' field empty, trying 'company'");
                search_entry.attrs.get("company").and_then(|v| v.first()).cloned()
            })
            .or_else(|| {
                info!("📋 'company' field empty, trying 'title'");
                search_entry.attrs.get("title").and_then(|v| v.first()).cloned()
            })
            .or_else(|| {
                info!("📋 'title' field empty, trying 'organizationalUnit'");
                search_entry.attrs.get("organizationalUnit").and_then(|v| v.first()).cloned()
            })
            .or_else(|| {
                info!("📋 'organizationalUnit' field empty, trying 'ou'");
                search_entry.attrs.get("ou").and_then(|v| v.first()).cloned()
            })
            .or_else(|| {
                info!("📋 'ou' field empty, trying 'division'");
                search_entry.attrs.get("division").and_then(|v| v.first()).cloned()
            })
            .or_else(|| {
                info!("📋 'division' field empty, trying 'physicalDeliveryOfficeName'");
                search_entry.attrs.get("physicalDeliveryOfficeName").and_then(|v| v.first()).cloned()
            })
            .or_else(|| {
                info!("📋 'physicalDeliveryOfficeName' field empty, trying 'description'");
                search_entry.attrs.get("description").and_then(|v| v.first()).cloned()
            });

        info!("✅ LDAP user created: username='{}', display_name='{}', first_name='{:?}', department='{:?}'", clean_username, display_name, first_name, department);

        if department.is_none() {
            info!("⚠️  No department information found in any AD attribute for user: {}", username);
        } else {
            info!("🎯 Department successfully extracted: '{:?}'", department);
        }

        User {
            user_id: clean_username.clone(),
            username: clean_username.clone(),
            email: format!("{clean_username}@nwfth.com"),
            display_name,
            is_active: true,
        }
    } else {
        // Extract clean username for fallback case too
        let clean_username = if username.contains('@') {
            username.split('@').next().unwrap_or(username).to_string()
        } else {
            username.to_string()
        };

        // Capitalize first letter for display
        let display_name = {
            let mut chars = clean_username.chars();
            match chars.next() {
                None => clean_username.clone(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        };

        User {
            user_id: clean_username.clone(),
            username: clean_username.clone(),
            email: format!("{clean_username}@nwfth.com"),
            display_name,
            is_active: true,
        }
    };

    ldap.unbind().await?;
    Ok(user)
}

async fn authenticate_sql(
    state: &AppState,
    username: &str,
    password: &str,
) -> Result<User, Box<dyn std::error::Error + Send + Sync>> {
    // Check if tbl_user table exists before attempting authentication
    if !state.database.table_exists("tbl_user").await? {
        return Err("Authentication service unavailable".into());
    }

    let query = r#"
        SELECT uname, pword, Fname, Lname, department
        FROM tbl_user
        WHERE uname = @P1 AND ad_enabled = 1
    "#;

    let mut client = state.database.get_client().await?;
    let mut query_builder = TiberiusQuery::new(query);
    query_builder.bind(username);

    let stream = query_builder.query(&mut client).await?;
    let rows: Vec<Vec<Row>> = stream.into_results().await?;

    if let Some(row) = rows.first().and_then(|r| r.first()) {
        let stored_password: &str = row.get("pword").unwrap_or("");

        // SEC-003: Bcrypt password verification with backward compatibility
        let password_valid = if stored_password.starts_with("$2") {
            // Password is bcrypt hashed
            bcrypt::verify(password, stored_password).unwrap_or(false)
        } else {
            // Legacy plain text password - check and log for migration
            let valid = password == stored_password;
            if valid {
                tracing::warn!("User '{}' is using legacy plain text password. Schedule migration to bcrypt.", username);
            }
            valid
        };

        if password_valid {
            let fname: Option<&str> = row.get("Fname");
            let lname: Option<&str> = row.get("Lname");

            // For SQL authentication, use Fname/Lname for display name
            let display_name = match (fname, lname) {
                (Some(f), Some(l)) => format!("{f} {l}"),
                (Some(f), None) => f.to_string(),
                (None, Some(l)) => l.to_string(),
                (None, None) => username.to_string(), // Fallback to username
            };
            let _department: Option<&str> = row.get("department");

            Ok(User {
                user_id: username.to_string(),
                username: username.to_string(),
                email: format!("{username}@nwfth.com"),
                display_name,
                is_active: true,
            })
        } else {
            // SEC-008 FIX: Generic error message to prevent user enumeration
            Err("Invalid credentials".into())
        }
    } else {
        // SEC-008 FIX: Same generic error to prevent user enumeration
        Err("Invalid credentials".into())
    }
}

/// SEC-005: Rate limiting middleware for authentication endpoint
/// Limits authentication attempts to 5 per minute per IP address
async fn rate_limit_middleware(
    State(limiter): State<Arc<DefaultKeyedRateLimiter<SocketAddr>>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<impl IntoResponse, StatusCode> {
    // Get client IP address
    let addr = request.extensions()
        .get::<SocketAddr>()
        .copied()
        .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 0)));

    // Check rate limit
    match limiter.check_key(&addr) {
        Ok(()) => {
            // Allow the request
            Ok(next.run(request).await)
        }
        Err(_) => {
            warn!("🚫 Rate limit exceeded for IP: {}", addr);
            Err(StatusCode::TOO_MANY_REQUESTS)
        }
    }
}

/// Serve the static Angular application with optimized path resolution
async fn handle_spa_or_static(State(state): State<AppState>, uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    // Don't handle API routes - return 404 to let them be handled by proper API handlers
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Check if it's a request for a static asset
    if path.starts_with("assets/") ||
       path.ends_with(".js") ||
       path.ends_with(".css") ||
       path.ends_with(".ico") ||
       path.ends_with(".png") ||
       path.ends_with(".jpg") ||
       path.ends_with(".svg") ||
       path.ends_with(".json") ||
       path.ends_with(".webmanifest") {
        // Use pre-determined static assets path for better performance
        let file_path = format!("{}/{}", state.static_assets_path, path);

        match tokio::fs::read(&file_path).await {
            Ok(content) => {
                let content_type = match path.split('.').next_back().unwrap_or("") {
                    "js" => "application/javascript",
                    "css" => "text/css",
                    "html" => "text/html",
                    "json" => "application/json",
                    "png" => "image/png",
                    "jpg" | "jpeg" => "image/jpeg",
                    "svg" => "image/svg+xml",
                    "ico" => "image/x-icon",
                    "webmanifest" => "application/manifest+json",
                    _ => "text/plain",
                };

                return ([(header::CONTENT_TYPE, content_type)], content).into_response();
            }
            Err(_) => {
                // File not found, serve index.html for SPA routing
            }
        }

        // File not found, serve index.html for SPA routing
        serve_index_html(&state.static_assets_path).await.into_response()
    } else {
        // For all other routes, serve index.html (SPA routing)
        serve_index_html(&state.static_assets_path).await.into_response()
    }
}

async fn serve_index_html(static_assets_path: &str) -> impl IntoResponse {
    let index_path = format!("{static_assets_path}/index.html");

    match tokio::fs::read_to_string(&index_path).await {
        Ok(content) => {
            info!("✅ Successfully served index.html from: {}", index_path);
            Html(content).into_response()
        }
        Err(e) => {
            warn!("🚨 Failed to read index.html from {}: {}", index_path, e);
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing with environment-based filtering
    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| {
        if cfg!(debug_assertions) {
            "putaway_backend=info,tower_http=warn".to_string()
        } else {
            "putaway_backend=warn,tower_http=error".to_string()
        }
    });

    std::env::set_var("RUST_LOG", &log_level);
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("🚀 Starting Putaway Backend v{}", VERSION);

    // Load environment variables from .env file
    dotenv::dotenv().ok();

    // Server configuration
    let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    // PUTAWAY: Default port changed from 4400 to 4402
    let port = std::env::var("SERVER_PORT")
        .unwrap_or_else(|_| "4402".to_string())
        .parse::<u16>()
        .unwrap_or(4402);

    // CORS configuration
    let cors_origins = std::env::var("CORS_ORIGINS").unwrap_or_else(|_| "*".to_string());

    info!("Server configured to run on {}:{}", host, port);
    info!("CORS origins: {}", cors_origins);

    // LDAP configuration
    // Default to LDAPS (port 636) with skip_verify for internal networks
    // Most corporate LDAP servers use self-signed certificates
    let ldap_url = std::env::var("LDAP_URL")
        .unwrap_or_else(|_| "ldaps://192.168.0.1:636".to_string());
    let use_ssl = std::env::var("LDAP_USE_SSL")
        .unwrap_or_else(|_| "true".to_string())  // Default to SSL enabled
        .parse()
        .unwrap_or(true);
    let skip_verify = std::env::var("LDAP_SKIP_VERIFY")
        .unwrap_or_else(|_| "true".to_string())  // Default to skip for internal/self-signed certs
        .parse()
        .unwrap_or(true);

    if use_ssl && !skip_verify {
        info!("🔒 LDAPS (SSL/TLS) enabled with certificate verification");
    } else if use_ssl && skip_verify {
        info!("🔒 LDAPS (SSL/TLS) enabled (certificate verification skipped for internal network)");
    } else {
        warn!("⚠️  LDAP using plain text (INSECURE - not recommended for production)");
    }

    let ldap_config = LdapConfig {
        url: ldap_url,
        base_dn: std::env::var("LDAP_BASE_DN").unwrap_or_else(|_| "DC=NWFTH,DC=com".to_string()),
        enabled: std::env::var("LDAP_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true),
        use_ssl,
        skip_verify,
    };

    info!(
        "LDAP configured: {} with base DN: {}",
        ldap_config.url, ldap_config.base_dn
    );

    // Initialize database connection with pooling
    let database = database::Database::new().await.expect("Failed to initialize database with connection pool");

    // Validate authentication tables exist in database
    info!("🔍 Validating authentication tables in database...");
    match database.table_exists("tbl_user").await {
        Ok(true) => {
            info!("✅ Authentication table 'tbl_user' found in database");
        }
        Ok(false) => {
            warn!("⚠️  Authentication table 'tbl_user' not found in database");
            warn!("    SQL authentication will be unavailable for local users");
            warn!("    LDAP authentication will still function normally");
            warn!("    Create the tbl_user table to enable SQL fallback authentication");
        }
        Err(e) => {
            warn!("⚠️  Failed to check authentication table: {}", e);
            warn!("    SQL authentication may be unavailable");
            warn!("    LDAP authentication will still function normally");
        }
    }

    // Initialize authentication service
    let auth_service = AuthService::new().expect("Failed to initialize JWT authentication service");

    // Determine static assets path at startup for better performance
    let static_assets_path = {
        let possible_paths = vec![
            "/app/frontend/dist/frontend/browser",  // Docker container path (production)
            "../frontend/dist/frontend/browser",    // Development relative path
            "frontend/dist/frontend/browser",       // Alternative relative path
            "./frontend/dist/frontend/browser",     // Current directory path
        ];

        let mut selected_path = possible_paths[0].to_string(); // Default fallback
        for path in possible_paths {
            if tokio::fs::metadata(path).await.is_ok() {
                selected_path = path.to_string();
                break;
            }
        }

        info!("📁 Static assets will be served from: {}", selected_path);
        selected_path
    };

    let state = AppState {
        database,
        ldap_config,
        auth_service,
        static_assets_path,
    };

    // SEC-004 FIX: Configure CORS with proper origin validation
    let cors = if cors_origins == "*" {
        // CRITICAL: Block wildcard CORS in production
        if std::env::var("RUST_ENV").unwrap_or_default() == "production" {
            panic!("🔒 SEC-004 CRITICAL: CORS wildcard (*) is not allowed in production. Set CORS_ORIGINS to specific origins (e.g., 'https://yourdomain.com').");
        }
        warn!("⚠️ SEC-004: CORS is configured with wildcard (*) - this is only acceptable for development!");
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
            .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::HeaderName::from_static("x-user-id")])
    } else {
        // SEC-004 FIX: Parse and validate specific origins
        info!("🔒 SEC-004: CORS configured for specific origins: {}", cors_origins);
        let origins: Vec<axum::http::HeaderValue> = cors_origins
            .split(',')
            .filter_map(|origin| {
                origin.trim().parse().ok()
            })
            .collect();

        if origins.is_empty() {
            warn!("⚠️ SEC-004: No valid CORS origins found in CORS_ORIGINS, falling back to localhost only");
            CorsLayer::new()
                .allow_origin("http://localhost:4400".parse::<axum::http::HeaderValue>().unwrap())
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::HeaderName::from_static("x-user-id")])
                .allow_credentials(true)
        } else {
            CorsLayer::new()
                .allow_origin(origins)
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::HeaderName::from_static("x-user-id")])
                .allow_credentials(true)
        }
    };

    // SEC-011: Security headers middleware
    let security_headers = tower_http::set_header::SetResponseHeaderLayer::overriding(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    let x_frame_options = tower_http::set_header::SetResponseHeaderLayer::overriding(
        header::X_FRAME_OPTIONS,
        header::HeaderValue::from_static("DENY"),
    );
    let x_xss_protection = tower_http::set_header::SetResponseHeaderLayer::overriding(
        header::HeaderName::from_static("x-xss-protection"),
        header::HeaderValue::from_static("1; mode=block"),
    );

    // SEC-005: Rate limiting for authentication endpoint (5 requests per minute per IP)
    let auth_rate_limiter = Arc::new(
        RateLimiter::keyed(Quota::per_minute(NonZeroU32::new(5).unwrap()))
    );

    // PUTAWAY ONLY: Build application with putaway routes only (no bulk-runs routes)
    let app = Router::new()
        // API routes
        .route("/api/health", get(health_check))
        .route("/api/database/status", get(database_status))
        .route("/api/auth/health", get(auth_health))
        .route("/api/auth/login", post(login))
        .route_layer(axum::middleware::from_fn_with_state(
            auth_rate_limiter.clone(),
            rate_limit_middleware,
        ))
        .route("/api/auth/status", get(auth_status))
        // Add putaway routes with Database state and JWT protection
        .nest(
            "/api/putaway",
            putaway::create_putaway_routes()
                .layer(from_fn_with_state(state.clone(), jwt_auth_middleware))
                .with_state(state.database.clone()),
        )
        // Serve static files from Angular dist (using detected path)
        .nest_service("/assets", ServeDir::new(format!("{}/assets", state.static_assets_path)))
        .fallback(handle_spa_or_static)
        .layer(cors)
        .layer(security_headers)
        .layer(x_frame_options)
        .layer(x_xss_protection)
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(&format!("{host}:{port}"))
        .await
        .expect("Failed to bind to address");

    info!("🎯 Putaway Server started successfully on http://{}:{}", host, port);
    info!("📁 Serving static files from {}", state.static_assets_path);
    info!("🔧 API endpoints available at http://{}:{}/api/", host, port);

    // Spawn connection pool monitoring task
    let db_for_monitoring = state.database.clone();
    tokio::spawn(async move {
        monitor_pool_health(db_for_monitoring).await;
    });

    axum::serve(listener, app)
        .await
        .expect("Server failed to start");
}

/// Monitor connection pool health and log warnings
async fn monitor_pool_health(database: database::Database) {
    let max_connections: u32 = 150; // Match DATABASE_MAX_CONNECTIONS default

    loop {
        time::sleep(Duration::from_secs(60)).await;
        let pool_status = database.get_pool_status();
        let usage_percent = (pool_status.total_connections as f64 / max_connections as f64) * 100.0;

        if usage_percent >= 80.0 {
            error!(
                connections = pool_status.total_connections,
                idle = pool_status.idle_connections,
                max = max_connections,
                utilization = %format!("{:.1}%", usage_percent),
                "⚠️ Connection pool utilization HIGH - consider increasing DATABASE_MAX_CONNECTIONS"
            );
        } else if usage_percent >= 70.0 {
            info!(
                connections = pool_status.total_connections,
                idle = pool_status.idle_connections,
                max = max_connections,
                utilization = %format!("{:.1}%", usage_percent),
                "⚡ Connection pool utilization elevated"
            );
        }
    }
}
