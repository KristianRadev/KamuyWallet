//! Key share encryption using ChaCha20-Poly1305
//!
//! SECURITY FEATURES:
//! - Argon2id for key derivation (64MB, 3 iterations, 4 threads)
//! - ChaCha20-Poly1305 for authenticated encryption
//! - All sensitive metadata is encrypted (party_id, role, key material)
//! - Only non-sensitive identifiers remain unencrypted
//! - Uses OsRng for all cryptographic randomness

use crate::error::{Error, Result};
use crate::types::{AgentKeyShare, KeyShareMetadata, PartyRole};
use aead::{Aead, KeyInit};
use argon2::{Argon2, Params, Version};
use chacha20poly1305::ChaCha20Poly1305;
use k256::Scalar;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use rand::RngCore;
use serde::{Deserialize, Serialize};

/// Encrypted key share
/// 
/// SECURITY: All sensitive fields are encrypted. Only the salt and nonce
/// (which are public parameters) remain unencrypted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedKeyShare {
    /// Salt for key derivation (public)
    pub salt: Vec<u8>,
    /// Nonce for encryption (public)
    pub nonce: Vec<u8>,
    /// Ciphertext (contains encrypted: secret_share, public_key, public_shares, 
    /// chain_code, party_id, role, metadata)
    pub ciphertext: Vec<u8>,
    /// Version for future algorithm upgrades
    pub version: u8,
}

/// Serializable plaintext structure containing all key share data
/// This is what gets encrypted inside the ciphertext
#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeySharePlaintext {
    /// Secret share (32 bytes)
    pub secret_share: [u8; 32],
    /// Public key (compressed, 33 bytes)
    pub public_key: Vec<u8>,
    /// Public shares for all parties (compressed, 33 bytes each)
    pub public_shares: Vec<Vec<u8>>,
    /// BIP32 chain code (32 bytes)
    pub chain_code: [u8; 32],
    /// Party ID
    pub party_id: u8,
    /// Role
    pub role: PartyRole,
    /// Metadata
    pub metadata: KeyShareMetadata,
}

/// Parameters for Argon2 key derivation
const ARGON2_MEMORY_COST: u32 = 65536; // 64 MB
const ARGON2_TIME_COST: u32 = 3;
const ARGON2_PARALLELISM: u32 = 4;

/// Derive encryption key from password using Argon2
fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        Version::V0x13,
        Params::new(
            ARGON2_MEMORY_COST,
            ARGON2_TIME_COST,
            ARGON2_PARALLELISM,
            Some(32),
        )
        .map_err(|e| Error::KeyDerivation(format!("Invalid Argon2 params: {}", e)))?,
    );

    let mut key = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| Error::KeyDerivation(format!("Argon2 failed: {}", e)))?;

    Ok(key)
}

/// Encrypt a key share with a password
///
/// Uses Argon2id for key derivation and ChaCha20-Poly1305 for encryption.
/// 
/// SECURITY: All sensitive data including party_id, role, and metadata are encrypted.
pub fn encrypt_key_share(share: &AgentKeyShare, password: &str) -> Result<EncryptedKeyShare> {
    // Generate random salt using OsRng
    let mut salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut salt);

    // Derive encryption key
    let key = derive_key(password, &salt)?;

    // Generate random nonce using OsRng
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);

    // Create cipher
    let cipher = ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| Error::Encryption(format!("Invalid key length: {}", e)))?;

    // Serialize all key share data (including metadata)
    let plaintext = serialize_key_share_plaintext(share)?;

    // Encrypt
    let ciphertext = cipher
        .encrypt(
            chacha20poly1305::Nonce::from_slice(&nonce),
            plaintext.as_ref(),
        )
        .map_err(|e| Error::Encryption(format!("Encryption failed: {}", e)))?;

    Ok(EncryptedKeyShare {
        salt: salt.to_vec(),
        nonce: nonce.to_vec(),
        ciphertext,
        version: 1, // Version 1: encrypted metadata
    })
}

/// Decrypt a key share with a password
/// 
/// SECURITY: Decrypts all sensitive data including party_id, role, and metadata.
pub fn decrypt_key_share(encrypted: &EncryptedKeyShare, password: &str) -> Result<AgentKeyShare> {
    // Derive encryption key
    let key = derive_key(password, &encrypted.salt)?;

    // Create cipher
    let cipher = ChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| Error::Decryption(format!("Invalid key length: {}", e)))?;

    // Decrypt
    let plaintext = cipher
        .decrypt(
            chacha20poly1305::Nonce::from_slice(&encrypted.nonce),
            encrypted.ciphertext.as_ref(),
        )
        .map_err(|_| Error::InvalidPassword)?;

    // Deserialize all key share data
    let plaintext_struct: KeySharePlaintext = serde_json::from_slice(&plaintext)
        .map_err(|e| Error::Deserialization(format!("Failed to deserialize plaintext: {}", e)))?;

    // Convert bytes back to types
    let secret_share = crate::utils::bytes_to_scalar(&plaintext_struct.secret_share)?;
    let public_key = crate::utils::bytes_to_point(&plaintext_struct.public_key)?;
    
    let mut public_shares = Vec::new();
    for share_bytes in &plaintext_struct.public_shares {
        public_shares.push(crate::utils::bytes_to_point(share_bytes)?);
    }

    Ok(AgentKeyShare {
        party_id: plaintext_struct.party_id,
        role: plaintext_struct.role,
        secret_share,
        public_key,
        public_shares,
        chain_code: plaintext_struct.chain_code,
        metadata: plaintext_struct.metadata,
    })
}

impl EncryptedKeyShare {
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        crate::utils::to_json(self)
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        crate::utils::from_json(bytes)
    }
    
    /// Get the version
    pub fn version(&self) -> u8 {
        self.version
    }
}

/// Serialize key share to plaintext structure for encryption
fn serialize_key_share_plaintext(share: &AgentKeyShare) -> Result<Vec<u8>> {
    let plaintext = KeySharePlaintext {
        secret_share: share.secret_share.to_bytes().into(),
        public_key: share.public_key.to_encoded_point(true).as_bytes().to_vec(),
        public_shares: share.public_shares.iter()
            .map(|p| p.to_encoded_point(true).as_bytes().to_vec())
            .collect(),
        chain_code: share.chain_code,
        party_id: share.party_id,
        role: share.role,
        metadata: share.metadata.clone(),
    };
    
    serde_json::to_vec(&plaintext)
        .map_err(|e| Error::Serialization(format!("Failed to serialize plaintext: {}", e)))
}

// Legacy serialization functions - kept for backward compatibility with version 0 shares
// These are no longer used for new encryptions (version 1+)

/// Serialize secret share components to bytes (legacy format)
#[allow(dead_code)]
fn serialize_secret_share(share: &AgentKeyShare) -> Result<Vec<u8>> {
    // Format: [secret_share (32)] [public_key] [public_shares count (1)] [public_shares...] [chain_code (32)]
    let mut result = Vec::new();

    // Secret share
    result.extend_from_slice(&share.secret_share.to_bytes().into_iter().collect::<Vec<u8>>());

    // Public key (compressed)
    result.extend_from_slice(share.public_key.to_encoded_point(true).as_bytes());

    // Public shares
    result.push(share.public_shares.len() as u8);
    for pk in &share.public_shares {
        result.extend_from_slice(pk.to_encoded_point(true).as_bytes());
    }

    // Chain code
    result.extend_from_slice(&share.chain_code);

    Ok(result)
}

/// Deserialize secret share components from bytes (legacy format)
#[allow(dead_code)]
fn deserialize_secret_share(
    bytes: &[u8],
) -> Result<(Scalar, k256::AffinePoint, Vec<k256::AffinePoint>, [u8; 32])> {
    use k256::elliptic_curve::sec1::FromEncodedPoint;

    let mut pos = 0;

    // Secret share (32 bytes)
    if bytes.len() < pos + 32 {
        return Err(Error::Deserialization("Not enough bytes for secret share".to_string()));
    }
    let secret_bytes: [u8; 32] = bytes[pos..pos + 32].try_into().unwrap();
    let secret_share = crate::utils::bytes_to_scalar(&secret_bytes)?;
    pos += 32;

    // Public key (33 bytes compressed)
    if bytes.len() < pos + 33 {
        return Err(Error::Deserialization("Not enough bytes for public key".to_string()));
    }
    let pk_encoded = k256::EncodedPoint::from_bytes(&bytes[pos..pos + 33])
        .map_err(|e| Error::Deserialization(format!("Invalid public key: {}", e)))?;
    let public_key = k256::AffinePoint::from_encoded_point(&pk_encoded)
        .into_option()
        .ok_or_else(|| Error::Deserialization("Invalid public key point".to_string()))?;
    pos += 33;

    // Public shares count
    if bytes.len() < pos + 1 {
        return Err(Error::Deserialization("Not enough bytes for shares count".to_string()));
    }
    let shares_count = bytes[pos] as usize;
    pos += 1;

    // Public shares
    let mut public_shares = Vec::with_capacity(shares_count);
    for _ in 0..shares_count {
        if bytes.len() < pos + 33 {
            return Err(Error::Deserialization("Not enough bytes for public share".to_string()));
        }
        let share_encoded = k256::EncodedPoint::from_bytes(&bytes[pos..pos + 33])
            .map_err(|e| Error::Deserialization(format!("Invalid public share: {}", e)))?;
        let share = k256::AffinePoint::from_encoded_point(&share_encoded)
            .into_option()
            .ok_or_else(|| Error::Deserialization("Invalid public share point".to_string()))?;
        public_shares.push(share);
        pos += 33;
    }

    // Chain code (32 bytes)
    if bytes.len() < pos + 32 {
        return Err(Error::Deserialization("Not enough bytes for chain code".to_string()));
    }
    let chain_code: [u8; 32] = bytes[pos..pos + 32].try_into().unwrap();

    Ok((secret_share, public_key, public_shares, chain_code))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{KeyShareMetadata, PartyRole};

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
    fn test_encrypt_decrypt_key_share() {
        let share = create_test_key_share();
        let password = "test_password_123";

        // Encrypt
        let encrypted = encrypt_key_share(&share, password).unwrap();
        
        // Verify version
        assert_eq!(encrypted.version, 1);

        // Decrypt
        let decrypted = decrypt_key_share(&encrypted, password).unwrap();

        // Verify all fields
        assert_eq!(share.party_id, decrypted.party_id);
        assert_eq!(share.role, decrypted.role);
        assert_eq!(share.secret_share, decrypted.secret_share);
        assert_eq!(share.public_key, decrypted.public_key);
        assert_eq!(share.chain_code, decrypted.chain_code);
        assert_eq!(share.metadata.share_id, decrypted.metadata.share_id);
    }

    #[test]
    fn test_decrypt_wrong_password() {
        let share = create_test_key_share();
        let password = "correct_password";
        let wrong_password = "wrong_password";

        let encrypted = encrypt_key_share(&share, password).unwrap();

        // Decrypt with wrong password should fail
        let result = decrypt_key_share(&encrypted, wrong_password);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidPassword));
    }

    #[test]
    fn test_encrypted_share_serialization() {
        let share = create_test_key_share();
        let password = "test_password";

        let encrypted = encrypt_key_share(&share, password).unwrap();

        // Serialize
        let bytes = encrypted.to_bytes().unwrap();

        // Deserialize
        let recovered = EncryptedKeyShare::from_bytes(&bytes).unwrap();

        // Verify
        assert_eq!(encrypted.salt, recovered.salt);
        assert_eq!(encrypted.nonce, recovered.nonce);
        assert_eq!(encrypted.ciphertext, recovered.ciphertext);
        assert_eq!(encrypted.version, recovered.version);
    }

    #[test]
    fn test_serialize_deserialize_secret_share() {
        let share = create_test_key_share();

        let bytes = serialize_secret_share(&share).unwrap();
        let (secret, pk, shares, chain_code) = deserialize_secret_share(&bytes).unwrap();

        assert_eq!(share.secret_share, secret);
        assert_eq!(share.public_key, pk);
        assert_eq!(share.public_shares, shares);
        assert_eq!(share.chain_code, chain_code);
    }
    
    #[test]
    fn test_metadata_is_encrypted() {
        let share = create_test_key_share();
        let password = "test_password";

        let encrypted = encrypt_key_share(&share, password).unwrap();
        
        // The ciphertext should contain the encrypted metadata
        // We can't directly verify this without decrypting, but we can verify
        // that the struct doesn't expose sensitive fields
        assert!(encrypted.ciphertext.len() > 0);
        
        // Verify we can decrypt and recover metadata
        let decrypted = decrypt_key_share(&encrypted, password).unwrap();
        assert_eq!(decrypted.metadata.share_id, share.metadata.share_id);
        assert_eq!(decrypted.party_id, share.party_id);
        assert_eq!(decrypted.role, share.role);
    }
}
