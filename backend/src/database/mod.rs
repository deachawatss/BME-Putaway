use crate::constants;
use anyhow::{Context, Result};
use bb8::Pool;
use bb8_tiberius::ConnectionManager;
use std::env;
use std::time::Duration;
use tiberius::{AuthMethod, Config, EncryptionLevel, Query, Row};
use tracing::{info, warn};

pub mod putaway;
pub mod putaway_db;

/// Database configuration with connection pooling
#[derive(Clone, Debug)]
pub struct DatabaseConfig {
    pub server: String,
    pub database: String,
    pub username: String,
    pub password: String,
    pub port: u16,
}

/// Database management with connection pooling for high performance
#[derive(Clone)]
pub struct Database {
    /// Connection pool for all database operations
    pool: Pool<ConnectionManager>,
    /// Database configuration
    config: DatabaseConfig,
    /// Maximum pool size
    max_pool_size: u32,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database")
            .field("database", &self.config.database)
            .field("server", &self.config.server)
            .field("pool_size", &"configured")
            .finish()
    }
}

impl Database {
    /// Initialize database with connection pooling
    pub async fn new() -> Result<Self> {
        info!("🔄 Initializing database with connection pooling");

        let config = Self::load_database_config()?;

        // Read connection pool configuration from environment variables
        let max_pool_size = env::var("DATABASE_MAX_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(20u32);

        let min_pool_size = env::var("DATABASE_MIN_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5u32);

        let connection_timeout = env::var("DATABASE_CONNECTION_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10u64);

        let pool = Self::create_pool(&config, max_pool_size, min_pool_size, connection_timeout).await?;

        info!(
            "✅ Connection pool initialized - Database: {}, Max connections: {}, Min idle: {}",
            config.database, max_pool_size, min_pool_size
        );

        Ok(Self { pool, config, max_pool_size })
    }

    /// Load database configuration from environment variables
    fn load_database_config() -> Result<DatabaseConfig> {
        let server = env::var("DATABASE_SERVER")
            .with_context(|| "Missing environment variable: DATABASE_SERVER")?;
        let database = env::var("DATABASE_NAME")
            .with_context(|| "Missing environment variable: DATABASE_NAME")?;
        let username = env::var("DATABASE_USERNAME")
            .with_context(|| "Missing environment variable: DATABASE_USERNAME")?;
        let password = env::var("DATABASE_PASSWORD")
            .with_context(|| "Missing environment variable: DATABASE_PASSWORD")?;
        let port = env::var("DATABASE_PORT")
            .unwrap_or_else(|_| constants::DEFAULT_DATABASE_PORT.to_string())
            .parse()
            .unwrap_or(constants::DEFAULT_DATABASE_PORT);

        Ok(DatabaseConfig {
            server,
            database,
            username,
            password,
            port,
        })
    }

    /// Create connection pool with configurable parameters
    async fn create_pool(
        config: &DatabaseConfig,
        max_size: u32,
        min_idle: u32,
        connection_timeout_secs: u64,
    ) -> Result<Pool<ConnectionManager>> {
        // Load database encryption settings
        let database_encryption = env::var("DATABASE_ENCRYPTION")
            .unwrap_or_else(|_| "false".to_string())
            .parse()
            .unwrap_or(false);

        let database_trust_cert = env::var("DATABASE_TRUST_CERT")
            .unwrap_or_else(|_| "false".to_string())
            .parse()
            .unwrap_or(false);

        let mut tiberius_config = Config::new();
        tiberius_config.host(&config.server);
        tiberius_config.port(config.port);
        tiberius_config.database(&config.database);
        tiberius_config.authentication(AuthMethod::sql_server(&config.username, &config.password));

        // Configure database encryption
        if database_encryption {
            info!("🔒 Database encryption enabled (TLS 1.3 required)");
            tiberius_config.encryption(EncryptionLevel::Required);
        } else {
            info!("⚠️  Database encryption disabled (not recommended for production)");
            tiberius_config.encryption(EncryptionLevel::NotSupported);
        }

        // Trust certificate for self-signed certificates (internal networks)
        if database_trust_cert {
            warn!("⚠️  Database certificate trust enabled (accepting self-signed certificates)");
            tiberius_config.trust_cert();
        }

        let manager = ConnectionManager::new(tiberius_config);

        // Configure connection pool settings (now using environment variables)
        let pool = Pool::builder()
            .max_size(max_size)  // Configurable max connections (default 150, production 150)
            .min_idle(Some(min_idle))  // Configurable warm connections (default 30, production 30)
            .connection_timeout(Duration::from_secs(connection_timeout_secs))  // Configurable timeout (default 30s)
            .idle_timeout(Some(Duration::from_secs(300)))  // Close idle connections after 5 minutes
            .max_lifetime(Some(Duration::from_secs(1800)))  // Recycle connections after 30 minutes
            .build(manager)
            .await
            .context("Failed to create connection pool")?;

        // Test pool connectivity with one connection
        let test_conn = pool.get().await
            .context("Failed to get test connection from pool")?;

        info!("✅ Connection pool test successful");
        drop(test_conn);

        Ok(pool)
    }

    /// Get pooled database client connection (reuses existing connections)
    pub async fn get_client(&self) -> Result<bb8::PooledConnection<'_, ConnectionManager>> {
        self.pool.get().await
            .with_context(|| format!("Failed to get connection from pool for database: {}", self.config.database))
    }

    /// Get database name
    pub fn get_database_name(&self) -> &str {
        &self.config.database
    }

    /// Check if a table exists in the database
    pub async fn table_exists(&self, table_name: &str) -> Result<bool> {
        let mut client = self.get_client().await?;

        let query = r#"
            SELECT COUNT(*) as table_count
            FROM INFORMATION_SCHEMA.TABLES
            WHERE TABLE_NAME = @P1 AND TABLE_TYPE = 'BASE TABLE'
        "#;

        let mut query_builder = Query::new(query);
        query_builder.bind(table_name);

        let stream = query_builder.query(&mut *client).await?;
        let rows: Vec<Vec<Row>> = stream.into_results().await?;

        if let Some(row) = rows.first().and_then(|r| r.first()) {
            let count: i32 = row.get("table_count").unwrap_or(0);
            Ok(count > 0)
        } else {
            Ok(false)
        }
    }

    /// Get connection pool statistics for monitoring
    pub fn get_pool_status(&self) -> PoolStatus {
        PoolStatus {
            total_connections: self.pool.state().connections,
            idle_connections: self.pool.state().idle_connections,
            max_size: self.max_pool_size,
        }
    }
}

/// Connection pool status for monitoring
#[derive(Debug, Clone, serde::Serialize)]
pub struct PoolStatus {
    pub total_connections: u32,
    pub idle_connections: u32,
    pub max_size: u32,
}

