//! # Approve/Reject Command
//!
//! Approve or reject pending items with v2.0 approval levels.
//!
//! ## Approval Types
//!
//! - `kamuy approve policy <id>` - Approve policy changes (TerminalPassword)
//! - `kamuy approve address <id>` - Approve new address over threshold (TerminalPassword)
//! - `kamuy approve tx <id>` - Approve transaction (optional override)

use crate::commands::{create_spinner, prompt_password, confirm};
use crate::context::CliContext;
use crate::{print_error, print_info, print_success, print_warning};
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

/// Approve a policy change request (TerminalPassword level)
/// Always prompts for password - required for policy changes
pub async fn approve_policy(ctx: Arc<CliContext>, id: String) -> Result<()> {
    println!("{}", "Approve Policy Change".bold().cyan());
    println!();

    // Get policy change request details
    let spinner = create_spinner("Fetching policy change request...");
    let record = match ctx.steward.get_policy_change_request(&id).await? {
        Some(r) => r,
        None => {
            spinner.finish_with_message("Not found".to_string());
            print_error(&format!("Policy change request {} not found", id));
            return Ok(());
        }
    };
    spinner.finish_with_message("Policy change request loaded".to_string());

    // Check if already resolved
    if record.status != kamuy_steward::types::PolicyChangeStatus::Pending {
        print_warning(&format!(
            "Policy change request already {}",
            record.status
        ));
        return Ok(());
    }

    println!();
    println!("{}", "Policy Change Details:".bold());
    println!("  ID: {}", record.request.id.to_string().cyan());
    println!("  Field: {}", record.request.field.cyan());
    println!("  Current Value: {}", record.request.current_value);
    println!("  New Value: {}", record.request.new_value.green());
    println!("  Reason: {}", record.request.reason);
    println!();

    // Confirm approval
    print_info("You are about to APPROVE this policy change.");
    print_warning("This requires terminal password verification.");
    println!();

    if !confirm("Approve this policy change?")? {
        println!("Aborted.");
        return Ok(());
    }

    println!();

    // Always prompt for password (TerminalPassword level)
    let password = prompt_password("Enter wallet password")?;

    // Submit approval
    let spinner = create_spinner("Submitting approval...");

    match ctx.steward.approve_policy_change(&id, &password).await {
        Ok(()) => {
            spinner.finish_with_message("Approved!".to_string());
            print_success(&format!(
                "Policy change approved: {} = {}",
                record.request.field, record.request.new_value
            ));
        }
        Err(e) => {
            spinner.finish_with_message("Failed".to_string());
            print_error(&format!("Approval failed: {}", e));
        }
    }

    Ok(())
}

/// Approve a new address over threshold (TerminalPassword level)
/// Always prompts for password - required for new addresses
pub async fn approve_address(ctx: Arc<CliContext>, id: String) -> Result<()> {
    println!("{}", "Approve New Address".bold().cyan());
    println!();

    // For now, addresses are approved through the transaction approval flow
    // This is a placeholder for future address whitelist approval
    print_info("Address approval is handled through transaction approval.");
    print_info("When a transaction to a new address exceeds the threshold,");
    print_info("use 'kamuy approve tx <id>' to approve.");

    // TODO: Implement address whitelist approval when that feature is added
    print_warning(&format!("Address approval ID {} not implemented yet", id));

    Ok(())
}

/// Approve a pending transaction
/// By default uses TelegramButton level, but can override with --with-password
pub async fn approve_tx(ctx: Arc<CliContext>, id: String, with_password: bool) -> Result<()> {
    println!("{}", "Approve Transaction".bold().cyan());
    println!();

    // Get transaction details
    let spinner = create_spinner("Fetching transaction...");
    let tx = match ctx.steward.get_transaction(&id).await? {
        Some(tx) => tx,
        None => {
            spinner.finish_with_message("Not found".to_string());
            print_error(&format!("Transaction {} not found", id));
            return Ok(());
        }
    };
    spinner.finish_with_message("Transaction loaded".to_string());

    println!();
    println!("{}", "Transaction Details:".bold());
    println!("  ID: {}", tx.id.to_string().cyan());
    println!("  Amount: {} {}", tx.request.value.cyan(), tx.request.token);
    println!("  To: {}", tx.request.to.cyan());
    println!("  Chain ID: {}", tx.request.chain_id);
    println!("  Status: {:?}", tx.status);
    println!();

    // Confirm approval
    print_info("You are about to APPROVE this transaction.");

    if with_password {
        print_warning("Using terminal password verification (override).");
    } else {
        print_info("This will submit approval via the standard approval flow.");
    }

    println!();

    if !confirm("Approve this transaction?")? {
        println!("Aborted.");
        return Ok(());
    }

    println!();

    if with_password {
        // Use password-protected approval (TerminalPassword level)
        let password = prompt_password("Enter wallet password")?;

        let spinner = create_spinner("Submitting approval with password...");

        match ctx.steward.approve_transaction_with_password(&id, &password).await {
            Ok(()) => {
                spinner.finish_with_message("Approved!".to_string());
                print_success(&format!("Transaction {} approved with password verification", id));
            }
            Err(e) => {
                spinner.finish_with_message("Failed".to_string());
                print_error(&format!("Approval failed: {}", e));
            }
        }
    } else {
        // Use standard approval flow (TelegramButton level)
        let spinner = create_spinner("Submitting approval...");

        match ctx.steward.approve_transaction(&id).await {
            Ok(()) => {
                spinner.finish_with_message("Approved!".to_string());
                print_success(&format!("Transaction {} approved", id));
            }
            Err(e) => {
                spinner.finish_with_message("Failed".to_string());
                print_error(&format!("Approval failed: {}", e));
            }
        }
    }

    Ok(())
}

/// Execute approve/reject command (legacy compatibility)
pub async fn execute(ctx: Arc<CliContext>, tx_id: String, approve: bool) -> Result<()> {
    let action = if approve { "Approve" } else { "Reject" };
    println!("{} {}", action, "Transaction".bold().cyan());
    println!();

    // Get transaction details
    let spinner = create_spinner("Fetching transaction...");
    let tx = match ctx.steward.get_transaction(&tx_id).await? {
        Some(tx) => tx,
        None => {
            spinner.finish_with_message("Not found".to_string());
            print_error(&format!("Transaction {} not found", tx_id));
            return Ok(());
        }
    };
    spinner.finish_with_message("Transaction loaded".to_string());

    println!();
    println!("{}", "Transaction Details:".bold());
    println!("  ID: {}", tx.id.to_string().cyan());
    println!("  Amount: {} {}", tx.request.value.cyan(), tx.request.token);
    println!("  To: {}", tx.request.to.cyan());
    println!("  Status: {:?}", tx.status);
    println!();

    // Confirm
    if approve {
        print_info("You are about to APPROVE this transaction.");
    } else {
        print_info("You are about to REJECT this transaction.");
    }

    if !confirm("Continue?")? {
        println!("Aborted.");
        return Ok(());
    }

    println!();

    // Execute
    let spinner = create_spinner(&format!("{}ing transaction...", action.to_lowercase()));

    if approve {
        ctx.steward.approve_transaction(&tx_id).await?;
        spinner.finish_with_message("Approved!".to_string());
        print_success(&format!("Transaction {} approved", tx_id));
    } else {
        ctx.steward.reject_transaction(&tx_id).await?;
        spinner.finish_with_message("Rejected!".to_string());
        print_success(&format!("Transaction {} rejected", tx_id));
    }

    Ok(())
}