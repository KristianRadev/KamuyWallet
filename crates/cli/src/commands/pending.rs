//! # Pending Command
//!
//! List pending approvals from the new inline Telegram approval flow.
//!
//! ## Usage
//!
//! - `kamuy pending` - Show pending transactions in table format
//! - `kamuy pending --format telegram` - Output for agent to forward
//! - `kamuy pending --format json` - Output as JSON

use crate::commands::create_spinner;
use crate::context::{ApprovalRequest, CliContext};
use crate::{print_info, print_table};
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

/// Execute pending command
pub async fn execute(ctx: Arc<CliContext>, format: crate::PendingFormat) -> Result<()> {
    let spinner = create_spinner("Fetching pending approvals...");
    let approvals: Vec<ApprovalRequest> = ctx.steward.get_pending_approvals().await?;
    spinner.finish_with_message(format!("Found {} pending", approvals.len()));

    if approvals.is_empty() {
        match format {
            crate::PendingFormat::Table => {
                print_info("No pending approvals.");
            }
            crate::PendingFormat::Telegram => {
                // Output empty indicator for agent
                println!("KAMUY_NO_PENDING_APPROVALS");
            }
            crate::PendingFormat::Json => {
                println!("{{\"approvals\": [], \"count\": 0}}");
            }
        }
        return Ok(());
    }

    match format {
        crate::PendingFormat::Table => {
            print_table_format(&approvals);
        }
        crate::PendingFormat::Telegram => {
            print_telegram_format(&approvals);
        }
        crate::PendingFormat::Json => {
            print_json_format(&approvals);
        }
    }

    Ok(())
}

/// Print pending approvals in human-readable table format
fn print_table_format(approvals: &[ApprovalRequest]) {
    println!();
    println!("{}", "Pending Approvals".bold().cyan());
    println!();

    let mut rows = Vec::new();

    for approval in approvals {
        let chain_name = match approval.chain_id {
            1 => "Ethereum",
            8453 => "Base",
            137 => "Polygon",
            42161 => "Arbitrum",
            10 => "Optimism",
            _ => "Unknown",
        };

        let status = match approval.status.as_str() {
            "pending" => "Pending".yellow(),
            _ => approval.status.clone().normal(),
        };

        rows.push(vec![
            approval.tx_id.clone(),
            format!("{} {}", approval.amount_display, approval.token),
            crate::commands::format_address(&approval.to),
            chain_name.to_string(),
            status.to_string(),
            approval.reason.clone(),
        ]);
    }

    print_table(
        vec!["ID", "Amount", "To", "Chain", "Status", "Reason"],
        rows,
    );

    println!();
    println!("Use 'kamuy approve <id>' or 'kamuy reject <id>' to act.");
}

/// Print pending approvals in Telegram format for agent to forward
/// This format is designed for the agent to display in its own Telegram chat
fn print_telegram_format(approvals: &[ApprovalRequest]) {
    for approval in approvals {
        // Format for agent to display
        // The agent will parse this and create its own message with buttons
        println!("KAMUY_APPROVAL_REQUEST");
        println!("TX_ID: {}", approval.tx_id);
        println!("TO: {}", approval.to);
        println!("AMOUNT: {} {}", approval.amount_display, approval.token);
        println!("CHAIN: {}", approval.chain_id);
        println!("REASON: {}", approval.reason);
        println!("EXPIRES: {}", approval.expires_at.to_rfc3339());
        println!("BUTTONS: [Approve] [Reject]");
        println!("COMMANDS: kamuy approve {} | kamuy reject {}", approval.tx_id, approval.tx_id);
        println!("KAMUY_END");
    }
}

/// Print pending approvals as JSON
fn print_json_format(approvals: &[ApprovalRequest]) {
    // Manual JSON serialization for the response
    let mut items = Vec::new();
    for approval in approvals {
        let item = format!(
            r#"{{"tx_id":"{}","to":"{}","amount_display":"{}","token":"{}","chain_id":{},"reason":"{}","status":"{}"}}"#,
            approval.tx_id,
            approval.to,
            approval.amount_display,
            approval.token,
            approval.chain_id,
            approval.reason.replace('"', "\\\""),
            approval.status
        );
        items.push(item);
    }
    let json = format!(r#"{{"approvals":[{}],"count":{}}}"#, items.join(","), approvals.len());
    println!("{}", json);
}
