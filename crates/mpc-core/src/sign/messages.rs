//! Message types for signing protocol

use crate::types::{PartyId, Scalar, Signature};
use crate::error::Result;
use k256::elliptic_curve::point::AffineCoordinates;
use serde::{Deserialize, Serialize};

/// Round 1 message: Nonce commitment broadcast
///
/// Each party broadcasts their nonce commitment R_i = k_i * G
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignRound1Message {
    /// Party ID
    pub party_id: PartyId,
    /// Nonce commitment R_i (compressed point)
    pub commitment: Vec<u8>,
}

/// Round 2 message: Partial signature
///
/// Each party sends their partial signature contribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignRound2Message {
    /// Party ID
    pub party_id: PartyId,
    /// Partial signature (sigma_i)
    pub partial: Vec<u8>,
}

/// Pre-signature state (after round 1)
#[derive(Debug, Clone)]
pub struct PreSignature {
    /// Combined R point
    pub r_point: k256::AffinePoint,
    /// r = R.x mod n
    pub r: Scalar,
    /// Nonce scalar for this party
    pub nonce: Scalar,
    /// Participating party IDs
    pub parties: Vec<PartyId>,
}

/// Partial signature from a party
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialSignature {
    /// Party ID
    pub party_id: PartyId,
    /// Sigma share (sigma_i = k_i * r + s_i * m)
    pub sigma_share: Vec<u8>,
}

impl PartialSignature {
    /// Create a new partial signature
    pub fn new(party_id: PartyId, sigma_share: Scalar) -> Self {
        Self {
            party_id,
            sigma_share: sigma_share.to_bytes().to_vec(),
        }
    }

    /// Get the sigma share as a scalar
    pub fn sigma(&self) -> Result<Scalar> {
        crate::utils::bytes_to_scalar(&self.sigma_share)
    }
}

/// Signing result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignResult {
    /// The final signature
    pub signature: Signature,
    /// The message that was signed
    pub message: [u8; 32],
    /// Participating parties
    pub parties: Vec<PartyId>,
}

impl SignResult {
    /// Create a new sign result
    pub fn new(signature: Signature, message: [u8; 32], parties: Vec<PartyId>) -> Self {
        Self {
            signature,
            message,
            parties,
        }
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        crate::utils::to_json(self)
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        crate::utils::from_json(bytes)
    }

    /// Get the signature as a hex string
    pub fn signature_hex(&self) -> String {
        self.signature.to_hex()
    }

    /// Verify the signature (for testing)
    pub fn verify(&self, public_key: &k256::AffinePoint) -> Result<bool> {
        use k256::elliptic_curve::ops::Reduce;
        use k256::{Scalar, U256};

        // Reconstruct the signature
        let r = self.signature.r;
        let s = self.signature.s;

        // Parse r and s as scalars
        let r_scalar = <Scalar as Reduce<U256>>::reduce_bytes(&r.into());
        let s_scalar = <Scalar as Reduce<U256>>::reduce_bytes(&s.into());

        // Compute message hash
        let m = <Scalar as Reduce<U256>>::reduce_bytes(&self.message.into());

        // Compute s^{-1}
        let s_inv = s_scalar.invert().into_option()
            .ok_or_else(|| crate::error::Error::Signature("Invalid signature: s is zero".to_string()))?;

        // Compute u1 = m * s^{-1}
        let u1 = m * s_inv;

        // Compute u2 = r * s^{-1}
        let u2 = r_scalar * s_inv;

        // Compute R' = u1 * G + u2 * PK
        let u1_g = k256::ProjectivePoint::GENERATOR * u1;
        let u2_pk = k256::ProjectivePoint::from(public_key) * u2;
        let r_prime = (u1_g + u2_pk).to_affine();

        // Check if R'.x mod n == r
        let r_prime_x: [u8; 32] = r_prime.x().into();
        let r_prime_scalar = <Scalar as Reduce<U256>>::reduce_bytes(&r_prime_x.into());

        Ok(r_prime_scalar == r_scalar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partial_signature() {
        let scalar = Scalar::from(42u64);
        let partial = PartialSignature::new(0, scalar);
        
        assert_eq!(partial.party_id, 0);
        assert_eq!(partial.sigma().unwrap(), scalar);
    }

    #[test]
    fn test_sign_result_serialization() {
        let signature = Signature::new([1u8; 32], [2u8; 32], 0);
        let message = [3u8; 32];
        let parties = vec![0, 1];
        
        let result = SignResult::new(signature, message, parties);
        
        let bytes = result.to_bytes().unwrap();
        let recovered = SignResult::from_bytes(&bytes).unwrap();
        
        assert_eq!(result.signature, recovered.signature);
        assert_eq!(result.message, recovered.message);
        assert_eq!(result.parties, recovered.parties);
    }
}
