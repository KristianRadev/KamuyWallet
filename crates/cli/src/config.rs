//! # CLI Configuration
//!
//! Configuration management for the Kamuy CLI.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// Steward service URL
    pub steward_url: String,
    /// API key for Steward service
    pub api_key: Option<String>,
    /// Default chain
    pub default_chain: String,
    /// Default chain ID
    pub default_chain_id: u64,
    /// Configuration directory
    pub config_dir: PathBuf,
    /// Data directory
    pub data_dir: PathBuf,
    /// User key file path
    pub user_key_file: Option<PathBuf>,
    /// Whether to use colors in output
    pub use_color: bool,
    /// Default gas limit
    pub default_gas_limit: u64,
    /// Whether to auto-submit transactions
    pub auto_submit: bool,
}

impl Default for CliConfig {
    fn default() -> Self {
        let config_dir = dirs::config_dir()
            .map(|d| d.join("kamuy"))
            .unwrap_or_else(|| PathBuf::from(".kamuy"));
        
        let data_dir = dirs::data_dir()
            .map(|d| d.join("kamuy"))
            .unwrap_or_else(|| PathBuf::from(".kamuy"));
        
        Self {
            steward_url: "http://localhost:8080".to_string(),
            api_key: None,
            default_chain: "base".to_string(),
            default_chain_id: 8453,
            config_dir,
            data_dir,
            user_key_file: None,
            use_color: true,
            default_gas_limit: 100000,
            auto_submit: false,
        }
    }
}

impl CliConfig {
    /// Load configuration from file
    pub fn load(path: Option<&str>) -> Result<Self> {
        let config_path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            Self::default_config_path()?
        };
        
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config file: {:?}", config_path))?;
            
            let config: CliConfig = toml::from_str(&content)
                .with_context(|| "Failed to parse config file")?;
            
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }
    
    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let config_path = Self::default_config_path()?;
        
        // Ensure directory exists
        std::fs::create_dir_all(&self.config_dir)
            .with_context(|| "Failed to create config directory")?;
        
        let content = toml::to_string_pretty(self)
            .with_context(|| "Failed to serialize config")?;
        
        std::fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config file: {:?}", config_path))?;
        
        // SECURITY FIX: Set restrictive permissions (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&config_path)?.permissions();
            perms.set_mode(0o600); // Owner read/write only
            std::fs::set_permissions(&config_path, perms)
                .with_context(|| "Failed to set config file permissions")?;
        }
        
        Ok(())
    }
    
    /// Get default config path
    pub fn default_config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .map(|d| d.join("kamuy"))
            .unwrap_or_else(|| PathBuf::from(".kamuy"));
        
        Ok(config_dir.join("config.toml"))
    }
    
    /// Create with Steward URL
    pub fn with_steward_url(mut self, url: Option<String>) -> Self {
        if let Some(url) = url {
            self.steward_url = url;
        }
        self
    }
    
    /// Create with API key
    pub fn with_api_key(mut self, key: Option<String>) -> Self {
        if key.is_some() {
            self.api_key = key;
        }
        self
    }
    
    /// Get user key path
    pub fn user_key_path(&self) -> PathBuf {
        self.user_key_file.clone()
            .unwrap_or_else(|| self.data_dir.join("user.key"))
    }

    /// Initialize configuration
    pub fn init() -> Result<Self> {
        let config = Self::default();
        
        // Create directories
        std::fs::create_dir_all(&config.config_dir)?;
        std::fs::create_dir_all(&config.data_dir)?;
        
        // Save default config
        config.save()?;
        
        Ok(config)
    }
}

// ============================================================================
// SimpleConfig - v2.0 Zero-Friction Configuration
// ============================================================================

/// Simplified v2.0 config stored at ~/.kamuy/config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleConfig {
    /// Config version
    pub version: String,
    /// Steward service URL
    pub steward_url: String,
    /// Auto-generated API key
    pub api_key: String,
    /// Path to steward log
    pub steward_log: PathBuf,
    /// Path to steward PID file
    pub steward_pid_file: PathBuf,
}

impl SimpleConfig {
    /// Default config path: ~/.kamuy/config.json
    pub fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(".kamuy").join("config.json"))
    }

    /// Data directory: ~/.kamuy/
    pub fn data_dir() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(".kamuy"))
    }

    /// Load config with priority: KAMUY_CONFIG env -> ~/.kamuy/config.json
    /// Also applies env var overrides for api_key and steward_url
    pub fn load() -> Result<Option<Self>> {
        // Check for explicit config path
        let path = if let Ok(custom_path) = std::env::var("KAMUY_CONFIG") {
            PathBuf::from(custom_path)
        } else {
            Self::config_path()?
        };

        if !path.exists() {
            return Ok(None);
        }

        Self::load_from_path(&path)
    }

    /// Load config from a specific path (for --config flag support)
    pub fn load_from_path(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config: {:?}", path))?;

        let mut config: SimpleConfig = serde_json::from_str(&content)
            .with_context(|| "Failed to parse config.json")?;

        // Apply env var overrides
        if let Ok(api_key) = std::env::var("KAMUY_API_KEY") {
            config.api_key = api_key;
        }
        if let Ok(steward_url) = std::env::var("KAMUY_STEWARD_URL") {
            config.steward_url = steward_url;
        }

        Ok(Some(config))
    }

    /// Save config to ~/.kamuy/config.json
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let dir = path.parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid config path"))?;

        std::fs::create_dir_all(dir)
            .with_context(|| "Failed to create ~/.kamuy directory")?;

        let content = serde_json::to_string_pretty(self)
            .with_context(|| "Failed to serialize config")?;

        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write config: {:?}", path))?;

        // Set restrictive permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&path, perms)?;
        }

        Ok(())
    }

    /// Generate new config with random API key
    pub fn generate() -> Result<Self> {
        let api_key = Self::generate_api_key();
        let data_dir = Self::data_dir()?;

        Ok(Self {
            version: "2.0".to_string(),
            steward_url: "http://127.0.0.1:8080".to_string(),
            api_key,
            steward_log: data_dir.join("steward.log"),
            steward_pid_file: data_dir.join("steward.pid"),
        })
    }

    /// Generate random 32-byte API key (hex encoded)
    fn generate_api_key() -> String {
        use rand::RngCore;
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        hex::encode(bytes)
    }

    /// Check if wallet exists by checking for steward database
    pub fn wallet_exists() -> Result<bool> {
        let data_dir = Self::data_dir()?;
        let db_path = data_dir.join("steward.db");
        Ok(db_path.exists())
    }

    /// Migrate from old ~/.config/kamuy/ config if it exists
    pub fn migrate_from_old_config() -> Result<()> {
        let old_config_dir = dirs::config_dir()
            .map(|d| d.join("kamuy"))
            .unwrap_or_else(|| PathBuf::from(".kamuy"));

        let old_config_path = old_config_dir.join("config.toml");
        if !old_config_path.exists() {
            return Ok(());
        }

        // Old config exists, read it
        let content = std::fs::read_to_string(&old_config_path)?;
        let old_config: toml::Value = toml::from_str(&content)?;

        // Extract values and create new config
        let new_config = Self {
            version: "2.0".to_string(),
            steward_url: old_config.get("steward_url")
                .and_then(|v| v.as_str())
                .unwrap_or("http://127.0.0.1:8080")
                .to_string(),
            api_key: old_config.get("api_key")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| Self::generate_api_key()),
            steward_log: Self::data_dir()?.join("steward.log"),
            steward_pid_file: Self::data_dir()?.join("steward.pid"),
        };

        new_config.save()?;
        println!("Migrated config from {} to ~/.kamuy/config.json", old_config_dir.display());

        Ok(())
    }
}

/// Get chain ID from chain name
pub fn chain_id_from_name(name: &str) -> Option<u64> {
    match name.to_lowercase().as_str() {
        "ethereum" | "eth" | "mainnet" => Some(1),
        "base" => Some(8453),
        "polygon" | "matic" => Some(137),
        "arbitrum" | "arb" => Some(42161),
        "optimism" | "op" => Some(10),
        "sepolia" => Some(11155111),
        "base-sepolia" => Some(84532),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_chain_id_from_name() {
        assert_eq!(chain_id_from_name("base"), Some(8453));
        assert_eq!(chain_id_from_name("ethereum"), Some(1));
        assert_eq!(chain_id_from_name("polygon"), Some(137));
        assert_eq!(chain_id_from_name("unknown"), None);
    }

    #[test]
    fn test_config_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let config = CliConfig {
            steward_url: "http://test:8080".to_string(),
            api_key: Some("test-key".to_string()),
            default_chain: "base".to_string(),
            default_chain_id: 8453,
            config_dir: temp_dir.path().to_path_buf(),
            data_dir: temp_dir.path().to_path_buf(),
            user_key_file: None,
            use_color: true,
            default_gas_limit: 100000,
            auto_submit: false,
        };

        // Save
        let content = toml::to_string_pretty(&config).unwrap();
        std::fs::write(&config_path, content).unwrap();

        // Load
        let loaded = CliConfig::load(Some(config_path.to_str().unwrap())).unwrap();
        assert_eq!(loaded.steward_url, "http://test:8080");
        assert_eq!(loaded.api_key, Some("test-key".to_string()));
    }

    #[test]
    fn test_simple_config_generate() {
        let config = super::SimpleConfig::generate().unwrap();
        assert_eq!(config.version, "2.0");
        // API key is 32 bytes hex-encoded = 64 chars (no 0x prefix)
        assert_eq!(config.api_key.len(), 64);
        assert_eq!(config.steward_url, "http://127.0.0.1:8080");
    }

    #[test]
    fn test_simple_config_wallet_exists() {
        // Should return false when no wallet exists
        let result = super::SimpleConfig::wallet_exists();
        assert!(result.is_ok());
    }
}
