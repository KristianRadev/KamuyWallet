//! # Policy Schema Validation
//!
//! JSON Schema validation for policy files.

#![allow(dead_code)]

use crate::error::{StewardError, Result};
use serde::{Deserialize, Serialize};

/// Policy schema wrapper
#[derive(Debug, Clone)]
pub struct PolicySchema;

impl PolicySchema {
    /// Get the JSON schema as a string
    pub fn as_str() -> &'static str {
        POLICY_SCHEMA
    }

    /// Validate a policy JSON string against the schema
    pub fn validate(json: &str) -> PolicyValidationResult {
        // Basic validation - check required fields exist
        let parsed: serde_json::Value = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => return PolicyValidationResult::invalid(vec![format!("Invalid JSON: {}", e)]),
        };

        let mut errors = Vec::new();

        // Check required fields
        let required = ["version", "max_per_tx", "max_daily", "require_approval_above"];
        for field in &required {
            if parsed.get(field).is_none() {
                errors.push(format!("Missing required field: {}", field));
            }
        }

        if errors.is_empty() {
            PolicyValidationResult::valid()
        } else {
            PolicyValidationResult::invalid(errors)
        }
    }
}

/// Policy validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyValidationResult {
    /// Whether the policy is valid
    pub valid: bool,
    /// Validation errors
    pub errors: Vec<String>,
    /// Validation warnings
    pub warnings: Vec<String>,
}

impl PolicyValidationResult {
    /// Create a valid result
    pub fn valid() -> Self {
        Self {
            valid: true,
            errors: vec![],
            warnings: vec![],
        }
    }

    /// Create an invalid result with errors
    pub fn invalid(errors: Vec<String>) -> Self {
        Self {
            valid: false,
            errors,
            warnings: vec![],
        }
    }

    /// Add a warning
    pub fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }
}

/// JSON Schema for policy validation
pub const POLICY_SCHEMA: &str = r#"
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["version", "max_per_tx", "max_daily", "require_approval_above"],
  "properties": {
    "version": {
      "type": "string",
      "pattern": "^\\d+\\.\\d+$"
    },
    "max_per_tx": {
      "type": "string",
      "pattern": "^\\d+(\\.\\d+)?$"
    },
    "max_daily": {
      "type": "string",
      "pattern": "^\\d+(\\.\\d+)?$"
    },
    "max_weekly": {
      "type": "string",
      "pattern": "^\\d+(\\.\\d+)?$"
    },
    "max_monthly": {
      "type": "string",
      "pattern": "^\\d+(\\.\\d+)?$"
    },
    "require_approval_above": {
      "type": "string",
      "pattern": "^\\d+(\\.\\d+)?$"
    },
    "allowed_tokens": {
      "type": "array",
      "items": {
        "type": "string"
      }
    },
    "whitelist": {
      "type": "array",
      "items": {
        "type": "string"
      }
    },
    "blocklist": {
      "type": "array",
      "items": {
        "type": "string"
      }
    },
    "time_windows": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["days", "start", "end"],
        "properties": {
          "days": {
            "type": "array",
            "items": {
              "type": "string",
              "enum": ["mon", "tue", "wed", "thu", "fri", "sat", "sun",
                       "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"]
            }
          },
          "start": {
            "type": "string",
            "pattern": "^\\d{2}:\\d{2}$"
          },
          "end": {
            "type": "string",
            "pattern": "^\\d{2}:\\d{2}$"
          }
        }
      }
    },
    "rate_limit_per_hour": {
      "type": "integer",
      "minimum": 1,
      "maximum": 1000
    },
    "rate_limit_per_day": {
      "type": "integer",
      "minimum": 1,
      "maximum": 10000
    },
    "max_slippage_bps": {
      "type": "integer",
      "minimum": 0,
      "maximum": 10000
    },
    "allowed_contract_methods": {
      "type": "array",
      "items": {
        "type": "string"
      }
    },
    "notify_on_approval": {
      "type": "boolean"
    },
    "notify_on_rejection": {
      "type": "boolean"
    }
  }
}
"#;

/// Validate policy JSON against schema
pub fn validate_policy_json(json: &str) -> Result<PolicyValidationResult> {
    // Parse JSON
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| StewardError::Validation(format!("Invalid JSON: {}", e)))?;

    let mut result = PolicyValidationResult::valid();

    // Check required fields
    let required = ["version", "max_per_tx", "max_daily", "require_approval_above"];
    for field in &required {
        if value.get(field).is_none() {
            result.errors.push(format!("Missing required field: {}", field));
        }
    }

    // Check amount fields are valid numbers
    let amount_fields = ["max_per_tx", "max_daily", "max_weekly", "max_monthly", "require_approval_above"];
    for field in &amount_fields {
        if let Some(val) = value.get(field) {
            if let Some(s) = val.as_str() {
                if s.parse::<f64>().is_err() {
                    result.errors.push(format!("Invalid amount for {}: {}", field, s));
                }
            } else {
                result.errors.push(format!("{} must be a string", field));
            }
        }
    }

    // Check rate limits
    if let Some(hour) = value.get("rate_limit_per_hour").and_then(|v| v.as_u64()) {
        if let Some(day) = value.get("rate_limit_per_day").and_then(|v| v.as_u64()) {
            if hour > day {
                result.errors.push("rate_limit_per_hour cannot exceed rate_limit_per_day".to_string());
            }
        }
    }

    // Check time windows
    if let Some(windows) = value.get("time_windows").and_then(|v| v.as_array()) {
        for (i, window) in windows.iter().enumerate() {
            if let Some(days) = window.get("days").and_then(|v| v.as_array()) {
                if days.is_empty() {
                    result.warnings.push(format!("Time window {} has no days specified", i));
                }
            }
        }
    }

    // Check for empty allowed_tokens (warning)
    if let Some(tokens) = value.get("allowed_tokens").and_then(|v| v.as_array()) {
        if tokens.is_empty() {
            result.warnings.push("allowed_tokens is empty - all tokens will be allowed".to_string());
        }
    }

    // Check for whitelist/blocklist overlap
    let whitelist: Vec<String> = value.get("whitelist")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    
    let blocklist: Vec<String> = value.get("blocklist")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    
    for item in &whitelist {
        if blocklist.contains(item) {
            result.errors.push(format!("'{}' is in both whitelist and blocklist", item));
        }
    }

    // Update valid flag based on errors
    if !result.errors.is_empty() {
        result.valid = false;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_policy() {
        let json = r#"{
            "version": "1.0",
            "max_per_tx": "100.00",
            "max_daily": "1000.00",
            "require_approval_above": "50.00"
        }"#;
        
        let result = validate_policy_json(json).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_missing_required() {
        let json = r#"{"version": "1.0"}"#;
        
        let result = validate_policy_json(json).unwrap();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("max_per_tx")));
    }

    #[test]
    fn test_invalid_amount() {
        let json = r#"{
            "version": "1.0",
            "max_per_tx": "invalid",
            "max_daily": "1000.00",
            "require_approval_above": "50.00"
        }"#;
        
        let result = validate_policy_json(json).unwrap();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("max_per_tx")));
    }

    #[test]
    fn test_whitelist_blocklist_overlap() {
        let json = r#"{
            "version": "1.0",
            "max_per_tx": "100.00",
            "max_daily": "1000.00",
            "require_approval_above": "50.00",
            "whitelist": ["openai.com"],
            "blocklist": ["openai.com"]
        }"#;
        
        let result = validate_policy_json(json).unwrap();
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("openai.com")));
    }
}
