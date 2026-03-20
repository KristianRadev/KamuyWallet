//! Distributed Signature Generation (DSG) protocol implementation
//!
//! This implements a 2-of-3 threshold ECDSA signing protocol.
//!
//! SECURITY FEATURES:
//! - Schnorr proofs of knowledge for nonce commitments (prevents rogue key attacks)
//! - Partial signature verification before combining (detects malicious signers)
//! - Replay protection via session ID binding to message and parties

use super::messages::{PartialSignature, PreSignature, SignRound1Message, SignRound2Message};
use super::Relay;
use crate::error::{Error, Result};
use crate::types::{AgentKeyShare, Message, PartyId, Scalar, SessionConfig, Signature, THRESHOLD};
use crate::utils::{
    bytes_to_point, bytes_to_scalar, compute_lagrange_coefficient, hash_to_scalar, scalar_to_bytes,
};
use k256::{
    AffinePoint, ProjectivePoint,
    elliptic_curve::{
        bigint::U256,
        group::prime::PrimeCurveAffine,
        ops::Reduce,
        point::AffineCoordinates,
        sec1::ToEncodedPoint,
        Field,
    },
};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};
use tracing::{debug, info, instrument};

/// Schnorr proof of knowledge for nonce commitment
/// 
/// This proves knowledge of the discrete log of R = k * G
/// without revealing k, preventing rogue key attacks.
#[derive(Debug, Clone)]
pub struct SchnorrProof {
    /// Commitment: t = g^r where r is random
    pub t: AffinePoint,
    /// Challenge: e = H(R || T || context)
    pub e: Scalar,
    /// Response: z = r + e * k
    pub z: Scalar,
}

impl SchnorrProof {
    /// Create a new Schnorr proof of knowledge of discrete log
    pub fn prove(k: &Scalar, r_point: &AffinePoint, context: &[u8]) -> Result<Self> {
        let mut rng = OsRng;
        
        // Generate random nonce r
        let r = Scalar::random(&mut rng);
        
        // Compute commitment T = r * G
        let t = (ProjectivePoint::GENERATOR * r).to_affine();
        
        // Compute challenge e = H(R || T || context)
        let mut hasher = Sha256::new();
        hasher.update(r_point.to_encoded_point(true).as_bytes());
        hasher.update(t.to_encoded_point(true).as_bytes());
        hasher.update(context);
        let e = hash_to_scalar(&hasher.finalize());
        
        // Compute response z = r + e * k
        let z = r + e * k;
        
        Ok(Self { t, e, z })
    }
    
    /// Verify the Schnorr proof
    pub fn verify(&self, r_point: &AffinePoint, context: &[u8]) -> Result<bool> {
        // Recompute challenge e' = H(R || T || context)
        let mut hasher = Sha256::new();
        hasher.update(r_point.to_encoded_point(true).as_bytes());
        hasher.update(self.t.to_encoded_point(true).as_bytes());
        hasher.update(context);
        let e_prime = hash_to_scalar(&hasher.finalize());
        
        // Verify e == e'
        if self.e != e_prime {
            return Ok(false);
        }
        
        // Verify: z * G == T + e * R
        let lhs = (ProjectivePoint::GENERATOR * self.z).to_affine();
        let rhs = (ProjectivePoint::from(self.t) + ProjectivePoint::from(r_point) * self.e).to_affine();
        
        Ok(lhs == rhs)
    }
    
    /// Serialize proof to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut result = Vec::new();
        result.extend_from_slice(self.t.to_encoded_point(true).as_bytes());
        result.extend_from_slice(&self.e.to_bytes());
        result.extend_from_slice(&self.z.to_bytes());
        result
    }
    
    /// Deserialize proof from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 33 + 32 + 32 {
            return Err(Error::Deserialization(format!(
                "Invalid SchnorrProof length: expected 97, got {}", bytes.len()
            )));
        }
        
        let t = bytes_to_point(&bytes[0..33])?;
        let e = bytes_to_scalar(&bytes[33..65])?;
        let z = bytes_to_scalar(&bytes[65..97])?;
        
        Ok(Self { t, e, z })
    }
}

/// Extended Round 1 message with Schnorr proof
#[derive(Debug, Clone)]
pub struct SignRound1MessageExtended {
    /// Party ID
    pub party_id: PartyId,
    /// Nonce commitment R_i (compressed point)
    pub commitment: Vec<u8>,
    /// Schnorr proof of knowledge of nonce
    pub proof: SchnorrProof,
    /// Timestamp for replay protection
    pub timestamp: i64,
}

impl SignRound1MessageExtended {
    /// Serialize to bytes for transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut result = Vec::new();
        result.push(self.party_id);
        result.extend_from_slice(&(self.commitment.len() as u32).to_be_bytes());
        result.extend_from_slice(&self.commitment);
        result.extend_from_slice(&self.proof.to_bytes());
        result.extend_from_slice(&self.timestamp.to_be_bytes());
        result
    }
    
    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 1 + 4 + 33 + 97 + 8 {
            return Err(Error::Deserialization("Invalid SignRound1MessageExtended".to_string()));
        }
        
        let party_id = bytes[0];
        let commitment_len = u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as usize;
        let commitment = bytes[5..5 + commitment_len].to_vec();
        let proof = SchnorrProof::from_bytes(&bytes[5 + commitment_len..5 + commitment_len + 97])?;
        let timestamp = i64::from_be_bytes([
            bytes[5 + commitment_len + 97],
            bytes[5 + commitment_len + 98],
            bytes[5 + commitment_len + 99],
            bytes[5 + commitment_len + 100],
            bytes[5 + commitment_len + 101],
            bytes[5 + commitment_len + 102],
            bytes[5 + commitment_len + 103],
            bytes[5 + commitment_len + 104],
        ]);
        
        Ok(Self {
            party_id,
            commitment,
            proof,
            timestamp,
        })
    }
}

/// Generate a cryptographically secure session ID bound to the signing context
/// 
/// This prevents replay attacks by binding the session ID to:
/// - The message being signed
/// - The participating parties
/// - A timestamp
/// - A random nonce
fn generate_session_id(message: &Message, parties: &[PartyId]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    
    // Include message hash
    hasher.update(message);
    
    // Include sorted party IDs (to ensure deterministic ordering)
    let mut sorted_parties = parties.to_vec();
    sorted_parties.sort();
    for party in &sorted_parties {
        hasher.update(&[*party]);
    }
    
    // Include timestamp (current time in seconds since epoch)
    let timestamp = chrono::Utc::now().timestamp();
    hasher.update(&timestamp.to_be_bytes());
    
    // Include random nonce from OsRng (cryptographically secure)
    let mut rng = OsRng;
    let mut random_nonce = [0u8; 32];
    rng.fill_bytes(&mut random_nonce);
    hasher.update(&random_nonce);
    
    let result = hasher.finalize();
    let mut session_id = [0u8; 32];
    session_id.copy_from_slice(&result);
    session_id
}

/// Run the distributed signature generation protocol
///
/// This implements a 2-round threshold ECDSA signing protocol:
/// 1. Each party generates a random nonce k_i and broadcasts R_i = k_i * G with Schnorr proof
/// 2. Parties verify Schnorr proofs, combine R_i to get R, then compute partial signatures
/// 3. Partial signatures are verified and combined using Lagrange interpolation
///
/// SECURITY FEATURES:
/// - Schnorr proofs prevent rogue key attacks
/// - Partial signature verification detects malicious signers
/// - Session ID is bound to message + parties + timestamp for replay protection
///
/// # Arguments
/// * `key_share` - This party's key share
/// * `message` - 32-byte message hash to sign
/// * `parties` - List of participating party IDs (must include this party)
/// * `relay` - Message relay for communication
///
/// # Returns
/// The ECDSA signature
#[instrument(skip(key_share, relay))]
pub async fn run_sign<R: Relay>(
    key_share: &AgentKeyShare,
    message: &Message,
    parties: &[PartyId],
    relay: &R,
) -> Result<Signature> {
    info!(
        party_id = key_share.party_id,
        role = %key_share.role,
        participants = ?parties,
        "Starting threshold signing"
    );

    // Validate threshold
    if parties.len() < THRESHOLD {
        return Err(Error::ThresholdNotMet {
            required: THRESHOLD,
            actual: parties.len(),
        });
    }

    // Verify this party is in the signing set
    if !parties.contains(&key_share.party_id) {
        return Err(Error::NotInSigningSet);
    }

    // Create session configuration with cryptographically secure session ID
    let session_id = generate_session_id(message, parties);
    let _config = SessionConfig {
        session_id,
        n_parties: parties.len(),
        threshold: THRESHOLD,
        party_id: key_share.party_id,
        role: key_share.role,
        parties: parties.to_vec(),
        timeout_secs: 60,
    };

    // ============ Round 1: Generate and broadcast nonce commitment with Schnorr proof ============
    debug!("Signing Round 1: Generating nonce commitment with Schnorr proof");

    let (nonce, commitment) = generate_nonce_commitment()?;
    
    // Generate Schnorr proof of knowledge of nonce
    let context = create_signing_context(message, parties);
    let schnorr_proof = SchnorrProof::prove(&nonce, &commitment, &context)?;
    
    // Create extended round 1 message with proof and timestamp
    let round1_msg = SignRound1MessageExtended {
        party_id: key_share.party_id,
        commitment: commitment.to_encoded_point(true).as_bytes().to_vec(),
        proof: schnorr_proof,
        timestamp: chrono::Utc::now().timestamp(),
    };

    // Serialize and broadcast
    let round1_bytes = round1_msg.to_bytes();
    relay.broadcast(&session_id, 1, &round1_bytes).await?;

    // Collect commitments from all signing parties
    let round1_bytes_list = relay
        .collect_broadcasts::<Vec<u8>>(&session_id, 1, parties.len())
        .await?;
    
    // Deserialize and verify each message
    let mut round1_messages = Vec::new();
    for bytes in &round1_bytes_list {
        let msg = SignRound1MessageExtended::from_bytes(bytes)?;
        
        // Verify timestamp is recent (within 5 minutes to prevent replay)
        let now = chrono::Utc::now().timestamp();
        if (now - msg.timestamp).abs() > 300 {
            return Err(Error::Protocol(format!(
                "Stale timestamp from party {}: {} (now: {})",
                msg.party_id, msg.timestamp, now
            )));
        }
        
        // Reconstruct the commitment point
        let r_i = bytes_to_point(&msg.commitment)?;
        
        // Verify Schnorr proof
        if !msg.proof.verify(&r_i, &context)? {
            return Err(Error::Protocol(format!(
                "Schnorr proof verification failed for party {}",
                msg.party_id
            )));
        }
        
        round1_messages.push(msg);
    }

    // Verify we have all commitments
    if round1_messages.len() != parties.len() {
        return Err(Error::Protocol(format!(
            "Expected {} commitments, got {}",
            parties.len(),
            round1_messages.len()
        )));
    }

    // ============ Compute combined R and r ============
    debug!("Computing combined R point");

    let pre_sig = compute_pre_signature_extended(&round1_messages, nonce, parties)?;

    // ============ Round 2: Create and send partial signature ============
    debug!("Signing Round 2: Creating partial signature");

    let partial = create_partial_signature(
        key_share,
        &pre_sig,
        message,
        parties,
    )?;

    // For simplicity, we broadcast partials and collect them
    // In production, this might go through a coordinator
    let partial_msg = SignRound2Message {
        party_id: key_share.party_id,
        partial: partial.sigma_share.clone(),
    };

    relay.broadcast(&session_id, 2, &partial_msg).await?;

    // Collect partial signatures from all signing parties
    let partial_messages = relay
        .collect_broadcasts::<SignRound2Message>(&session_id, 2, parties.len())
        .await?;

    // Convert to PartialSignature structs and verify each one
    let mut partial_sigs: Vec<PartialSignature> = Vec::new();
    for msg in &partial_messages {
        let partial = PartialSignature {
            party_id: msg.party_id,
            sigma_share: msg.partial.clone(),
        };
        
        // Verify the partial signature before accepting it
        // This detects malicious signers who might try to corrupt the final signature
        let sender_round1 = round1_messages.iter()
            .find(|m| m.party_id == msg.party_id)
            .ok_or_else(|| Error::Protocol(format!(
                "Missing Round 1 message for party {}", msg.party_id
            )))?;
        
        let r_i = bytes_to_point(&sender_round1.commitment)?;
        
        verify_partial_signature(
            &partial,
            &r_i,
            &key_share.public_shares[msg.party_id as usize],
            &pre_sig.r,
            message,
            parties,
        )?;
        
        partial_sigs.push(partial);
    }

    // Verify we have all partials
    if partial_sigs.len() != parties.len() {
        return Err(Error::Protocol(format!(
            "Expected {} partial signatures, got {}",
            parties.len(),
            partial_sigs.len()
        )));
    }

    // ============ Combine partial signatures ============
    debug!("Combining partial signatures");

    let signature = combine_partial_signatures(&pre_sig, &partial_sigs, message)?;

    info!(
        party_id = key_share.party_id,
        r = hex::encode(&signature.r),
        s = hex::encode(&signature.s),
        "Signing completed successfully"
    );

    Ok(signature)
}

/// Generate a random nonce and its commitment
fn generate_nonce_commitment() -> Result<(Scalar, AffinePoint)> {
    let mut rng = OsRng;
    
    // Generate random nonce k using OsRng (cryptographically secure)
    let nonce = Scalar::random(&mut rng);
    
    // Compute commitment R = k * G
    let commitment = (ProjectivePoint::GENERATOR * nonce).to_affine();
    
    Ok((nonce, commitment))
}

/// Create signing context for Schnorr proofs and session ID generation
fn create_signing_context(message: &Message, parties: &[PartyId]) -> Vec<u8> {
    let mut context = Vec::new();
    context.extend_from_slice(message);
    let mut sorted = parties.to_vec();
    sorted.sort();
    for p in &sorted {
        context.push(*p);
    }
    context
}

/// Compute the pre-signature from extended Round 1 messages
fn compute_pre_signature_extended(
    round1_messages: &[SignRound1MessageExtended],
    nonce: Scalar,
    parties: &[PartyId],
) -> Result<PreSignature> {
    // Combine all R_i to get R
    let mut r_point = ProjectivePoint::IDENTITY;
    
    for msg in round1_messages {
        let r_i = bytes_to_point(&msg.commitment)?;
        r_point = r_point + ProjectivePoint::from(r_i);
    }
    
    let r_point = r_point.to_affine();

    // Check for point at infinity
    if r_point.is_identity().into() {
        return Err(Error::Signature("Invalid R point (point at infinity)".to_string()));
    }

    // Compute r = R.x mod n
    let r_bytes: [u8; 32] = r_point.x().into();
    let r = <Scalar as Reduce<U256>>::reduce_bytes(&r_bytes.into());
    
    // Ensure r is not zero
    if r.is_zero().into() {
        return Err(Error::Signature("R.x is zero, retry with different nonce".to_string()));
    }
    
    Ok(PreSignature {
        r_point,
        r,
        nonce,
        parties: parties.to_vec(),
    })
}

/// Compute the pre-signature (combined R and r value)
#[allow(dead_code)]
fn compute_pre_signature(
    round1_messages: &[SignRound1Message],
    nonce: Scalar,
    parties: &[PartyId],
) -> Result<PreSignature> {
    // Combine all R_i to get R
    let mut r_point = ProjectivePoint::IDENTITY;
    
    for msg in round1_messages {
        let r_i = bytes_to_point(&msg.commitment)?;
        r_point = r_point + ProjectivePoint::from(r_i);
    }
    
    let r_point = r_point.to_affine();

    // Check for point at infinity
    if r_point.is_identity().into() {
        return Err(Error::Signature("Invalid R point (point at infinity)".to_string()));
    }

    // Compute r = R.x mod n
    let r_bytes: [u8; 32] = r_point.x().into();
    let r = <Scalar as Reduce<U256>>::reduce_bytes(&r_bytes.into());
    
    // Ensure r is not zero
    if r.is_zero().into() {
        return Err(Error::Signature("R.x is zero, retry with different nonce".to_string()));
    }
    
    Ok(PreSignature {
        r_point,
        r,
        nonce,
        parties: parties.to_vec(),
    })
}

/// Create a partial signature
///
/// sigma_i = k_i * r + lambda_i * s_i * m
///
/// where:
/// - k_i is the nonce
/// - r is the combined R's x-coordinate
/// - lambda_i is the Lagrange coefficient
/// - s_i is the secret share
/// - m is the message hash
fn create_partial_signature(
    key_share: &AgentKeyShare,
    pre_sig: &PreSignature,
    message: &Message,
    parties: &[PartyId],
) -> Result<PartialSignature> {
    // Compute Lagrange coefficient for this party
    let lambda = compute_lagrange_coefficient(key_share.party_id, parties)?;
    
    // Compute message scalar
    let m = <Scalar as Reduce<U256>>::reduce_bytes(&(*message).into());
    
    // Compute sigma_i = k_i * r + lambda_i * s_i * m
    let term1 = pre_sig.nonce * pre_sig.r;
    let term2 = lambda * key_share.secret_share * m;
    let sigma = term1 + term2;
    
    Ok(PartialSignature::new(key_share.party_id, sigma))
}

/// Verify a partial signature from another party
/// 
/// The verification equation is:
/// sigma_i * G == k_i * r * G + lambda_i * PK_i * m
/// 
/// where:
/// - sigma_i is the partial signature
/// - k_i * G = R_i (the nonce commitment from Round 1)
/// - PK_i is the public share of party i
/// - lambda_i is the Lagrange coefficient
/// - r is the combined nonce x-coordinate
/// - m is the message hash
fn verify_partial_signature(
    partial: &PartialSignature,
    r_i: &AffinePoint,  // k_i * G from Round 1
    pk_i: &AffinePoint,  // Public share of party i
    r: &Scalar,          // Combined R.x
    message: &Message,
    parties: &[PartyId],
) -> Result<()> {
    // Parse the partial signature
    let sigma_i = partial.sigma()?;
    
    // Compute Lagrange coefficient for this party
    let lambda_i = compute_lagrange_coefficient(partial.party_id, parties)?;
    
    // Compute message scalar
    let m = <Scalar as Reduce<U256>>::reduce_bytes(&(*message).into());
    
    // Compute LHS: sigma_i * G
    let lhs = (ProjectivePoint::GENERATOR * sigma_i).to_affine();
    
    // Compute RHS: k_i * r * G + lambda_i * PK_i * m
    //            = r * R_i + lambda_i * m * PK_i
    let term1 = ProjectivePoint::from(r_i) * r;
    let term2 = ProjectivePoint::from(pk_i) * (lambda_i * m);
    let rhs = (term1 + term2).to_affine();
    
    // Verify LHS == RHS
    if lhs != rhs {
        return Err(Error::InvalidSignature(format!(
            "Partial signature verification failed for party {}",
            partial.party_id
        )));
    }
    
    Ok(())
}

/// Combine partial signatures using Lagrange interpolation
///
/// sigma = sum(lambda_i * sigma_i) = k * r + s * m
///
/// The final signature is (r, s) where s = sigma mod n
fn combine_partial_signatures(
    pre_sig: &PreSignature,
    partials: &[PartialSignature],
    _message: &Message,
) -> Result<Signature> {
    // Sum up all partial signatures with Lagrange coefficients
    let mut sigma = Scalar::ZERO;
    
    for partial in partials {
        let lambda = compute_lagrange_coefficient(partial.party_id, &pre_sig.parties)?;
        let sigma_i = partial.sigma()?;
        sigma = sigma + lambda * sigma_i;
    }
    
    // Ensure s is in the lower half (BIP-62 compliance)
    // If s > n/2, use s' = n - s
    // n/2 for secp256k1
    let n_half_bytes: [u8; 32] = hex::decode("7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF5D576E7357A4501DDFE92F46681B20A0").unwrap().try_into().unwrap();
    let n_half = <Scalar as Reduce<U256>>::reduce_bytes(&n_half_bytes.into());

    let s = if bool::from(sigma > n_half) {
        Scalar::ZERO - sigma // n - s
    } else {
        sigma
    };
    
    // Convert r and s to bytes
    let r_bytes = scalar_to_bytes(&pre_sig.r);
    let s_bytes = scalar_to_bytes(&s);
    
    // Compute recovery ID
    // recid = (R.y is odd ? 1 : 0) | (s > n/2 ? 2 : 0)
    // Check if y coordinate is odd
    let r_point_bytes = pre_sig.r_point.to_encoded_point(false);
    let is_y_odd = if let Some(y_bytes) = r_point_bytes.y() {
        y_bytes.as_slice().last().unwrap() & 1 == 1
    } else {
        false
    };

    let recid = if is_y_odd { 1 } else { 0 };
    
    Ok(Signature::new(r_bytes, s_bytes, recid))
}

/// Sign a message with a single key share (for testing only)
///
/// This is NOT a threshold signature - it's just for testing the math.
#[cfg(test)]
pub fn sign_single(
    secret_key: &Scalar,
    message: &Message,
) -> Result<Signature> {
    let mut rng = OsRng;
    
    // Generate nonce
    let k = Scalar::random(&mut rng);

    // Compute R = k * G
    let r_point = (ProjectivePoint::GENERATOR * k).to_affine();

    // Compute r = R.x mod n
    let r_point_encoded = r_point.to_encoded_point(false);
    let x_bytes = r_point_encoded.x().unwrap();
    let r_bytes: [u8; 32] = (*x_bytes).into();
    let r = <Scalar as Reduce<U256>>::reduce_bytes(&r_bytes.into());

    // Compute m
    let m = <Scalar as Reduce<U256>>::reduce_bytes(&(*message).into());

    // Compute s = k^{-1} * (m + r * sk)
    let k_inv = k.invert().into_option()
        .ok_or_else(|| Error::Signature("k is zero".to_string()))?;
    let s = k_inv * (m + r * secret_key);

    // Ensure s is in lower half (use reduce_bytes to convert bytes to scalar)
    let n_half_bytes: [u8; 32] = [
        0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0x5d, 0x57, 0x6e, 0x73, 0x57, 0xa4, 0x50, 0x1d,
        0xdf, 0xe9, 0x2f, 0x46, 0x68, 0x1b, 0x20, 0xa0,
    ];
    let n_half = <Scalar as Reduce<U256>>::reduce_bytes(&n_half_bytes.into());

    let s = if s > n_half {
        Scalar::ZERO - s
    } else {
        s
    };

    // Get y coordinate to determine recovery id
    let r_point_encoded = r_point.to_encoded_point(false);
    let is_y_odd = r_point_encoded.y()
        .map(|y| y.as_slice().last().unwrap() & 1 == 1)
        .unwrap_or(false);
    let recid = if is_y_odd { 1 } else { 0 };
    
    Ok(Signature::new(
        scalar_to_bytes(&r),
        scalar_to_bytes(&s),
        recid,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{KeyShareMetadata, PartyRole};
    use crate::sign::messages::SignResult;
    use k256::AffinePoint;

    fn create_test_key_share(party_id: PartyId, secret: u64) -> AgentKeyShare {
        let secret_scalar = Scalar::from(secret);
        let public_key = (ProjectivePoint::GENERATOR * secret_scalar).to_affine();
        
        AgentKeyShare {
            party_id,
            role: PartyRole::from_party_id(party_id).unwrap(),
            secret_share: secret_scalar,
            public_key,
            public_shares: vec![public_key],
            chain_code: [0u8; 32],
            metadata: KeyShareMetadata::new(PartyRole::from_party_id(party_id).unwrap()),
        }
    }

    #[test]
    fn test_generate_nonce_commitment() {
        let (nonce, commitment) = generate_nonce_commitment().unwrap();
        
        // Verify R = k * G
        let expected = (ProjectivePoint::GENERATOR * nonce).to_affine();
        assert_eq!(commitment, expected);
    }

    #[test]
    fn test_create_partial_signature() {
        // Create a simple test case
        let key_share = create_test_key_share(0, 5);
        
        // Create a pre-signature
        let pre_sig = PreSignature {
            r_point: AffinePoint::GENERATOR,
            r: Scalar::from(7u64),
            nonce: Scalar::from(3u64),
            parties: vec![0, 1],
        };
        
        let message = [1u8; 32];
        let parties = vec![0, 1];
        
        let partial = create_partial_signature(&key_share, &pre_sig, &message, &parties).unwrap();
        
        assert_eq!(partial.party_id, 0);
        // sigma = k * r + lambda * s * m
        // We just verify it's not zero
        assert!(!bool::from(partial.sigma().unwrap().is_zero()));
    }

    #[test]
    fn test_sign_single() {
        let secret = Scalar::from(12345u64);
        let message = [1u8; 32];
        
        let signature = sign_single(&secret, &message).unwrap();
        
        // Verify signature is valid
        let public_key = (ProjectivePoint::GENERATOR * secret).to_affine();
        
        let result = SignResult::new(signature, message, vec![0]);
        assert!(result.verify(&public_key).unwrap());
    }

    #[test]
    fn test_signature_verification() {
        // Test that we can verify a signature
        let secret = Scalar::from(42u64);
        let message = b"test message";
        let message_hash = crate::utils::keccak256_hash(message);
        
        let signature = sign_single(&secret, &message_hash).unwrap();
        
        // Compute public key
        let public_key = (ProjectivePoint::GENERATOR * secret).to_affine();
        
        // Verify
        let result = SignResult::new(signature, message_hash, vec![0]);
        assert!(result.verify(&public_key).unwrap());
    }
}
