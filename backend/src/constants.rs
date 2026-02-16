// Application Constants
// Centralized constants for putaway backend - used in main.rs and handlers

/// Default server host
pub const DEFAULT_SERVER_HOST: &str = "0.0.0.0";

/// Default server port (4402 for putaway)
pub const DEFAULT_SERVER_PORT: u16 = 4402;

/// Default CORS origins for development
pub const DEFAULT_CORS_ORIGINS: &str = "*";

/// Default database port (SQL Server)
pub const DEFAULT_DATABASE_PORT: u16 = 49381;

/// Default LDAP port (LDAPS)
pub const DEFAULT_LDAP_PORT: u16 = 636;
