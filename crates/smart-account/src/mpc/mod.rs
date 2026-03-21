//! # MPC Signature Utilities
//!
//! Handles 2-of-3 threshold signature formatting for MpcSmartAccount.
//!
//! ## Signature Format
//!
//! The MpcSmartAccount expects signatures in the following format:
//! - [0:1] - Party indices (packed: lower nibble = first party, upper nibble = second party)
//! - [1:66] - First ECDSA signature (r, s, v)
//! - [66:131] - Second ECDSA signature (r, s, v)
//!
//! Party indices:
//! - 0 = Agent
//! - 1 = Steward
//! - 2 = User
//!
//! Examples:
//! - 0x10 = Party 0 (Agent, lower nibble) and Party 1 (Steward, upper nibble)
//! - 0x20 = Party 0 (Agent) and Party 2 (User)
//! - 0x21 = Party 1 (Steward) and Party 2 (User)

use ethers::types::{Address, H256};
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use sha3::{Digest, Keccak256};

/// MPC Party identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Party {
    /// Agent (Key #1) - AI agent that initiates transactions
    Agent = 0,
    /// Steward (Key #2) - Policy engine that co-signs compliant transactions
    Steward = 1,
    /// User (Key #3) - Ultimate owner for recovery and overrides
    User = 2,
}

impl Party {
    /// Get party from index
    pub fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Party::Agent),
            1 => Some(Party::Steward),
            2 => Some(Party::User),
            _ => None,
        }
    }

    /// Get party index
    pub fn index(&self) -> u8 {
        *self as u8
    }
}

/// MPC Signature containing two party signatures
#[derive(Debug, Clone)]
pub struct MpcSignature {
    /// First signing party
    pub party1: Party,
    /// Second signing party
    pub party2: Party,
    /// First ECDSA signature (r, s, v)
    pub signature1: [u8; 65],
    /// Second ECDSA signature (r, s, v)
    pub signature2: [u8; 65],
}

impl MpcSignature {
    /// Create a new MPC signature
    pub fn new(party1: Party, party2: Party, sig1: [u8; 65], sig2: [u8; 65]) -> Self {
        Self {
            party1,
            party2,
            signature1: sig1,
            signature2: sig2,
        }
    }

    /// Encode the signature for MpcSmartAccount
    /// Returns 131 bytes: [party_indices: 1] [sig1: 65] [sig2: 65]
    pub fn encode(&self) -> Vec<u8> {
        let party_indices = self.party1.index() | (self.party2.index() << 4);
        let mut encoded = Vec::with_capacity(131);
        encoded.push(party_indices);
        encoded.extend_from_slice(&self.signature1);
        encoded.extend_from_slice(&self.signature2);
        encoded
    }

    /// Decode a signature from bytes
    pub fn decode(bytes: &[u8]) -> crate::Result<Self> {
        if bytes.len() != 131 {
            return Err(crate::SmartAccountError::Signature(
                format!("Invalid MPC signature length: expected 131, got {}", bytes.len())
            ));
        }

        let party_indices = bytes[0];
        let party1_idx = party_indices & 0x0F;
        let party2_idx = (party_indices >> 4) & 0x0F;

        let party1 = Party::from_index(party1_idx)
            .ok_or_else(|| crate::SmartAccountError::Signature(
                format!("Invalid party index: {}", party1_idx)
            ))?;

        let party2 = Party::from_index(party2_idx)
            .ok_or_else(|| crate::SmartAccountError::Signature(
                format!("Invalid party index: {}", party2_idx)
            ))?;

        let mut signature1 = [0u8; 65];
        let mut signature2 = [0u8; 65];
        signature1.copy_from_slice(&bytes[1..66]);
        signature2.copy_from_slice(&bytes[66..131]);

        Ok(Self {
            party1,
            party2,
            signature1,
            signature2,
        })
    }
}

/// Sign a message hash with a private key (ECDSA)
/// Returns 65 bytes: r (32) + s (32) + v (1)
pub fn sign_message(private_key: &[u8; 32], message_hash: &H256) -> crate::Result<[u8; 65]> {
    use k256::ecdsa::SigningKey;

    let signing_key = SigningKey::from_bytes(private_key.into())
        .map_err(|e| crate::SmartAccountError::Signature(format!("Invalid private key: {}", e)))?;

    let (signature, recovery_id) = signing_key
        .sign_prehash_recoverable(message_hash.as_bytes())
        .map_err(|e| crate::SmartAccountError::Signature(format!("Signing failed: {}", e)))?;

    let mut sig_bytes = [0u8; 65];
    sig_bytes[..32].copy_from_slice(signature.r().to_bytes().as_slice());
    sig_bytes[32..64].copy_from_slice(signature.s().to_bytes().as_slice());
    sig_bytes[64] = recovery_id.to_byte() + 27; // Ethereum recovery offset

    Ok(sig_bytes)
}

/// Recover the signer address from a signature
pub fn recover_address(message_hash: &H256, signature: &[u8; 65]) -> crate::Result<Address> {
    use k256::elliptic_curve::sec1::ToEncodedPoint;

    let r = k256::FieldBytes::from_slice(&signature[..32]);
    let s = k256::FieldBytes::from_slice(&signature[32..64]);
    let v = signature[64];

    let recovery_id = RecoveryId::from_byte(v - 27)
        .ok_or_else(|| crate::SmartAccountError::Signature("Invalid recovery id".to_string()))?;

    let sig = Signature::from_scalars(*r, *s)
        .map_err(|e| crate::SmartAccountError::Signature(format!("Invalid signature: {}", e)))?;

    let verifying_key = VerifyingKey::recover_from_prehash(message_hash.as_bytes(), &sig, recovery_id)
        .map_err(|e| crate::SmartAccountError::Signature(format!("Recovery failed: {}", e)))?;

    // Convert to Ethereum address
    let public_key = verifying_key.to_encoded_point(false);
    let public_key_bytes = public_key.as_bytes();

    // Skip the first byte (0x04 prefix for uncompressed)
    let hash = Keccak256::digest(&public_key_bytes[1..]);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..]);

    Ok(Address::from(address))
}

/// Derive Ethereum address from a private key
pub fn derive_address(private_key: &[u8; 32]) -> crate::Result<Address> {
    use k256::ecdsa::SigningKey;
    use k256::elliptic_curve::sec1::ToEncodedPoint;

    let signing_key = SigningKey::from_bytes(private_key.into())
        .map_err(|e| crate::SmartAccountError::Signature(format!("Invalid private key: {}", e)))?;

    let verifying_key = signing_key.verifying_key();
    let public_key = verifying_key.to_encoded_point(false);
    let public_key_bytes = public_key.as_bytes();

    let hash = Keccak256::digest(&public_key_bytes[1..]);
    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..]);

    Ok(Address::from(address))
}

/// Test MPC key set for development
#[derive(Debug, Clone)]
pub struct TestMpcKeys {
    /// Agent private key (32 bytes)
    pub agent_key: [u8; 32],
    /// Steward private key (32 bytes)
    pub steward_key: [u8; 32],
    /// User private key (32 bytes)
    pub user_key: [u8; 32],
    /// Agent address
    pub agent_address: Address,
    /// Steward address
    pub steward_address: Address,
    /// User address
    pub user_address: Address,
}

impl TestMpcKeys {
    /// Generate deterministic test keys from a seed
    /// WARNING: Only use for testing, never in production!
    pub fn from_seed(seed: &[u8]) -> Self {
        use sha2::{Sha256, Digest as Sha256Digest};

        // Generate deterministic keys from seed
        let agent_seed = Sha256::digest(&[seed, b"agent"].concat());
        let steward_seed = Sha256::digest(&[seed, b"steward"].concat());
        let user_seed = Sha256::digest(&[seed, b"user"].concat());

        let mut agent_key = [0u8; 32];
        let mut steward_key = [0u8; 32];
        let mut user_key = [0u8; 32];

        agent_key.copy_from_slice(&agent_seed);
        steward_key.copy_from_slice(&steward_seed);
        user_key.copy_from_slice(&user_seed);

        let agent_address = derive_address(&agent_key).expect("Valid agent key");
        let steward_address = derive_address(&steward_key).expect("Valid steward key");
        let user_address = derive_address(&user_key).expect("Valid user key");

        Self {
            agent_key,
            steward_key,
            user_key,
            agent_address,
            steward_address,
            user_address,
        }
    }

    /// Get the key for a party
    pub fn get_key(&self, party: Party) -> &[u8; 32] {
        match party {
            Party::Agent => &self.agent_key,
            Party::Steward => &self.steward_key,
            Party::User => &self.user_key,
        }
    }

    /// Get the address for a party
    pub fn get_address(&self, party: Party) -> Address {
        match party {
            Party::Agent => self.agent_address,
            Party::Steward => self.steward_address,
            Party::User => self.user_address,
        }
    }

    /// Sign a message hash with two parties
    pub fn sign_with(&self, party1: Party, party2: Party, message_hash: &H256) -> crate::Result<MpcSignature> {
        let sig1 = sign_message(self.get_key(party1), message_hash)?;
        let sig2 = sign_message(self.get_key(party2), message_hash)?;

        Ok(MpcSignature::new(party1, party2, sig1, sig2))
    }

    /// Create Agent + Steward signature (for auto-approved transactions)
    pub fn sign_agent_steward(&self, message_hash: &H256) -> crate::Result<MpcSignature> {
        self.sign_with(Party::Agent, Party::Steward, message_hash)
    }

    /// Create Agent + User signature (for user override)
    pub fn sign_agent_user(&self, message_hash: &H256) -> crate::Result<MpcSignature> {
        self.sign_with(Party::Agent, Party::User, message_hash)
    }

    /// Create Steward + User signature (for recovery)
    pub fn sign_steward_user(&self, message_hash: &H256) -> crate::Result<MpcSignature> {
        self.sign_with(Party::Steward, Party::User, message_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers::types::H256;
    use rand::Rng;

    #[test]
    fn test_mpc_signature_encode_decode() {
        let sig1 = [0x01u8; 65];
        let sig2 = [0x02u8; 65];
        let mpc_sig = MpcSignature::new(Party::Agent, Party::Steward, sig1, sig2);

        let encoded = mpc_sig.encode();
        assert_eq!(encoded.len(), 131);
        // Agent (0) | Steward (1) << 4 = 0 | 16 = 0x10
        assert_eq!(encoded[0], 0x10);

        let decoded = MpcSignature::decode(&encoded).unwrap();
        assert_eq!(decoded.party1, Party::Agent);
        assert_eq!(decoded.party2, Party::Steward);
        assert_eq!(decoded.signature1, sig1);
        assert_eq!(decoded.signature2, sig2);
    }

    #[test]
    fn test_test_key_generation() {
        let seed = b"test_seed_12345";
        let keys = TestMpcKeys::from_seed(seed);

        // Verify addresses are different
        assert_ne!(keys.agent_address, keys.steward_address);
        assert_ne!(keys.agent_address, keys.user_address);
        assert_ne!(keys.steward_address, keys.user_address);

        // Verify deterministic
        let keys2 = TestMpcKeys::from_seed(seed);
        assert_eq!(keys.agent_address, keys2.agent_address);
        assert_eq!(keys.steward_address, keys2.steward_address);
        assert_eq!(keys.user_address, keys2.user_address);
    }

    #[test]
    fn test_sign_and_recover() {
        let keys = TestMpcKeys::from_seed(b"test_sign");
        let message_hash = H256::random();

        let sig = sign_message(&keys.agent_key, &message_hash).unwrap();
        let recovered = recover_address(&message_hash, &sig).unwrap();

        assert_eq!(recovered, keys.agent_address);
    }
}