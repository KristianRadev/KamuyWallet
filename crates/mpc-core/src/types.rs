//! Core types for Kamuy MPC

use crate::error::{Error, Result};
use k256::AffinePoint;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export Scalar from k256 for use throughout the crate
pub use k256::Scalar;

/// Number of parties in the threshold scheme
pub const N_PARTIES: usize = 3;

/// Threshold required for signing (2-of-3)
pub const THRESHOLD: usize = 2;

/// Party ID for the Agent (AI agent)
pub const PARTY_AGENT: PartyId = 0;

/// Party ID for the Steward (policy engine)
pub const PARTY_STEWARD: PartyId = 1;

/// Party ID for the User (ultimate owner)
pub const PARTY_USER: PartyId = 2;

/// Party identifier (0 = Agent, 1 = Steward, 2 = User)
pub type PartyId = u8;

/// Session identifier (32 bytes)
pub type SessionId = [u8; 32];

/// Public key (secp256k1 point)
pub type PublicKey = AffinePoint;

/// Message hash (32 bytes)
pub type Message = [u8; 32];

/// Role of a party in the MPC protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Hash)]
pub enum PartyRole {
    /// AI Agent - initiates transactions
    #[default]
    Agent,
    /// Steward - validates and co-signs
    Steward,
    /// User - ultimate owner
    User,
}

impl PartyRole {
    /// Get the party ID for this role
    pub const fn party_id(&self) -> PartyId {
        match self {
            PartyRole::Agent => PARTY_AGENT,
            PartyRole::Steward => PARTY_STEWARD,
            PartyRole::User => PARTY_USER,
        }
    }

    /// Get the role from party ID
    pub fn from_party_id(id: PartyId) -> Result<Self> {
        match id {
            PARTY_AGENT => Ok(PartyRole::Agent),
            PARTY_STEWARD => Ok(PartyRole::Steward),
            PARTY_USER => Ok(PartyRole::User),
            _ => Err(Error::InvalidPartyId(id)),
        }
    }
}

impl std::fmt::Display for PartyRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PartyRole::Agent => write!(f, "Agent"),
            PartyRole::Steward => write!(f, "Steward"),
            PartyRole::User => write!(f, "User"),
        }
    }
}

/// Supported blockchain types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ChainType {
    /// Ethereum and EVM-compatible chains
    Evm,
    /// Bitcoin
    Bitcoin,
    /// Solana
    Solana,
}

impl std::fmt::Display for ChainType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChainType::Evm => write!(f, "EVM"),
            ChainType::Bitcoin => write!(f, "Bitcoin"),
            ChainType::Solana => write!(f, "Solana"),
        }
    }
}

/// Session configuration for MPC operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Unique session identifier
    pub session_id: SessionId,
    /// Total number of parties
    pub n_parties: usize,
    /// Threshold required for signing
    pub threshold: usize,
    /// This party's ID
    pub party_id: PartyId,
    /// This party's role
    pub role: PartyRole,
    /// List of participating party IDs
    pub parties: Vec<PartyId>,
    /// Timeout in seconds
    pub timeout_secs: u64,
}

impl SessionConfig {
    /// Create a new session configuration
    pub fn new(
        session_id: SessionId,
        party_id: PartyId,
        role: PartyRole,
        parties: Vec<PartyId>,
    ) -> Self {
        Self {
            session_id,
            n_parties: N_PARTIES,
            threshold: THRESHOLD,
            party_id,
            role,
            parties,
            timeout_secs: 60,
        }
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.n_parties != N_PARTIES {
            return Err(Error::InvalidConfig(format!(
                "Expected {} parties, got {}",
                N_PARTIES, self.n_parties
            )));
        }
        if self.threshold != THRESHOLD {
            return Err(Error::InvalidConfig(format!(
                "Expected threshold of {}, got {}",
                THRESHOLD, self.threshold
            )));
        }
        if self.party_id >= self.n_parties as PartyId {
            return Err(Error::InvalidPartyId(self.party_id));
        }
        if self.parties.len() < self.threshold {
            return Err(Error::ThresholdNotMet {
                required: self.threshold,
                actual: self.parties.len(),
            });
        }
        Ok(())
    }
}

/// Metadata for a key share
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyShareMetadata {
    /// Unique share identifier
    pub share_id: String,
    /// Role of this share
    pub role: PartyRole,
    /// Creation timestamp
    pub created_at: i64,
    /// Last refresh timestamp
    pub last_refreshed_at: Option<i64>,
    /// Derived addresses for each chain
    pub addresses: HashMap<ChainType, String>,
    /// Optional label
    pub label: Option<String>,
}

impl KeyShareMetadata {
    /// Create new metadata
    pub fn new(role: PartyRole) -> Self {
        Self {
            share_id: uuid::Uuid::new_v4().to_string(),
            role,
            created_at: chrono::Utc::now().timestamp(),
            last_refreshed_at: None,
            addresses: HashMap::new(),
            label: Some(format!("{} key share", role)),
        }
    }
}

impl Default for KeyShareMetadata {
    fn default() -> Self {
        Self::new(PartyRole::Agent)
    }
}

/// Agent key share (secret + metadata)
#[derive(Clone, Serialize, Deserialize)]
pub struct AgentKeyShare {
    /// Party ID
    pub party_id: PartyId,
    /// Party role
    pub role: PartyRole,
    /// Secret share (scalar)
    pub secret_share: Scalar,
    /// Aggregated public key
    pub public_key: PublicKey,
    /// Public shares for all parties
    pub public_shares: Vec<PublicKey>,
    /// BIP32 chain code
    pub chain_code: [u8; 32],
    /// Metadata
    pub metadata: KeyShareMetadata,
}

impl AgentKeyShare {
    /// Create a new key share
    pub fn new(
        party_id: PartyId,
        role: PartyRole,
        secret_share: Scalar,
        public_key: PublicKey,
        public_shares: Vec<PublicKey>,
        chain_code: [u8; 32],
    ) -> Self {
        let metadata = KeyShareMetadata::new(role);
        Self {
            party_id,
            role,
            secret_share,
            public_key,
            public_shares,
            chain_code,
            metadata,
        }
    }

    /// Get the Ethereum address for this key share
    pub fn eth_address(&self) -> Result<String> {
        crate::key::derive_eth_address(&self.public_key)
    }

    /// Get the public key bytes (compressed)
    pub fn public_key_bytes(&self) -> Vec<u8> {
        self.public_key.to_encoded_point(true).as_bytes().to_vec()
    }

    /// Get the public key bytes (uncompressed)
    pub fn public_key_bytes_uncompressed(&self) -> Vec<u8> {
        self.public_key.to_encoded_point(false).as_bytes().to_vec()
    }
}

impl std::fmt::Debug for AgentKeyShare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentKeyShare")
            .field("party_id", &self.party_id)
            .field("role", &self.role)
            .field("public_key", &hex::encode(self.public_key_bytes()))
            .field("chain_code", &hex::encode(self.chain_code))
            .field("metadata", &self.metadata)
            .finish()
    }
}

/// ECDSA signature
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// r component (32 bytes)
    pub r: [u8; 32],
    /// s component (32 bytes)
    pub s: [u8; 32],
    /// Recovery ID (0-3)
    pub recid: u8,
}

impl Signature {
    /// Create a new signature
    pub fn new(r: [u8; 32], s: [u8; 32], recid: u8) -> Self {
        Self { r, s, recid }
    }

    /// Get the signature as a 64-byte array (r || s)
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut bytes = [0u8; 64];
        bytes[..32].copy_from_slice(&self.r);
        bytes[32..].copy_from_slice(&self.s);
        bytes
    }

    /// Get the signature as a hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }

    /// Parse from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 64 {
            return Err(Error::Deserialization(
                format!("Invalid signature length: expected 64, got {}", bytes.len())
            ));
        }
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&bytes[..32]);
        s.copy_from_slice(&bytes[32..]);
        Ok(Self { r, s, recid: 0 })
    }
}

/// Transaction request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRequest {
    /// Request ID
    pub request_id: String,
    /// Destination address
    pub to: String,
    /// Amount as string (for precision)
    pub value: String,
    /// Token symbol
    pub token: String,
    /// Chain ID
    pub chain_id: u64,
    /// Nonce
    pub nonce: u64,
    /// Gas price
    pub gas_price: Option<String>,
    /// Gas limit
    pub gas_limit: Option<u64>,
    /// Data payload
    pub data: Option<Vec<u8>>,
    /// Timestamp
    pub timestamp: i64,
}

impl TransactionRequest {
    /// Create a new transaction request
    pub fn new(
        request_id: impl Into<String>,
        to: impl Into<String>,
        value: impl Into<String>,
        token: impl Into<String>,
        chain_id: u64,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            to: to.into(),
            value: value.into(),
            token: token.into(),
            chain_id,
            nonce: 0,
            gas_price: None,
            gas_limit: None,
            data: None,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// Compute the hash of this transaction for signing
    pub fn hash(&self) -> Message {
        use sha3::{Digest, Keccak256};
        
        let mut hasher = Keccak256::new();
        hasher.update(self.request_id.as_bytes());
        hasher.update(self.to.as_bytes());
        hasher.update(self.value.as_bytes());
        hasher.update(self.token.as_bytes());
        hasher.update(&self.chain_id.to_be_bytes());
        hasher.update(&self.nonce.to_be_bytes());
        if let Some(data) = &self.data {
            hasher.update(data);
        }
        hasher.update(&self.timestamp.to_be_bytes());
        
        hasher.finalize().into()
    }
}

/// Compute Keccak256 hash
pub fn keccak256_hash(data: &[u8]) -> Message {
    use sha3::{Digest, Keccak256};
    Keccak256::digest(data).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_party_role() {
        assert_eq!(PartyRole::Agent.party_id(), 0);
        assert_eq!(PartyRole::Steward.party_id(), 1);
        assert_eq!(PartyRole::User.party_id(), 2);
        
        assert_eq!(PartyRole::from_party_id(0).unwrap(), PartyRole::Agent);
        assert_eq!(PartyRole::from_party_id(1).unwrap(), PartyRole::Steward);
        assert_eq!(PartyRole::from_party_id(2).unwrap(), PartyRole::User);
        assert!(PartyRole::from_party_id(3).is_err());
    }

    #[test]
    fn test_signature_bytes() {
        let sig = Signature::new([1u8; 32], [2u8; 32], 0);
        let bytes = sig.to_bytes();
        assert_eq!(bytes.len(), 64);
        assert_eq!(&bytes[..32], &[1u8; 32]);
        assert_eq!(&bytes[32..], &[2u8; 32]);
    }

    #[test]
    fn test_transaction_hash() {
        let tx = TransactionRequest::new("req1", "0x123", "100", "USDC", 1);
        let hash = tx.hash();
        assert_eq!(hash.len(), 32);
    }
}
