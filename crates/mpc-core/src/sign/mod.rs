//! Distributed Signature Generation (DSG) module
//!
//! This module implements 2-of-3 threshold ECDSA signing.

pub mod messages;
pub mod protocol;

pub use messages::{PartialSignature, PreSignature, SignRound1Message, SignRound2Message};
pub use protocol::run_sign;

use async_trait::async_trait;
use crate::error::Result;
use crate::types::{PartyId, SessionId};

/// Relay trait for signing communication
///
/// Implement this trait to provide communication between parties during signing.
#[async_trait]
pub trait Relay: Send + Sync {
    /// Broadcast a message to all signing parties
    async fn broadcast<T: serde::Serialize + Send + Sync>(
        &self,
        session_id: &SessionId,
        round: u8,
        message: &T,
    ) -> Result<()>;

    /// Collect broadcast messages from all signing parties
    async fn collect_broadcasts<T: serde::de::DeserializeOwned + Send>(
        &self,
        session_id: &SessionId,
        round: u8,
        count: usize,
    ) -> Result<Vec<T>>;

    /// Send a partial signature to the coordinator
    async fn send_partial<T: serde::Serialize + Send + Sync>(
        &self,
        session_id: &SessionId,
        to: PartyId,
        message: &T,
    ) -> Result<()>;

    /// Collect partial signatures
    async fn collect_partials<T: serde::de::DeserializeOwned + Send>(
        &self,
        session_id: &SessionId,
        from: &[PartyId],
    ) -> Result<Vec<T>>;
}

/// Local relay for testing (in-memory communication)
pub struct LocalRelay {
    // In-memory storage for messages
}

impl LocalRelay {
    /// Create a new local relay
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Relay for LocalRelay {
    async fn broadcast<T: serde::Serialize + Send + Sync>(
        &self,
        _session_id: &SessionId,
        _round: u8,
        _message: &T,
    ) -> Result<()> {
        // TODO: Implement in-memory broadcast
        Ok(())
    }

    async fn collect_broadcasts<T: serde::de::DeserializeOwned + Send>(
        &self,
        _session_id: &SessionId,
        _round: u8,
        _count: usize,
    ) -> Result<Vec<T>> {
        // TODO: Implement in-memory broadcast collection
        Ok(vec![])
    }

    async fn send_partial<T: serde::Serialize + Send + Sync>(
        &self,
        _session_id: &SessionId,
        _to: PartyId,
        _message: &T,
    ) -> Result<()> {
        // TODO: Implement in-memory partial send
        Ok(())
    }

    async fn collect_partials<T: serde::de::DeserializeOwned + Send>(
        &self,
        _session_id: &SessionId,
        _from: &[PartyId],
    ) -> Result<Vec<T>> {
        // TODO: Implement in-memory partial collection
        Ok(vec![])
    }
}

/// Signing state for tracking progress
#[derive(Debug, Clone, Default)]
pub struct SignState {
    /// Round 1 messages received (party_id -> message)
    pub round1_messages: std::collections::HashMap<PartyId, SignRound1Message>,
    /// Partial signatures received (party_id -> partial)
    pub partials: std::collections::HashMap<PartyId, PartialSignature>,
    /// Current round
    pub current_round: u8,
}

impl SignState {
    /// Create a new signing state
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a round 1 message
    pub fn add_round1(&mut self, message: SignRound1Message) {
        self.round1_messages.insert(message.party_id, message);
    }

    /// Add a partial signature
    pub fn add_partial(&mut self, partial: PartialSignature) {
        self.partials.insert(partial.party_id, partial);
    }

    /// Check if all round 1 messages are received
    pub fn has_all_round1(&self, expected_count: usize) -> bool {
        self.round1_messages.len() == expected_count
    }

    /// Check if all partials are received
    pub fn has_all_partials(&self, expected_count: usize) -> bool {
        self.partials.len() == expected_count
    }

    /// Get sorted round 1 messages
    pub fn get_sorted_round1(&self) -> Vec<&SignRound1Message> {
        let mut messages: Vec<_> = self.round1_messages.values().collect();
        messages.sort_by_key(|m| m.party_id);
        messages
    }

    /// Get sorted partial signatures
    pub fn get_sorted_partials(&self) -> Vec<&PartialSignature> {
        let mut partials: Vec<_> = self.partials.values().collect();
        partials.sort_by_key(|p| p.party_id);
        partials
    }

    /// Advance to next round
    pub fn advance_round(&mut self) {
        self.current_round += 1;
    }
}
