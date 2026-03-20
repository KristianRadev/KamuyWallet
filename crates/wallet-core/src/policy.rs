//! Policy engine for transaction validation
//!
//! This module implements the policy system for Kamuy Wallet.

use crate::transaction::{Transaction, TransactionStatus};
use chrono::{Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Policy decision result
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    /// Transaction is approved
    Approve,
    /// Transaction is rejected
    Reject { reason: String },
    /// Requires additional approval (e.g., user confirmation)
    RequireAdditionalApproval { reason: String },
}

impl PolicyDecision {
    /// Check if this decision allows the transaction to proceed
    pub fn is_approved(&self) -> bool {
        matches!(self, PolicyDecision::Approve)
    }

    /// Get the reason for non-approval
    pub fn reason(&self) -> Option<&str> {
        match self {
            PolicyDecision::Reject { reason } => Some(reason),
            PolicyDecision::RequireAdditionalApproval { reason } => Some(reason),
            PolicyDecision::Approve => None,
        }
    }
}

/// Individual policy rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyRule {
    /// Maximum amount per transaction
    MaxPerTx { amount: String, token: String },
    /// Maximum daily spending
    MaxDaily { amount: String, token: String },
    /// Require approval above threshold
    RequireApprovalAbove { amount: String, token: String },
    /// Allowed tokens
    AllowedTokens { tokens: Vec<String> },
    /// Whitelist of addresses
    Whitelist { addresses: Vec<String> },
    /// Blocklist of addresses
    Blocklist { addresses: Vec<String> },
    /// Allowed time windows
    TimeWindows { windows: Vec<TimeWindow> },
    /// Rate limit
    RateLimit { max_tx: u32, period: TimePeriod },
}

impl PolicyRule {
    /// Evaluate this rule against a transaction
    fn evaluate(&self, tx: &Transaction, history: &TransactionHistory) -> PolicyDecision {
        match self {
            PolicyRule::MaxPerTx { amount, token } => {
                if tx.token.to_lowercase() == token.to_lowercase() {
                    if let Ok(tx_amount) = parse_amount(&tx.amount) {
                        if let Ok(limit) = parse_amount(amount) {
                            if tx_amount > limit {
                                return PolicyDecision::Reject {
                                    reason: format!(
                                        "Amount {} exceeds max per tx limit {}",
                                        tx.amount, amount
                                    ),
                                };
                            }
                        }
                    }
                }
            }
            PolicyRule::MaxDaily { amount, token } => {
                if tx.token.to_lowercase() == token.to_lowercase() {
                    if let Ok(limit) = parse_amount(amount) {
                        let daily_total = history.get_daily_total(tx.token.clone());
                        if daily_total > limit {
                            return PolicyDecision::Reject {
                                reason: format!(
                                    "Daily spending {} would exceed limit {}",
                                    daily_total, amount
                                ),
                            };
                        }
                    }
                }
            }
            PolicyRule::RequireApprovalAbove { amount, token } => {
                if tx.token.to_lowercase() == token.to_lowercase() {
                    if let Ok(tx_amount) = parse_amount(&tx.amount) {
                        if let Ok(threshold) = parse_amount(amount) {
                            if tx_amount > threshold {
                                return PolicyDecision::RequireAdditionalApproval {
                                    reason: format!(
                                        "Amount {} requires approval (threshold: {})",
                                        tx.amount, amount
                                    ),
                                };
                            }
                        }
                    }
                }
            }
            PolicyRule::AllowedTokens { tokens } => {
                let token_lower = tx.token.to_lowercase();
                let allowed: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
                if !allowed.contains(&token_lower) {
                    return PolicyDecision::Reject {
                        reason: format!("Token {} not in allowed list", tx.token),
                    };
                }
            }
            PolicyRule::Whitelist { addresses } => {
                let whitelist: HashSet<String> =
                    addresses.iter().map(|a| a.to_lowercase()).collect();
                let dest_lower = tx.to.to_lowercase();
                // If whitelist is not empty and address is not in whitelist, reject
                if !whitelist.is_empty() && !whitelist.contains(&dest_lower) {
                    return PolicyDecision::Reject {
                        reason: format!("Address {} not in whitelist", tx.to),
                    };
                }
            }
            PolicyRule::Blocklist { addresses } => {
                let blocklist: HashSet<String> =
                    addresses.iter().map(|a| a.to_lowercase()).collect();
                let dest_lower = tx.to.to_lowercase();
                if blocklist.contains(&dest_lower) {
                    return PolicyDecision::Reject {
                        reason: format!("Address {} is blocklisted", tx.to),
                    };
                }
            }
            PolicyRule::TimeWindows { windows } => {
                let now = Utc::now();
                let current_day = now.weekday().num_days_from_sunday() as u8;
                let current_time = format!("{:02}:{:02}", now.hour(), now.minute());

                let mut allowed = false;
                for window in windows {
                    if window.days.contains(&current_day) {
                        if current_time >= window.start && current_time <= window.end {
                            allowed = true;
                            break;
                        }
                    }
                }

                if !allowed {
                    return PolicyDecision::Reject {
                        reason: "Transaction outside allowed time window".to_string(),
                    };
                }
            }
            PolicyRule::RateLimit { max_tx, period } => {
                let tx_count = history.get_period_count(period);
                if tx_count >= *max_tx as usize {
                    return PolicyDecision::Reject {
                        reason: format!(
                            "Rate limit exceeded: {} tx per {:?}",
                            max_tx, period
                        ),
                    };
                }
            }
        }

        // Rule passed, continue to next rule
        PolicyDecision::Approve
    }
}

/// Time window for policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Days of week (0 = Sunday, 6 = Saturday)
    pub days: Vec<u8>,
    /// Start time (HH:MM)
    pub start: String,
    /// End time (HH:MM)
    pub end: String,
}

/// Time period for rate limiting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimePeriod {
    Hour,
    Day,
    Week,
    Month,
}

impl TimePeriod {
    /// Get the time window in seconds
    fn window_seconds(&self) -> i64 {
        match self {
            TimePeriod::Hour => 3600,
            TimePeriod::Day => 86400,
            TimePeriod::Week => 604800,
            TimePeriod::Month => 2592000, // 30 days
        }
    }
}

/// Transaction history for policy evaluation
#[derive(Debug, Clone, Default)]
pub struct TransactionHistory {
    transactions: Vec<Transaction>,
}

impl TransactionHistory {
    /// Add a transaction to history
    pub fn add(&mut self, tx: Transaction) {
        self.transactions.push(tx);
    }

    /// Get total amount spent today for a token
    pub fn get_daily_total(&self, token: String) -> f64 {
        let today = Utc::now().date_naive();
        let token_lower = token.to_lowercase();

        self.transactions
            .iter()
            .filter(|tx| {
                let tx_date = chrono::DateTime::from_timestamp(tx.created_at, 0)
                    .map(|dt| dt.date_naive())
                    .unwrap_or_default();
                tx.token.to_lowercase() == token_lower
                    && tx_date == today
                    && tx.status == TransactionStatus::Confirmed
            })
            .filter_map(|tx| parse_amount(&tx.amount).ok())
            .sum()
    }

    /// Get transaction count in a period
    pub fn get_period_count(&self, period: &TimePeriod) -> usize {
        let now = Utc::now().timestamp();
        let window = period.window_seconds();
        let cutoff = now - window;

        self.transactions
            .iter()
            .filter(|tx| tx.created_at >= cutoff && tx.status == TransactionStatus::Confirmed)
            .count()
    }

    /// Get pending transaction count
    pub fn pending_count(&self) -> usize {
        self.transactions
            .iter()
            .filter(|tx| tx.status == TransactionStatus::Pending)
            .count()
    }
}

/// Complete policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    /// Policy version
    pub version: String,
    /// Policy rules
    pub rules: Vec<PolicyRule>,
    /// Default action if no rule matches
    pub default_action: PolicyDecision,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            rules: vec![],
            default_action: PolicyDecision::RequireAdditionalApproval {
                reason: "No matching policy".to_string(),
            },
        }
    }
}

/// Policy engine for evaluating transactions
#[derive(Debug, Clone)]
pub struct PolicyEngine {
    config: PolicyConfig,
    history: Arc<RwLock<TransactionHistory>>,
}

impl PolicyEngine {
    /// Create a new policy engine
    pub fn new(config: PolicyConfig) -> Self {
        Self {
            config,
            history: Arc::new(RwLock::new(TransactionHistory::default())),
        }
    }

    /// Create with custom history (for testing)
    pub fn with_history(config: PolicyConfig, history: TransactionHistory) -> Self {
        Self {
            config,
            history: Arc::new(RwLock::new(history)),
        }
    }

    /// Evaluate a transaction against the policy
    pub async fn evaluate(&self, tx: &Transaction) -> PolicyDecision {
        // If no rules, use default action
        if self.config.rules.is_empty() {
            return self.config.default_action.clone();
        }

        let history = self.history.read().await;

        // Evaluate each rule in order
        for rule in &self.config.rules {
            let decision = rule.evaluate(tx, &history);

            // If rule rejected or requires approval, return immediately
            if !decision.is_approved() {
                return decision;
            }
        }

        // All rules passed
        PolicyDecision::Approve
    }

    /// Evaluate synchronously (for non-async contexts)
    pub fn evaluate_sync(&self, tx: &Transaction) -> PolicyDecision {
        // If no rules, use default action
        if self.config.rules.is_empty() {
            return self.config.default_action.clone();
        }

        let history = self.history.blocking_read();

        // Evaluate each rule in order
        for rule in &self.config.rules {
            let decision = rule.evaluate(tx, &history);

            // If rule rejected or requires approval, return immediately
            if !decision.is_approved() {
                return decision;
            }
        }

        // All rules passed
        PolicyDecision::Approve
    }

    /// Record a transaction in history
    pub async fn record_transaction(&self, tx: Transaction) {
        let mut history = self.history.write().await;
        history.add(tx);
    }

    /// Record a transaction synchronously
    pub fn record_transaction_sync(&self, tx: Transaction) {
        let mut history = self.history.blocking_write();
        history.add(tx);
    }

    /// Get the policy configuration
    pub fn config(&self) -> &PolicyConfig {
        &self.config
    }

    /// Update the policy configuration
    pub fn update_config(&mut self, config: PolicyConfig) {
        self.config = config;
    }

    /// Get transaction history
    pub async fn history(&self) -> Arc<RwLock<TransactionHistory>> {
        Arc::clone(&self.history)
    }
}

impl Default for PolicyEngine {
    fn default() -> Self {
        Self::new(PolicyConfig::default())
    }
}

/// Parse amount string to f64 (handles various formats)
fn parse_amount(amount: &str) -> Result<f64, ()> {
    // Remove whitespace
    let amount = amount.trim();

    // Try parsing directly
    amount
        .replace(',', "")
        .parse::<f64>()
        .map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_tx(amount: &str, token: &str, to: &str) -> Transaction {
        Transaction::new(
            "test-id",
            "req-123",
            to,
            amount,
            token,
            8453,
        )
    }

    #[test]
    fn test_max_per_tx_rule_reject() {
        let rule = PolicyRule::MaxPerTx {
            amount: "100".to_string(),
            token: "ETH".to_string(),
        };
        let tx = create_test_tx("150", "ETH", "0x123");
        let history = TransactionHistory::default();

        let result = rule.evaluate(&tx, &history);
        assert!(matches!(result, PolicyDecision::Reject { .. }));
    }

    #[test]
    fn test_max_per_tx_rule_approve() {
        let rule = PolicyRule::MaxPerTx {
            amount: "100".to_string(),
            token: "ETH".to_string(),
        };
        let tx = create_test_tx("50", "ETH", "0x123");
        let history = TransactionHistory::default();

        let result = rule.evaluate(&tx, &history);
        assert!(result.is_approved());
    }

    #[test]
    fn test_blocklist_rule() {
        let rule = PolicyRule::Blocklist {
            addresses: vec!["0xbad".to_string(), "0xevil".to_string()],
        };
        let tx = create_test_tx("10", "ETH", "0xbad");
        let history = TransactionHistory::default();

        let result = rule.evaluate(&tx, &history);
        assert!(matches!(result, PolicyDecision::Reject { .. }));
    }

    #[test]
    fn test_whitelist_rule() {
        let rule = PolicyRule::Whitelist {
            addresses: vec!["0x123".to_string(), "0x456".to_string()],
        };
        let tx = create_test_tx("10", "ETH", "0x789");
        let history = TransactionHistory::default();

        let result = rule.evaluate(&tx, &history);
        assert!(matches!(result, PolicyDecision::Reject { .. }));
    }

    #[test]
    fn test_whitelist_empty_allows() {
        let rule = PolicyRule::Whitelist {
            addresses: vec![],
        };
        let tx = create_test_tx("10", "ETH", "0x789");
        let history = TransactionHistory::default();

        let result = rule.evaluate(&tx, &history);
        assert!(result.is_approved());
    }

    #[test]
    fn test_allowed_tokens_reject() {
        let rule = PolicyRule::AllowedTokens {
            tokens: vec!["ETH".to_string(), "USDC".to_string()],
        };
        let tx = create_test_tx("10", "BTC", "0x123");
        let history = TransactionHistory::default();

        let result = rule.evaluate(&tx, &history);
        assert!(matches!(result, PolicyDecision::Reject { .. }));
    }

    #[test]
    fn test_rate_limit() {
        let rule = PolicyRule::RateLimit {
            max_tx: 2,
            period: TimePeriod::Hour,
        };

        let mut history = TransactionHistory::default();
        // Add 2 confirmed transactions
        for i in 0..2 {
            let mut tx = create_test_tx("10", "ETH", "0x123");
            tx.status = TransactionStatus::Confirmed;
            tx.created_at = chrono::Utc::now().timestamp();
            history.add(tx);
        }

        let tx = create_test_tx("10", "ETH", "0x456");
        let result = rule.evaluate(&tx, &history);
        assert!(matches!(result, PolicyDecision::Reject { .. }));
    }

    #[test]
    fn test_policy_engine_with_rules() {
        let rules = vec![
            PolicyRule::Blocklist {
                addresses: vec!["0xbad".to_string()],
            },
            PolicyRule::MaxPerTx {
                amount: "1000".to_string(),
                token: "ETH".to_string(),
            },
        ];

        let config = PolicyConfig {
            version: "1.0".to_string(),
            rules,
            default_action: PolicyDecision::RequireAdditionalApproval {
                reason: "Default".to_string(),
            },
        };

        let engine = PolicyEngine::new(config);

        // Test blocked address
        let tx = create_test_tx("10", "ETH", "0xbad");
        let result = engine.evaluate_sync(&tx);
        assert!(matches!(result, PolicyDecision::Reject { .. }));

        // Test over limit
        let tx = create_test_tx("2000", "ETH", "0x123");
        let result = engine.evaluate_sync(&tx);
        assert!(matches!(result, PolicyDecision::Reject { .. }));

        // Test valid
        let tx = create_test_tx("100", "ETH", "0x456");
        let result = engine.evaluate_sync(&tx);
        assert!(result.is_approved());
    }

    #[test]
    fn test_empty_rules_uses_default() {
        let config = PolicyConfig {
            version: "1.0".to_string(),
            rules: vec![],
            default_action: PolicyDecision::RequireAdditionalApproval {
                reason: "No rules configured".to_string(),
            },
        };

        let engine = PolicyEngine::new(config);
        let tx = create_test_tx("10", "ETH", "0x123");

        let result = engine.evaluate_sync(&tx);
        assert!(matches!(
            result,
            PolicyDecision::RequireAdditionalApproval { .. }
        ));
    }
}
