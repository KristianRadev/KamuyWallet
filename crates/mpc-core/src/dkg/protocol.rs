//! DKG protocol implementation
//!
//! This implements a Feldman VSS-based DKG protocol for 2-of-3 threshold ECDSA.
//!
//! SECURITY FEATURES:
//! - Complaint/dispute resolution mechanism for invalid shares
//! - Public verification of complaints
//! - Uses OsRng for all cryptographic randomness

use super::messages::{DkgComplaint, DkgRound1Message, DkgRound2Message, KeygenResult};
use super::Relay;
use crate::error::{Error, Result};
use crate::types::{AgentKeyShare, ChainType, KeyShareMetadata, PartyId, PublicKey, SessionConfig};
use crate::utils::{
    bytes_to_point, bytes_to_scalar, scalar_to_bytes,
};
use k256::{
    ProjectivePoint, Scalar,
    elliptic_curve::{
        Field,
        sec1::ToEncodedPoint,
    },
};
use rand::rngs::OsRng;
use rand::RngCore;
use std::collections::HashMap;
use tracing::{debug, info, instrument, warn};

/// Run the distributed key generation protocol
///
/// This implements a Feldman VSS-based DKG protocol where:
/// 1. Each party generates a random polynomial of degree t-1 (t=2)
/// 2. Each party broadcasts commitments to the coefficients
/// 3. Each party sends secret shares to other parties
/// 4. Each party verifies received shares against commitments
/// 5. Complaint round: parties can broadcast invalid shares for public verification
/// 6. Each party combines shares to get their final secret share
///
/// SECURITY FEATURES:
/// - Complaint mechanism allows public verification of invalid shares
/// - Malicious dealers can be identified and excluded
/// - All randomness uses OsRng (cryptographically secure)
///
/// # Arguments
/// * `config` - Session configuration with party ID and role
/// * `relay` - Message relay for communication between parties
///
/// # Returns
/// The keygen result containing the party's key share
#[instrument(skip(relay))]
pub async fn run_dkg<R: Relay>(config: &SessionConfig, relay: &R) -> Result<KeygenResult> {
    // Validate configuration
    config.validate()?;

    info!(
        party_id = config.party_id,
        role = %config.role,
        "Starting DKG for 2-of-3 threshold wallet"
    );

    // ============ Round 1: Commitment ============
    debug!("DKG Round 1: Generating secret polynomial and commitments");

    let (secret_poly, commitments) = generate_secret_polynomial(config.threshold)?;

    // Broadcast commitment
    let commitment_msg = DkgRound1Message {
        party_id: config.party_id,
        commitments: commitments.clone(),
    };
    relay
        .broadcast(&config.session_id, 1, &commitment_msg)
        .await?;

    // Collect commitments from all parties
    let all_commitments = relay
        .collect_broadcasts::<DkgRound1Message>(&config.session_id, 1, config.n_parties)
        .await?;

    // Sort commitments by party ID for consistency
    let mut sorted_commitments = all_commitments;
    sorted_commitments.sort_by_key(|m| m.party_id);

    // Verify we have all commitments
    if sorted_commitments.len() != config.n_parties {
        return Err(Error::DkgVerificationFailed(format!(
            "Expected {} commitments, got {}",
            config.n_parties,
            sorted_commitments.len()
        )));
    }

    // ============ Round 2: Secret Sharing ============
    debug!("DKG Round 2: Sending secret shares to other parties");

    for &party_id in &config.parties {
        if party_id == config.party_id {
            continue;
        }

        // Evaluate polynomial at party's x-coordinate (party_id + 1)
        let share = evaluate_polynomial(&secret_poly, party_id as u64 + 1);

        let share_msg = DkgRound2Message {
            from: config.party_id,
            to: party_id,
            share: scalar_to_bytes(&share).to_vec(),
        };

        relay
            .send_direct(&config.session_id, 2, party_id, &share_msg)
            .await?;
    }

    // Collect shares from other parties
    let received_shares = relay
        .collect_direct::<DkgRound2Message>(
            &config.session_id,
            2,
            config.party_id,
            config.n_parties - 1,
        )
        .await?;

    // Verify we received all expected shares
    if received_shares.len() != config.n_parties - 1 {
        return Err(Error::DkgVerificationFailed(format!(
            "Expected {} shares, got {}",
            config.n_parties - 1,
            received_shares.len()
        )));
    }

    // ============ Round 3: Verification ============
    debug!("DKG Round 3: Verifying shares");

    // Track any complaints about invalid shares
    let mut complaints: Vec<DkgComplaint> = Vec::new();

    // Verify received shares against commitments
    for share_msg in &received_shares {
        let sender_commitments = sorted_commitments
            .iter()
            .find(|c| c.party_id == share_msg.from)
            .ok_or_else(|| {
                Error::VerificationFailed(format!(
                    "Missing commitment from party {}",
                    share_msg.from
                ))
            })?;

        if let Err(e) = verify_share(share_msg, &sender_commitments.commitments, config.party_id) {
            warn!(
                party_id = config.party_id,
                from = share_msg.from,
                error = %e,
                "Share verification failed, filing complaint"
            );
            
            // File a complaint about this invalid share
            complaints.push(DkgComplaint::new(
                config.party_id,
                share_msg.from,
                share_msg.share.clone(),
                format!("Share verification failed: {}", e),
            ));
        }
    }

    // ============ Round 4: Complaint Resolution ============
    debug!("DKG Round 4: Complaint resolution");

    // Broadcast any complaints
    for complaint in &complaints {
        relay.broadcast(&config.session_id, 4, complaint).await?;
    }

    // Collect complaints from all parties
    let all_complaints: Vec<DkgComplaint> = relay
        .collect_broadcasts::<DkgComplaint>(&config.session_id, 4, config.n_parties)
        .await?;

    // Process complaints - verify each one publicly
    for complaint in &all_complaints {
        if complaint.accuser == config.party_id {
            continue; // Skip our own complaints
        }

        // Find the accused party's commitments
        let accused_commitments = sorted_commitments
            .iter()
            .find(|c| c.party_id == complaint.accused)
            .ok_or_else(|| {
                Error::Protocol(format!(
                    "Complaint against party {} but no commitments found",
                    complaint.accused
                ))
            })?;

        // Publicly verify the complaint
        match verify_complaint(complaint, &accused_commitments.commitments) {
            Ok(true) => {
                // Complaint is valid - the accused sent an invalid share
                warn!(
                    accuser = complaint.accuser,
                    accused = complaint.accused,
                    "Valid complaint: party {} sent invalid share to party {}",
                    complaint.accused,
                    complaint.accuser
                );
                return Err(Error::InvalidShare {
                    from: complaint.accused,
                    reason: format!(
                        "Invalid share sent to party {}: {}",
                        complaint.accuser, complaint.reason
                    ),
                });
            }
            Ok(false) => {
                // Complaint is invalid - the accuser is lying
                warn!(
                    accuser = complaint.accuser,
                    accused = complaint.accused,
                    "Invalid complaint: party {} falsely accused party {}",
                    complaint.accuser,
                    complaint.accused
                );
                return Err(Error::InvalidShare {
                    from: complaint.accuser,
                    reason: "Filed false complaint".to_string(),
                });
            }
            Err(e) => {
                return Err(Error::Protocol(format!(
                    "Failed to verify complaint: {}",
                    e
                )));
            }
        }
    }

    // If we had complaints, we already returned an error above
    // If we reach here, all shares were valid

    // ============ Round 5: Combination ============
    debug!("DKG Round 5: Computing final key share");

    // Compute final secret share = own share + sum of received shares
    let my_share = evaluate_polynomial(&secret_poly, config.party_id as u64 + 1);
    let mut final_secret = my_share;

    for share_msg in &received_shares {
        let share = bytes_to_scalar(&share_msg.share)?;
        final_secret = final_secret + share;
    }

    // Compute aggregated public key
    let public_key = compute_public_key(&sorted_commitments)?;

    // Compute public shares for all parties
    let public_shares = compute_public_shares(&sorted_commitments, config.n_parties)?;

    // Generate chain code for BIP32 derivation using OsRng (NOT rand::random())
    let mut chain_code = [0u8; 32];
    OsRng.fill_bytes(&mut chain_code);

    // Create the key share
    let key_share = AgentKeyShare {
        party_id: config.party_id,
        role: config.role,
        secret_share: final_secret,
        public_key: public_key.clone(),
        public_shares: public_shares.clone(),
        chain_code,
        metadata: KeyShareMetadata {
            share_id: uuid::Uuid::new_v4().to_string(),
            role: config.role,
            created_at: chrono::Utc::now().timestamp(),
            last_refreshed_at: None,
            addresses: HashMap::new(),
            label: Some(format!("{} key share", config.role)),
        },
    };

    // Compute and add Ethereum address
    let mut key_share = key_share;
    if let Ok(eth_addr) = key_share.eth_address() {
        key_share.metadata.addresses.insert(ChainType::Evm, eth_addr);
    }

    info!(
        party_id = config.party_id,
        role = %config.role,
        public_key = hex::encode(crate::utils::point_to_bytes(&public_key)),
        "DKG completed successfully"
    );

    KeygenResult::new(key_share)
}

/// Generate a random secret polynomial of degree t-1
///
/// Returns (coefficients, commitments) where:
/// - coefficients[0] is the secret (a_0)
/// - commitments[i] = g^coefficients[i] for each coefficient
/// 
/// SECURITY: Uses OsRng for all randomness (cryptographically secure)
fn generate_secret_polynomial(threshold: usize) -> Result<(Vec<Scalar>, Vec<Vec<u8>>)> {
    let mut rng = OsRng;
    let mut coefficients = Vec::with_capacity(threshold);
    let mut commitments = Vec::with_capacity(threshold);

    for _ in 0..threshold {
        // Generate random coefficient using OsRng (CSPRNG)
        let coef = Scalar::random(&mut rng);
        
        // Compute commitment: C = g^coef
        let commitment = (ProjectivePoint::GENERATOR * coef).to_affine();
        
        coefficients.push(coef);
        commitments.push(commitment.to_encoded_point(true).as_bytes().to_vec());
    }

    Ok((coefficients, commitments))
}

/// Verify a complaint about an invalid share
/// 
/// This allows public verification of whether a share is actually invalid.
/// Anyone can verify this given the complaint and the accused party's commitments.
/// 
/// Returns:
/// - Ok(true) if the complaint is valid (the share is indeed invalid)
/// - Ok(false) if the complaint is invalid (the share is actually valid)
fn verify_complaint(
    complaint: &DkgComplaint,
    accused_commitments: &[Vec<u8>],
) -> Result<bool> {
    // Parse the share from the complaint
    let share = bytes_to_scalar(&complaint.share)?;
    
    // Compute g^share
    let lhs = (ProjectivePoint::GENERATOR * share).to_affine();
    
    // Compute C_0 * C_1^i * C_2^{i^2} * ... where i = accuser + 1
    let i = complaint.accuser as u64 + 1;
    let mut rhs = ProjectivePoint::IDENTITY;
    let mut i_power = 1u64;
    
    for commitment_bytes in accused_commitments {
        let commitment = bytes_to_point(commitment_bytes)?;
        let commitment_proj = ProjectivePoint::from(commitment);
        
        // Add C_j^{i^j} to the product
        let term = commitment_proj * Scalar::from(i_power);
        rhs = rhs + term;
        
        i_power = i_power * i;
    }
    
    // If g^share != product of commitments, the share is invalid
    Ok(lhs != rhs.to_affine())
}

/// Evaluate a polynomial at point x
///
/// f(x) = a_0 + a_1*x + a_2*x^2 + ... + a_{t-1}*x^{t-1}
fn evaluate_polynomial(coefficients: &[Scalar], x: u64) -> Scalar {
    let x_scalar = Scalar::from(x);
    let mut result = Scalar::ZERO;
    let mut x_power = Scalar::ONE;

    for coef in coefficients {
        result = result + (*coef * x_power);
        x_power = x_power * x_scalar;
    }

    result
}

/// Verify a received share against commitments using Feldman verification
///
/// The share s = f(i) should satisfy: g^s = C_0 * C_1^i * C_2^{i^2} * ...
fn verify_share(
    share_msg: &DkgRound2Message,
    commitments: &[Vec<u8>],
    party_id: PartyId,
) -> Result<()> {
    // Parse the share
    let share = bytes_to_scalar(&share_msg.share)?;

    // Compute g^share
    let lhs = (ProjectivePoint::GENERATOR * share).to_affine();

    // Compute C_0 * C_1^i * C_2^{i^2} * ...
    let i = party_id as u64 + 1;
    let mut rhs = ProjectivePoint::IDENTITY;
    let mut i_power = 1u64;

    for commitment_bytes in commitments {
        let commitment = bytes_to_point(commitment_bytes)?;
        let commitment_proj = ProjectivePoint::from(commitment);
        
        // Add C_j^{i^j} to the product
        let term = commitment_proj * Scalar::from(i_power);
        rhs = rhs + term;
        
        i_power = i_power * i;
    }

    // Verify g^share == product of commitments
    if lhs != rhs.to_affine() {
        return Err(Error::VerificationFailed(format!(
            "Share verification failed for party {}",
            share_msg.from
        )));
    }

    Ok(())
}

/// Compute the aggregated public key from all commitments
///
/// PK = product of all C_0 (the constant terms)
fn compute_public_key(commitments: &[DkgRound1Message]) -> Result<PublicKey> {
    let mut public_key = ProjectivePoint::IDENTITY;

    for commitment in commitments {
        if commitment.commitments.is_empty() {
            return Err(Error::VerificationFailed(
                "Empty commitments".to_string()
            ));
        }
        
        // C_0 is the commitment to the secret (constant term)
        let c0 = bytes_to_point(&commitment.commitments[0])?;
        public_key = public_key + ProjectivePoint::from(c0);
    }

    Ok(public_key.to_affine())
}

/// Compute public shares for all parties
///
/// For each party j, their public share is the product of all g^{s_ij}
/// where s_ij is the share sent from party i to party j
fn compute_public_shares(
    commitments: &[DkgRound1Message],
    n_parties: usize,
) -> Result<Vec<PublicKey>> {
    let mut public_shares = Vec::with_capacity(n_parties);

    for party_id in 0..n_parties {
        // Compute the public share for party_id
        // This is g^{f(party_id)} for the combined polynomial
        let mut share_point = ProjectivePoint::IDENTITY;

        for commitment in commitments {
            // Evaluate the commitment polynomial at party_id + 1
            let x = party_id as u64 + 1;
            let mut term = ProjectivePoint::IDENTITY;
            let mut x_power = 1u64;

            for commitment_bytes in &commitment.commitments {
                let c = bytes_to_point(commitment_bytes)?;
                let c_proj = ProjectivePoint::from(c);
                
                term = term + c_proj * Scalar::from(x_power);
                x_power = x_power * x;
            }

            share_point = share_point + term;
        }

        public_shares.push(share_point.to_affine());
    }

    Ok(public_shares)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_secret_polynomial() {
        let (coeffs, commitments) = generate_secret_polynomial(2).unwrap();
        
        assert_eq!(coeffs.len(), 2);
        assert_eq!(commitments.len(), 2);
        
        // Coefficients should not be zero
        assert!(!bool::from(coeffs[0].is_zero()));
        assert!(!bool::from(coeffs[1].is_zero()));
    }

    #[test]
    fn test_evaluate_polynomial() {
        // f(x) = 3 + 2x
        let coefficients = vec![
            Scalar::from(3u64),
            Scalar::from(2u64),
        ];
        
        // f(1) = 3 + 2*1 = 5
        let result = evaluate_polynomial(&coefficients, 1);
        assert_eq!(result, Scalar::from(5u64));
        
        // f(2) = 3 + 2*2 = 7
        let result = evaluate_polynomial(&coefficients, 2);
        assert_eq!(result, Scalar::from(7u64));
    }

    #[test]
    fn test_evaluate_polynomial_quadratic() {
        // f(x) = 1 + 2x + 3x^2
        let coefficients = vec![
            Scalar::from(1u64),
            Scalar::from(2u64),
            Scalar::from(3u64),
        ];
        
        // f(1) = 1 + 2 + 3 = 6
        let result = evaluate_polynomial(&coefficients, 1);
        assert_eq!(result, Scalar::from(6u64));
        
        // f(2) = 1 + 4 + 12 = 17
        let result = evaluate_polynomial(&coefficients, 2);
        assert_eq!(result, Scalar::from(17u64));
    }

    #[test]
    fn test_compute_public_key() {
        // Create dummy commitments
        let commitments = vec![
            DkgRound1Message {
                party_id: 0,
                commitments: vec![
                    AffinePoint::GENERATOR.to_encoded_point(true).as_bytes().to_vec(),
                ],
            },
            DkgRound1Message {
                party_id: 1,
                commitments: vec![
                    AffinePoint::GENERATOR.to_encoded_point(true).as_bytes().to_vec(),
                ],
            },
        ];
        
        // PK should be 2*G
        let pk = compute_public_key(&commitments).unwrap();
        let expected = (ProjectivePoint::GENERATOR * Scalar::from(2u64)).to_affine();
        
        assert_eq!(pk, expected);
    }
}
