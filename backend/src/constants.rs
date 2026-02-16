// Application Constants
// Centralized constants to avoid magic numbers

/// Default server configuration
pub const DEFAULT_SERVER_HOST: &str = "0.0.0.0";
pub const DEFAULT_SERVER_PORT: u16 = 4400;
pub const DEFAULT_FRONTEND_PORT: u16 = 4200;

/// Database connection pool defaults
pub const DEFAULT_MAX_CONNECTIONS: u32 = 150;
pub const DEFAULT_MIN_CONNECTIONS: u32 = 30;
pub const DEFAULT_CONNECTION_TIMEOUT_SECS: u64 = 10;

/// JWT configuration defaults
pub const DEFAULT_JWT_DURATION_HOURS: i64 = 8;
pub const MIN_JWT_SECRET_LENGTH: usize = 32;

/// Rate limiting configuration
pub const AUTH_RATE_LIMIT_PER_MINUTE: u32 = 5;

/// LDAP configuration
pub const LDAP_CONNECTION_TIMEOUT_SECS: u64 = 5;

/// Pagination defaults
pub const DEFAULT_PAGE_SIZE: u32 = 50;
pub const MAX_PAGE_SIZE: u32 = 100;

/// Search query limits
pub const MAX_SEARCH_QUERY_LENGTH: usize = 100;

/// Pool monitoring interval
pub const POOL_MONITOR_INTERVAL_SECS: u64 = 60;
pub const POOL_HIGH_USAGE_THRESHOLD: f64 = 80.0;
pub const POOL_ELEVATED_USAGE_THRESHOLD: f64 = 70.0;

/// API response messages
pub const MSG_AUTH_SUCCESS: &str = "Authentication successful";
pub const MSG_AUTH_FAILED: &str = "Invalid credentials";
pub const MSG_UNAUTHORIZED: &str = "Unauthorized";
