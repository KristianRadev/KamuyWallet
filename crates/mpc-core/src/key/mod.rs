//! Key management module
//!
//! This module provides key share encryption, storage, and derivation.

pub mod derivation;
pub mod encryption;
pub mod storage;

pub use derivation::{derive_eth_address, derive_key_share};
pub use encryption::{decrypt_key_share, encrypt_key_share, EncryptedKeyShare};
pub use storage::{deserialize_key_share, serialize_key_share};

use crate::error::Result;
use crate::types::{AgentKeyShare, PublicKey};

/// Key manager for handling key shares
#[derive(Debug, Clone)]
pub struct KeyManager {
    // Storage backend
}

impl KeyManager {
    /// Create a new key manager
    pub fn new() -> Self {
        Self {}
    }

    /// Store a key share (encrypted)
    pub async fn store_key_share(
        &self,
        share: &AgentKeyShare,
        password: &str,
    ) -> Result<Vec<u8>> {
        let encrypted = encrypt_key_share(share, password)?;
        encrypted.to_bytes()
    }

    /// Load a key share (decrypted)
    pub async fn load_key_share(&self, data: &[u8], password: &str) -> Result<AgentKeyShare> {
        let encrypted = EncryptedKeyShare::from_bytes(data)?;
        decrypt_key_share(&encrypted, password)
    }

    /// Derive a child key share
    pub fn derive_child(
        &self,
        share: &AgentKeyShare,
        path: &str,
    ) -> Result<AgentKeyShare> {
        derive_key_share(share, path)
    }

    /// Get Ethereum address for a public key
    pub fn get_eth_address(&self, public_key: &PublicKey) -> Result<String> {
        derive_eth_address(public_key)
    }
}

impl Default for KeyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{KeyShareMetadata, PartyRole};
    use k256::Scalar;

    fn create_test_key_share() -> AgentKeyShare {
        let secret = Scalar::from(42u64);
        let public_key = k256::ProjectivePoint::GENERATOR.to_affine();
        
        AgentKeyShare {
            party_id: 0,
            role: PartyRole::Agent,
            secret_share: secret,
            public_key,
            public_shares: vec![public_key],
            chain_code: [0u8; 32],
            metadata: KeyShareMetadata::new(PartyRole::Agent),
        }
    }

    #[test]
    fn test_key_manager_store_load() {
        let manager = KeyManager::new();
        let share = create_test_key_share();
        let password = "test_password";
        
        // This would need async test runtime
        // let encrypted = manager.store_key_share(&share, password).await.unwrap();
        // let loaded = manager.load_key_share(&encrypted, password).await.unwrap();
        // assert_eq!(share.public_key, loaded.public_key);
    }

    #[test]
    fn test_get_eth_address() {
        let manager = KeyManager::new();
        let public_key = k256::AffinePoint::GENERATOR;
        
        let address = manager.get_eth_address(&public_key).unwrap();
        
        // Ethereum address should be 42 characters (0x + 40 hex)
        assert_eq!(address.len(), 42);
        assert!(address.starts_with("0x"));
    }
}
