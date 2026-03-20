//! Whitelist management for v2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A whitelisted address entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhitelistEntry {
    /// Human-readable label (e.g., "OpenAI", "AWS")
    pub label: String,
    /// When this address was added
    pub added_at: DateTime<Utc>,
    /// Total amount sent to this address (in USDC micros)
    pub total_sent: u64,
}

impl WhitelistEntry {
    /// Create a new whitelist entry
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            added_at: Utc::now(),
            total_sent: 0,
        }
    }
}

/// Whitelist management
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Whitelist(HashMap<String, WhitelistEntry>);

impl Whitelist {
    /// Create empty whitelist
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Check if address is whitelisted
    pub fn contains(&self, address: &str) -> bool {
        self.0.contains_key(address)
    }

    /// Get entry for an address
    pub fn get(&self, address: &str) -> Option<&WhitelistEntry> {
        self.0.get(address)
    }

    /// Get mutable entry
    pub fn get_mut(&mut self, address: &str) -> Option<&mut WhitelistEntry> {
        self.0.get_mut(address)
    }

    /// Add a new address
    pub fn add(&mut self, address: impl Into<String>, label: impl Into<String>) {
        self.0.insert(address.into(), WhitelistEntry::new(label));
    }

    /// Update total sent amount
    pub fn update_total_sent(&mut self, address: &str, amount: u64) {
        if let Some(entry) = self.0.get_mut(address) {
            entry.total_sent = entry.total_sent.saturating_add(amount);
        }
    }

    /// Get all entries
    pub fn entries(&self) -> &HashMap<String, WhitelistEntry> {
        &self.0
    }

    /// Count entries
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whitelist_add_and_contains() {
        let mut whitelist = Whitelist::new();
        assert!(!whitelist.contains("0x1234"));

        whitelist.add("0x1234", "Test Address");
        assert!(whitelist.contains("0x1234"));

        let entry = whitelist.get("0x1234").unwrap();
        assert_eq!(entry.label, "Test Address");
    }

    #[test]
    fn test_update_total_sent() {
        let mut whitelist = Whitelist::new();
        whitelist.add("0x1234", "Test");

        whitelist.update_total_sent("0x1234", 1000);
        assert_eq!(whitelist.get("0x1234").unwrap().total_sent, 1000);

        whitelist.update_total_sent("0x1234", 500);
        assert_eq!(whitelist.get("0x1234").unwrap().total_sent, 1500);
    }
}