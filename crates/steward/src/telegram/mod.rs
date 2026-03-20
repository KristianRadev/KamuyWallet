//! # Telegram Bot Module
//!
//! User interface via Telegram for approvals and wallet management.
#![allow(dead_code)]

pub mod bot;
pub mod commands;
pub mod notifications;

use crate::config::TelegramConfig;
use crate::error::{StewardError, Result};

/// Validate Telegram configuration
pub fn validate_config(config: &TelegramConfig) -> Result<()> {
    if config.enabled {
        if config.token.is_none() {
            return Err(StewardError::Config(
                "Telegram enabled but no token provided".to_string()
            ));
        }
        
        // Validate token format (should be digits:alphanumeric)
        let token = config.token.as_ref().unwrap();
        if !token.contains(':') {
            return Err(StewardError::Config(
                "Invalid Telegram token format".to_string()
            ));
        }
    }
    
    Ok(())
}

/// Check if chat is allowed
pub fn is_chat_allowed(config: &TelegramConfig, chat_id: i64) -> bool {
    if config.allowed_chats.is_empty() {
        return true; // Allow all if not restricted
    }
    config.allowed_chats.contains(&chat_id)
}

/// Format transaction for display
pub fn format_transaction(record: &crate::types::TransactionRecord) -> String {
    let status_emoji = match record.status {
        crate::types::TransactionStatus::Pending => "⏳",
        crate::types::TransactionStatus::Approved => "✅",
        crate::types::TransactionStatus::AwaitingApproval => "🔄",
        crate::types::TransactionStatus::Confirmed => "✅",
        crate::types::TransactionStatus::Rejected => "❌",
        crate::types::TransactionStatus::Failed => "⚠️",
        _ => "❓",
    };
    
    format!(
        "{} *Transaction*\n\
        ━━━━━━━━━━━━━━━\n\
        ID: `{}`\n\
        Amount: {} {}\n\
        To: `{}`\n\
        Chain: {}\n\
        Status: {}\n\
        Time: {}",
        status_emoji,
        record.id,
        record.request.value,
        record.request.token,
        record.request.to,
        record.request.chain_id,
        record.status,
        record.created_at.format("%Y-%m-%d %H:%M UTC")
    )
}

/// Format policy for display (v2.0)
pub fn format_policy(policy: &crate::policy::rules::PolicyRules) -> String {
    // Format whitelist entries
    let whitelist = if policy.whitelist.is_empty() {
        "Any destination".to_string()
    } else {
        policy.whitelist.entries()
            .iter()
            .map(|(addr, entry)| format!("{} ({})", addr, entry.label))
            .collect::<Vec<_>>()
            .join("\n")
    };

    // Format spending tracker status
    let daily_spent_usdc = policy.spending_tracker.daily_spent as f64 / 1_000_000.0;
    let weekly_spent_usdc = policy.spending_tracker.weekly_spent as f64 / 1_000_000.0;
    let max_per_tx_usdc = policy.max_per_tx as f64 / 1_000_000.0;
    let max_daily_usdc = policy.max_daily as f64 / 1_000_000.0;
    let max_weekly_usdc = policy.max_weekly as f64 / 1_000_000.0;
    let auto_add_threshold_usdc = policy.auto_add_threshold as f64 / 1_000_000.0;

    format!(
        "🔐 Current Policy (v2.0)\n\
        ━━━━━━━━━━━━━━━\n\
        Spending Limits:\n\
        - Per transaction: {:.2} USDC\n\
        - Daily: {:.2} USDC (spent: {:.2})\n\
        - Weekly: {:.2} USDC (spent: {:.2})\n\
        - Auto-add threshold: {:.2} USDC\n\
        \n\
        Token: USDC (gasless)\n\
        \n\
        Whitelist:\n\
        {}",
        max_per_tx_usdc,
        max_daily_usdc, daily_spent_usdc,
        max_weekly_usdc, weekly_spent_usdc,
        auto_add_threshold_usdc,
        whitelist
    )
}

/// Escape markdown characters for Telegram MarkdownV2
/// In MarkdownV2, these characters must be escaped: _ * [ ] ( ) ~ ` > # + - = | { } . !
pub fn escape_markdown(text: &str) -> String {
    let mut result = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        match c {
            '_' | '*' | '[' | ']' | '(' | ')' | '~' | '`' | '>' | '#' | '+' | '-' | '=' | '|' | '{' | '}' | '.' | '!' | '\\' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

/// Truncate address for display
pub fn truncate_address(address: &str) -> String {
    if address.len() > 20 {
        format!("{}...{}", &address[..8], &address[address.len()-8..])
    } else {
        address.to_string()
    }
}

/// Format wallet info
pub fn format_wallet_info(
    address: &str,
    balance: &str,
    pending: u32,
) -> String {
    format!(
        "👛 *Wallet*\n\
        ━━━━━━━━━━━━━━━\n\
        Address: `{}`\n\
        Balance: {}\n\
        Pending transactions: {}",
        address,
        balance,
        pending
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_markdown() {
        let text = "test_hello *world* [link](url) `code`";
        let escaped = escape_markdown(text);
        assert!(escaped.contains("\\_"));
        assert!(escaped.contains("\\*"));
        assert!(escaped.contains("\\["));
        assert!(escaped.contains("\\`"));
    }

    #[test]
    fn test_truncate_address() {
        let addr = "0x742d35Cc6634C0532925a3b844Bc9e7595f4e2E4";
        let truncated = truncate_address(addr);
        assert!(truncated.contains("..."));
        assert!(truncated.len() < addr.len());
    }

    #[test]
    fn test_is_chat_allowed() {
        let config = TelegramConfig {
            token: None,
            enabled: false,
            webhook_url: None,
            webhook_port: 8443,
            webhook_secret: None,
            allowed_chats: vec![123456],
            notifications: crate::config::NotificationConfig {
                on_auto_approve: false,
                on_approval_required: true,
                on_rejection: true,
                on_execution: true,
                on_error: true,
            },
        };
        
        assert!(is_chat_allowed(&config, 123456));
        assert!(!is_chat_allowed(&config, 999999));
        
        // Empty allowed_chats = allow all
        let config2 = TelegramConfig {
            allowed_chats: vec![],
            ..config
        };
        assert!(is_chat_allowed(&config2, 999999));
    }
}
