//! # Policy Engine Module (v2.0)
//!
//! Validates transactions against user-defined security policies.
//!
//! ## v2.0 Policy Rules
//!
//! - Spending limits (per tx, daily, weekly)
//! - Whitelist with labels for destinations
//! - Auto-add threshold for new addresses
//! - USDC only (gasless via Pimlico)

pub mod engine;
pub mod rules;
pub mod schema;
pub mod whitelist;

pub use engine::PolicyEngine;
pub use rules::{PolicyRules, SpendingTracker};
pub use whitelist::{Whitelist, WhitelistEntry};
#[allow(unused_imports)]
pub use schema::{PolicySchema, PolicyValidationResult};