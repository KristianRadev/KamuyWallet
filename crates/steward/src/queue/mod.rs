//! # Transaction Queue Module
//!
//! Manages the queue of pending transaction requests from agents.
//! Handles ordering, prioritization, expiration, and completion notifications.
#![allow(dead_code)]

pub mod processor;
pub mod notifier;

use crate::error::{StewardError, Result};
use crate::storage::StewardStorage;
use crate::types::{TransactionId, TransactionRecord, TransactionRequest, TransactionStatus};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

// Re-export notifier types
pub use notifier::{TransactionNotifier, TransactionResult, TransactionFinalStatus, NotifierRef};

/// Transaction queue for managing pending requests
pub struct TransactionQueue {
    /// In-memory queue of transaction IDs
    queue: Arc<Mutex<VecDeque<TransactionId>>>,
    /// Storage backend
    storage: Arc<StewardStorage>,
    /// Maximum queue size
    max_size: usize,
    /// Currently processing transaction
    processing: Arc<RwLock<Option<TransactionId>>>,
}

impl TransactionQueue {
    /// Create a new transaction queue
    pub fn new(storage: Arc<StewardStorage>) -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            storage,
            max_size: 1000,
            processing: Arc::new(RwLock::new(None)),
        }
    }

    /// Create with custom max size
    pub fn with_max_size(storage: Arc<StewardStorage>, max_size: usize) -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            storage,
            max_size,
            processing: Arc::new(RwLock::new(None)),
        }
    }

    /// Submit a new transaction request
    pub async fn submit(&self, request: TransactionRequest) -> Result<TransactionRecord> {
        // Check queue size
        let queue_size = self.queue.lock().await.len();
        if queue_size >= self.max_size {
            return Err(StewardError::Queue(
                format!("Queue is full (max {} transactions)", self.max_size)
            ));
        }

        // Create transaction record
        let record = TransactionRecord::new(request);
        
        // Save to storage
        self.storage.save_transaction(&record).await?;
        
        // Add to queue
        self.queue.lock().await.push_back(record.id);
        
        info!(
            transaction_id = %record.id,
            queue_position = queue_size + 1,
            "Transaction submitted to queue"
        );

        Ok(record)
    }

    /// Get the next transaction to process
    /// SECURITY FIX: Combined check-and-pop into atomic operation to prevent race condition
    pub async fn next(&self) -> Result<Option<TransactionRecord>> {
        // Atomically check processing state and pop from queue
        // This prevents race condition where two threads could both get a transaction
        let mut processing_guard = self.processing.write().await;
        
        // Check if already processing something
        if processing_guard.is_some() {
            return Ok(None);
        }
        
        // Get next from queue (must hold both locks)
        let id = self.queue.lock().await.pop_front();
        
        if let Some(id) = id {
            // Mark as processing while still holding the write lock
            *processing_guard = Some(id);
            // Release processing lock before I/O
            drop(processing_guard);

            // Load from storage
            match self.storage.get_transaction(id).await? {
                Some(record) => {
                    info!(transaction_id = %id, "Processing transaction from queue");
                    Ok(Some(record))
                }
                None => {
                    warn!(transaction_id = %id, "Transaction not found in storage");
                    self.clear_processing().await;
                    Ok(None)
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Mark current transaction as complete
    pub async fn complete(&self, record: &TransactionRecord) -> Result<()> {
        // Update in storage
        self.storage.update_transaction(record).await?;
        
        // Clear processing flag
        self.clear_processing().await;
        
        info!(
            transaction_id = %record.id,
            status = %record.status,
            "Transaction processing complete"
        );

        Ok(())
    }

    /// Clear the processing flag
    pub async fn clear_processing(&self) {
        let mut processing = self.processing.write().await;
        *processing = None;
    }

    /// Get queue size
    pub async fn size(&self) -> usize {
        self.queue.lock().await.len()
    }

    /// Check if queue is empty
    pub async fn is_empty(&self) -> bool {
        self.queue.lock().await.is_empty()
    }

    /// Get queue contents (transaction IDs)
    pub async fn list(&self) -> Vec<TransactionId> {
        self.queue.lock().await.iter().copied().collect()
    }

    /// Remove a transaction from the queue
    pub async fn remove(&self, id: TransactionId) -> Result<()> {
        let mut queue = self.queue.lock().await;
        let initial_len = queue.len();
        queue.retain(|&x| x != id);
        
        if queue.len() == initial_len {
            return Err(StewardError::NotFound(
                format!("Transaction {} not found in queue", id)
            ));
        }

        info!(transaction_id = %id, "Transaction removed from queue");
        Ok(())
    }

    /// Requeue a transaction (put back at front)
    pub async fn requeue(&self, id: TransactionId) -> Result<()> {
        // Verify transaction exists
        if self.storage.get_transaction(id).await?.is_none() {
            return Err(StewardError::NotFound(
                format!("Transaction {} not found", id)
            ));
        }

        let mut queue = self.queue.lock().await;
        queue.push_front(id);
        
        // Clear processing if this was the current transaction
        let mut processing = self.processing.write().await;
        if *processing == Some(id) {
            *processing = None;
        }

        info!(transaction_id = %id, "Transaction requeued");
        Ok(())
    }

    /// Get current processing transaction
    pub async fn current(&self) -> Option<TransactionId> {
        *self.processing.read().await
    }

    /// Clear expired transactions from queue
    pub async fn clear_expired(&self) -> Result<usize> {
        let ids: Vec<TransactionId> = self.queue.lock().await.iter().copied().collect();
        let mut cleared = 0;

        for id in ids {
            if let Some(record) = self.storage.get_transaction(id).await? {
                if record.is_expired() {
                    self.remove(id).await.ok();
                    
                    // Update status to expired
                    let mut record = record;
                    record.set_status(TransactionStatus::Expired);
                    self.storage.update_transaction(&record).await?;
                    
                    cleared += 1;
                    info!(transaction_id = %id, "Expired transaction cleared from queue");
                }
            }
        }

        Ok(cleared)
    }

    /// Get position of a transaction in the queue
    pub async fn position(&self, id: TransactionId) -> Option<usize> {
        self.queue.lock().await.iter().position(|&x| x == id)
    }

    /// Get estimated wait time (rough estimate based on position)
    pub async fn estimated_wait_seconds(&self, id: TransactionId) -> Option<u64> {
        self.position(id).await.map(|pos| pos as u64 * 5) // Assume 5 seconds per tx
    }
}

/// Queue statistics
#[derive(Debug, Clone)]
pub struct QueueStats {
    /// Total transactions in queue
    pub pending: usize,
    /// Currently processing
    pub processing: usize,
    /// Completed today
    pub completed_today: u64,
    /// Failed today
    pub failed_today: u64,
    /// Average processing time (seconds)
    pub avg_processing_time: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TransactionRequest;

    async fn create_test_queue() -> TransactionQueue {
        let storage = Arc::new(StewardStorage::new("sqlite::memory:").await.unwrap());
        TransactionQueue::new(storage)
    }

    #[tokio::test]
    async fn test_submit_and_next() {
        let queue = create_test_queue().await;

        // SECURITY: Use wei format (integer) not decimal strings
        let request = TransactionRequest::new(
            "req1", "0x123", "100000000", "USDC", 1, "agent1"
        );

        // Submit
        let record = queue.submit(request).await.unwrap();
        assert_eq!(queue.size().await, 1);

        // Get next
        let next = queue.next().await.unwrap();
        assert!(next.is_some());
        assert_eq!(next.unwrap().id, record.id);
    }

    #[tokio::test]
    async fn test_queue_ordering() {
        let queue = create_test_queue().await;

        // Submit multiple
        // SECURITY: Use wei format (integer) not decimal strings
        for i in 0..3 {
            let request = TransactionRequest::new(
                format!("req{}", i),
                "0x123",
                "100000000",
                "USDC",
                1,
                "agent1"
            );
            queue.submit(request).await.unwrap();
        }

        assert_eq!(queue.size().await, 3);

        // Should come out in order
        let first = queue.next().await.unwrap().unwrap();
        assert_eq!(first.request.request_id, "req0");
    }

    #[tokio::test]
    async fn test_remove() {
        let queue = create_test_queue().await;

        // SECURITY: Use wei format (integer) not decimal strings
        let request = TransactionRequest::new("req1", "0x123", "100000000", "USDC", 1, "agent1");
        let record = queue.submit(request).await.unwrap();

        assert_eq!(queue.size().await, 1);

        queue.remove(record.id).await.unwrap();
        assert_eq!(queue.size().await, 0);
    }

    #[tokio::test]
    async fn test_requeue() {
        let queue = create_test_queue().await;

        // SECURITY: Use wei format (integer) not decimal strings
        let request1 = TransactionRequest::new("req1", "0x123", "100000000", "USDC", 1, "agent1");
        let request2 = TransactionRequest::new("req2", "0x123", "200000000", "USDC", 1, "agent1");

        let record1 = queue.submit(request1).await.unwrap();
        queue.submit(request2).await.unwrap();

        // Get first (removes from queue)
        let _ = queue.next().await.unwrap();

        // Requeue it
        queue.requeue(record1.id).await.unwrap();

        // Should be first again
        let next = queue.next().await.unwrap().unwrap();
        assert_eq!(next.id, record1.id);
    }

    #[tokio::test]
    async fn test_max_size() {
        let storage = Arc::new(StewardStorage::new("sqlite::memory:").await.unwrap());
        let queue = TransactionQueue::with_max_size(storage, 2);

        // Submit 2 (should succeed)
        // SECURITY: Use wei format (integer) not decimal strings
        for i in 0..2 {
            let request = TransactionRequest::new(
                format!("req{}", i),
                "0x123", "100000000", "USDC", 1, "agent1"
            );
            queue.submit(request).await.unwrap();
        }

        // Third should fail
        let request = TransactionRequest::new("req3", "0x123", "300000000", "USDC", 1, "agent1");
        assert!(queue.submit(request).await.is_err());
    }
}
