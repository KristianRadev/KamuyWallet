//! Distributed Key Generation (DKG) module
//!
//! This module implements a Feldman VSS-based DKG protocol for 2-of-3 threshold signing.

pub mod messages;
pub mod protocol;

pub use messages::{DkgRound1Message, DkgRound2Message, KeygenResult};
pub use protocol::run_dkg;

use async_trait::async_trait;
use crate::error::Result;
use crate::types::SessionId;

/// Relay trait for DKG communication
///
/// Implement this trait to provide communication between parties during DKG.
#[async_trait]
pub trait Relay: Send + Sync {
    /// Broadcast a message to all parties
    async fn broadcast<T: serde::Serialize + Send + Sync>(
        &self,
        session_id: &SessionId,
        round: u8,
        message: &T,
    ) -> Result<()>;

    /// Send a message to a specific party
    async fn send_direct<T: serde::Serialize + Send + Sync>(
        &self,
        session_id: &SessionId,
        round: u8,
        to: crate::types::PartyId,
        message: &T,
    ) -> Result<()>;

    /// Collect broadcast messages from all parties
    async fn collect_broadcasts<T: serde::de::DeserializeOwned + Send>(
        &self,
        session_id: &SessionId,
        round: u8,
        count: usize,
    ) -> Result<Vec<T>>;

    /// Collect direct messages sent to this party
    async fn collect_direct<T: serde::de::DeserializeOwned + Send>(
        &self,
        session_id: &SessionId,
        round: u8,
        to: crate::types::PartyId,
        count: usize,
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

    async fn send_direct<T: serde::Serialize + Send + Sync>(
        &self,
        _session_id: &SessionId,
        _round: u8,
        _to: crate::types::PartyId,
        _message: &T,
    ) -> Result<()> {
        // TODO: Implement in-memory direct send
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

    async fn collect_direct<T: serde::de::DeserializeOwned + Send>(
        &self,
        _session_id: &SessionId,
        _round: u8,
        _to: crate::types::PartyId,
        _count: usize,
    ) -> Result<Vec<T>> {
        // TODO: Implement in-memory direct message collection
        Ok(vec![])
    }
}
