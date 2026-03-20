//! Mathematical utilities for MPC

use crate::error::{Error, Result};
use crate::types::{PartyId, Scalar};
use k256::{
    AffinePoint, ProjectivePoint,
    elliptic_curve::{
        Field,
        bigint::U256,
        ops::Reduce,
        sec1::{FromEncodedPoint, ToEncodedPoint},
    },
};

/// Compute the Lagrange coefficient for a party
///
/// λᵢ(0) = Πⱼ≠ᵢ (0 - xⱼ) / (xᵢ - xⱼ)
///
/// where xᵢ = party_id + 1 (to avoid x=0)
pub fn compute_lagrange_coefficient(party_id: PartyId, parties: &[PartyId]) -> Result<Scalar> {
    let x_i = Scalar::from(party_id as u64 + 1);
    
    let mut numerator = Scalar::ONE;
    let mut denominator = Scalar::ONE;
    
    for &other_id in parties {
        if other_id == party_id {
            continue;
        }
        
        let x_j = Scalar::from(other_id as u64 + 1);
        
        // numerator *= (0 - x_j) = -x_j
        numerator = numerator * (Scalar::ZERO - x_j);
        
        // denominator *= (x_i - x_j)
        denominator = denominator * (x_i - x_j);
    }
    
    // λᵢ = numerator / denominator = numerator * denominator^(-1)
    let denom_inv = denominator.invert().into_option()
        .ok_or_else(|| Error::Internal("Failed to invert denominator in Lagrange interpolation".to_string()))?;
    
    Ok(numerator * denom_inv)
}

/// Hash bytes to a scalar
pub fn hash_to_scalar(data: &[u8]) -> Scalar {
    use sha2::{Digest, Sha256};
    
    let hash = Sha256::digest(data);
    <Scalar as Reduce<U256>>::reduce_bytes(&hash.into())
}

/// Hash two scalars to a scalar
pub fn hash_scalars(a: &Scalar, b: &Scalar) -> Scalar {
    let mut data = Vec::with_capacity(64);
    data.extend_from_slice(&a.to_bytes());
    data.extend_from_slice(&b.to_bytes());
    hash_to_scalar(&data)
}

/// Convert a scalar to bytes (32 bytes)
pub fn scalar_to_bytes(scalar: &Scalar) -> [u8; 32] {
    scalar.to_bytes().into()
}

/// Convert bytes to a scalar
pub fn bytes_to_scalar(bytes: &[u8]) -> Result<Scalar> {
    if bytes.len() != 32 {
        return Err(Error::Deserialization(
            format!("Invalid scalar length: expected 32, got {}", bytes.len())
        ));
    }
    
    let bytes_array: [u8; 32] = bytes.try_into().map_err(|_| {
        Error::Deserialization("Failed to convert bytes to array".to_string())
    })?;
    
    Ok(<Scalar as Reduce<U256>>::reduce_bytes(&bytes_array.into()))
}

/// Convert an affine point to bytes (compressed)
pub fn point_to_bytes(point: &AffinePoint) -> Vec<u8> {
    point.to_encoded_point(true).as_bytes().to_vec()
}

/// Convert bytes to an affine point
pub fn bytes_to_point(bytes: &[u8]) -> Result<AffinePoint> {
    let encoded = k256::EncodedPoint::from_bytes(bytes)
        .map_err(|e| Error::Deserialization(format!("Invalid point encoding: {}", e)))?;
    
    Option::from(AffinePoint::from_encoded_point(&encoded))
        .ok_or_else(|| Error::Deserialization("Invalid point".to_string()))
}

/// Add two affine points
pub fn point_add(a: &AffinePoint, b: &AffinePoint) -> AffinePoint {
    let sum = ProjectivePoint::from(a) + ProjectivePoint::from(b);
    sum.to_affine()
}

/// Multiply a point by a scalar
pub fn point_mul(point: &AffinePoint, scalar: &Scalar) -> AffinePoint {
    let product = ProjectivePoint::from(point) * scalar;
    product.to_affine()
}

/// Compute the generator point
pub fn generator() -> AffinePoint {
    AffinePoint::GENERATOR
}

/// Generate a random scalar
pub fn random_scalar() -> Scalar {
    use rand::rngs::OsRng;
    Scalar::random(&mut OsRng)
}

/// Check if a scalar is zero (constant time)
pub fn is_zero(scalar: &Scalar) -> bool {
    scalar.is_zero().into()
}

/// Compute modular inverse of a scalar
pub fn invert_scalar(scalar: &Scalar) -> Result<Scalar> {
    scalar.invert().into_option()
        .ok_or_else(|| Error::Internal("Cannot invert zero scalar".to_string()))
}

/// Compute the hash of a message using Keccak256
pub fn keccak256_hash(data: &[u8]) -> [u8; 32] {
    use sha3::{Digest, Keccak256};
    Keccak256::digest(data).into()
}

/// Compute the hash of a message using SHA256
pub fn sha256_hash(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(data).into()
}

/// Serialize a scalar for transmission
pub fn serialize_scalar(scalar: &Scalar) -> Vec<u8> {
    scalar.to_bytes().to_vec()
}

/// Deserialize a scalar from bytes
pub fn deserialize_scalar(bytes: &[u8]) -> Result<Scalar> {
    bytes_to_scalar(bytes)
}

/// Serialize a point for transmission
pub fn serialize_point(point: &AffinePoint) -> Vec<u8> {
    point_to_bytes(point)
}

/// Deserialize a point from bytes
pub fn deserialize_point(bytes: &[u8]) -> Result<AffinePoint> {
    bytes_to_point(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lagrange_coefficient() {
        // Test with parties 0 and 1
        let parties = vec![0, 1];
        
        let lambda_0 = compute_lagrange_coefficient(0, &parties).unwrap();
        let lambda_1 = compute_lagrange_coefficient(1, &parties).unwrap();
        
        // λ₀ + λ₁ should equal 1 (for x=0)
        let sum = lambda_0 + lambda_1;
        assert_eq!(sum, Scalar::ONE);
    }

    #[test]
    fn test_lagrange_coefficient_three_parties() {
        // Test with parties 0, 1, and 2
        let parties = vec![0, 1, 2];
        
        let lambda_0 = compute_lagrange_coefficient(0, &parties).unwrap();
        let lambda_1 = compute_lagrange_coefficient(1, &parties).unwrap();
        let lambda_2 = compute_lagrange_coefficient(2, &parties).unwrap();
        
        // λ₀ + λ₁ + λ₂ should equal 1 (for x=0)
        let sum = lambda_0 + lambda_1 + lambda_2;
        assert_eq!(sum, Scalar::ONE);
    }

    #[test]
    fn test_hash_to_scalar() {
        let data = b"test data";
        let scalar = hash_to_scalar(data);
        
        // Should not be zero
        assert!(!bool::from(scalar.is_zero()));
        
        // Same input should produce same output
        let scalar2 = hash_to_scalar(data);
        assert_eq!(scalar, scalar2);
        
        // Different input should produce different output
        let scalar3 = hash_to_scalar(b"different data");
        assert_ne!(scalar, scalar3);
    }

    #[test]
    fn test_scalar_bytes_roundtrip() {
        let scalar = random_scalar();
        let bytes = scalar_to_bytes(&scalar);
        let recovered = bytes_to_scalar(&bytes).unwrap();
        
        assert_eq!(scalar, recovered);
    }

    #[test]
    fn test_point_bytes_roundtrip() {
        let point = generator();
        let bytes = point_to_bytes(&point);
        let recovered = bytes_to_point(&bytes).unwrap();
        
        assert_eq!(point, recovered);
    }

    #[test]
    fn test_point_add() {
        let g = generator();
        let two_g = point_add(&g, &g);
        let expected = point_mul(&g, &Scalar::from(2u64));
        
        assert_eq!(two_g, expected);
    }

    #[test]
    fn test_point_mul() {
        let g = generator();
        let scalar = Scalar::from(3u64);
        let three_g = point_mul(&g, &scalar);
        
        // 3G = G + G + G
        let two_g = point_add(&g, &g);
        let expected = point_add(&two_g, &g);
        
        assert_eq!(three_g, expected);
    }

    #[test]
    fn test_invert_scalar() {
        let scalar = Scalar::from(5u64);
        let inv = invert_scalar(&scalar).unwrap();
        
        // scalar * inv should equal 1
        let product = scalar * inv;
        assert_eq!(product, Scalar::ONE);
    }

    #[test]
    fn test_invert_zero_fails() {
        let zero = Scalar::ZERO;
        assert!(invert_scalar(&zero).is_err());
    }

    #[test]
    fn test_keccak256() {
        let data = b"hello world";
        let hash = keccak256_hash(data);
        
        // Should be 32 bytes
        assert_eq!(hash.len(), 32);
        
        // Same input should produce same hash
        let hash2 = keccak256_hash(data);
        assert_eq!(hash, hash2);
    }
}
