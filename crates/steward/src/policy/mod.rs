//! # Policy Engine Module
//!
//! Validates transactions against user-defined security policies.
//!
//! ## Policy Rules
//!
//! - Spending limits (per tx, daily, weekly, monthly)
//! - Whitelist/blacklist for destinations
//! - Time windows for allowed transactions
//! - Rate limiting (transactions per hour/day)
//! - Token restrictions
//! - Approval thresholds

pub mod engine;
pub mod rules;
pub mod schema;
pub mod whitelist;

pub use engine::PolicyEngine;
pub use whitelist::{Whitelist, WhitelistEntry};
#[allow(unused_imports)]
pub use rules::{PolicyRules, TimeWindow};
#[allow(unused_imports)]
pub use schema::{PolicySchema, PolicyValidationResult};

use crate::error::{StewardError, Result};
use crate::types::{is_supported_stablecoin, PolicyCheck, PolicyDecision, PolicyResult, TransactionRequest};
use chrono::{Datelike, Timelike, Utc};
use std::collections::HashMap;

/// Evaluate a transaction against policy rules
pub fn evaluate_transaction(
    request: &TransactionRequest,
    rules: &PolicyRules,
    spending_tracker: &mut SpendingTracker,
) -> Result<PolicyResult> {
    let mut checks = Vec::new();
    let mut all_passed = true;
    let mut require_approval = false;

    // Check 0: Stablecoin only
    let stablecoin_passed = is_supported_stablecoin(&request.token);
    checks.push(PolicyCheck {
        name: "stablecoin_only".to_string(),
        passed: stablecoin_passed,
        value: request.token.clone(),
        limit: "USDC, USDT, DAI".to_string(),
        message: if stablecoin_passed {
            format!("Token {} is a supported stablecoin", request.token)
        } else {
            format!("Token {} is not a supported stablecoin (only USDC, USDT, DAI)", request.token)
        },
    });
    if !stablecoin_passed {
        return Ok(PolicyResult {
            passed: false,
            checks,
            decision: PolicyDecision::Reject,
            reason: format!("Token {} is not a supported stablecoin", request.token),
            evaluated_at: Utc::now(),
        });
    }

    // Check 1: Max per transaction
    let tx_amount = parse_amount(&request.value)?;
    let max_per_tx = parse_amount(&rules.max_per_tx)?;
    let per_tx_passed = tx_amount <= max_per_tx;
    checks.push(PolicyCheck {
        name: "max_per_tx".to_string(),
        passed: per_tx_passed,
        value: request.value.clone(),
        limit: rules.max_per_tx.clone(),
        message: if per_tx_passed {
            format!("Transaction amount {} is within limit {}", request.value, rules.max_per_tx)
        } else {
            format!("Transaction amount {} exceeds limit {}", request.value, rules.max_per_tx)
        },
    });
    if !per_tx_passed {
        all_passed = false;
    }

    // Check 2: Daily spending limit
    // SECURITY: Use wei (u128) for precise arithmetic
    let daily_spent_wei = spending_tracker.get_daily_spent_wei(&request.token);
    let daily_limit_wei = parse_amount(&rules.max_daily)?;
    let new_daily_total_wei = daily_spent_wei + tx_amount;
    let daily_passed = new_daily_total_wei <= daily_limit_wei;
    checks.push(PolicyCheck {
        name: "max_daily".to_string(),
        passed: daily_passed,
        value: format_amount(new_daily_total_wei),
        limit: rules.max_daily.clone(),
        message: if daily_passed {
            format!("Daily total {} is within limit {}", format_amount(new_daily_total_wei), rules.max_daily)
        } else {
            format!("Daily total {} would exceed limit {}", format_amount(new_daily_total_wei), rules.max_daily)
        },
    });
    if !daily_passed {
        all_passed = false;
    }

    // Check 3: Token allowed
    let token_passed = rules.allowed_tokens.is_empty() 
        || rules.allowed_tokens.contains(&request.token.to_uppercase());
    checks.push(PolicyCheck {
        name: "allowed_tokens".to_string(),
        passed: token_passed,
        value: request.token.clone(),
        limit: rules.allowed_tokens.join(", "),
        message: if token_passed {
            format!("Token {} is allowed", request.token)
        } else {
            format!("Token {} is not in allowed list", request.token)
        },
    });
    if !token_passed {
        all_passed = false;
    }

    // Check 4: Blocklist
    let blocklist_passed = !rules.blocklist.iter().any(|blocked| {
        matches_pattern(&request.to, blocked)
    });
    checks.push(PolicyCheck {
        name: "blocklist".to_string(),
        passed: blocklist_passed,
        value: request.to.clone(),
        limit: rules.blocklist.join(", "),
        message: if blocklist_passed {
            format!("Destination {} is not blocked", request.to)
        } else {
            format!("Destination {} is in blocklist", request.to)
        },
    });
    if !blocklist_passed {
        all_passed = false;
    }

    // Check 5: Whitelist (if set)
    let whitelist_passed = rules.whitelist.is_empty() 
        || rules.whitelist.iter().any(|allowed| {
            matches_pattern(&request.to, allowed)
        });
    checks.push(PolicyCheck {
        name: "whitelist".to_string(),
        passed: whitelist_passed,
        value: request.to.clone(),
        limit: if rules.whitelist.is_empty() { 
            "(any)".to_string() 
        } else { 
            rules.whitelist.join(", ") 
        },
        message: if whitelist_passed {
            format!("Destination {} is whitelisted", request.to)
        } else {
            format!("Destination {} is not in whitelist", request.to)
        },
    });
    if !whitelist_passed {
        all_passed = false;
    }

    // Check 6: Time window
    let time_passed = rules.time_windows.is_empty()
        || rules.time_windows.iter().any(|window| {
            is_within_time_window(window)
        });
    checks.push(PolicyCheck {
        name: "time_window".to_string(),
        passed: time_passed,
        value: Utc::now().format("%H:%M %a").to_string(),
        limit: rules.time_windows.iter()
            .map(|w| format!("{}-{} on {:?}", w.start, w.end, w.days))
            .collect::<Vec<_>>()
            .join("; "),
        message: if time_passed {
            "Current time is within allowed window".to_string()
        } else {
            "Current time is outside allowed window".to_string()
        },
    });
    if !time_passed {
        all_passed = false;
    }

    // Check 7: Rate limit
    let tx_count = spending_tracker.get_daily_count();
    let rate_passed = tx_count < rules.rate_limit_per_day;
    checks.push(PolicyCheck {
        name: "rate_limit".to_string(),
        passed: rate_passed,
        value: tx_count.to_string(),
        limit: rules.rate_limit_per_day.to_string(),
        message: if rate_passed {
            format!("Transaction count {} is within daily limit {}", tx_count, rules.rate_limit_per_day)
        } else {
            format!("Transaction count {} exceeds daily limit {}", tx_count, rules.rate_limit_per_day)
        },
    });
    if !rate_passed {
        all_passed = false;
    }

    // Check 8: Require approval above threshold
    let approval_threshold = parse_amount(&rules.require_approval_above)?;
    if tx_amount > approval_threshold {
        require_approval = true;
        checks.push(PolicyCheck {
            name: "require_approval_above".to_string(),
            passed: true, // This is a warning, not a failure
            value: request.value.clone(),
            limit: rules.require_approval_above.clone(),
            message: format!("Amount {} exceeds approval threshold {}", request.value, rules.require_approval_above),
        });
    }

    // Determine decision
    let decision = if !all_passed {
        PolicyDecision::Reject
    } else if require_approval {
        PolicyDecision::RequireApproval
    } else {
        PolicyDecision::AutoApprove
    };

    let reason = if !all_passed {
        "Policy violation detected".to_string()
    } else if require_approval {
        "Transaction amount requires user approval".to_string()
    } else {
        "All policy checks passed".to_string()
    };

    Ok(PolicyResult {
        passed: all_passed,
        checks,
        decision,
        reason,
        evaluated_at: chrono::Utc::now(),
    })
}

/// Parse amount string to integer (wei) for precise arithmetic
/// SECURITY: Uses integer arithmetic only, no floating-point to avoid precision issues
fn parse_amount(amount: &str) -> Result<u128> {
    // SECURITY FIX: Only parse as integer (wei format), reject decimal strings
    // This prevents floating-point precision attacks
    if let Ok(wei) = amount.parse::<u128>() {
        return Ok(wei);
    }

    // Reject decimal/float formats - all amounts must be in wei (integer)
    Err(StewardError::Validation(
        format!("Invalid amount format: {}. Amounts must be integers in wei.", amount)
    ))
}

/// Format amount for display (convert wei back to decimal)
fn format_amount(wei: u128) -> String {
    let integer_part = wei / 1_000_000;
    let fractional_part = wei % 1_000_000;
    format!("{}.{:06}", integer_part, fractional_part)
}

/// Check if string matches a pattern (supports wildcards)
/// SECURITY: All comparisons are case-sensitive to avoid ambiguity
fn matches_pattern(value: &str, pattern: &str) -> bool {
    if pattern.ends_with('*') {
        let prefix = &pattern[..pattern.len() - 1];
        value.starts_with(prefix)
    } else if pattern.starts_with('*') {
        let suffix = &pattern[1..];
        value.ends_with(suffix)
    } else {
        value == pattern  // Case-sensitive exact match
    }
}

/// Check if current time is within the time window
fn is_within_time_window(window: &rules::TimeWindow) -> bool {
    let now = Utc::now();
    let current_day = now.weekday();
    let current_time = now.time();

    // Check day
    let day_match = window.days.iter().any(|d| {
        matches_day(current_day, d)
    });

    if !day_match {
        return false;
    }

    // Parse time window
    let start_parts: Vec<u32> = window.start.split(':')
        .filter_map(|s| s.parse().ok())
        .collect();
    let end_parts: Vec<u32> = window.end.split(':')
        .filter_map(|s| s.parse().ok())
        .collect();

    if start_parts.len() != 2 || end_parts.len() != 2 {
        return true; // Invalid format, allow by default
    }

    let start_minutes = start_parts[0] * 60 + start_parts[1];
    let end_minutes = end_parts[0] * 60 + end_parts[1];
    let current_minutes = current_time.hour() * 60 + current_time.minute();

    current_minutes >= start_minutes && current_minutes <= end_minutes
}

/// Match day string to chrono weekday
fn matches_day(weekday: chrono::Weekday, day_str: &str) -> bool {
    let day_lower = day_str.to_lowercase();
    match weekday {
        chrono::Weekday::Mon => day_lower == "mon" || day_lower == "monday",
        chrono::Weekday::Tue => day_lower == "tue" || day_lower == "tuesday",
        chrono::Weekday::Wed => day_lower == "wed" || day_lower == "wednesday",
        chrono::Weekday::Thu => day_lower == "thu" || day_lower == "thursday",
        chrono::Weekday::Fri => day_lower == "fri" || day_lower == "friday",
        chrono::Weekday::Sat => day_lower == "sat" || day_lower == "saturday",
        chrono::Weekday::Sun => day_lower == "sun" || day_lower == "sunday",
    }
}

/// Tracks spending for policy enforcement
/// SECURITY: Uses u128 (wei) for precise integer arithmetic, avoiding float precision issues
#[derive(Debug, Default)]
pub struct SpendingTracker {
    daily_spent: HashMap<String, u128>,  // Amounts in wei (6 decimals)
    daily_count: u32,
    last_reset: chrono::DateTime<chrono::Utc>,
}

impl SpendingTracker {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            daily_spent: HashMap::new(),
            daily_count: 0,
            last_reset: Utc::now(),
        }
    }

    /// Get daily spent amount in wei
    pub fn get_daily_spent_wei(&mut self, token: &str) -> u128 {
        self.check_reset();
        *self.daily_spent.get(&token.to_uppercase()).unwrap_or(&0)
    }

    /// Get daily spent amount as decimal string for display
    #[allow(dead_code)]
    pub fn get_daily_spent(&mut self, token: &str) -> String {
        format_amount(self.get_daily_spent_wei(token))
    }

    pub fn get_daily_count(&mut self) -> u32 {
        self.check_reset();
        self.daily_count
    }

    /// Record transaction amount in wei
    pub fn record_transaction_wei(&mut self, token: &str, amount_wei: u128) {
        self.check_reset();
        let token_upper = token.to_uppercase();
        let current = self.daily_spent.entry(token_upper).or_insert(0);
        *current += amount_wei;
        self.daily_count += 1;
    }

    /// Record transaction amount from decimal string
    #[allow(dead_code)]
    pub fn record_transaction(&mut self, token: &str, amount: &str) -> Result<()> {
        let wei = parse_amount(amount)?;
        self.record_transaction_wei(token, wei);
        Ok(())
    }

    fn check_reset(&mut self) {
        let now = Utc::now();
        // Check if we've crossed a day boundary in UTC
        let days_since_reset = (now - self.last_reset).num_days();
        if days_since_reset >= 1 {
            self.daily_spent.clear();
            self.daily_count = 0;
            self.last_reset = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::rules::TimeWindow;

    #[test]
    fn test_parse_amount() {
        // SECURITY FIX: Only integer (wei) format is accepted, no decimal/float
        assert_eq!(parse_amount("100000000").unwrap(), 100_000_000); // 100 USDC in wei (6 decimals)
        assert_eq!(parse_amount("0").unwrap(), 0);
        assert_eq!(parse_amount("1000000").unwrap(), 1_000_000); // 1 USDC in wei
        assert!(parse_amount("invalid").is_err());
        assert!(parse_amount("-100").is_err()); // Negative not allowed
        assert!(parse_amount("100.50").is_err()); // Decimal strings no longer accepted
    }

    #[test]
    fn test_format_amount() {
        assert_eq!(format_amount(100_500_000), "100.500000");
        assert_eq!(format_amount(1_000_000), "1.000000");
        assert_eq!(format_amount(0), "0.000000");
    }

    #[test]
    fn test_matches_pattern() {
        assert!(matches_pattern("0x123abc", "0x123*"));
        assert!(matches_pattern("openai.com", "openai*"));
        assert!(matches_pattern("test@example.com", "*@example.com"));
        assert!(matches_pattern("exact", "exact"));
        assert!(!matches_pattern("0x456def", "0x123*"));
        // SECURITY: Case-sensitive matching
        assert!(!matches_pattern("Exact", "exact")); // Case-sensitive
        assert!(!matches_pattern("EXACT", "exact")); // Case-sensitive
        assert!(matches_pattern("0x123ABC", "0x123*")); // Prefix match is case-sensitive
    }

    #[test]
    fn test_spending_tracker() {
        let mut tracker = SpendingTracker::new();
        
        // Use wei amounts
        tracker.record_transaction_wei("USDC", 100_500_000); // 100.50
        tracker.record_transaction_wei("USDC", 50_000_000);  // 50.00
        tracker.record_transaction_wei("USDT", 25_000_000);  // 25.00
        
        // Check wei amounts
        assert_eq!(tracker.get_daily_spent_wei("USDC"), 150_500_000);
        assert_eq!(tracker.get_daily_spent_wei("USDT"), 25_000_000);
        assert_eq!(tracker.get_daily_count(), 3);
        
        // Check formatted amounts
        assert_eq!(tracker.get_daily_spent("USDC"), "150.500000");
    }
}
