//! # Steward Service Configuration
//!
//! Configuration is loaded from environment variables and config files.
//!
//! ## Environment Variables
//!
//! - `STEWARD_API_PORT` - HTTP API port (default: 8080)
//! - `STEWARD_DATABASE_URL` - Database connection string
//! - `STEWARD_POLICY_FILE` - Path to policy file
//! - `STEWARD_TELEGRAM_TOKEN` - Telegram bot token
//! - `STEWARD_TELEGRAM_ENABLED` - Enable Telegram bot (default: true)
//! - `STEWARD_LOG_LEVEL` - Logging level (default: info)
//! - `STEWARD_RATE_LIMIT_RPM` - Rate limit per minute (default: 60)

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Steward service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StewardConfig {
    /// API server configuration
    pub api: ApiConfig,
    /// Database configuration
    pub database: DatabaseConfig,
    /// Policy configuration
    pub policy: PolicyConfig,
    /// Telegram bot configuration
    pub telegram: TelegramConfig,
    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,
    /// Security configuration
    pub security: SecurityConfig,
    /// Approval channel configuration
    pub approval: ApprovalConfig,
    /// Pimlico gas sponsorship configuration
    pub pimlico: PimlicoConfig,
}

impl StewardConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();

        let api = ApiConfig::from_env()?;
        let database = DatabaseConfig::from_env()?;
        let policy = PolicyConfig::from_env()?;
        let telegram = TelegramConfig::from_env()?;
        let rate_limit = RateLimitConfig::from_env()?;
        let security = SecurityConfig::from_env()?;
        let approval = ApprovalConfig::from_env()?;
        let pimlico = PimlicoConfig::from_env()?;

        Ok(Self {
            api,
            database,
            policy,
            telegram,
            rate_limit,
            security,
            approval,
            pimlico,
        })
    }

    /// Get database URL
    pub fn database_url(&self) -> &str {
        &self.database.url
    }

    /// Get policy file path
    pub fn policy_file(&self) -> &PathBuf {
        &self.policy.file_path
    }

    /// Check if Telegram is enabled
    pub fn telegram_enabled(&self) -> bool {
        self.telegram.enabled
    }
}

/// API server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// Port to listen on
    pub port: u16,
    /// Host to bind to
    pub host: String,
    /// Enable CORS
    pub cors_enabled: bool,
    /// API key for agent authentication
    pub api_key: Option<String>,
    /// Request timeout in seconds
    pub request_timeout_secs: u64,
    /// Maximum request body size in bytes
    pub max_body_size: usize,
    /// Long-polling timeout for transaction submission (seconds)
    /// Agent waits up to this time for signature. If approval takes longer,
    /// returns pending status and agent can poll.
    pub long_poll_timeout_secs: u64,
    /// Default long-poll timeout for agent requests
    pub default_wait_timeout_secs: u64,
    /// Test mode: bypass key loading for UX testing
    /// WARNING: Only use for development/testing, never in production
    pub test_mode: bool,
}

impl ApiConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            port: std::env::var("STEWARD_API_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8080),
            host: std::env::var("STEWARD_API_HOST")
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
            cors_enabled: std::env::var("STEWARD_CORS_ENABLED")
                .map(|s| s == "true")
                .unwrap_or(true),
            api_key: std::env::var("STEWARD_API_KEY").ok(),
            request_timeout_secs: std::env::var("STEWARD_REQUEST_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            max_body_size: std::env::var("STEWARD_MAX_BODY_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1024 * 1024), // 1MB
            long_poll_timeout_secs: std::env::var("STEWARD_LONG_POLL_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30), // 30 seconds default for long-polling
            default_wait_timeout_secs: std::env::var("STEWARD_DEFAULT_WAIT_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30), // Default wait time if not specified by agent
            test_mode: std::env::var("STEWARD_TEST_MODE")
                .map(|s| s == "true")
                .unwrap_or(false),
        })
    }
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database URL
    pub url: String,
    /// Maximum connections in pool
    pub max_connections: u32,
    /// Connection timeout in seconds
    pub connection_timeout_secs: u64,
    /// Enable WAL mode (SQLite only)
    pub wal_mode: bool,
}

impl DatabaseConfig {
    pub fn from_env() -> Result<Self> {
        let url = std::env::var("STEWARD_DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://./steward.db".to_string());

        Ok(Self {
            url,
            max_connections: std::env::var("STEWARD_DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            connection_timeout_secs: std::env::var("STEWARD_DB_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            wal_mode: std::env::var("STEWARD_DB_WAL_MODE")
                .map(|s| s == "true")
                .unwrap_or(true),
        })
    }
}

/// Policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    /// Path to policy file
    pub file_path: PathBuf,
    /// Auto-reload policy on change
    pub auto_reload: bool,
    /// Default policy if file doesn't exist
    pub default_policy: String,
}

impl PolicyConfig {
    pub fn from_env() -> Result<Self> {
        let file_path = std::env::var("STEWARD_POLICY_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./policy.json"));

        Ok(Self {
            file_path,
            auto_reload: std::env::var("STEWARD_POLICY_AUTO_RELOAD")
                .map(|s| s == "true")
                .unwrap_or(false),
            default_policy: std::env::var("STEWARD_DEFAULT_POLICY")
                .unwrap_or_else(|_| include_str!("../default_policy.json").to_string()),
        })
    }
}

/// Telegram bot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    /// Bot token from BotFather
    pub token: Option<String>,
    /// Enable Telegram bot
    pub enabled: bool,
    /// Webhook URL (optional, uses polling if not set)
    pub webhook_url: Option<String>,
    /// Webhook port
    pub webhook_port: u16,
    /// Webhook secret token
    pub webhook_secret: Option<String>,
    /// Allowed chat IDs (empty = allow all)
    pub allowed_chats: Vec<i64>,
    /// Notification settings
    pub notifications: NotificationConfig,
}

impl TelegramConfig {
    pub fn from_env() -> Result<Self> {
        let token = std::env::var("STEWARD_TELEGRAM_TOKEN").ok();
        let enabled = std::env::var("STEWARD_TELEGRAM_ENABLED")
            .map(|s| s == "true")
            .unwrap_or(token.is_some());

        let allowed_chats = std::env::var("STEWARD_TELEGRAM_ALLOWED_CHATS")
            .ok()
            .map(|s| {
                s.split(',')
                    .filter_map(|id| id.trim().parse().ok())
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            token,
            enabled,
            webhook_url: std::env::var("STEWARD_TELEGRAM_WEBHOOK_URL").ok(),
            webhook_port: std::env::var("STEWARD_TELEGRAM_WEBHOOK_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8443),
            webhook_secret: std::env::var("STEWARD_TELEGRAM_WEBHOOK_SECRET").ok(),
            allowed_chats,
            notifications: NotificationConfig::from_env()?,
        })
    }
}

/// Notification settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    /// Notify on auto-approved transactions
    pub on_auto_approve: bool,
    /// Notify on approval required
    pub on_approval_required: bool,
    /// Notify on transaction rejection
    pub on_rejection: bool,
    /// Notify on transaction execution
    pub on_execution: bool,
    /// Notify on errors
    pub on_error: bool,
}

impl NotificationConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            on_auto_approve: std::env::var("STEWARD_NOTIFY_AUTO_APPROVE")
                .map(|s| s == "true")
                .unwrap_or(false),
            on_approval_required: std::env::var("STEWARD_NOTIFY_APPROVAL")
                .map(|s| s == "true")
                .unwrap_or(true),
            on_rejection: std::env::var("STEWARD_NOTIFY_REJECTION")
                .map(|s| s == "true")
                .unwrap_or(true),
            on_execution: std::env::var("STEWARD_NOTIFY_EXECUTION")
                .map(|s| s == "true")
                .unwrap_or(true),
            on_error: std::env::var("STEWARD_NOTIFY_ERROR")
                .map(|s| s == "true")
                .unwrap_or(true),
        })
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Requests per minute
    pub requests_per_minute: u32,
    /// Burst size
    pub burst_size: u32,
    /// Per-IP rate limiting
    pub per_ip: bool,
    /// Per-agent rate limiting
    pub per_agent: bool,
}

impl RateLimitConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            requests_per_minute: std::env::var("STEWARD_RATE_LIMIT_RPM")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60),
            burst_size: std::env::var("STEWARD_RATE_LIMIT_BURST")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            per_ip: std::env::var("STEWARD_RATE_LIMIT_PER_IP")
                .map(|s| s == "true")
                .unwrap_or(true),
            per_agent: std::env::var("STEWARD_RATE_LIMIT_PER_AGENT")
                .map(|s| s == "true")
                .unwrap_or(true),
        })
    }
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Argon2 memory cost (KB)
    pub argon2_memory: u32,
    /// Argon2 iterations
    pub argon2_iterations: u32,
    /// Argon2 parallelism
    pub argon2_parallelism: u32,
    /// Session timeout in minutes
    pub session_timeout_minutes: u64,
    /// Maximum pending transactions
    pub max_pending_transactions: usize,
    /// Require password for policy changes
    pub require_password_for_policy: bool,
}

impl SecurityConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            argon2_memory: std::env::var("STEWARD_ARGON2_MEMORY")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(65536), // 64MB
            argon2_iterations: std::env::var("STEWARD_ARGON2_ITERATIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),
            argon2_parallelism: std::env::var("STEWARD_ARGON2_PARALLELISM")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(4),
            session_timeout_minutes: std::env::var("STEWARD_SESSION_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            max_pending_transactions: std::env::var("STEWARD_MAX_PENDING")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),
            require_password_for_policy: std::env::var("STEWARD_PASSWORD_FOR_POLICY")
                .map(|s| s == "true")
                .unwrap_or(true),
        })
    }
}

/// Approval channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalConfig {
    /// Timeout for approval in seconds
    pub timeout_secs: u64,
    /// Enable terminal approval (fallback)
    pub terminal_enabled: bool,
    /// Enable Telegram approval
    pub telegram_enabled: bool,
}

impl ApprovalConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            timeout_secs: std::env::var("STEWARD_APPROVAL_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(300), // 5 minutes default
            terminal_enabled: std::env::var("STEWARD_APPROVAL_TERMINAL")
                .map(|s| s == "true")
                .unwrap_or(true), // Enabled by default for testing
            telegram_enabled: std::env::var("STEWARD_APPROVAL_TELEGRAM")
                .map(|s| s == "true")
                .unwrap_or(true), // Enabled if Telegram is configured
        })
    }
}

/// Pimlico configuration for gas sponsorship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PimlicoConfig {
    /// API key for Pimlico
    pub api_key: Option<String>,
    /// Chain ID
    pub chain_id: u64,
    /// Custom RPC URL (optional)
    pub rpc_url: Option<String>,
    /// Enable gas sponsorship
    pub enabled: bool,
    /// EntryPoint address (optional, defaults to v0.7)
    pub entry_point: Option<String>,
    /// Factory address for smart account deployment
    pub factory: Option<String>,
    /// USDC contract address
    pub usdc: Option<String>,
}

impl PimlicoConfig {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("STEWARD_PIMLICO_API_KEY").ok();
        let chain_id = std::env::var("STEWARD_CHAIN_ID")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(84532); // Base Sepolia default

        Ok(Self {
            enabled: api_key.is_some(),
            api_key,
            chain_id,
            rpc_url: std::env::var("STEWARD_PIMLICO_RPC_URL").ok(),
            entry_point: std::env::var("STEWARD_ENTRY_POINT").ok(),
            factory: std::env::var("STEWARD_FACTORY").ok(),
            usdc: std::env::var("STEWARD_USDC").ok(),
        })
    }

    /// Load configuration from a JSON file
    pub fn from_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path, e))?;
        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", path, e))?;

        let api_key = json.get("apiKey")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let chain_id = json.get("chainId")
            .and_then(|v| v.as_u64())
            .unwrap_or(84532);
        let rpc_url = json.get("rpcUrl")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let entry_point = json.get("entryPoint")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let factory = json.get("factory")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let usdc = json.get("usdc")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Ok(Self {
            enabled: api_key.is_some(),
            api_key,
            chain_id,
            rpc_url,
            entry_point,
            factory,
            usdc,
        })
    }

    /// Get the RPC URL for Pimlico
    pub fn get_rpc_url(&self) -> String {
        self.rpc_url.clone().unwrap_or_else(|| {
            let chain_name = match self.chain_id {
                84532 => "base-sepolia",
                8453 => "base",
                1 => "ethereum",
                11155111 => "sepolia",
                _ => "base-sepolia",
            };
            format!("https://api.pimlico.io/v1/{}/rpc", chain_name)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ApiConfig::from_env().unwrap();
        assert_eq!(config.port, 8080);
        assert_eq!(config.host, "0.0.0.0");
        assert!(config.cors_enabled);
    }

    #[test]
    fn test_rate_limit_config() {
        let config = RateLimitConfig::from_env().unwrap();
        assert_eq!(config.requests_per_minute, 60);
        assert_eq!(config.burst_size, 10);
    }
}
