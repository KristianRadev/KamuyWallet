//! # Policy Engine
//!
//! Core engine for loading, caching, and evaluating policies.

use super::rules::PolicyRules;
use crate::error::{StewardError, Result};
use crate::types::ApprovalLevel;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};

/// Policy engine for evaluating transactions
pub struct PolicyEngine {
    /// Current policy rules
    rules: Arc<RwLock<PolicyRules>>,
    /// Path to policy file
    policy_path: PathBuf,
    /// Auto-reload enabled
    auto_reload: bool,
    /// Last loaded timestamp
    last_loaded: Arc<RwLock<std::time::SystemTime>>,
}

impl PolicyEngine {
    /// Create a new policy engine
    pub fn new(policy_path: &Path) -> Result<Self> {
        let rules = if policy_path.exists() {
            info!("Loading policy from file: {:?}", policy_path);
            PolicyRules::from_file(policy_path)?
        } else {
            info!("Policy file not found, using defaults");
            PolicyRules::default()
        };

        Ok(Self {
            rules: Arc::new(RwLock::new(rules)),
            policy_path: policy_path.to_path_buf(),
            auto_reload: false,
            last_loaded: Arc::new(RwLock::new(std::time::SystemTime::now())),
        })
    }

    /// Create with auto-reload enabled
    pub fn with_auto_reload(policy_path: &Path) -> Result<Self> {
        let mut engine = Self::new(policy_path)?;
        engine.auto_reload = true;
        Ok(engine)
    }

    /// Get current rules (read lock)
    pub async fn rules(&self) -> PolicyRules {
        self.rules.read().await.clone()
    }

    /// Update policy rules
    pub async fn update_rules(&self, new_rules: PolicyRules) -> Result<()> {
        // Validate new rules
        new_rules.validate()?;

        // Update in memory
        let mut rules = self.rules.write().await;
        *rules = new_rules;

        // Update timestamp
        let mut last_loaded = self.last_loaded.write().await;
        *last_loaded = std::time::SystemTime::now();

        info!("Policy rules updated successfully");
        Ok(())
    }

    /// Update a specific rule
    pub async fn update_rule(&self, key: &str, value: &str) -> Result<()> {
        let mut rules = self.rules.write().await;
        rules.update(key, value)?;
        
        info!("Policy rule {} updated to {}", key, value);
        Ok(())
    }

    /// Save current rules to file
    pub async fn save(&self) -> Result<()> {
        let rules = self.rules.read().await;
        rules.save_to_file(&self.policy_path)?;
        
        info!("Policy saved to {:?}", self.policy_path);
        Ok(())
    }

    /// Reload rules from file
    pub async fn reload(&self) -> Result<()> {
        if !self.policy_path.exists() {
            return Err(StewardError::NotFound(
                format!("Policy file not found: {:?}", self.policy_path)
            ));
        }

        let new_rules = PolicyRules::from_file(&self.policy_path)?;
        
        let mut rules = self.rules.write().await;
        *rules = new_rules;

        let mut last_loaded = self.last_loaded.write().await;
        *last_loaded = std::time::SystemTime::now();

        info!("Policy reloaded from {:?}", self.policy_path);
        Ok(())
    }

    /// Check if policy file has been modified and reload if needed
    pub async fn check_and_reload(&self) -> Result<()> {
        if !self.auto_reload {
            return Ok(());
        }

        if !self.policy_path.exists() {
            return Ok(());
        }

        let metadata = std::fs::metadata(&self.policy_path)
            .map_err(|e| StewardError::Storage(format!("Failed to read policy file metadata: {}", e)))?;
        
        let modified = metadata.modified()
            .map_err(|e| StewardError::Storage(format!("Failed to get modification time: {}", e)))?;
        
        let last_loaded = *self.last_loaded.read().await;
        
        if modified > last_loaded {
            info!("Policy file modified, reloading...");
            self.reload().await?;
        }

        Ok(())
    }

    /// Start auto-reload task
    pub async fn start_auto_reload(&self, interval_secs: u64) {
        if !self.auto_reload {
            return;
        }

        let engine = self.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
            
            loop {
                interval.tick().await;
                
                if let Err(e) = engine.check_and_reload().await {
                    error!("Auto-reload error: {}", e);
                }
            }
        });
    }

    /// Reset to default policy
    pub async fn reset_to_default(&self) -> Result<()> {
        let default = PolicyRules::default();
        self.update_rules(default).await?;
        self.save().await?;
        
        info!("Policy reset to defaults");
        Ok(())
    }

    /// Get policy as JSON
    pub async fn to_json(&self) -> Result<String> {
        let rules = self.rules.read().await;
        serde_json::to_string_pretty(&*rules)
            .map_err(|e| StewardError::Serialization(e.to_string()))
    }

    /// Get policy as YAML
    pub async fn to_yaml(&self) -> Result<String> {
        let rules = self.rules.read().await;
        serde_yaml::to_string(&*rules)
            .map_err(|e| StewardError::Serialization(e.to_string()))
    }

    /// Validate a policy update without applying it
    pub async fn validate_update(&self, key: &str, value: &str) -> Result<()> {
        let rules = self.rules.read().await.clone();

        // Try to update (this validates)
        let mut test_rules = rules;
        test_rules.update(key, value)?;

        Ok(())
    }

    /// Evaluate a transaction and return required approval level
    /// Implements "higher-security-wins" logic
    pub async fn evaluate_transaction(
        &self,
        to: &str,
        amount: u64,
    ) -> (ApprovalLevel, Vec<String>) {
        let rules = self.rules.read().await.clone();
        let mut violations = Vec::new();
        let mut max_approval = ApprovalLevel::AutoApprove;

        // Check spending tracker (reset if needed)
        let mut tracker = rules.spending_tracker.clone();
        tracker.check_and_reset();

        // Check 1: Amount exceeds per-transaction limit
        if amount > rules.max_per_tx {
            violations.push(format!(
                "Amount {} exceeds max_per_tx {}",
                amount, rules.max_per_tx
            ));
            max_approval = std::cmp::max(max_approval, ApprovalLevel::TelegramButton);
        }

        // Check 2: Would exceed daily limit
        if tracker.would_exceed_daily(amount, rules.max_daily) {
            violations.push(format!(
                "Would exceed daily limit (spent: {}, limit: {}, amount: {})",
                tracker.daily_spent, rules.max_daily, amount
            ));
            max_approval = std::cmp::max(max_approval, ApprovalLevel::TelegramButton);
        }

        // Check 3: Would exceed weekly limit
        if tracker.would_exceed_weekly(amount, rules.max_weekly) {
            violations.push(format!(
                "Would exceed weekly limit (spent: {}, limit: {}, amount: {})",
                tracker.weekly_spent, rules.max_weekly, amount
            ));
            max_approval = std::cmp::max(max_approval, ApprovalLevel::TelegramButton);
        }

        // Check 4: Address not whitelisted
        if !rules.is_whitelisted(to) {
            violations.push(format!("Address {} is not whitelisted", to));

            if amount > rules.auto_add_threshold {
                // Over threshold requires terminal password
                max_approval = std::cmp::max(max_approval, ApprovalLevel::TerminalPassword);
            } else {
                // Under threshold: Telegram button to add and pay
                max_approval = std::cmp::max(max_approval, ApprovalLevel::TelegramButton);
            }
        }

        (max_approval, violations)
    }

    /// Add address to whitelist
    pub async fn add_to_whitelist(&self, address: impl Into<String>, label: impl Into<String>) {
        let mut rules = self.rules.write().await;
        rules.add_to_whitelist(address, label);
    }

    /// Record spending after successful transaction
    pub async fn record_spending(&self, amount: u64, to_address: &str) {
        let mut rules = self.rules.write().await;
        rules.record_spending(amount, to_address);
    }
}

impl Clone for PolicyEngine {
    fn clone(&self) -> Self {
        // This is a bit of a hack - we can't clone the RwLock contents
        // In practice, this should be done via Arc
        Self {
            rules: Arc::clone(&self.rules),
            policy_path: self.policy_path.clone(),
            auto_reload: self.auto_reload,
            last_loaded: Arc::clone(&self.last_loaded),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ApprovalLevel;
    use tempfile::TempDir;
    use std::sync::{Mutex, OnceLock};

    // Mutex to serialize tests that use STEWARD_POLICY_DIR env var
    static POLICY_DIR_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

    fn get_mutex() -> &'static Mutex<()> {
        POLICY_DIR_MUTEX.get_or_init(|| Mutex::new(()))
    }

    #[tokio::test]
    async fn test_policy_engine_new() {
        let _guard = get_mutex().lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("policy.json");

        // Set the allowed directory to the temp dir
        std::env::set_var("STEWARD_POLICY_DIR", temp_dir.path());

        let engine = PolicyEngine::new(&policy_path).unwrap();
        let rules = engine.rules().await;

        // Should have default rules since file doesn't exist
        assert_eq!(rules.version, "2.0");
        assert_eq!(rules.max_per_tx, 100_000_000); // 100 USDC in micros

        std::env::remove_var("STEWARD_POLICY_DIR");
    }

    #[tokio::test]
    async fn test_policy_update() {
        let _guard = get_mutex().lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("policy.json");

        // Set the allowed directory to the temp dir
        std::env::set_var("STEWARD_POLICY_DIR", temp_dir.path());

        let engine = PolicyEngine::new(&policy_path).unwrap();

        // Update a rule
        engine.update_rule("max_per_tx", "200000000").await.unwrap();

        let rules = engine.rules().await;
        assert_eq!(rules.max_per_tx, 200_000_000); // 200 USDC in micros

        std::env::remove_var("STEWARD_POLICY_DIR");
    }

    #[tokio::test]
    async fn test_policy_save_and_reload() {
        let _guard = get_mutex().lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("policy.json");

        // Set the allowed directory to the temp dir
        std::env::set_var("STEWARD_POLICY_DIR", temp_dir.path());

        let engine = PolicyEngine::new(&policy_path).unwrap();

        // Update and save
        engine.update_rule("max_per_tx", "500000000").await.unwrap();
        engine.save().await.unwrap();

        // Create new engine (should load from file)
        let engine2 = PolicyEngine::new(&policy_path).unwrap();
        let rules = engine2.rules().await;

        assert_eq!(rules.max_per_tx, 500_000_000); // 500 USDC in micros

        std::env::remove_var("STEWARD_POLICY_DIR");
    }

    #[tokio::test]
    async fn test_policy_validation() {
        let _guard = get_mutex().lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("policy.json");

        // Set the allowed directory to the temp dir
        std::env::set_var("STEWARD_POLICY_DIR", temp_dir.path());

        let engine = PolicyEngine::new(&policy_path).unwrap();

        // Invalid update should fail
        let result = engine.update_rule("max_per_tx", "invalid").await;
        assert!(result.is_err());

        // Valid update should succeed
        let result = engine.update_rule("max_per_tx", "200000000").await;
        assert!(result.is_ok());

        std::env::remove_var("STEWARD_POLICY_DIR");
    }

    #[tokio::test]
    async fn test_auto_approve_within_limits() {
        let _guard = get_mutex().lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("policy.json");
        std::env::set_var("STEWARD_POLICY_DIR", temp_dir.path());

        let engine = PolicyEngine::new(&policy_path).unwrap();

        engine.add_to_whitelist("0x1234", "Test").await;

        let (level, violations) = engine.evaluate_transaction("0x1234", 50_000_000).await;
        assert_eq!(level, ApprovalLevel::AutoApprove);
        assert!(violations.is_empty());

        std::env::remove_var("STEWARD_POLICY_DIR");
    }

    #[tokio::test]
    async fn test_telegram_button_for_over_per_tx() {
        let _guard = get_mutex().lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("policy.json");
        std::env::set_var("STEWARD_POLICY_DIR", temp_dir.path());

        let engine = PolicyEngine::new(&policy_path).unwrap();

        engine.add_to_whitelist("0x1234", "Test").await;

        let (level, violations) = engine.evaluate_transaction("0x1234", 150_000_000).await;
        assert_eq!(level, ApprovalLevel::TelegramButton);
        assert!(!violations.is_empty());

        std::env::remove_var("STEWARD_POLICY_DIR");
    }

    #[tokio::test]
    async fn test_terminal_password_for_new_address_over_threshold() {
        let _guard = get_mutex().lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("policy.json");
        std::env::set_var("STEWARD_POLICY_DIR", temp_dir.path());

        let engine = PolicyEngine::new(&policy_path).unwrap();

        // 100_000_000 > auto_add_threshold (50_000_000)
        let (level, violations) = engine.evaluate_transaction("0x5678", 100_000_000).await;
        assert_eq!(level, ApprovalLevel::TerminalPassword);
        assert!(violations.iter().any(|v| v.contains("not whitelisted")));

        std::env::remove_var("STEWARD_POLICY_DIR");
    }

    #[tokio::test]
    async fn test_telegram_button_for_new_address_under_threshold() {
        let _guard = get_mutex().lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("policy.json");
        std::env::set_var("STEWARD_POLICY_DIR", temp_dir.path());

        let engine = PolicyEngine::new(&policy_path).unwrap();

        // 25_000_000 <= auto_add_threshold (50_000_000)
        let (level, violations) = engine.evaluate_transaction("0x5678", 25_000_000).await;
        assert_eq!(level, ApprovalLevel::TelegramButton);
        assert!(violations.iter().any(|v| v.contains("not whitelisted")));

        std::env::remove_var("STEWARD_POLICY_DIR");
    }

    #[tokio::test]
    async fn test_higher_security_wins() {
        let _guard = get_mutex().lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("policy.json");
        std::env::set_var("STEWARD_POLICY_DIR", temp_dir.path());

        let engine = PolicyEngine::new(&policy_path).unwrap();

        // New address over threshold + over per_tx limit
        // TerminalPassword (higher) should win over TelegramButton
        let (level, _) = engine.evaluate_transaction("0x9999", 150_000_000).await;
        assert_eq!(level, ApprovalLevel::TerminalPassword);

        std::env::remove_var("STEWARD_POLICY_DIR");
    }
}
