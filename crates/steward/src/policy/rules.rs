//! # Policy Rules Definition (v2.0)
//!
//! Defines the structure and validation of policy rules for Kamuy Wallet v2.0.

use crate::error::{StewardError, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use super::whitelist::Whitelist;

/// Spending tracker for daily and weekly limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendingTracker {
    /// Amount spent today (in USDC micros)
    pub daily_spent: u64,
    /// Amount spent this week (in USDC micros)
    pub weekly_spent: u64,
    /// When daily counter was last reset
    pub last_reset_daily: DateTime<Utc>,
    /// When weekly counter was last reset
    pub last_reset_weekly: DateTime<Utc>,
}

impl Default for SpendingTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SpendingTracker {
    /// Create new spending tracker
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            daily_spent: 0,
            weekly_spent: 0,
            last_reset_daily: now,
            last_reset_weekly: now,
        }
    }

    /// Check and reset counters if needed
    pub fn check_and_reset(&mut self) {
        let now = Utc::now();

        // Check daily reset (more than 24 hours since last reset)
        if now > self.last_reset_daily + Duration::hours(24) {
            self.daily_spent = 0;
            self.last_reset_daily = now;
        }

        // Check weekly reset (more than 7 days since last reset)
        if now > self.last_reset_weekly + Duration::days(7) {
            self.weekly_spent = 0;
            self.last_reset_weekly = now;
        }
    }

    /// Add spending amount
    pub fn add_spending(&mut self, amount: u64) {
        self.check_and_reset();
        self.daily_spent = self.daily_spent.saturating_add(amount);
        self.weekly_spent = self.weekly_spent.saturating_add(amount);
    }

    /// Check if adding amount would exceed daily limit
    pub fn would_exceed_daily(&self, amount: u64, limit: u64) -> bool {
        self.daily_spent.saturating_add(amount) > limit
    }

    /// Check if adding amount would exceed weekly limit
    pub fn would_exceed_weekly(&self, amount: u64, limit: u64) -> bool {
        self.weekly_spent.saturating_add(amount) > limit
    }
}

/// Policy rules configuration (v2.0)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRules {
    /// Policy version
    pub version: String,
    /// Maximum amount per transaction (in USDC micros)
    pub max_per_tx: u64,
    /// Maximum daily spending (in USDC micros)
    pub max_daily: u64,
    /// Maximum weekly spending (in USDC micros)
    pub max_weekly: u64,
    /// Auto-add threshold for new addresses (in USDC micros)
    /// Addresses paid under this amount can be auto-added to whitelist
    /// with Telegram button approval. Over this amount requires terminal password.
    pub auto_add_threshold: u64,
    /// Token is always USDC for v2.0 (gasless via Pimlico)
    #[serde(default = "default_token")]
    pub token: String,
    /// Whether transactions are gasless (sponsored by paymaster)
    #[serde(default = "default_true")]
    pub gasless: bool,
    /// Whitelisted addresses with metadata
    #[serde(default)]
    pub whitelist: Whitelist,
    /// Spending tracker for daily/weekly limits
    #[serde(default)]
    pub spending_tracker: SpendingTracker,
}

fn default_token() -> String {
    "USDC".to_string()
}

fn default_true() -> bool {
    true
}

impl PolicyRules {
    /// Create default policy rules for v2.0
    pub fn default() -> Self {
        Self {
            version: "2.0".to_string(),
            max_per_tx: 100_000_000,           // 100 USDC (in micros)
            max_daily: 500_000_000,            // 500 USDC
            max_weekly: 2_000_000_000,         // 2000 USDC
            auto_add_threshold: 50_000_000,    // 50 USDC
            token: "USDC".to_string(),
            gasless: true,
            whitelist: Whitelist::new(),
            spending_tracker: SpendingTracker::new(),
        }
    }

    /// Load from JSON file
    pub fn from_file(path: &std::path::Path) -> Result<Self> {
        let canonical_path = path.canonicalize()
            .map_err(|e| StewardError::Config(format!("Invalid policy file path: {}", e)))?;

        let current_dir = std::env::current_dir()
            .map_err(|e| StewardError::Config(format!("Cannot determine working directory: {}", e)))?;
        let allowed_prefix = std::env::var("STEWARD_POLICY_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or(current_dir);

        let canonical_allowed = allowed_prefix.canonicalize()
            .map_err(|e| StewardError::Config(format!("Invalid allowed directory: {}", e)))?;

        if !canonical_path.starts_with(&canonical_allowed) {
            return Err(StewardError::Config(
                "Policy file path is outside allowed directory".to_string()
            ));
        }

        let content = std::fs::read_to_string(&canonical_path)
            .map_err(|e| StewardError::Config(format!("Failed to read policy file: {}", e)))?;

        Self::from_json(&content)
    }

    /// Load from JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        let rules: PolicyRules = serde_json::from_str(json)
            .map_err(|e| StewardError::Config(format!("Invalid policy JSON: {}", e)))?;
        rules.validate()?;
        Ok(rules)
    }

    /// Save to JSON file
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| StewardError::Serialization(e.to_string()))?;
        std::fs::write(path, json)
            .map_err(|e| StewardError::Storage(format!("Failed to write policy file: {}", e)))?;
        Ok(())
    }

    /// Validate policy rules
    pub fn validate(&self) -> Result<()> {
        if self.max_per_tx > self.max_daily {
            return Err(StewardError::Validation(
                "max_per_tx cannot exceed max_daily".to_string()
            ));
        }
        if self.max_daily > self.max_weekly {
            return Err(StewardError::Validation(
                "max_daily cannot exceed max_weekly".to_string()
            ));
        }
        if self.auto_add_threshold > self.max_per_tx {
            return Err(StewardError::Validation(
                "auto_add_threshold cannot exceed max_per_tx".to_string()
            ));
        }
        if self.token != "USDC" {
            return Err(StewardError::Validation(
                "v2.0 only supports USDC token".to_string()
            ));
        }
        Ok(())
    }

    /// Update a specific rule
    pub fn update(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "max_per_tx" => {
                self.max_per_tx = value.parse()
                    .map_err(|_| StewardError::Validation("Invalid max_per_tx value".to_string()))?;
            }
            "max_daily" => {
                self.max_daily = value.parse()
                    .map_err(|_| StewardError::Validation("Invalid max_daily value".to_string()))?;
            }
            "max_weekly" => {
                self.max_weekly = value.parse()
                    .map_err(|_| StewardError::Validation("Invalid max_weekly value".to_string()))?;
            }
            "auto_add_threshold" => {
                self.auto_add_threshold = value.parse()
                    .map_err(|_| StewardError::Validation("Invalid auto_add_threshold value".to_string()))?;
            }
            _ => return Err(StewardError::Validation(format!("Unknown policy key: {}", key))),
        }
        self.validate()?;
        Ok(())
    }

    /// Add address to whitelist
    pub fn add_to_whitelist(&mut self, address: impl Into<String>, label: impl Into<String>) {
        self.whitelist.add(address, label);
    }

    /// Check if address is whitelisted
    pub fn is_whitelisted(&self, address: &str) -> bool {
        self.whitelist.contains(address)
    }

    /// Record spending (updates tracker and whitelist totals)
    pub fn record_spending(&mut self, amount: u64, to_address: &str) {
        self.spending_tracker.add_spending(amount);
        self.whitelist.update_total_sent(to_address, amount);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy_v2() {
        let policy = PolicyRules::default();
        assert_eq!(policy.version, "2.0");
        assert_eq!(policy.max_per_tx, 100_000_000);
        assert_eq!(policy.max_daily, 500_000_000);
        assert_eq!(policy.auto_add_threshold, 50_000_000);
        assert_eq!(policy.token, "USDC");
        assert!(policy.gasless);
        assert!(policy.whitelist.is_empty());
        assert!(policy.validate().is_ok());
    }

    #[test]
    fn test_policy_validation() {
        let mut policy = PolicyRules::default();

        policy.max_per_tx = 600_000_000;
        policy.max_daily = 500_000_000;
        assert!(policy.validate().is_err());

        policy.max_per_tx = 100_000_000;
        assert!(policy.validate().is_ok());
    }

    #[test]
    fn test_whitelist_operations() {
        let mut policy = PolicyRules::default();
        assert!(!policy.is_whitelisted("0x1234"));

        policy.add_to_whitelist("0x1234", "Test Address");
        assert!(policy.is_whitelisted("0x1234"));
    }

    #[test]
    fn test_spending_recording() {
        let mut policy = PolicyRules::default();
        policy.add_to_whitelist("0x1234", "Test");

        policy.record_spending(50_000_000, "0x1234");
        assert_eq!(policy.spending_tracker.daily_spent, 50_000_000);
        assert_eq!(policy.spending_tracker.weekly_spent, 50_000_000);
        assert_eq!(policy.whitelist.get("0x1234").unwrap().total_sent, 50_000_000);
    }

    #[test]
    fn test_json_roundtrip_v2() {
        let mut policy = PolicyRules::default();
        policy.add_to_whitelist("0xabc123", "OpenAI");

        let json = serde_json::to_string(&policy).unwrap();
        let parsed = PolicyRules::from_json(&json).unwrap();

        assert_eq!(policy.version, parsed.version);
        assert!(parsed.is_whitelisted("0xabc123"));
    }

    #[test]
    fn test_spending_tracker() {
        let mut tracker = SpendingTracker::new();
        assert_eq!(tracker.daily_spent, 0);
        assert_eq!(tracker.weekly_spent, 0);

        tracker.add_spending(1000);
        assert_eq!(tracker.daily_spent, 1000);
        assert_eq!(tracker.weekly_spent, 1000);

        tracker.add_spending(500);
        assert_eq!(tracker.daily_spent, 1500);
        assert_eq!(tracker.weekly_spent, 1500);
    }

    #[test]
    fn test_spending_tracker_limits() {
        let mut tracker = SpendingTracker::new();
        tracker.add_spending(800);

        assert!(!tracker.would_exceed_daily(100, 1000));
        assert!(tracker.would_exceed_daily(300, 1000));
    }
}