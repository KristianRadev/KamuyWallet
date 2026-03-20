//! # Kamuy Wallet Core
//!
//! Core wallet library for Kamuy Wallet.
//!
//! This crate provides:
//! - Policy engine for transaction validation
//! - Transaction builder for EVM chains
//! - Integration with MPC core for signing
//! - Wallet state management

pub mod policy;
pub mod transaction;
pub mod wallet;

pub use policy::{PolicyConfig, PolicyDecision, PolicyEngine, PolicyRule};
pub use transaction::{Transaction, TransactionBuilder, TransactionStatus};
pub use wallet::{Wallet, WalletConfig, WalletState};

// Re-export MPC types
pub use kamuy_mpc_core::{
    AgentKeyShare, ChainType, PartyId, PartyRole, PublicKey, SessionConfig, Signature,
    PARTY_AGENT, PARTY_STEWARD, PARTY_USER, N_PARTIES, THRESHOLD,
};

/// Protocol version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}
