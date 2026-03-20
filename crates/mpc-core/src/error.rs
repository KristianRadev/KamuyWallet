//! Error types for Kamuy MPC

use thiserror::Error;

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for MPC operations
#[derive(Error, Debug, Clone)]
pub enum Error {
    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Invalid party ID
    #[error("Invalid party ID: {0}")]
    InvalidPartyId(u8),

    /// Invalid party role
    #[error("Invalid party role: {0}")]
    InvalidPartyRole(String),

    /// Threshold not met
    #[error("Threshold not met: required {required}, got {actual}")]
    ThresholdNotMet {
        required: usize,
        actual: usize,
    },

    /// Invalid signing parties
    #[error("Invalid signing parties: {0}")]
    InvalidSigningParties(String),

    /// This party not in signing set
    #[error("This party is not in the signing set")]
    NotInSigningSet,

    /// DKG verification failed
    #[error("DKG verification failed: {0}")]
    DkgVerificationFailed(String),

    /// Share verification failed
    #[error("Share verification failed: {0}")]
    VerificationFailed(String),

    /// Invalid share
    #[error("Invalid share from party {from}: {reason}")]
    InvalidShare {
        from: u8,
        reason: String,
    },

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Deserialization error
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// Encryption error
    #[error("Encryption error: {0}")]
    Encryption(String),

    /// Decryption error
    #[error("Decryption error: {0}")]
    Decryption(String),

    /// Invalid password
    #[error("Invalid password")]
    InvalidPassword,

    /// Key derivation error
    #[error("Key derivation error: {0}")]
    KeyDerivation(String),

    /// Invalid derivation path
    #[error("Invalid derivation path: {0}")]
    InvalidDerivationPath(String),

    /// Signature error
    #[error("Signature error: {0}")]
    Signature(String),

    /// Invalid signature
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    /// Protocol error
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// Timeout
    #[error("Operation timed out after {0} seconds")]
    Timeout(u64),

    /// Communication error
    #[error("Communication error: {0}")]
    Communication(String),

    /// Relay error
    #[error("Relay error: {0}")]
    Relay(String),

    /// Policy violation
    #[error("Policy violation: {0}")]
    PolicyViolation(String),

    /// Not implemented
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Custom error
    #[error("{0}")]
    Custom(String),
}

impl Error {
    /// Create a custom error
    pub fn custom(msg: impl Into<String>) -> Self {
        Error::Custom(msg.into())
    }

    /// Check if this is a timeout error
    pub fn is_timeout(&self) -> bool {
        matches!(self, Error::Timeout(_))
    }

    /// Check if this is a policy violation
    pub fn is_policy_violation(&self) -> bool {
        matches!(self, Error::PolicyViolation(_))
    }

    /// Check if this is a verification failure
    pub fn is_verification_failed(&self) -> bool {
        matches!(self, Error::VerificationFailed(_))
    }
}

/// Convert from k256 errors
impl From<k256::elliptic_curve::Error> for Error {
    fn from(e: k256::elliptic_curve::Error) -> Self {
        Error::Internal(format!("Elliptic curve error: {}", e))
    }
}

/// Convert from serde_json errors
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Serialization(format!("JSON error: {}", e))
    }
}

/// Convert from hex errors
impl From<hex::FromHexError> for Error {
    fn from(e: hex::FromHexError) -> Self {
        Error::Deserialization(format!("Hex error: {}", e))
    }
}

/// Convert from base64 errors
impl From<base64::DecodeError> for Error {
    fn from(e: base64::DecodeError) -> Self {
        Error::Deserialization(format!("Base64 error: {}", e))
    }
}

/// Convert from argon2 errors
impl From<argon2::Error> for Error {
    fn from(e: argon2::Error) -> Self {
        Error::KeyDerivation(format!("Argon2 error: {}", e))
    }
}

/// Convert from aead errors
impl From<aead::Error> for Error {
    fn from(e: aead::Error) -> Self {
        Error::Encryption(format!("AEAD error: {}", e))
    }
}

/// Convert from chrono errors
impl From<chrono::ParseError> for Error {
    fn from(e: chrono::ParseError) -> Self {
        Error::Deserialization(format!("Date parsing error: {}", e))
    }
}

/// Convert from uuid errors
impl From<uuid::Error> for Error {
    fn from(e: uuid::Error) -> Self {
        Error::Internal(format!("UUID error: {}", e))
    }
}

/// Convert from std::io errors
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Internal(format!("IO error: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::InvalidConfig("test".to_string());
        assert_eq!(err.to_string(), "Invalid configuration: test");

        let err = Error::ThresholdNotMet { required: 2, actual: 1 };
        assert_eq!(err.to_string(), "Threshold not met: required 2, got 1");
    }

    #[test]
    fn test_error_checks() {
        let timeout = Error::Timeout(60);
        assert!(timeout.is_timeout());
        assert!(!timeout.is_policy_violation());

        let policy = Error::PolicyViolation("test".to_string());
        assert!(policy.is_policy_violation());
        assert!(!policy.is_timeout());
    }

    #[test]
    fn test_custom_error() {
        let err = Error::custom("custom message");
        assert_eq!(err.to_string(), "custom message");
    }
}
