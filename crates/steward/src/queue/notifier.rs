//! # Transaction Notifier
//!
//! Provides a mechanism for waiting on transaction completion.
//! Used for the hybrid long-polling API pattern.

use crate::types::TransactionId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, oneshot};
use tracing::{info, warn};

/// Notification channel for a single transaction
type TxChannel = oneshot::Sender<TransactionResult>;

/// Result of a transaction
#[derive(Debug, Clone, serde::Serialize)]
pub struct TransactionResult {
    /// Transaction ID
    pub tx_id: TransactionId,
    /// Final status
    pub status: TransactionFinalStatus,
    /// Signature (if signed)
    pub signature: Option<String>,
    /// Transaction hash (if submitted)
    pub tx_hash: Option<String>,
    /// Error message (if failed/rejected)
    pub error: Option<String>,
    /// Reason (if rejected or needs approval)
    pub reason: Option<String>,
}

/// Final transaction status for notification
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionFinalStatus {
    /// Transaction was signed and submitted
    Signed,
    /// Transaction was confirmed on-chain
    Confirmed,
    /// Transaction was rejected by policy
    Rejected,
    /// Transaction was rejected by user
    UserRejected,
    /// Transaction failed during processing
    Failed,
    /// Transaction is still pending approval
    PendingApproval,
    /// Transaction expired
    Expired,
}

/// Manager for transaction completion notifications
#[derive(Debug, Default)]
pub struct TransactionNotifier {
    /// Map of transaction IDs to their notification channels
    pending: RwLock<HashMap<TransactionId, TxChannel>>,
}

impl TransactionNotifier {
    /// Create a new notifier
    pub fn new() -> Self {
        Self {
            pending: RwLock::new(HashMap::new()),
        }
    }

    /// Register interest in a transaction completion
    /// Returns a receiver that will be notified when the transaction completes
    pub async fn subscribe(&self, tx_id: TransactionId) -> oneshot::Receiver<TransactionResult> {
        let (tx, rx) = oneshot::channel();

        let mut pending = self.pending.write().await;
        pending.insert(tx_id, tx);

        info!(
            transaction_id = %tx_id,
            "Subscribed to transaction completion"
        );

        rx
    }

    /// Notify all waiters that a transaction has completed
    pub async fn notify(&self, result: TransactionResult) {
        let mut pending = self.pending.write().await;

        if let Some(channel) = pending.remove(&result.tx_id) {
            match channel.send(result.clone()) {
                Ok(()) => {
                    info!(
                        transaction_id = %result.tx_id,
                        status = ?result.status,
                        "Notified transaction completion"
                    );
                }
                Err(_) => {
                    warn!(
                        transaction_id = %result.tx_id,
                        "Failed to notify - receiver dropped"
                    );
                }
            }
        } else {
            // No one waiting, that's fine (transaction polled later)
            info!(
                transaction_id = %result.tx_id,
                "Transaction completed, no waiters"
            );
        }
    }

    /// Remove a subscription (e.g., if timeout occurs)
    pub async fn unsubscribe(&self, tx_id: &TransactionId) {
        let mut pending = self.pending.write().await;
        pending.remove(tx_id);
    }

    /// Get count of pending subscriptions
    pub async fn pending_count(&self) -> usize {
        self.pending.read().await.len()
    }
}

/// Shared notifier reference
pub type NotifierRef = Arc<TransactionNotifier>;

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_subscribe_and_notify() {
        let notifier = Arc::new(TransactionNotifier::new());
        let tx_id = TransactionId::new();

        // Subscribe
        let rx = notifier.subscribe(tx_id).await;

        // Notify
        let result = TransactionResult {
            tx_id,
            status: TransactionFinalStatus::Signed,
            signature: Some("0x123".to_string()),
            tx_hash: Some("0xabc".to_string()),
            error: None,
            reason: None,
        };
        notifier.notify(result.clone()).await;

        // Receive
        let received = rx.await.expect("Should receive notification");
        assert_eq!(received.tx_id, tx_id);
        assert_eq!(received.status, TransactionFinalStatus::Signed);
    }

    #[tokio::test]
    async fn test_no_waiters() {
        let notifier = Arc::new(TransactionNotifier::new());
        let tx_id = TransactionId::new();

        // Notify without subscriber (should not panic)
        let result = TransactionResult {
            tx_id,
            status: TransactionFinalStatus::Rejected,
            signature: None,
            tx_hash: None,
            error: Some("Test rejection".to_string()),
            reason: Some("Policy violation".to_string()),
        };
        notifier.notify(result).await;

        // Should complete without error
    }

    #[tokio::test]
    async fn test_timeout() {
        let notifier = Arc::new(TransactionNotifier::new());
        let tx_id = TransactionId::new();

        // Subscribe
        let rx = notifier.subscribe(tx_id).await;

        // Wait with timeout (no notification)
        let result = timeout(Duration::from_millis(100), rx).await;
        assert!(result.is_err()); // Should timeout
    }
}