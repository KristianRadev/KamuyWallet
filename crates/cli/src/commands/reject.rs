//! # Reject Command
//!
//! Reject a pending approval request using the new inline Telegram approval flow.
//!
//! ## Usage
//!
//! - `kamuy reject <tx_id>` - Reject a pending transaction
//!
//! The CLI calls the Steward API endpoint POST /api/v1/approval/respond
//! with decision "reject".

use crate::commands::{confirm, create_spinner};
use crate::context::CliContext;
use crate::{print_error, print_info, print_success};
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

/// Execute reject command
/// Called when user runs: kamuy reject <tx_id>
pub async fn execute(ctx: Arc<CliContext>, tx_id: String) -> Result<()> {
    println!("{}", "Reject Transaction".bold().red());
    println!();

    // First check if this is a pending approval via the new API
    let spinner = create_spinner("Fetching approval request...");
    let approval = match ctx.steward.get_pending_request(&tx_id).await? {
        Some(a) => a,
        None => {
            spinner.finish_with_message("Not found".to_string());
            print_error(&format!("No pending approval found for transaction {}", tx_id));
            print_info("The transaction may have already been resolved or timed out.");
            return Ok(());
        }
    };
    spinner.finish_with_message("Approval request loaded".to_string());

    println!();
    println!("{}", "Transaction Details:".bold());
    println!("  ID: {}", approval.tx_id.cyan());
    println!("  Amount: {} {}", approval.amount_display.cyan(), approval.token);
    println!("  To: {}", approval.to.cyan());
    println!("  Chain ID: {}", approval.chain_id);
    println!("  Reason: {}", approval.reason);
    println!();

    print_info("You are about to REJECT this transaction.");
    println!();

    if !confirm("Reject this transaction?")? {
        println!("Aborted.");
        return Ok(());
    }

    println!();

    // Execute via the new approval/respond API
    let spinner = create_spinner("Submitting rejection...");

    match ctx.steward.respond_to_approval(&tx_id, "reject").await {
        Ok(result) => {
            spinner.finish_with_message("Rejected!".to_string());
            print_success(&format!("Transaction {} rejected", tx_id));
            println!("  Decision recorded: {}", result.decision);
            println!("  Resolved: {}", result.resolved);
        }
        Err(e) => {
            spinner.finish_with_message("Failed".to_string());
            print_error(&format!("Rejection failed: {}", e));
        }
    }

    Ok(())
}
