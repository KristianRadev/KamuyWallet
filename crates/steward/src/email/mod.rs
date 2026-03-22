//! # Email Module
//!
//! Email backup functionality for wallet keys.
//! Sends encrypted user_key to the user's email for recovery.

mod sender;

pub use sender::{send_backup_email, EmailBackupResult};