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
    /// Agent key file path
    pub agent_key_file: Option<PathBuf>,
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
            agent_key_file: None,
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
    
    /// Get agent key path
    pub fn agent_key_path(&self) -> PathBuf {
        self.agent_key_file.clone()
            .unwrap_or_else(|| self.data_dir.join("agent.key"))
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

/// Get chain name from chain ID
pub fn chain_name_from_id(id: u64) -> Option<String> {
    match id {
        1 => Some("ethereum".to_string()),
        8453 => Some("base".to_string()),
        137 => Some("polygon".to_string()),
        42161 => Some("arbitrum".to_string()),
        10 => Some("optimism".to_string()),
        11155111 => Some("sepolia".to_string()),
        84532 => Some("base-sepolia".to_string()),
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
            agent_key_file: None,
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
}
