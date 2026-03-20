//! # Approval Channels
//!
//! Pluggable approval system supporting multiple channels:
//! - Telegram: Mobile notifications with inline buttons
//! - Terminal: Interactive console approval
//! - Auto: No approval needed (policy-based)

use crate::error::{Result, StewardError};
use crate::types::TransactionRecord;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, RwLock};
use tokio::time::timeout;

/// Decision from approval request
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approved,
    Rejected,
    TimedOut,
}

/// Channel for requesting user approval
#[async_trait]
pub trait ApprovalChannel: Send + Sync {
    /// Request approval for a transaction
    /// Returns decision when user responds or timeout occurs
    async fn request_approval(
        &self,
        tx: &TransactionRecord,
    ) -> Result<ApprovalDecision>;

    /// Check if this channel is available/configured
    fn is_available(&self) -> bool;
}

/// Pending approval request
#[derive(Debug)]
struct PendingApproval {
    sender: oneshot::Sender<ApprovalDecision>,
}

/// Manager for pending approvals (shared between channels)
#[derive(Debug, Default)]
pub struct PendingApprovals {
    pending: Arc<RwLock<HashMap<crate::types::TransactionId, PendingApproval>>>,
}

impl PendingApprovals {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a pending approval and get a receiver
    pub async fn register(&self, tx_id: crate::types::TransactionId) -> oneshot::Receiver<ApprovalDecision> {
        let (tx, rx) = oneshot::channel();
        let mut pending = self.pending.write().await;
        pending.insert(tx_id, PendingApproval {
            sender: tx,
        });
        rx
    }

    /// Resolve a pending approval (called when user responds)
    pub async fn resolve(&self, tx_id: &crate::types::TransactionId, decision: ApprovalDecision) -> bool {
        let mut pending = self.pending.write().await;
        if let Some(p) = pending.remove(tx_id) {
            let _ = p.sender.send(decision);
            true
        } else {
            false
        }
    }

    /// Check if there's a pending approval
    pub async fn has_pending(&self, tx_id: &crate::types::TransactionId) -> bool {
        self.pending.read().await.contains_key(tx_id)
    }

    /// Get count of pending approvals
    pub async fn count(&self) -> usize {
        self.pending.read().await.len()
    }
}

impl Clone for PendingApprovals {
    fn clone(&self) -> Self {
        Self {
            pending: self.pending.clone(),
        }
    }
}

/// Composite channel that tries multiple channels
pub struct CompositeApprovalChannel {
    channels: Vec<Box<dyn ApprovalChannel>>,
    timeout: Duration,
    pending: PendingApprovals,
}

impl CompositeApprovalChannel {
    pub fn new(channels: Vec<Box<dyn ApprovalChannel>>, timeout: Duration) -> Self {
        Self {
            channels,
            timeout,
            pending: PendingApprovals::new(),
        }
    }

    /// Create with shared pending approvals
    pub fn with_pending(
        channels: Vec<Box<dyn ApprovalChannel>>,
        timeout: Duration,
        pending: PendingApprovals,
    ) -> Self {
        Self {
            channels,
            timeout,
            pending,
        }
    }

    /// Request approval using first available channel
    pub async fn request_approval(
        &self,
        tx: &TransactionRecord,
    ) -> Result<ApprovalDecision> {
        // Find first available channel
        tracing::debug!("Looking for available approval channels (count: {})", self.channels.len());

        let channel = self
            .channels
            .iter()
            .enumerate()
            .find_map(|(i, c)| {
                let available = c.is_available();
                tracing::debug!("Channel {}: available={}", i, available);
                if available { Some(c) } else { None }
            });

        let channel = match channel {
            Some(c) => c,
            None => {
                tracing::error!("No approval channel available!");
                return Err(StewardError::Config("No approval channel available".to_string()));
            }
        };

        tracing::info!(
            transaction_id = %tx.id,
            timeout_secs = self.timeout.as_secs(),
            "Requesting approval via channel"
        );

        // Request approval with timeout
        match timeout(self.timeout, channel.request_approval(tx)).await {
            Ok(result) => {
                tracing::info!(transaction_id = %tx.id, result = ?result, "Approval result received");
                result
            }
            Err(_) => {
                tracing::warn!(transaction_id = %tx.id, "Approval request timed out");
                // Timeout - remove from pending
                self.pending.resolve(&tx.id, ApprovalDecision::TimedOut).await;
                Ok(ApprovalDecision::TimedOut)
            }
        }
    }

    /// Resolve a pending approval (called by Telegram callback or API)
    pub async fn resolve_approval(
        &self,
        tx_id: &crate::types::TransactionId,
        decision: ApprovalDecision,
    ) -> bool {
        self.pending.resolve(tx_id, decision).await
    }

    /// Get pending approvals manager
    pub fn pending(&self) -> PendingApprovals {
        self.pending.clone()
    }
}

/// Configuration for approval channels
#[derive(Debug, Clone)]
pub struct ApprovalChannelConfig {
    pub telegram: Option<crate::config::TelegramConfig>,
    pub terminal_enabled: bool,
    pub timeout_secs: u64,
}

impl ApprovalChannelConfig {
    /// Create channels based on configuration
    pub fn create_channels(&self) -> CompositeApprovalChannel {
        let mut channels: Vec<Box<dyn ApprovalChannel>> = Vec::new();

        // Telegram first (if configured and enabled)
        #[cfg(feature = "telegram")]
        if let Some(ref tg_config) = self.telegram {
            if tg_config.enabled {
                channels.push(Box::new(TelegramApprovalChannel::new(tg_config.clone())));
            }
        }

        // Terminal fallback
        if self.terminal_enabled {
            channels.push(Box::new(TerminalApprovalChannel::new()));
        }

        CompositeApprovalChannel::new(channels, Duration::from_secs(self.timeout_secs))
    }

    /// Create channels with shared pending approvals
    pub fn create_channels_with_pending(&self, pending: PendingApprovals) -> CompositeApprovalChannel {
        let mut channels: Vec<Box<dyn ApprovalChannel>> = Vec::new();

        #[cfg(feature = "telegram")]
        if let Some(ref tg_config) = self.telegram {
            if tg_config.enabled {
                channels.push(Box::new(TelegramApprovalChannel::with_pending(
                    tg_config.clone(),
                    pending.clone(),
                )));
            }
        }

        if self.terminal_enabled {
            channels.push(Box::new(TerminalApprovalChannel::new()));
        }

        CompositeApprovalChannel::with_pending(channels, Duration::from_secs(self.timeout_secs), pending)
    }
}

/// Terminal-based approval channel
pub struct TerminalApprovalChannel;

impl TerminalApprovalChannel {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ApprovalChannel for TerminalApprovalChannel {
    async fn request_approval(
        &self,
        tx: &TransactionRecord,
    ) -> Result<ApprovalDecision> {
        println!("\n╔═══════════════════════════════════════════════════════════╗");
        println!("║           TRANSACTION REQUIRES APPROVAL                   ║");
        println!("╠═══════════════════════════════════════════════════════════╣");
        println!("║ Transaction ID: {}", tx.id);
        println!("║ To:           {}", tx.request.to);
        println!("║ Amount:       {} {}", crate::types::format_amount(&tx.request.value, &tx.request.token), tx.request.token);
        println!("║ Chain:        {} (ID: {})", chain_name(tx.request.chain_id), tx.request.chain_id);
        if let Some(ref result) = tx.policy_result {
            println!("║ Reason:       {}", result.reason);
        }
        println!("╚═══════════════════════════════════════════════════════════╝");
        println!("\nApprove this transaction? [y/N/timeout]: ");

        // Read user input
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| StewardError::Io(e.to_string()))?;

        let decision = input.trim().to_lowercase();
        if decision == "y" || decision == "yes" {
            Ok(ApprovalDecision::Approved)
        } else {
            Ok(ApprovalDecision::Rejected)
        }
    }

    fn is_available(&self) -> bool {
        // Check if stdin is interactive
        atty::is(atty::Stream::Stdin)
    }
}

/// Telegram-based approval channel
#[cfg(feature = "telegram")]
pub struct TelegramApprovalChannel {
    config: crate::config::TelegramConfig,
    pending: PendingApprovals,
}

#[cfg(feature = "telegram")]
impl TelegramApprovalChannel {
    pub fn new(config: crate::config::TelegramConfig) -> Self {
        Self {
            config,
            pending: PendingApprovals::new(),
        }
    }

    pub fn with_pending(config: crate::config::TelegramConfig, pending: PendingApprovals) -> Self {
        Self { config, pending }
    }

    /// Send approval request to Telegram and wait for response
    async fn send_and_wait(&self, tx: &TransactionRecord) -> Result<ApprovalDecision> {
        use teloxide::prelude::*;
        use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

        let token = self.config.token.as_ref()
            .ok_or_else(|| StewardError::Config("No Telegram token".to_string()))?;

        let bot = Bot::new(token);

        // Format the message - escape special chars for MarkdownV2
        let amount_display = crate::types::format_amount(&tx.request.value, &tx.request.token);
        let addr_short = truncate_address(&tx.request.to);
        let chain = chain_name(tx.request.chain_id);
        let reason = tx.policy_result.as_ref()
            .map(|r| r.reason.as_str())
            .unwrap_or("Amount exceeds auto-approve limit");

        // Build message as plain text
        let text = format!(
            "⚠️ Approval Required\n\
            ━━━━━━━━━━━━━━━\n\n\
            💰 Amount: {}\n\
            📍 To: {}\n\
            ⛓ Chain: {}\n\
            🪙 Token: {}\n\n\
            📝 Reason: {}\n\n\
            ⏰ Time: {}",
            amount_display,
            addr_short,
            chain,
            tx.request.token,
            reason,
            chrono::Utc::now().format("%Y-%m-%d %H:%M UTC")
        );

        // Create approve/reject buttons
        let keyboard = InlineKeyboardMarkup::new(vec![
            vec![
                InlineKeyboardButton::callback("✅ Approve", format!("approve:{}", tx.id)),
                InlineKeyboardButton::callback("❌ Reject", format!("reject:{}", tx.id)),
            ],
        ]);

        // Register pending approval
        let rx = self.pending.register(tx.id).await;

        // Send to all allowed chats
        let mut sent = false;
        for chat_id in &self.config.allowed_chats {
            match bot.send_message(ChatId(*chat_id), &text)
                .reply_markup(keyboard.clone())
                .await {
                Ok(_) => {
                    tracing::info!(
                        chat_id = chat_id,
                        transaction_id = %tx.id,
                        "Sent approval request via Telegram"
                    );
                    sent = true;
                }
                Err(e) => {
                    tracing::warn!(
                        chat_id = chat_id,
                        error = %e,
                        "Failed to send approval request"
                    );
                }
            }
        }

        if !sent {
            // No chats configured or all failed
            self.pending.resolve(&tx.id, ApprovalDecision::TimedOut).await;
            return Err(StewardError::Telegram("No valid chats to send approval".to_string()));
        }

        // Wait for response (the callback handler will resolve this)
        match rx.await {
            Ok(decision) => Ok(decision),
            Err(_) => {
                // Channel closed (shouldn't happen)
                Ok(ApprovalDecision::TimedOut)
            }
        }
    }
}

#[cfg(feature = "telegram")]
#[async_trait]
impl ApprovalChannel for TelegramApprovalChannel {
    async fn request_approval(
        &self,
        tx: &TransactionRecord,
    ) -> Result<ApprovalDecision> {
        self.send_and_wait(tx).await
    }

    fn is_available(&self) -> bool {
        let available = self.config.enabled && self.config.token.is_some() && !self.config.allowed_chats.is_empty();
        tracing::debug!(
            "TelegramApprovalChannel is_available: enabled={}, has_token={}, chats_count={}, result={}",
            self.config.enabled,
            self.config.token.is_some(),
            self.config.allowed_chats.len(),
            available
        );
        available
    }
}

/// Helper function to get chain name
pub fn chain_name(chain_id: u64) -> &'static str {
    match chain_id {
        1 => "Ethereum",
        8453 => "Base",
        137 => "Polygon",
        42161 => "Arbitrum",
        10 => "Optimism",
        _ => "Unknown",
    }
}

/// Truncate address for display
fn truncate_address(address: &str) -> String {
    if address.len() > 20 {
        format!("{}...{}", &address[..8], &address[address.len()-8..])
    } else {
        address.to_string()
    }
}

// For non-telegram builds
#[cfg(not(feature = "telegram"))]
pub struct TelegramApprovalChannel;

#[cfg(not(feature = "telegram"))]
impl TelegramApprovalChannel {
    pub fn new(_config: crate::config::TelegramConfig) -> Self {
        Self
    }
}

#[cfg(not(feature = "telegram"))]
#[async_trait]
impl ApprovalChannel for TelegramApprovalChannel {
    async fn request_approval(
        &self,
        _tx: &TransactionRecord,
    ) -> Result<ApprovalDecision> {
        Err(StewardError::Config("Telegram feature not enabled".to_string()))
    }

    fn is_available(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_name() {
        assert_eq!(chain_name(1), "Ethereum");
        assert_eq!(chain_name(8453), "Base");
        assert_eq!(chain_name(137), "Polygon");
        assert_eq!(chain_name(999), "Unknown");
    }

    #[tokio::test]
    async fn test_pending_approvals() {
        let pending = PendingApprovals::new();
        let tx_id = crate::types::TransactionId::new();

        assert!(!pending.has_pending(&tx_id).await);

        let _rx = pending.register(tx_id).await;
        assert!(pending.has_pending(&tx_id).await);
        assert_eq!(pending.count().await, 1);

        let resolved = pending.resolve(&tx_id, ApprovalDecision::Approved).await;
        assert!(resolved);
        assert!(!pending.has_pending(&tx_id).await);
    }
}