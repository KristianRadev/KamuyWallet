//! # History Command
//!
//! Show transaction history.

use crate::commands::create_spinner;
use crate::context::CliContext;
use crate::{print_info, print_table};
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

/// Execute history command
pub async fn execute(ctx: Arc<CliContext>, limit: u32) -> Result<()> {
    println!("{}", "📜 Transaction History".bold().cyan());
    println!();
    
    let spinner = create_spinner("Fetching history...");
    
    // Get recent transactions
    // Note: This would need a new API endpoint for paginated history
    // For now, we'll show a placeholder
    
    spinner.finish_with_message("History loaded");
    
    println!();
    print_info("Transaction history feature coming soon.");
    println!();
    println!("Use 'kamuy pending' to see pending transactions.");
    
    Ok(())
}
