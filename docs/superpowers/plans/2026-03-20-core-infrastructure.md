# Phase 1: Core Infrastructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the v2.0 policy schema with auto-add threshold, structured whitelist, spending tracker, USDC-only enforcement, and higher-security-wins logic.

**Architecture:** Replace existing `PolicyRules`, `SpendingTracker`, and `evaluate_transaction` in the Steward crate with v2.0 versions. Add new types for whitelist entries. Modify CLI to support terminal password approval.

**Tech Stack:** Rust, serde (JSON serialization), tokio (async), SQLite (storage)

**Spec Document:** `/home/santo6500/kamuy-wallet/docs/superpowers/specs/2026-03-20-simplified-ux-design.md`

---

## Breaking Changes Notice

This phase introduces breaking changes:
1. **PolicyRules schema** - Changes from String amounts to u64, Vec whitelist to HashMap
2. **SpendingTracker** - Replaced with weekly tracking support
3. **evaluate_transaction** - Moved to PolicyEngine with new signature
4. **Existing tests** - Tests in `engine.rs` that reference old String-based API will need updating

**Migration:** Existing policy files must be manually updated. No automatic migration.

---

## File Structure

### Files to Create:
- `crates/steward/src/policy/whitelist.rs` - WhitelistEntry struct and management

### Files to Modify:
- `crates/steward/src/policy/mod.rs` - Replace SpendingTracker, evaluate_transaction
- `crates/steward/src/policy/rules.rs` - Replace PolicyRules with v2.0
- `crates/steward/src/policy/engine.rs` - Add evaluate_transaction method
- `crates/steward/src/types.rs` - Add ApprovalLevel enum
- `crates/steward/src/storage/mod.rs` - Add spending tracker storage
- `crates/steward/src/api/routes.rs` - Update for v2.0

---

## Task 1: Add ApprovalLevel Enum

**Files:**
- Modify: `crates/steward/src/types.rs`

- [ ] **Step 1: Add ApprovalLevel enum**

Add after line 463 (after the closing brace of the `PolicyDecision` enum):

```rust
/// Approval level required for an operation
/// Used to implement "higher-security-wins" logic
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalLevel {
    /// Auto-approved (within all limits)
    AutoApprove,
    /// Telegram button approval required
    TelegramButton,
    /// Terminal password required (highest security)
    TerminalPassword,
}

impl std::fmt::Display for ApprovalLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApprovalLevel::AutoApprove => write!(f, "auto_approve"),
            ApprovalLevel::TelegramButton => write!(f, "telegram_button"),
            ApprovalLevel::TerminalPassword => write!(f, "terminal_password"),
        }
    }
}
```

- [ ] **Step 2: Add test for ApprovalLevel ordering**

Add to the tests module at the end of the file:

```rust
    #[test]
    fn test_approval_level_ordering() {
        // Higher security = higher value
        assert!(ApprovalLevel::TerminalPassword > ApprovalLevel::TelegramButton);
        assert!(ApprovalLevel::TelegramButton > ApprovalLevel::AutoApprove);
        assert!(ApprovalLevel::AutoApprove < ApprovalLevel::TerminalPassword);
    }
```

- [ ] **Step 3: Run tests to verify**

Run: `cargo test -p kamuy-steward types::tests`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/steward/src/types.rs
git commit -m "feat(types): add ApprovalLevel enum for security tier logic"
```

---

## Task 2: Create WhitelistEntry Struct

**Files:**
- Create: `crates/steward/src/policy/whitelist.rs`

- [ ] **Step 1: Create whitelist module**

```rust
// crates/steward/src/policy/whitelist.rs
//! Whitelist management for v2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A whitelisted address entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhitelistEntry {
    /// Human-readable label (e.g., "OpenAI", "AWS")
    pub label: String,
    /// When this address was added
    pub added_at: DateTime<Utc>,
    /// Total amount sent to this address (in USDC micros)
    pub total_sent: u64,
}

impl WhitelistEntry {
    /// Create a new whitelist entry
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            added_at: Utc::now(),
            total_sent: 0,
        }
    }
}

/// Whitelist management
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Whitelist(HashMap<String, WhitelistEntry>);

impl Whitelist {
    /// Create empty whitelist
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Check if address is whitelisted
    pub fn contains(&self, address: &str) -> bool {
        self.0.contains_key(address)
    }

    /// Get entry for an address
    pub fn get(&self, address: &str) -> Option<&WhitelistEntry> {
        self.0.get(address)
    }

    /// Get mutable entry
    pub fn get_mut(&mut self, address: &str) -> Option<&mut WhitelistEntry> {
        self.0.get_mut(address)
    }

    /// Add a new address
    pub fn add(&mut self, address: impl Into<String>, label: impl Into<String>) {
        self.0.insert(address.into(), WhitelistEntry::new(label));
    }

    /// Update total sent amount
    pub fn update_total_sent(&mut self, address: &str, amount: u64) {
        if let Some(entry) = self.0.get_mut(address) {
            entry.total_sent = entry.total_sent.saturating_add(amount);
        }
    }

    /// Get all entries
    pub fn entries(&self) -> &HashMap<String, WhitelistEntry> {
        &self.0
    }

    /// Count entries
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whitelist_add_and_contains() {
        let mut whitelist = Whitelist::new();
        assert!(!whitelist.contains("0x1234"));

        whitelist.add("0x1234", "Test Address");
        assert!(whitelist.contains("0x1234"));

        let entry = whitelist.get("0x1234").unwrap();
        assert_eq!(entry.label, "Test Address");
    }

    #[test]
    fn test_update_total_sent() {
        let mut whitelist = Whitelist::new();
        whitelist.add("0x1234", "Test");

        whitelist.update_total_sent("0x1234", 1000);
        assert_eq!(whitelist.get("0x1234").unwrap().total_sent, 1000);

        whitelist.update_total_sent("0x1234", 500);
        assert_eq!(whitelist.get("0x1234").unwrap().total_sent, 1500);
    }
}
```

- [ ] **Step 2: Run tests to verify**

Run: `cargo test -p kamuy-steward policy::whitelist`
Expected: All tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/steward/src/policy/whitelist.rs
git commit -m "feat(policy): add WhitelistEntry struct for v2.0 schema"
```

---

## Task 3: Replace PolicyRules with v2.0

**Files:**
- Modify: `crates/steward/src/policy/rules.rs`

- [ ] **Step 1: Update imports and remove TimeWindow**

Replace the top of the file with:

```rust
//! # Policy Rules Definition (v2.0)
//!
//! Defines the structure and validation of policy rules for Kamuy Wallet v2.0.

use crate::error::{StewardError, Result};
use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
            last_reset_weekly: Self::start_of_week(now),
        }
    }

    /// Get start of the week (Monday 00:00 UTC)
    fn start_of_week(dt: DateTime<Utc>) -> DateTime<Utc> {
        let weekday = dt.weekday().num_days_from_monday() as i64;
        dt - Duration::days(weekday)
            - Duration::hours(dt.hour() as i64)
            - Duration::minutes(dt.minute() as i64)
            - Duration::seconds(dt.second() as i64)
    }

    /// Check and reset counters if needed
    pub fn check_and_reset(&mut self) {
        let now = Utc::now();

        // Check daily reset (more than 24 hours)
        if now > self.last_reset_daily + Duration::hours(24) {
            self.daily_spent = 0;
            self.last_reset_daily = now;
        }

        // Check weekly reset (if we're in a new week)
        let current_week_start = Self::start_of_week(now);
        if current_week_start > self.last_reset_weekly {
            self.weekly_spent = 0;
            self.last_reset_weekly = current_week_start;
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
```

- [ ] **Step 2: Replace PolicyRules struct**

Replace the existing `PolicyRules` struct with:

```rust
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
```

- [ ] **Step 3: Update tests**

Replace the tests module with:

```rust
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
```

- [ ] **Step 4: Run tests to verify**

Run: `cargo test -p kamuy-steward policy::rules`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/steward/src/policy/rules.rs
git commit -m "feat(policy): replace PolicyRules with v2.0 schema"
```

---

## Task 4: Replace SpendingTracker and evaluate_transaction in mod.rs

**Files:**
- Modify: `crates/steward/src/policy/mod.rs`

- [ ] **Step 1: Update module exports**

Replace the beginning of `mod.rs` with:

```rust
//! # Policy Engine Module (v2.0)
//!
//! Validates transactions against user-defined security policies.
//!
//! ## v2.0 Policy Rules
//!
//! - Spending limits (per tx, daily, weekly)
//! - Whitelist with labels for destinations
//! - Auto-add threshold for new addresses
//! - USDC only (gasless via Pimlico)

pub mod engine;
pub mod rules;
pub mod schema;
pub mod whitelist;

pub use engine::PolicyEngine;
pub use rules::{PolicyRules, SpendingTracker};
pub use whitelist::{Whitelist, WhitelistEntry};
#[allow(unused_imports)]
pub use schema::{PolicySchema, PolicyValidationResult};
```

- [ ] **Step 2: Remove old SpendingTracker and evaluate_transaction**

Delete from line 29 (after the imports `use crate::error...` through `use std::collections...`) to line 445 (end of old tests).

This removes:
- The old `evaluate_transaction` function (lines 29-241)
- Helper functions `parse_amount`, `format_amount`, `matches_pattern`, `is_within_time_window`, `matches_day` (lines 243-325)
- The old `SpendingTracker` struct (lines 327-390)
- Old tests (lines 392-445)

Keep only the module documentation and exports from Step 1.

- [ ] **Step 3: Update tests to match new API**

If there are remaining tests, update them to use the new API. Remove tests that reference the old `evaluate_transaction` or old `SpendingTracker`.

**IMPORTANT:** The existing tests in `engine.rs` reference the old String-based API:
```rust
// Old test (will fail after Task 3):
assert_eq!(rules.max_per_tx, "100.00");  // max_per_tx is now u64, not String
```

These tests must be removed or updated in Task 5 when we add new tests.

- [ ] **Step 4: Run tests to verify**

Run: `cargo test -p kamuy-steward policy`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/steward/src/policy/mod.rs
git commit -m "refactor(policy): remove old SpendingTracker and evaluate_transaction for v2.0"
```

---

## Task 5: Implement Higher-Security-Wins Logic in PolicyEngine

**Files:**
- Modify: `crates/steward/src/policy/engine.rs`

- [ ] **Step 1: Add import and evaluate_transaction method**

First, add this import at the top of the file with the existing imports:

```rust
use crate::types::ApprovalLevel;
```

Then add to `PolicyEngine` impl:

impl PolicyEngine {
    // ... existing methods ...

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
```

- [ ] **Step 2: Add tests**

First, remove or comment out the existing tests in `engine.rs` that reference the old String-based API (e.g., `assert_eq!(rules.max_per_tx, "100.00")`).

Then add to the tests module (make sure to add the import at the top of the tests module):

```rust
use crate::types::ApprovalLevel;

    #[tokio::test]
    async fn test_auto_approve_within_limits() {
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("policy.json");
        let engine = PolicyEngine::new(&policy_path).unwrap();

        engine.add_to_whitelist("0x1234", "Test").await;

        let (level, violations) = engine.evaluate_transaction("0x1234", 50_000_000).await;
        assert_eq!(level, ApprovalLevel::AutoApprove);
        assert!(violations.is_empty());
    }

    #[tokio::test]
    async fn test_telegram_button_for_over_per_tx() {
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("policy.json");
        let engine = PolicyEngine::new(&policy_path).unwrap();

        engine.add_to_whitelist("0x1234", "Test").await;

        let (level, violations) = engine.evaluate_transaction("0x1234", 150_000_000).await;
        assert_eq!(level, ApprovalLevel::TelegramButton);
        assert!(!violations.is_empty());
    }

    #[tokio::test]
    async fn test_terminal_password_for_new_address_over_threshold() {
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("policy.json");
        let engine = PolicyEngine::new(&policy_path).unwrap();

        let (level, violations) = engine.evaluate_transaction("0x5678", 100_000_000).await;
        assert_eq!(level, ApprovalLevel::TerminalPassword);
        assert!(violations.iter().any(|v| v.contains("not whitelisted")));
    }

    #[tokio::test]
    async fn test_telegram_button_for_new_address_under_threshold() {
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("policy.json");
        let engine = PolicyEngine::new(&policy_path).unwrap();

        let (level, violations) = engine.evaluate_transaction("0x5678", 25_000_000).await;
        assert_eq!(level, ApprovalLevel::TelegramButton);
        assert!(violations.iter().any(|v| v.contains("not whitelisted")));
    }

    #[tokio::test]
    async fn test_higher_security_wins() {
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("policy.json");
        let engine = PolicyEngine::new(&policy_path).unwrap();

        // New address over threshold + over per_tx limit
        // TerminalPassword (higher) should win over TelegramButton
        let (level, _) = engine.evaluate_transaction("0x9999", 150_000_000).await;
        assert_eq!(level, ApprovalLevel::TerminalPassword);
    }
```

- [ ] **Step 3: Run tests to verify**

Run: `cargo test -p kamuy-steward policy::engine`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/steward/src/policy/engine.rs
git commit -m "feat(policy): implement higher-security-wins evaluation logic"
```

---

## Task 6: Update API Routes for v2.0

**Files:**
- Modify: `crates/steward/src/api/routes.rs`

- [ ] **Step 1: Update validate_transaction_fields for USDC-only**

Find the `validate_transaction_fields` function and replace it with:

```rust
/// Validate transaction request fields (v2.0: USDC only)
fn validate_transaction_fields(request: &SubmitTransactionRequest) -> Result<(), String> {
    if request.value.is_empty() {
        return Err("Amount required".to_string());
    }

    match request.value.parse::<u128>() {
        Ok(_) => {}
        Err(_) => {
            return Err("Invalid amount format: must be integer in micros".to_string());
        }
    }

    if !is_valid_ethereum_address(&request.to) {
        return Err("Invalid Ethereum address format".to_string());
    }

    // v2.0: Only USDC allowed
    if request.token.to_uppercase() != "USDC" {
        return Err("Only USDC is supported in v2.0".to_string());
    }

    if request.chain_id == 0 {
        return Err("Invalid chain ID".to_string());
    }

    let known_chains = [1u64, 8453, 137, 42161, 10, 11155111, 84532];
    if !known_chains.contains(&request.chain_id) {
        return Err("Unsupported chain ID".to_string());
    }

    Ok(())
}
```

- [ ] **Step 2: Update get_policy endpoint**

Find the `get_policy` function and replace with:

```rust
/// Get current policy (v2.0 format)
pub async fn get_policy(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();

    let rules = state.policy_engine.read().await.rules().await;

    let policy_response = serde_json::json!({
        "version": rules.version,
        "max_per_tx": rules.max_per_tx,
        "max_daily": rules.max_daily,
        "max_weekly": rules.max_weekly,
        "auto_add_threshold": rules.auto_add_threshold,
        "token": rules.token,
        "gasless": rules.gasless,
        "whitelist": rules.whitelist.entries(),
        "spending_tracker": {
            "daily_spent": rules.spending_tracker.daily_spent,
            "weekly_spent": rules.spending_tracker.weekly_spent,
            "last_reset_daily": rules.spending_tracker.last_reset_daily,
            "last_reset_weekly": rules.spending_tracker.last_reset_weekly,
        }
    });

    success(policy_response, request_id).into_response()
}
```

- [ ] **Step 3: Update valid fields in request_policy_change**

Find the `valid_fields` array and update:

```rust
    let valid_fields = [
        "max_per_tx", "max_daily", "max_weekly",
        "auto_add_threshold",
    ];
```

- [ ] **Step 4: Run tests to verify**

Run: `cargo test -p kamuy-steward api::routes`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/steward/src/api/routes.rs
git commit -m "feat(api): update routes for v2.0 USDC-only policy"
```

---

## Task 7: Add Storage Methods for Spending Tracker

**Files:**
- Modify: `crates/steward/src/storage/mod.rs`

- [ ] **Step 1: Add spending_tracker table to init()**

Add to the `init()` method after other table creations:

```rust
        // Create spending_tracker table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS spending_tracker (
                id INTEGER PRIMARY KEY,
                daily_spent TEXT NOT NULL,
                weekly_spent TEXT NOT NULL,
                last_reset_daily TEXT NOT NULL,
                last_reset_weekly TEXT NOT NULL
            )
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to create spending_tracker table: {}", e)))?;
```

- [ ] **Step 2: Add load/save methods**

Add to `StewardStorage` impl:

```rust
    /// Save spending tracker state
    pub async fn save_spending_tracker(&self, tracker: &crate::policy::rules::SpendingTracker) -> Result<()> {
        let daily_spent = tracker.daily_spent.to_string();
        let weekly_spent = tracker.weekly_spent.to_string();
        let last_reset_daily = tracker.last_reset_daily.to_rfc3339();
        let last_reset_weekly = tracker.last_reset_weekly.to_rfc3339();

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO spending_tracker (id, daily_spent, weekly_spent, last_reset_daily, last_reset_weekly)
            VALUES (1, ?, ?, ?, ?)
            "#
        )
        .bind(daily_spent)
        .bind(weekly_spent)
        .bind(last_reset_daily)
        .bind(last_reset_weekly)
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to save spending tracker: {}", e)))?;

        Ok(())
    }

    /// Load spending tracker state
    pub async fn load_spending_tracker(&self) -> Result<crate::policy::rules::SpendingTracker> {
        use crate::policy::rules::SpendingTracker;

        let row = sqlx::query(
            r#"
            SELECT daily_spent, weekly_spent, last_reset_daily, last_reset_weekly
            FROM spending_tracker WHERE id = 1
            "#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to load spending tracker: {}", e)))?;

        match row {
            Some(row) => {
                let daily_spent: String = row.get("daily_spent");
                let weekly_spent: String = row.get("weekly_spent");
                let last_reset_daily: String = row.get("last_reset_daily");
                let last_reset_weekly: String = row.get("last_reset_weekly");

                Ok(SpendingTracker {
                    daily_spent: daily_spent.parse().unwrap_or(0),
                    weekly_spent: weekly_spent.parse().unwrap_or(0),
                    last_reset_daily: DateTime::parse_from_rfc3339(&last_reset_daily)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    last_reset_weekly: DateTime::parse_from_rfc3339(&last_reset_weekly)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            }
            None => Ok(SpendingTracker::new()),
        }
    }
```

- [ ] **Step 3: Add test**

Add to the existing tests module (the `create_test_storage` helper already exists at the end of the file):

```rust
    #[tokio::test]
    async fn test_spending_tracker_storage() {
        let storage = create_test_storage().await;

        let mut tracker = crate::policy::rules::SpendingTracker::new();
        tracker.add_spending(100_000_000);
        tracker.add_spending(50_000_000);

        storage.save_spending_tracker(&tracker).await.unwrap();

        let loaded = storage.load_spending_tracker().await.unwrap();
        assert_eq!(loaded.daily_spent, 150_000_000);
        assert_eq!(loaded.weekly_spent, 150_000_000);
    }
```

- [ ] **Step 4: Run tests to verify**

Run: `cargo test -p kamuy-steward storage`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/steward/src/storage/mod.rs
git commit -m "feat(storage): add spending tracker persistence methods"
```

---

## Task 8: Run Full Test Suite and Build

- [ ] **Step 1: Run all steward tests**

Run: `cargo test -p kamuy-steward`
Expected: All tests pass

- [ ] **Step 2: Run clippy for linting**

Run: `cargo clippy -p kamuy-steward -- -D warnings`
Expected: No errors (fix any warnings)

- [ ] **Step 3: Build release to verify**

Run: `cargo build --release -p kamuy-steward`
Expected: Build succeeds

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "feat: complete Phase 1 core infrastructure for v2.0"
```

---

## Summary

Phase 1 delivers:

1. **ApprovalLevel Enum** - Three-tier security: AutoApprove, TelegramButton, TerminalPassword
2. **WhitelistEntry Struct** - Labels, timestamps, total sent tracking
3. **PolicyRules v2.0** - u64 amounts, auto_add_threshold, SpendingTracker, USDC-only
4. **SpendingTracker v2.0** - Daily/weekly tracking with automatic reset
5. **Higher-Security-Wins Logic** - `evaluate_transaction` returns highest required approval level
6. **Updated API** - v2.0 format responses, USDC-only validation
7. **Storage Methods** - Persist spending tracker state

This foundation enables Phase 2 (OpenClaw Integration) and Phase 3 (User Experience Polish).