//! # Error Types for Steward Service
//!
//! Comprehensive error handling for all Steward service operations.
#![allow(dead_code)]

use thiserror::Error;

/// Result type alias for Steward service
pub type Result<T> = std::result::Result<T, StewardError>;

/// Main error type for Steward service
#[derive(Error, Debug, Clone)]
pub enum StewardError {
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Database error
    #[error("Database error: {0}")]
    Database(String),

    /// Policy error
    #[error("Policy error: {0}")]
    Policy(String),

    /// Transaction error
    #[error("Transaction error: {0}")]
    Transaction(String),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),

    /// Authentication error
    #[error("Authentication error: {0}")]
    Auth(String),

    /// Authorization error
    #[error("Authorization error: {0}")]
    Unauthorized(String),

    /// Not found error
    #[error("Not found: {0}")]
    NotFound(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(String),

    /// MPC error
    #[error("MPC error: {0}")]
    Mpc(String),

    /// Signing error
    #[error("Signing error: {0}")]
    Signing(String),

    /// Telegram error
    #[error("Telegram error: {0}")]
    Telegram(String),

    /// Queue error
    #[error("Queue error: {0}")]
    Queue(String),

    /// Storage error
    #[error("Storage error: {0}")]
    Storage(String),

    /// Encryption error
    #[error("Encryption error: {0}")]
    Encryption(String),

    /// Decryption error
    #[error("Decryption error: {0}")]
    Decryption(String),

    /// Invalid password
    #[error("Invalid password")]
    InvalidPassword,

    /// Key not loaded
    #[error("Steward key not loaded - use /unlock command")]
    KeyNotLoaded,

    /// Timeout error
    #[error("Operation timed out: {0}")]
    Timeout(String),

    /// Network error
    #[error("Network error: {0}")]
    Network(String),

    /// Pimlico API error
    #[error("Pimlico error: {0}")]
    Pimlico(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Deserialization error
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Not implemented
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// Custom error
    #[error("{0}")]
    Custom(String),
}

impl StewardError {
    /// Create a custom error
    pub fn custom(msg: impl Into<String>) -> Self {
        StewardError::Custom(msg.into())
    }

    /// Check if this is a validation error
    pub fn is_validation(&self) -> bool {
        matches!(self, StewardError::Validation(_))
    }

    /// Check if this is an auth error
    pub fn is_auth(&self) -> bool {
        matches!(self, StewardError::Auth(_) | StewardError::Unauthorized(_))
    }

    /// Check if this is a not found error
    pub fn is_not_found(&self) -> bool {
        matches!(self, StewardError::NotFound(_))
    }

    /// Check if this is a rate limit error
    pub fn is_rate_limit(&self) -> bool {
        matches!(self, StewardError::RateLimit(_))
    }

/// Get HTTP status code for this error
    pub fn status_code(&self) -> u16 {
        match self {
            StewardError::Config(_) => 500,
            StewardError::Database(_) => 500,
            StewardError::Policy(_) => 400,
            StewardError::Transaction(_) => 400,
            StewardError::Validation(_) => 400,
            StewardError::Auth(_) => 401,
            StewardError::Unauthorized(_) => 403,
            StewardError::NotFound(_) => 404,
            StewardError::RateLimit(_) => 429,
            StewardError::Io(_) => 500,
            StewardError::Mpc(_) => 500,
            StewardError::Signing(_) => 500,
            StewardError::Telegram(_) => 500,
            StewardError::Queue(_) => 500,
            StewardError::Storage(_) => 500,
            StewardError::Encryption(_) => 500,
            StewardError::Decryption(_) => 400,
            StewardError::InvalidPassword => 401,
            StewardError::KeyNotLoaded => 503,
            StewardError::Timeout(_) => 504,
            StewardError::Network(_) => 502,
            StewardError::Pimlico(_) => 502,
            StewardError::Serialization(_) => 400,
            StewardError::Deserialization(_) => 400,
            StewardError::Internal(_) => 500,
            StewardError::NotImplemented(_) => 501,
            StewardError::Custom(_) => 500,
        }
    }
}

/// Convert from sqlx errors
impl From<sqlx::Error> for StewardError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => StewardError::NotFound("Record not found".to_string()),
            sqlx::Error::Database(db_err) => {
                StewardError::Database(format!("Database error: {}", db_err))
            }
            _ => StewardError::Database(format!("Database error: {}", e)),
        }
    }
}

/// Convert from serde_json errors
impl From<serde_json::Error> for StewardError {
    fn from(e: serde_json::Error) -> Self {
        StewardError::Serialization(format!("JSON error: {}", e))
    }
}

/// Convert from MPC core errors
impl From<kamuy_mpc_core::Error> for StewardError {
    fn from(e: kamuy_mpc_core::Error) -> Self {
        StewardError::Mpc(e.to_string())
    }
}

/// Convert from std::io errors
impl From<std::io::Error> for StewardError {
    fn from(e: std::io::Error) -> Self {
        StewardError::Internal(format!("IO error: {}", e))
    }
}

/// Convert from hex errors
impl From<hex::FromHexError> for StewardError {
    fn from(e: hex::FromHexError) -> Self {
        StewardError::Deserialization(format!("Hex error: {}", e))
    }
}

/// Convert from base64 errors
impl From<base64::DecodeError> for StewardError {
    fn from(e: base64::DecodeError) -> Self {
        StewardError::Deserialization(format!("Base64 error: {}", e))
    }
}

/// Convert from anyhow errors
impl From<anyhow::Error> for StewardError {
    fn from(e: anyhow::Error) -> Self {
        StewardError::Internal(e.to_string())
    }
}

/// Convert from argon2 errors
impl From<argon2::Error> for StewardError {
    fn from(e: argon2::Error) -> Self {
        StewardError::Encryption(format!("Argon2 error: {}", e))
    }
}

/// API error response for HTTP endpoints
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiError {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// HTTP status code
    pub status: u16,
    /// Request ID for tracking
    pub request_id: String,
}

impl From<&StewardError> for ApiError {
    fn from(error: &StewardError) -> Self {
        let code = match error {
            StewardError::Config(_) => "CONFIG_ERROR",
            StewardError::Database(_) => "DATABASE_ERROR",
            StewardError::Policy(_) => "POLICY_ERROR",
            StewardError::Transaction(_) => "TRANSACTION_ERROR",
            StewardError::Validation(_) => "VALIDATION_ERROR",
            StewardError::Auth(_) => "AUTH_ERROR",
            StewardError::Unauthorized(_) => "UNAUTHORIZED",
            StewardError::NotFound(_) => "NOT_FOUND",
            StewardError::RateLimit(_) => "RATE_LIMIT",
            StewardError::Io(_) => "IO_ERROR",
            StewardError::Mpc(_) => "MPC_ERROR",
            StewardError::Signing(_) => "SIGNING_ERROR",
            StewardError::Telegram(_) => "TELEGRAM_ERROR",
            StewardError::Queue(_) => "QUEUE_ERROR",
            StewardError::Storage(_) => "STORAGE_ERROR",
            StewardError::Encryption(_) => "ENCRYPTION_ERROR",
StewardError::Decryption(_) => "DECRYPTION_ERROR",
            StewardError::InvalidPassword => "INVALID_PASSWORD",
            StewardError::KeyNotLoaded => "KEY_NOT_LOADED",
            StewardError::Timeout(_) => "TIMEOUT",
            StewardError::Network(_) => "NETWORK_ERROR",
            StewardError::Pimlico(_) => "PIMLICO_ERROR",
            StewardError::Serialization(_) => "SERIALIZATION_ERROR",
            StewardError::Deserialization(_) => "DESERIALIZATION_ERROR",
            StewardError::Internal(_) => "INTERNAL_ERROR",
            StewardError::NotImplemented(_) => "NOT_IMPLEMENTED",
            StewardError::Custom(_) => "CUSTOM_ERROR",
        }
        .to_string();

        Self {
            code,
            message: error.to_string(),
            status: error.status_code(),
            request_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

impl From<teloxide::RequestError> for StewardError {
    fn from(e: teloxide::RequestError) -> Self {
        StewardError::Telegram(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        assert_eq!(StewardError::NotFound("test".to_string()).status_code(), 404);
        assert_eq!(StewardError::Auth("test".to_string()).status_code(), 401);
        assert_eq!(StewardError::Unauthorized("test".to_string()).status_code(), 403);
        assert_eq!(StewardError::RateLimit("test".to_string()).status_code(), 429);
        assert_eq!(StewardError::Validation("test".to_string()).status_code(), 400);
    }

    #[test]
    fn test_error_checks() {
        let validation = StewardError::Validation("test".to_string());
        assert!(validation.is_validation());
        assert!(!validation.is_auth());

        let auth = StewardError::Auth("test".to_string());
        assert!(auth.is_auth());
        assert!(!auth.is_validation());
    }

    #[test]
    fn test_api_error_from_steward_error() {
        let error = StewardError::NotFound("transaction".to_string());
        let api_error = ApiError::from(&error);
        
        assert_eq!(api_error.code, "NOT_FOUND");
        assert_eq!(api_error.status, 404);
        assert!(api_error.message.contains("transaction"));
    }
}
