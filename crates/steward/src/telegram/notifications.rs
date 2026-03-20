//! # Telegram Notifications
//!
//! Send notifications to users about transaction events.

use super::format_transaction;
use crate::config::TelegramConfig;
use crate::error::Result;
use crate::types::TransactionRecord;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode};
use tracing::{info, warn};

/// Send approval request notification
pub async fn send_approval_request(
    config: &TelegramConfig,
    record: &TransactionRecord,
) -> Result<()> {
    if !config.notifications.on_approval_required {
        return Ok(());
    }
    
    let token = config.token.as_ref()
        .ok_or(crate::error::StewardError::Config("No Telegram token".to_string()))?;
    
    let bot = Bot::new(token);
    
    // Format the message
    let text = format!(
        "⚠️ *Approval Required*\n\
        ━━━━━━━━━━━━━━━\n\
        \n\
        {}\n\
        \n\
        *Reason:* {}\n\
        \n\
        Please approve or reject this transaction.",
        format_transaction(record),
        record.policy_result.as_ref()
            .map(|r| r.reason.as_str())
            .unwrap_or("Policy violation")
    );
    
    // Create approve/reject buttons
    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback(
                "✅ Approve",
                format!("approve:{}", record.id)
            ),
            InlineKeyboardButton::callback(
                "❌ Reject",
                format!("reject:{}", record.id)
            ),
        ],
    ]);
    
    // Send to all allowed chats
    for chat_id in &config.allowed_chats {
        match bot.send_message(ChatId(*chat_id), &text)
            .parse_mode(ParseMode::MarkdownV2)
            .reply_markup(keyboard.clone())
            .await {
            Ok(_) => {
                info!(
                    chat_id = chat_id,
                    transaction_id = %record.id,
                    "Sent approval request notification"
                );
            }
            Err(e) => {
                warn!(
                    chat_id = chat_id,
                    error = %e,
                    "Failed to send approval request"
                );
            }
        }
    }
    
    Ok(())
}

/// Send auto-approval notification
pub async fn send_auto_approval(
    config: &TelegramConfig,
    record: &TransactionRecord,
) -> Result<()> {
    if !config.notifications.on_auto_approve {
        return Ok(());
    }
    
    let token = config.token.as_ref()
        .ok_or(crate::error::StewardError::Config("No Telegram token".to_string()))?;
    
    let bot = Bot::new(token);
    
    let text = format!(
        "✅ *Transaction Auto-Approved*\n\
        ━━━━━━━━━━━━━━━\n\
        \n\
        {}\n\
        \n\
        This transaction was automatically approved by policy.",
        format_transaction(record)
    );
    
    for chat_id in &config.allowed_chats {
        bot.send_message(ChatId(*chat_id), &text)
            .parse_mode(ParseMode::MarkdownV2)
            .await
            .ok();
    }
    
    Ok(())
}

/// Send rejection notification
pub async fn send_rejection(
    config: &TelegramConfig,
    record: &TransactionRecord,
) -> Result<()> {
    if !config.notifications.on_rejection {
        return Ok(());
    }
    
    let token = config.token.as_ref()
        .ok_or(crate::error::StewardError::Config("No Telegram token".to_string()))?;
    
    let bot = Bot::new(token);
    
    let reason = record.policy_result.as_ref()
        .map(|r| r.reason.clone())
        .unwrap_or_else(|| "Policy violation".to_string());
    
    let text = format!(
        "❌ *Transaction Rejected*\n\
        ━━━━━━━━━━━━━━━\n\
        \n\
        {}\n\
        \n\
        *Reason:* {}",
        format_transaction(record),
        reason
    );
    
    for chat_id in &config.allowed_chats {
        bot.send_message(ChatId(*chat_id), &text)
            .parse_mode(ParseMode::MarkdownV2)
            .await
            .ok();
    }
    
    Ok(())
}

/// Send execution confirmation
pub async fn send_execution(
    config: &TelegramConfig,
    record: &TransactionRecord,
) -> Result<()> {
    if !config.notifications.on_execution {
        return Ok(());
    }
    
    let token = config.token.as_ref()
        .ok_or(crate::error::StewardError::Config("No Telegram token".to_string()))?;
    
    let bot = Bot::new(token);
    
    let tx_hash = record.tx_hash.as_ref()
        .map(|h| format!("`{}`", h))
        .unwrap_or_else(|| "Pending".to_string());
    
    let text = format!(
        "✅ *Transaction Executed*\n\
        ━━━━━━━━━━━━━━━\n\
        \n\
        {}\n\
        \n\
        *Transaction Hash:* {}\n\
        *Status:* Confirmed",
        format_transaction(record),
        tx_hash
    );
    
    for chat_id in &config.allowed_chats {
        bot.send_message(ChatId(*chat_id), &text)
            .parse_mode(ParseMode::MarkdownV2)
            .await
            .ok();
    }
    
    Ok(())
}

/// Send error notification
pub async fn send_error(
    config: &TelegramConfig,
    error: &str,
) -> Result<()> {
    if !config.notifications.on_error {
        return Ok(());
    }
    
    let token = config.token.as_ref()
        .ok_or(crate::error::StewardError::Config("No Telegram token".to_string()))?;
    
    let bot = Bot::new(token);
    
    let text = format!(
        "⚠️ *Error*\n\
        ━━━━━━━━━━━━━━━\n\
        \n\
        {}",
        error
    );
    
    for chat_id in &config.allowed_chats {
        bot.send_message(ChatId(*chat_id), &text)
            .parse_mode(ParseMode::MarkdownV2)
            .await
            .ok();
    }
    
    Ok(())
}

/// Send daily summary
pub async fn send_daily_summary(
    config: &TelegramConfig,
    stats: crate::queue::QueueStats,
) -> Result<()> {
    let token = config.token.as_ref()
        .ok_or(crate::error::StewardError::Config("No Telegram token".to_string()))?;
    
    let bot = Bot::new(token);
    
    let text = format!(
        "📊 *Daily Summary*\n\
        ━━━━━━━━━━━━━━━\n\
        \n\
        • Pending: {}\n\
        • Processing: {}\n\
        • Completed today: {}\n\
        • Failed today: {}\n\
        • Avg processing time: {:.1}s",
        stats.pending,
        stats.processing,
        stats.completed_today,
        stats.failed_today,
        stats.avg_processing_time
    );
    
    for chat_id in &config.allowed_chats {
        bot.send_message(ChatId(*chat_id), &text)
            .parse_mode(ParseMode::MarkdownV2)
            .await
            .ok();
    }
    
    Ok(())
}
