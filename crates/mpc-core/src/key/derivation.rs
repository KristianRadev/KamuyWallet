//! BIP32 key derivation for MPC key shares

use crate::error::{Error, Result};
use crate::types::AgentKeyShare;
use crate::utils::point_add;
use hmac::Mac;
use k256::{
    AffinePoint, ProjectivePoint, Scalar,
    elliptic_curve::{
        bigint::U256,
        ops::Reduce,
        sec1::ToEncodedPoint,
    },
};
use tiny_keccak::{Hasher, Keccak};

/// Derive a child key share using BIP32
///
/// This implements hierarchical deterministic key derivation for threshold keys.
/// Each party can independently derive child shares without communication.
///
/// # Arguments
/// * `share` - Parent key share
/// * `path` - Derivation path (e.g., "m/44'/60'/0'/0/0")
///
/// # Returns
/// The derived child key share
pub fn derive_key_share(share: &AgentKeyShare, path: &str) -> Result<AgentKeyShare> {
    // Parse path
    let indices = parse_derivation_path(path)?;

    // Start with parent share
    let mut current = share.clone();

    // Derive through each level
    for index in indices {
        current = derive_child_share(&current, index)?;
    }

    Ok(current)
}

/// Derive a direct child share
fn derive_child_share(share: &AgentKeyShare, index: u32) -> Result<AgentKeyShare> {
    // HMAC-SHA512(parent_chain_code, parent_public_key || index)
    let mut data = Vec::new();
    data.extend_from_slice(share.public_key.to_encoded_point(true).as_bytes());
    data.extend_from_slice(&index.to_be_bytes());

    let mut hmac = hmac::Hmac::<sha2::Sha512>::new_from_slice(&share.chain_code)
        .map_err(|_| Error::KeyDerivation("HMAC error: invalid key length".to_string()))?;
    hmac.update(&data);
    let result = hmac.finalize().into_bytes();

    // Split into left (32 bytes) and right (32 bytes)
    let left = &result[..32];
    let right = &result[32..];

    // Left becomes the tweak
    let mut left_array = [0u8; 32];
    left_array.copy_from_slice(left);
    let tweak = <Scalar as Reduce<U256>>::reduce_bytes(&left_array.into());

    // Right becomes the new chain code
    let mut new_chain_code = [0u8; 32];
    new_chain_code.copy_from_slice(right);

    // Derive new secret share: s' = s + tweak
    let new_secret = share.secret_share + tweak;

    // Derive new public key: PK' = PK + G*tweak
    let tweak_point = (ProjectivePoint::GENERATOR * tweak).to_affine();
    let new_public_key = point_add(&share.public_key, &tweak_point);

    // Derive new public shares
    let mut new_public_shares = Vec::with_capacity(share.public_shares.len());
    for pk in &share.public_shares {
        let new_pk = point_add(pk, &tweak_point);
        new_public_shares.push(new_pk);
    }

    // Create child share
    let mut child = AgentKeyShare {
        party_id: share.party_id,
        role: share.role,
        secret_share: new_secret,
        public_key: new_public_key,
        public_shares: new_public_shares,
        chain_code: new_chain_code,
        metadata: share.metadata.clone(),
    };

    // Update metadata
    child.metadata.last_refreshed_at = Some(chrono::Utc::now().timestamp());

    Ok(child)
}

/// Parse a BIP32 derivation path
///
/// Supports paths like:
/// - "m/44'/60'/0'/0/0" (hardened and normal indices)
/// - "44'/60'/0'/0/0" (without leading m)
fn parse_derivation_path(path: &str) -> Result<Vec<u32>> {
    let path = path.trim();
    let path = if path.starts_with("m/") {
        &path[2..]
    } else if path.starts_with("m") {
        &path[1..]
    } else {
        path
    };

    if path.is_empty() {
        return Ok(vec![]);
    }

    let mut indices = Vec::new();

    for part in path.split('/') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        // Check for hardened index (')
        let (num_str, is_hardened) = if part.ends_with('\'') || part.ends_with("h") {
            (&part[..part.len() - 1], true)
        } else {
            (part, false)
        };

        // Parse number
        let index: u32 = num_str
            .parse()
            .map_err(|e| Error::InvalidDerivationPath(format!("Invalid index '{}': {}", part, e)))?;

        // Apply hardened offset
        let final_index = if is_hardened {
            if index >= 0x80000000 {
                return Err(Error::InvalidDerivationPath(
                    format!("Hardened index {} too large", index)
                ));
            }
            index + 0x80000000
        } else {
            index
        };

        indices.push(final_index);
    }

    Ok(indices)
}

/// Derive Ethereum address from public key
///
/// Ethereum address = last 20 bytes of Keccak256(public_key)[12..]
pub fn derive_eth_address(public_key: &AffinePoint) -> Result<String> {
    // Get uncompressed public key (65 bytes: 0x04 || x || y)
    let uncompressed = public_key.to_encoded_point(false);
    let pk_bytes = uncompressed.as_bytes();

    // Remove the 0x04 prefix (64 bytes remaining)
    let pk_64 = &pk_bytes[1..];

    // Keccak256 hash
    let hash = keccak256_hash(pk_64);

    // Take last 20 bytes
    let address_bytes = &hash[12..];

    // Convert to hex string with 0x prefix
    let address = format!("0x{}", hex::encode(address_bytes));

    // Apply EIP-55 checksum
    Ok(to_checksum_address(&address))
}

/// Compute Keccak256 hash
fn keccak256_hash(data: &[u8]) -> [u8; 32] {
    let mut output = [0u8; 32];
    let mut hasher = Keccak::v256();
    hasher.update(data);
    hasher.finalize(&mut output);
    output
}

/// Apply EIP-55 checksum to Ethereum address
fn to_checksum_address(address: &str) -> String {
    let address = address.to_lowercase();
    let address_clean = if address.starts_with("0x") {
        &address[2..]
    } else {
        &address
    };

    let hash = keccak256_hash(address_clean.as_bytes());
    let hash_hex = hex::encode(hash);

    let mut result = String::with_capacity(42);
    result.push_str("0x");

    for (i, c) in address_clean.chars().enumerate() {
        if c.is_ascii_digit() {
            result.push(c);
        } else {
            // Check if corresponding hash nibble is >= 8
            let hash_nibble = hash_hex.chars().nth(i).unwrap();
            let hash_val = hash_nibble.to_digit(16).unwrap();
            if hash_val >= 8 {
                result.push(c.to_ascii_uppercase());
            } else {
                result.push(c);
            }
        }
    }

    result
}

/// Derive Bitcoin address from public key (P2PKH)
pub fn derive_btc_address(public_key: &AffinePoint) -> Result<String> {
    // Get compressed public key
    let compressed = public_key.to_encoded_point(true);
    let pk_bytes = compressed.as_bytes();

    // SHA256 then RIPEMD160 (hash160)
    use sha2::{Digest, Sha256};
    let sha_hash = Sha256::digest(pk_bytes);
    let ripemd_hash = ripemd::Ripemd160::digest(&sha_hash);

    // Add version byte (0x00 for mainnet P2PKH)
    let mut versioned = vec![0x00];
    versioned.extend_from_slice(&ripemd_hash);

    // Base58Check encode
    Ok(base58_encode_check(&versioned))
}

/// Base58Check encoding
fn base58_encode_check(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    // Double SHA256 for checksum
    let hash1 = Sha256::digest(data);
    let hash2 = Sha256::digest(&hash1);
    let checksum = &hash2[..4];

    // Append checksum
    let mut full = data.to_vec();
    full.extend_from_slice(checksum);

    // Base58 encode
    bs58::encode(full).into_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{KeyShareMetadata, PartyRole};

    fn create_test_key_share() -> AgentKeyShare {
        let secret = Scalar::from(42u64);
        let public_key = (ProjectivePoint::GENERATOR * secret).to_affine();

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
    fn test_parse_derivation_path() {
        // Standard path
        let indices = parse_derivation_path("m/44'/60'/0'/0/0").unwrap();
        assert_eq!(indices, vec![
            44 + 0x80000000,
            60 + 0x80000000,
            0 + 0x80000000,
            0,
            0
        ]);

        // Without leading m
        let indices = parse_derivation_path("44'/60'/0'/0/0").unwrap();
        assert_eq!(indices, vec![
            44 + 0x80000000,
            60 + 0x80000000,
            0 + 0x80000000,
            0,
            0
        ]);

        // Mixed hardened and normal
        let indices = parse_derivation_path("m/44'/0'/0/1").unwrap();
        assert_eq!(indices, vec![
            44 + 0x80000000,
            0 + 0x80000000,
            0,
            1
        ]);

        // Empty path
        let indices = parse_derivation_path("m").unwrap();
        assert!(indices.is_empty());

        // Using 'h' for hardened
        let indices = parse_derivation_path("m/44h/60h/0h/0/0").unwrap();
        assert_eq!(indices, vec![
            44 + 0x80000000,
            60 + 0x80000000,
            0 + 0x80000000,
            0,
            0
        ]);
    }

    #[test]
    fn test_derive_child_share() {
        let parent = create_test_key_share();

        // Derive child at index 0
        let child = derive_child_share(&parent, 0).unwrap();

        // Child should have different secret and public key
        assert_ne!(child.secret_share, parent.secret_share);
        assert_ne!(child.public_key, parent.public_key);
        assert_ne!(child.chain_code, parent.chain_code);

        // But same party ID and role
        assert_eq!(child.party_id, parent.party_id);
        assert_eq!(child.role, parent.role);

        // Verify: G * s' = PK'
        let computed_pk = (ProjectivePoint::GENERATOR * child.secret_share).to_affine();
        assert_eq!(computed_pk, child.public_key);
    }

    #[test]
    fn test_derive_key_share() {
        let parent = create_test_key_share();

        // Derive along a path
        let child = derive_key_share(&parent, "m/44'/60'/0'/0/0").unwrap();

        // Should be different from parent
        assert_ne!(child.secret_share, parent.secret_share);
        assert_ne!(child.public_key, parent.public_key);
    }

    #[test]
    fn test_derive_eth_address() {
        // Use a known test vector
        // Private key: 0x0000000000000000000000000000000000000000000000000000000000000001
        // Public key: 0x0279BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
        // Address: 0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf

        let secret = Scalar::ONE;
        let public_key = (ProjectivePoint::GENERATOR * secret).to_affine();

        let address = derive_eth_address(&public_key).unwrap();

        // Should be valid format
        assert_eq!(address.len(), 42);
        assert!(address.starts_with("0x"));

        // Known test vector (case-insensitive comparison)
        let expected = "0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf";
        assert_eq!(address.to_lowercase(), expected.to_lowercase());
    }

    #[test]
    fn test_to_checksum_address() {
        let lowercase = "0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed";
        let checksummed = to_checksum_address(lowercase);
        assert_eq!(checksummed, "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed");
    }

    #[test]
    fn test_keccak256() {
        let data = b"hello";
        let hash = keccak256_hash(data);

        // Known hash for "hello"
        let expected_hex = "1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8";
        assert_eq!(hex::encode(hash), expected_hex);
    }
}
