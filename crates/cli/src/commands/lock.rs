//! # Lock Command
//!
//! Unload Steward key.

use crate::context::CliContext;
use crate::{print_info, print_success};
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

/// Execute lock command
pub async fn execute(_ctx: Arc<CliContext>) -> Result<()> {
    println!("{}", "🔒 Lock Wallet".bold().cyan());
    println!();
    
    // In a real implementation, this would call the Steward to unload the key
    // For now, just inform the user
    
    print_info("To lock the wallet, the Steward service must be restarted.");
    println!();
    println!("Run:");
    println!("  pkill kamuy-steward");
    println!("  kamuy-steward");
    println!();
    print_success("This will unload the key from memory");
    
    Ok(())
}
