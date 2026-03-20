//! Key share serialization for storage

use crate::error::{Error, Result};
use crate::types::{AgentKeyShare, ChainType, KeyShareMetadata, PartyRole};
use base64::Engine;
use serde::{Deserialize, Serialize};

/// Serializable key share format for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializableKeyShare {
    version: u8,
    party_id: u8,
    role: String,
    secret_share: String, // hex encoded
    public_key: String,   // hex encoded
    public_shares: Vec<String>, // hex encoded
    chain_code: String,   // hex encoded
    metadata: SerializableMetadata,
}

/// Serializable metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializableMetadata {
    share_id: String,
    role: String,
    created_at: i64,
    last_refreshed_at: Option<i64>,
    addresses: Vec<(String, String)>, // (chain_type, address)
    label: Option<String>,
}

impl From<&AgentKeyShare> for SerializableKeyShare {
    fn from(share: &AgentKeyShare) -> Self {
        Self {
            version: 1,
            party_id: share.party_id,
            role: format!("{:?}", share.role),
            secret_share: hex::encode(share.secret_share.to_bytes()),
            public_key: hex::encode(crate::utils::point_to_bytes(&share.public_key)),
            public_shares: share
                .public_shares
                .iter()
                .map(|pk| hex::encode(crate::utils::point_to_bytes(pk)))
                .collect(),
            chain_code: hex::encode(share.chain_code),
            metadata: SerializableMetadata::from(&share.metadata),
        }
    }
}

impl TryFrom<SerializableKeyShare> for AgentKeyShare {
    type Error = Error;

    fn try_from(serialized: SerializableKeyShare) -> Result<Self> {
        // Parse role
        let role = match serialized.role.as_str() {
            "Agent" => PartyRole::Agent,
            "Steward" => PartyRole::Steward,
            "User" => PartyRole::User,
            _ => return Err(Error::Deserialization(format!(
                "Unknown role: {}",
                serialized.role
            ))),
        };

        // Parse secret share
        let secret_bytes = hex::decode(&serialized.secret_share)
            .map_err(|e| Error::Deserialization(format!("Invalid secret share: {}", e)))?;
        let secret_share = crate::utils::bytes_to_scalar(&secret_bytes)?;

        // Parse public key
        let public_key_bytes = hex::decode(&serialized.public_key)
            .map_err(|e| Error::Deserialization(format!("Invalid public key: {}", e)))?;
        let public_key = crate::utils::bytes_to_point(&public_key_bytes)?;

        // Parse public shares
        let mut public_shares = Vec::new();
        for share_hex in &serialized.public_shares {
            let share_bytes = hex::decode(share_hex)
                .map_err(|e| Error::Deserialization(format!("Invalid public share: {}", e)))?;
            public_shares.push(crate::utils::bytes_to_point(&share_bytes)?);
        }

        // Parse chain code
        let chain_code = hex::decode(&serialized.chain_code)
            .map_err(|e| Error::Deserialization(format!("Invalid chain code: {}", e)))?;
        let chain_code: [u8; 32] = chain_code.try_into().map_err(|_| {
            Error::Deserialization("Invalid chain code length".to_string())
        })?;

        // Parse metadata
        let metadata = KeyShareMetadata::try_from(serialized.metadata)?;

        Ok(AgentKeyShare {
            party_id: serialized.party_id,
            role,
            secret_share,
            public_key,
            public_shares,
            chain_code,
            metadata,
        })
    }
}

impl From<&KeyShareMetadata> for SerializableMetadata {
    fn from(metadata: &KeyShareMetadata) -> Self {
        Self {
            share_id: metadata.share_id.clone(),
            role: format!("{:?}", metadata.role),
            created_at: metadata.created_at,
            last_refreshed_at: metadata.last_refreshed_at,
            addresses: metadata
                .addresses
                .iter()
                .map(|(chain, addr)| (format!("{:?}", chain), addr.clone()))
                .collect(),
            label: metadata.label.clone(),
        }
    }
}

impl TryFrom<SerializableMetadata> for KeyShareMetadata {
    type Error = Error;

    fn try_from(serialized: SerializableMetadata) -> Result<Self> {
        // Parse role
        let role = match serialized.role.as_str() {
            "Agent" => PartyRole::Agent,
            "Steward" => PartyRole::Steward,
            "User" => PartyRole::User,
            _ => return Err(Error::Deserialization(format!(
                "Unknown role: {}",
                serialized.role
            ))),
        };

        // Parse addresses
        let mut addresses = std::collections::HashMap::new();
        for (chain_str, addr) in &serialized.addresses {
            let chain = match chain_str.as_str() {
                "Evm" => ChainType::Evm,
                "Bitcoin" => ChainType::Bitcoin,
                "Solana" => ChainType::Solana,
                _ => continue, // Skip unknown chains
            };
            addresses.insert(chain, addr.clone());
        }

        Ok(KeyShareMetadata {
            share_id: serialized.share_id,
            role,
            created_at: serialized.created_at,
            last_refreshed_at: serialized.last_refreshed_at,
            addresses,
            label: serialized.label,
        })
    }
}

/// Serialize a key share to bytes (JSON format)
pub fn serialize_key_share(share: &AgentKeyShare) -> Result<Vec<u8>> {
    let serializable = SerializableKeyShare::from(share);
    serde_json::to_vec(&serializable)
        .map_err(|e| Error::Serialization(format!("Failed to serialize key share: {}", e)))
}

/// Deserialize a key share from bytes
pub fn deserialize_key_share(bytes: &[u8]) -> Result<AgentKeyShare> {
    let serializable: SerializableKeyShare = serde_json::from_slice(bytes)
        .map_err(|e| Error::Deserialization(format!("Failed to deserialize key share: {}", e)))?;
    AgentKeyShare::try_from(serializable)
}

/// Serialize a key share to a JSON string
pub fn serialize_key_share_to_string(share: &AgentKeyShare) -> Result<String> {
    let serializable = SerializableKeyShare::from(share);
    serde_json::to_string(&serializable)
        .map_err(|e| Error::Serialization(format!("Failed to serialize key share: {}", e)))
}

/// Deserialize a key share from a JSON string
pub fn deserialize_key_share_from_string(s: &str) -> Result<AgentKeyShare> {
    let serializable: SerializableKeyShare = serde_json::from_str(s)
        .map_err(|e| Error::Deserialization(format!("Failed to deserialize key share: {}", e)))?;
    AgentKeyShare::try_from(serializable)
}

/// Export key share to a portable format (base64 encoded JSON)
pub fn export_key_share(share: &AgentKeyShare) -> Result<String> {
    let json = serialize_key_share_to_string(share)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(json.as_bytes()))
}

/// Import key share from portable format
pub fn import_key_share(data: &str) -> Result<AgentKeyShare> {
    let json_bytes = base64::engine::general_purpose::STANDARD.decode(data)
        .map_err(|e| Error::Deserialization(format!("Invalid base64: {}", e)))?;
    deserialize_key_share(&json_bytes)
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
    fn test_serialize_deserialize_key_share() {
        let share = create_test_key_share();

        // Serialize
        let bytes = serialize_key_share(&share).unwrap();

        // Deserialize
        let recovered = deserialize_key_share(&bytes).unwrap();

        // Verify
        assert_eq!(share.party_id, recovered.party_id);
        assert_eq!(share.role, recovered.role);
        assert_eq!(share.secret_share, recovered.secret_share);
        assert_eq!(share.public_key, recovered.public_key);
        assert_eq!(share.chain_code, recovered.chain_code);
        assert_eq!(share.metadata.share_id, recovered.metadata.share_id);
    }

    #[test]
    fn test_serialize_to_string() {
        let share = create_test_key_share();

        let json = serialize_key_share_to_string(&share).unwrap();

        // Should be valid JSON
        assert!(json.contains("\"version\":"));
        assert!(json.contains("\"party_id\":"));
        assert!(json.contains("\"role\":"));
    }

    #[test]
    fn test_export_import() {
        let share = create_test_key_share();

        // Export
        let exported = export_key_share(&share).unwrap();

        // Should be base64
        assert!(!exported.contains('"'));
        assert!(!exported.contains('{'));

        // Import
        let recovered = import_key_share(&exported).unwrap();

        // Verify
        assert_eq!(share.party_id, recovered.party_id);
        assert_eq!(share.secret_share, recovered.secret_share);
    }

    #[test]
    fn test_with_addresses() {
        let mut share = create_test_key_share();
        share.metadata.addresses.insert(
            ChainType::Evm,
            "0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf".to_string(),
        );

        let bytes = serialize_key_share(&share).unwrap();
        let recovered = deserialize_key_share(&bytes).unwrap();

        assert_eq!(
            recovered.metadata.addresses.get(&ChainType::Evm),
            Some(&"0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf".to_string())
        );
    }
}
