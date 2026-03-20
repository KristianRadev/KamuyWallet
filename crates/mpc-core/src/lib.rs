//! # Kamuy MPC Core
//!
//! Core library for MPC-based threshold signatures in Kamuy Wallet.
//!
//! This crate provides:
//! - **2-of-3 Threshold ECDSA**: Distributed key generation and signing
//! - **Feldman VSS**: Verifiable secret sharing for DKG
//! - **Key Management**: Encrypted storage and BIP32 derivation
//! - **WASM Support**: Browser and Node.js compatibility
//!
//! ## Architecture
//!
//! The MPC protocol uses a 2-of-3 threshold scheme where:
//! - Agent (Key #1): AI agent that initiates transactions
//! - Steward (Key #2): Policy engine that co-signs compliant transactions
//! - User (Key #3): Ultimate owner for recovery and overrides
//!
//! Any 2 of 3 parties can sign, enabling:
//! - Agent + Steward: Auto-approve policy-compliant transactions
//! - Agent + User: User override for policy violations
//! - Steward + User: Recovery scenario
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use kamuy_mpc_core::{SessionConfig, PartyRole, run_dkg, run_sign};
//!
//! // Configure session
//! let config = SessionConfig {
//!     party_id: 0, // Agent
//!     role: PartyRole::Agent,
//!     n_parties: 3,
//!     threshold: 2,
//!     // ...
//! };
//!
//! // Run DKG to generate key shares
//! let key_share = run_dkg(&config, &relay).await?;
//!
//! // Sign a message (requires 2 parties)
//! let signature = run_sign(&key_share, &message, &[0, 1], &relay).await?;
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

pub mod dkg;
pub mod error;
pub mod key;
pub mod sign;
pub mod types;
pub mod utils;

// Re-export main types
pub use types::{
    AgentKeyShare, ChainType, KeyShareMetadata, Message, PartyId, PartyRole, PublicKey,
    SessionConfig, SessionId, Signature, TransactionRequest, N_PARTIES, THRESHOLD,
    PARTY_AGENT, PARTY_STEWARD, PARTY_USER,
};

// Re-export error types
pub use error::{Error, Result};

// Re-export DKG functions
pub use dkg::{run_dkg, DkgRound1Message, DkgRound2Message, KeygenResult};

// Re-export signing functions
pub use sign::{run_sign, SignRound1Message, SignRound2Message, PartialSignature, PreSignature};

// Re-export key management
pub use key::{
    decrypt_key_share, derive_eth_address, derive_key_share, encrypt_key_share,
    deserialize_key_share, serialize_key_share, EncryptedKeyShare,
};

// Re-export utilities
pub use utils::{compute_lagrange_coefficient, hash_to_scalar, keccak256_hash};

/// Protocol version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Run the complete MPC protocol for wallet creation
///
/// This is a convenience function that runs DKG and returns
/// the key share for the current party.
pub async fn create_wallet<R: dkg::Relay>(
    config: &SessionConfig,
    relay: &R,
) -> Result<AgentKeyShare> {
    let result = run_dkg(config, relay).await?;
    Ok(result.key_share)
}

/// Sign a transaction hash with the given key share
///
/// This is a convenience wrapper around the signing protocol.
pub async fn sign_transaction<R: sign::Relay>(
    key_share: &AgentKeyShare,
    tx_hash: &[u8; 32],
    parties: &[PartyId],
    relay: &R,
) -> Result<Signature> {
    run_sign(key_share, tx_hash, parties, relay).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(N_PARTIES, 3);
        assert_eq!(THRESHOLD, 2);
        assert_eq!(PARTY_AGENT, 0);
        assert_eq!(PARTY_STEWARD, 1);
        assert_eq!(PARTY_USER, 2);
    }

    #[test]
    fn test_party_role_ids() {
        assert_eq!(PartyRole::Agent.party_id(), 0);
        assert_eq!(PartyRole::Steward.party_id(), 1);
        assert_eq!(PartyRole::User.party_id(), 2);
    }
}
