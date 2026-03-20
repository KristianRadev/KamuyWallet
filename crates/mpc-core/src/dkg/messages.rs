//! Message types for DKG protocol

use crate::types::{AgentKeyShare, PartyId, PublicKey};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// DKG Complaint message for dispute resolution
/// 
/// When a party receives an invalid share, they broadcast a complaint
/// containing the invalid share. This allows public verification of
/// the complaint and identification of malicious dealers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DkgComplaint {
    /// Party filing the complaint (the victim)
    pub accuser: PartyId,
    /// Party being accused (the dealer who sent invalid share)
    pub accused: PartyId,
    /// The invalid share (revealed for public verification)
    pub share: Vec<u8>,
    /// Reason for the complaint
    pub reason: String,
    /// Timestamp of the complaint
    pub timestamp: i64,
}

impl DkgComplaint {
    /// Create a new DKG complaint
    pub fn new(
        accuser: PartyId,
        accused: PartyId,
        share: Vec<u8>,
        reason: String,
    ) -> Self {
        Self {
            accuser,
            accused,
            share,
            reason,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

/// Round 1 message: Commitment broadcast
///
/// Each party broadcasts commitments to their secret polynomial coefficients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DkgRound1Message {
    /// Party ID
    pub party_id: PartyId,
    /// Commitments to polynomial coefficients (C_i = g^a_i for each coefficient)
    pub commitments: Vec<Vec<u8>>,
}

/// Round 2 message: Secret share sent directly
///
/// Each party sends secret shares to other parties via private channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DkgRound2Message {
    /// Sender party ID
    pub from: PartyId,
    /// Recipient party ID
    pub to: PartyId,
    /// Secret share (f(from) evaluated at recipient's point)
    pub share: Vec<u8>,
}

/// Result of successful DKG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeygenResult {
    /// The generated key share
    pub key_share: AgentKeyShare,
    /// Public key of all parties
    pub public_key: PublicKey,
    /// Public shares for all parties
    pub public_shares: Vec<PublicKey>,
}

impl KeygenResult {
    /// Create a new keygen result
    pub fn new(key_share: AgentKeyShare) -> Result<Self> {
        let public_key = key_share.public_key;
        let public_shares = key_share.public_shares.clone();
        
        Ok(Self {
            key_share,
            public_key,
            public_shares,
        })
    }

    /// Get the Ethereum address
    pub fn eth_address(&self) -> Result<String> {
        crate::key::derive_eth_address(&self.public_key)
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        crate::utils::to_json(self)
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        crate::utils::from_json(bytes)
    }
}

/// DKG state for tracking progress
#[derive(Debug, Clone, Default)]
pub struct DkgState {
    /// Round 1 messages received (party_id -> message)
    pub round1_messages: HashMap<PartyId, DkgRound1Message>,
    /// Round 2 messages received (from -> message)
    pub round2_messages: HashMap<PartyId, DkgRound2Message>,
    /// Current round
    pub current_round: u8,
}

impl DkgState {
    /// Create a new DKG state
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a round 1 message
    pub fn add_round1(&mut self, message: DkgRound1Message) {
        self.round1_messages.insert(message.party_id, message);
    }

    /// Add a round 2 message
    pub fn add_round2(&mut self, message: DkgRound2Message) {
        self.round2_messages.insert(message.from, message);
    }

    /// Check if all round 1 messages are received
    pub fn has_all_round1(&self, expected_count: usize) -> bool {
        self.round1_messages.len() == expected_count
    }

    /// Check if all round 2 messages are received
    pub fn has_all_round2(&self, expected_count: usize) -> bool {
        self.round2_messages.len() == expected_count
    }

    /// Get sorted round 1 messages
    pub fn get_sorted_round1(&self) -> Vec<&DkgRound1Message> {
        let mut messages: Vec<_> = self.round1_messages.values().collect();
        messages.sort_by_key(|m| m.party_id);
        messages
    }

    /// Get round 2 messages for a specific recipient
    pub fn get_round2_for(&self, to: PartyId) -> Vec<&DkgRound2Message> {
        self.round2_messages
            .values()
            .filter(|m| m.to == to)
            .collect()
    }

    /// Advance to next round
    pub fn advance_round(&mut self) {
        self.current_round += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dkg_state() {
        let mut state = DkgState::new();
        
        // Add round 1 messages
        state.add_round1(DkgRound1Message {
            party_id: 0,
            commitments: vec![vec![1, 2, 3]],
        });
        state.add_round1(DkgRound1Message {
            party_id: 1,
            commitments: vec![vec![4, 5, 6]],
        });
        
        assert!(!state.has_all_round1(3));
        
        state.add_round1(DkgRound1Message {
            party_id: 2,
            commitments: vec![vec![7, 8, 9]],
        });
        
        assert!(state.has_all_round1(3));
        
        // Check sorted order
        let sorted = state.get_sorted_round1();
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].party_id, 0);
        assert_eq!(sorted[1].party_id, 1);
        assert_eq!(sorted[2].party_id, 2);
    }

    #[test]
    fn test_keygen_result_serialization() {
        // Create a dummy key share for testing
        use crate::types::{KeyShareMetadata, PartyRole};
        use k256::Scalar;
        
        let key_share = AgentKeyShare {
            party_id: 0,
            role: PartyRole::Agent,
            secret_share: Scalar::ONE,
            public_key: k256::AffinePoint::GENERATOR,
            public_shares: vec![k256::AffinePoint::GENERATOR],
            chain_code: [0u8; 32],
            metadata: KeyShareMetadata::new(PartyRole::Agent),
        };
        
        let result = KeygenResult::new(key_share).unwrap();
        
        let bytes = result.to_bytes().unwrap();
        let recovered = KeygenResult::from_bytes(&bytes).unwrap();
        
        assert_eq!(result.public_key, recovered.public_key);
    }
}
