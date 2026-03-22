//! # Signing Module
//!
//! Coordinates MPC-style signing for Kamuy Wallet transactions.
//!
//! The smart contract expects TWO separate ECDSA signatures from any 2 of 3 parties:
//! - Signature format: [partyIndices: 1 byte][sig1: 65 bytes][sig2: 65 bytes] = 131 bytes
//! - partyIndices: lower nibble = first party, upper nibble = second party
//!
//! For testnet, we use local signing where Steward holds both Agent and Steward keys.

use crate::error::{StewardError, Result};
use k256::ecdsa::{SigningKey, Signature, signature::Signer};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

/// Party indices for MPC signing
pub const PARTY_AGENT: u8 = 0;
pub const PARTY_STEWARD: u8 = 1;
pub const PARTY_USER: u8 = 2;

/// Signing coordinator for Steward
pub struct SigningCoordinator {
    /// Steward private key (for signing)
    steward_key: Arc<Mutex<Option<SigningKey>>>,
    /// Agent private key (for co-signing in local mode)
    agent_key: Arc<Mutex<Option<SigningKey>>>,
    /// User private key (optional, for recovery)
    user_key: Arc<Mutex<Option<SigningKey>>>,
    /// Signing state
    state: Arc<Mutex<SigningState>>,
}

/// Current signing state
#[derive(Debug, Clone, Default)]
struct SigningState {
    /// Completed signatures count
    completed_signatures: usize,
}

/// MPC signature result (two signatures combined)
#[derive(Debug, Clone)]
pub struct MpcSignature {
    /// Party indices byte
    pub party_indices: u8,
    /// First signature (r, s, v)
    pub sig1: [u8; 65],
    /// Second signature (r, s, v)
    pub sig2: [u8; 65],
}

impl MpcSignature {
    /// Convert to bytes for UserOperation
    /// Format: [party_indices: 1][sig1: 65][sig2: 65] = 131 bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(131);
        bytes.push(self.party_indices);
        bytes.extend_from_slice(&self.sig1);
        bytes.extend_from_slice(&self.sig2);
        bytes
    }
    
    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }
}

impl SigningCoordinator {
    /// Create a new signing coordinator
    pub fn new() -> Self {
        Self {
            steward_key: Arc::new(Mutex::new(None)),
            agent_key: Arc::new(Mutex::new(None)),
            user_key: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(SigningState::default())),
        }
    }

    /// Load keys from hex private keys
    pub async fn load_keys_from_hex(
        &self,
        steward_private_hex: &str,
        agent_private_hex: &str,
        user_private_hex: Option<&str>,
    ) -> Result<()> {
        // Parse steward key
        let steward_bytes = hex::decode(steward_private_hex.trim_start_matches("0x"))
            .map_err(|e| StewardError::Validation(format!("Invalid steward key hex: {}", e)))?;
        let steward_key = SigningKey::from_bytes((&steward_bytes[..]).into())
            .map_err(|e| StewardError::Validation(format!("Invalid steward key: {}", e)))?;
        
        // Parse agent key - strip both 0x and ag_ prefixes for backwards compatibility
        let agent_cleaned = agent_private_hex.trim_start_matches("0x").trim_start_matches("ag_");
        let agent_bytes = hex::decode(agent_cleaned)
            .map_err(|e| StewardError::Validation(format!("Invalid agent key hex: {}", e)))?;
        let agent_key = SigningKey::from_bytes((&agent_bytes[..]).into())
            .map_err(|e| StewardError::Validation(format!("Invalid agent key: {}", e)))?;
        
        // Store keys
        *self.steward_key.lock().await = Some(steward_key);
        *self.agent_key.lock().await = Some(agent_key);
        
        // Optionally load user key - strip both 0x and us_ prefixes for backwards compatibility
        if let Some(user_hex) = user_private_hex {
            let user_cleaned = user_hex.trim_start_matches("0x").trim_start_matches("us_");
            let user_bytes = hex::decode(user_cleaned)
                .map_err(|e| StewardError::Validation(format!("Invalid user key hex: {}", e)))?;
            let user_key = SigningKey::from_bytes((&user_bytes[..]).into())
                .map_err(|e| StewardError::Validation(format!("Invalid user key: {}", e)))?;
            *self.user_key.lock().await = Some(user_key);
        }
        
        info!("Signing keys loaded successfully");
        Ok(())
    }

    /// Check if keys are loaded
    pub async fn is_keys_loaded(&self) -> bool {
        self.steward_key.lock().await.is_some() && self.agent_key.lock().await.is_some()
    }

    /// Unload keys (for security)
    pub async fn unload_keys(&self) {
        *self.steward_key.lock().await = None;
        *self.agent_key.lock().await = None;
        *self.user_key.lock().await = None;
        info!("Signing keys unloaded");
    }

    /// Sign a 32-byte hash with both Agent and Steward keys
    /// Returns combined MPC signature
    pub async fn sign_hash(&self, hash: &[u8; 32]) -> Result<MpcSignature> {
        // Get keys
        let steward_key = self.steward_key.lock().await.clone()
            .ok_or(StewardError::KeyNotLoaded)?;
        let agent_key = self.agent_key.lock().await.clone()
            .ok_or(StewardError::KeyNotLoaded)?;

        info!("Signing hash with Agent + Steward keys");

        // Sign with Agent key (party 0)
        let agent_sig: Signature = agent_key.sign(hash);
        let (agent_r, agent_s) = agent_sig.split_bytes();
        let agent_v = recovery_id(agent_key.clone(), hash, &agent_sig)?;
        
        // Sign with Steward key (party 1)
        let steward_sig: Signature = steward_key.sign(hash);
        let (steward_r, steward_s) = steward_sig.split_bytes();
        let steward_v = recovery_id(steward_key.clone(), hash, &steward_sig)?;

        // Build party indices byte: lower nibble = first party (Agent=0), upper nibble = second party (Steward=1)
        // Format: (party2 << 4) | party1
        let party_indices: u8 = (PARTY_STEWARD << 4) | PARTY_AGENT;

        // Build sig1 (Agent) and sig2 (Steward)
        let mut sig1 = [0u8; 65];
        sig1[..32].copy_from_slice(&agent_r);
        sig1[32..64].copy_from_slice(&agent_s);
        sig1[64] = agent_v;

        let mut sig2 = [0u8; 65];
        sig2[..32].copy_from_slice(&steward_r);
        sig2[32..64].copy_from_slice(&steward_s);
        sig2[64] = steward_v;

        // Update stats
        {
            let mut state = self.state.lock().await;
            state.completed_signatures += 1;
        }

        info!(
            party_indices = party_indices,
            "MPC signature created successfully"
        );

        Ok(MpcSignature {
            party_indices,
            sig1,
            sig2,
        })
    }

    /// Sign a transaction hash for UserOperation
    pub async fn sign_user_operation(
        &self,
        user_op_hash: &[u8; 32],
    ) -> Result<MpcSignature> {
        info!("Signing UserOperation hash");
        self.sign_hash(user_op_hash).await
    }

    /// Get signing statistics
    pub async fn stats(&self) -> SigningStats {
        let state = self.state.lock().await;
        SigningStats {
            completed_signatures: state.completed_signatures,
        }
    }
}

/// Calculate recovery ID for signature
fn recovery_id(key: SigningKey, hash: &[u8; 32], sig: &Signature) -> Result<u8> {
    use k256::ecdsa::VerifyingKey;
    
    // Get the verifying key
    let verifying_key = VerifyingKey::from(&key);
    let expected_address = verifying_key.to_encoded_point(false);
    
    // Try recovery IDs 0-3
    for v in 0u8..4 {
        if let Ok(recovered) = k256::ecdsa::RecoveryId::try_from(v) {
            if let Ok(recovered_key) = VerifyingKey::recover_from_prehash(hash, sig, recovered) {
                if recovered_key.to_encoded_point(false) == expected_address {
                    // Ethereum uses v = 27 + recovery_id
                    return Ok(27 + v);
                }
            }
        }
    }
    
    // Default to 27 if we can't determine
    Ok(27)
}

/// Signing statistics
#[derive(Debug, Clone)]
pub struct SigningStats {
    /// Number of completed signatures
    pub completed_signatures: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_signing_coordinator() {
        let coordinator = SigningCoordinator::new();
        
        assert!(!coordinator.is_keys_loaded().await);
        
        // Generate test keys
        let agent_key = k256::SecretKey::random(&mut rand::rngs::OsRng);
        let steward_key = k256::SecretKey::random(&mut rand::rngs::OsRng);
        
        let agent_hex = hex::encode(agent_key.to_bytes());
        let steward_hex = hex::encode(steward_key.to_bytes());
        
        coordinator.load_keys_from_hex(&steward_hex, &agent_hex, None).await.unwrap();
        
        assert!(coordinator.is_keys_loaded().await);
        
        // Test signing
        let hash = [1u8; 32];
        let sig = coordinator.sign_hash(&hash).await.unwrap();
        
        assert_eq!(sig.to_bytes().len(), 131);
        
        coordinator.unload_keys().await;
        assert!(!coordinator.is_keys_loaded().await);
    }
}