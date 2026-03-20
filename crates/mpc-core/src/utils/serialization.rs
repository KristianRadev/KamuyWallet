//! Serialization utilities for MPC

use base64::Engine;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

/// Serialize to JSON bytes
pub fn to_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(value)
        .map_err(|e| Error::Serialization(format!("JSON serialization failed: {}", e)))
}

/// Deserialize from JSON bytes
pub fn from_json<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes)
        .map_err(|e| Error::Deserialization(format!("JSON deserialization failed: {}", e)))
}

/// Serialize to JSON string
pub fn to_json_string<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|e| Error::Serialization(format!("JSON serialization failed: {}", e)))
}

/// Deserialize from JSON string
pub fn from_json_string<T: for<'de> Deserialize<'de>>(s: &str) -> Result<T> {
    serde_json::from_str(s)
        .map_err(|e| Error::Deserialization(format!("JSON deserialization failed: {}", e)))
}

/// Serialize to hex string
pub fn to_hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

/// Deserialize from hex string
pub fn from_hex(s: &str) -> Result<Vec<u8>> {
    hex::decode(s)
        .map_err(|e| Error::Deserialization(format!("Hex decode failed: {}", e)))
}

/// Serialize to base64 string
pub fn to_base64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Deserialize from base64 string
pub fn from_base64(s: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(s)
        .map_err(|e| Error::Deserialization(format!("Base64 decode failed: {}", e)))
}

/// Encode bytes to a string (tries hex first, falls back to base64)
pub fn encode_bytes(bytes: &[u8]) -> String {
    // Use hex for short data, base64 for longer data
    if bytes.len() <= 64 {
        format!("0x{}", to_hex(bytes))
    } else {
        format!("b64:{}", to_base64(bytes))
    }
}

/// Decode bytes from a string (auto-detects hex or base64)
pub fn decode_bytes(s: &str) -> Result<Vec<u8>> {
    if s.starts_with("0x") {
        from_hex(&s[2..])
    } else if s.starts_with("b64:") {
        from_base64(&s[4..])
    } else {
        // Try hex without prefix
        from_hex(s)
    }
}

/// Serialize a struct to bytes (using bincode if available, otherwise JSON)
#[cfg(feature = "std")]
pub fn to_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    // For now, use JSON
    to_json(value)
}

/// Deserialize a struct from bytes
#[cfg(feature = "std")]
pub fn from_bytes<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
    // For now, use JSON
    from_json(bytes)
}

/// Serialize to a compact binary format
pub fn to_compact_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    // Use JSON for now, can be replaced with bincode later
    to_json(value)
}

/// Deserialize from compact binary format
pub fn from_compact_bytes<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
    // Use JSON for now, can be replaced with bincode later
    from_json(bytes)
}

/// Encode a u64 to bytes (big-endian)
pub fn u64_to_bytes(n: u64) -> [u8; 8] {
    n.to_be_bytes()
}

/// Decode bytes to u64 (big-endian)
pub fn bytes_to_u64(bytes: &[u8]) -> Result<u64> {
    if bytes.len() != 8 {
        return Err(Error::Deserialization(
            format!("Invalid u64 length: expected 8, got {}", bytes.len())
        ));
    }
    let arr: [u8; 8] = bytes.try_into().map_err(|_| {
        Error::Deserialization("Failed to convert bytes to array".to_string())
    })?;
    Ok(u64::from_be_bytes(arr))
}

/// Encode a u32 to bytes (big-endian)
pub fn u32_to_bytes(n: u32) -> [u8; 4] {
    n.to_be_bytes()
}

/// Decode bytes to u32 (big-endian)
pub fn bytes_to_u32(bytes: &[u8]) -> Result<u32> {
    if bytes.len() != 4 {
        return Err(Error::Deserialization(
            format!("Invalid u32 length: expected 4, got {}", bytes.len())
        ));
    }
    let arr: [u8; 4] = bytes.try_into().map_err(|_| {
        Error::Deserialization("Failed to convert bytes to array".to_string())
    })?;
    Ok(u32::from_be_bytes(arr))
}

/// Encode a u16 to bytes (big-endian)
pub fn u16_to_bytes(n: u16) -> [u8; 2] {
    n.to_be_bytes()
}

/// Decode bytes to u16 (big-endian)
pub fn bytes_to_u16(bytes: &[u8]) -> Result<u16> {
    if bytes.len() != 2 {
        return Err(Error::Deserialization(
            format!("Invalid u16 length: expected 2, got {}", bytes.len())
        ));
    }
    let arr: [u8; 2] = bytes.try_into().map_err(|_| {
        Error::Deserialization("Failed to convert bytes to array".to_string())
    })?;
    Ok(u16::from_be_bytes(arr))
}

/// Concatenate byte arrays
pub fn concat_bytes(arrays: &[&[u8]]) -> Vec<u8> {
    let total_len = arrays.iter().map(|a| a.len()).sum();
    let mut result = Vec::with_capacity(total_len);
    for arr in arrays {
        result.extend_from_slice(arr);
    }
    result
}

/// Split bytes at a given position
pub fn split_bytes(bytes: &[u8], at: usize) -> Result<(Vec<u8>, Vec<u8>)> {
    if at > bytes.len() {
        return Err(Error::Deserialization(
            format!("Split position {} exceeds length {}", at, bytes.len())
        ));
    }
    Ok((bytes[..at].to_vec(), bytes[at..].to_vec()))
}

/// Truncate bytes to a maximum length
pub fn truncate_bytes(bytes: &[u8], max_len: usize) -> Vec<u8> {
    if bytes.len() <= max_len {
        bytes.to_vec()
    } else {
        bytes[..max_len].to_vec()
    }
}

/// Pad bytes to a minimum length
pub fn pad_bytes(bytes: &[u8], min_len: usize) -> Vec<u8> {
    if bytes.len() >= min_len {
        bytes.to_vec()
    } else {
        let mut result = vec![0u8; min_len - bytes.len()];
        result.extend_from_slice(bytes);
        result
    }
}

/// Convert a string to bytes
pub fn string_to_bytes(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

/// Convert bytes to a string
pub fn bytes_to_string(bytes: &[u8]) -> Result<String> {
    String::from_utf8(bytes.to_vec())
        .map_err(|e| Error::Deserialization(format!("Invalid UTF-8: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestStruct {
        name: String,
        value: u64,
    }

    #[test]
    fn test_json_roundtrip() {
        let original = TestStruct {
            name: "test".to_string(),
            value: 42,
        };
        
        let bytes = to_json(&original).unwrap();
        let recovered: TestStruct = from_json(&bytes).unwrap();
        
        assert_eq!(original, recovered);
    }

    #[test]
    fn test_json_string_roundtrip() {
        let original = TestStruct {
            name: "test".to_string(),
            value: 42,
        };
        
        let s = to_json_string(&original).unwrap();
        let recovered: TestStruct = from_json_string(&s).unwrap();
        
        assert_eq!(original, recovered);
    }

    #[test]
    fn test_hex_roundtrip() {
        let bytes = vec![0x01, 0x02, 0x03, 0x04];
        let hex = to_hex(&bytes);
        let recovered = from_hex(&hex).unwrap();
        
        assert_eq!(bytes, recovered);
    }

    #[test]
    fn test_base64_roundtrip() {
        let bytes = vec![0x01, 0x02, 0x03, 0x04];
        let b64 = to_base64(&bytes);
        let recovered = from_base64(&b64).unwrap();
        
        assert_eq!(bytes, recovered);
    }

    #[test]
    fn test_encode_decode_bytes() {
        let short = vec![0x01, 0x02, 0x03];
        let encoded = encode_bytes(&short);
        assert!(encoded.starts_with("0x"));
        let recovered = decode_bytes(&encoded).unwrap();
        assert_eq!(short, recovered);

        let long = vec![0u8; 100];
        let encoded = encode_bytes(&long);
        assert!(encoded.starts_with("b64:"));
        let recovered = decode_bytes(&encoded).unwrap();
        assert_eq!(long, recovered);
    }

    #[test]
    fn test_u64_roundtrip() {
        let original: u64 = 0x1234567890ABCDEF;
        let bytes = u64_to_bytes(original);
        let recovered = bytes_to_u64(&bytes).unwrap();
        
        assert_eq!(original, recovered);
    }

    #[test]
    fn test_u32_roundtrip() {
        let original: u32 = 0x12345678;
        let bytes = u32_to_bytes(original);
        let recovered = bytes_to_u32(&bytes).unwrap();
        
        assert_eq!(original, recovered);
    }

    #[test]
    fn test_u16_roundtrip() {
        let original: u16 = 0x1234;
        let bytes = u16_to_bytes(original);
        let recovered = bytes_to_u16(&bytes).unwrap();
        
        assert_eq!(original, recovered);
    }

    #[test]
    fn test_concat_bytes() {
        let a = &[0x01, 0x02];
        let b = &[0x03, 0x04];
        let c = &[0x05];
        
        let result = concat_bytes(&[a, b, c]);
        assert_eq!(result, vec![0x01, 0x02, 0x03, 0x04, 0x05]);
    }

    #[test]
    fn test_split_bytes() {
        let bytes = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let (left, right) = split_bytes(&bytes, 2).unwrap();
        
        assert_eq!(left, vec![0x01, 0x02]);
        assert_eq!(right, vec![0x03, 0x04, 0x05]);
    }

    #[test]
    fn test_split_bytes_error() {
        let bytes = vec![0x01, 0x02];
        assert!(split_bytes(&bytes, 5).is_err());
    }

    #[test]
    fn test_truncate_bytes() {
        let bytes = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        
        let truncated = truncate_bytes(&bytes, 3);
        assert_eq!(truncated, vec![0x01, 0x02, 0x03]);
        
        let not_truncated = truncate_bytes(&bytes, 10);
        assert_eq!(not_truncated, bytes);
    }

    #[test]
    fn test_pad_bytes() {
        let bytes = vec![0x01, 0x02];
        
        let padded = pad_bytes(&bytes, 5);
        assert_eq!(padded, vec![0x00, 0x00, 0x00, 0x01, 0x02]);
        
        let not_padded = pad_bytes(&bytes, 2);
        assert_eq!(not_padded, bytes);
    }

    #[test]
    fn test_string_roundtrip() {
        let original = "Hello, World!";
        let bytes = string_to_bytes(original);
        let recovered = bytes_to_string(&bytes).unwrap();
        
        assert_eq!(original, recovered);
    }
}
