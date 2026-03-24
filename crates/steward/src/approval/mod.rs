//! # Approval Channels
//!
//! Pluggable approval system supporting multiple channels:
//! - Telegram: Mobile notifications via agent's own Telegram bot (inline flow)
//! - Terminal: Interactive console approval
//! - Auto: No approval needed (policy-based)
//!
//! ## Inline Telegram Flow
//!
//! The Steward does NOT run its own Telegram bot. Instead, it provides API endpoints
//! that the agent uses to handle approval through its own Telegram channel:
//!
//! 1. Steward receives transaction that requires approval
//! 2. Steward stores pending approval and returns to agent
//! 3. Agent polls GET /approval/pending to get pending approvals
//! 4. Agent displays approval request in its own Telegram chat with user
//! 5. When user responds, agent calls POST /approval/respond
//! 6. Steward resolves the pending approval and continues processing

use crate::error::{Result, StewardError};
use crate::types::{ApprovalDecision, TransactionRecord, ApprovalRequest};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, RwLock};
use tokio::time::timeout;

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

/// Pending approval request with sender for notification
#[derive(Debug)]
struct PendingApproval {
    sender: oneshot::Sender<ApprovalDecision>,
    request: ApprovalRequest,
}

/// Manager for pending approvals (shared between channels and API)
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
    pub async fn register(
        &self,
        tx_id: crate::types::TransactionId,
        request: ApprovalRequest,
    ) -> oneshot::Receiver<ApprovalDecision> {
        let (tx, rx) = oneshot::channel();
        let mut pending = self.pending.write().await;
        pending.insert(tx_id, PendingApproval {
            sender: tx,
            request,
        });
        rx
    }

    /// Resolve a pending approval (called when user responds via API)
    pub async fn resolve(
        &self,
        tx_id: &crate::types::TransactionId,
        decision: ApprovalDecision,
    ) -> bool {
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

    /// Get all pending approval requests
    pub async fn get_pending_requests(&self) -> Vec<ApprovalRequest> {
        let pending = self.pending.read().await;
        pending.values().map(|p| p.request.clone()).collect()
    }

    /// Get a specific pending request
    pub async fn get_request(&self, tx_id: &crate::types::TransactionId) -> Option<ApprovalRequest> {
        let pending = self.pending.read().await;
        pending.get(tx_id).map(|p| p.request.clone())
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

    /// Resolve a pending approval (called by API endpoint)
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

    /// Get pending approval requests (for API)
    pub async fn get_pending_requests(&self) -> Vec<ApprovalRequest> {
        self.pending.get_pending_requests().await
    }

    /// Get a specific pending request (for API)
    pub async fn get_pending_request(&self, tx_id: &crate::types::TransactionId) -> Option<ApprovalRequest> {
        self.pending.get_request(tx_id).await
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

        // Telegram inline approval (via API, not bot)
        // This is enabled when telegram is configured, but uses the API flow
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

/// Telegram-based approval channel (inline flow - no bot)
///
/// This channel does NOT run its own Telegram bot. Instead, it:
/// 1. Registers pending approvals in a shared store
/// 2. Provides API endpoints for the agent to poll and respond
/// 3. The agent displays approvals in its own Telegram chat
/// 4. User responses come back via API, not Telegram callbacks
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

    /// Register approval request and wait for API response
    /// This is the inline flow - Steward doesn't send Telegram messages,
    /// it just stores the request and waits for the agent to respond via API
    async fn register_and_wait(&self, tx: &TransactionRecord) -> Result<ApprovalDecision> {
        use crate::types::{ApprovalRequest, ApprovalRequestStatus};

        // Format the amount for display
        let amount_display = crate::types::format_amount(&tx.request.value, &tx.request.token);

        // Get reason from policy result
        let reason = tx.policy_result.as_ref()
            .map(|r| r.reason.clone())
            .unwrap_or_else(|| "Amount exceeds auto-approve limit".to_string());

        // Create approval request
        let now = chrono::Utc::now();
        let approval_request = ApprovalRequest {
            tx_id: tx.id,
            to: tx.request.to.clone(),
            amount_display,
            token: tx.request.token.clone(),
            chain_id: tx.request.chain_id,
            reason,
            created_at: now,
            expires_at: now + chrono::Duration::seconds(self.config.notifications.timeout_secs as i64),
            status: ApprovalRequestStatus::Pending,
        };

        tracing::info!(
            transaction_id = %tx.id,
            "Registered inline approval request - agent should poll /approval/pending"
        );

        // Register pending approval and wait for API response
        let rx = self.pending.register(tx.id, approval_request).await;

        // Wait for response (the API endpoint will resolve this)
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
        self.register_and_wait(tx).await
    }

    fn is_available(&self) -> bool {
        // Telegram inline approval is available when enabled
        // We don't need a token or allowed_chats since the agent handles Telegram
        let available = self.config.enabled;
        tracing::debug!(
            "TelegramApprovalChannel is_available: enabled={}, result={}",
            self.config.enabled,
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
    use crate::types::{ApprovalRequest, ApprovalRequestStatus};
    use chrono::Utc;

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

        // Create a test approval request
        let request = ApprovalRequest {
            tx_id,
            to: "0x1234567890".to_string(),
            amount_display: "100 USDC".to_string(),
            token: "USDC".to_string(),
            chain_id: 1,
            reason: "Test approval".to_string(),
            created_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(5),
            status: ApprovalRequestStatus::Pending,
        };

        let _rx = pending.register(tx_id, request).await;
        assert!(pending.has_pending(&tx_id).await);
        assert_eq!(pending.count().await, 1);

        let resolved = pending.resolve(&tx_id, ApprovalDecision::Approved).await;
        assert!(resolved);
        assert!(!pending.has_pending(&tx_id).await);
    }
}