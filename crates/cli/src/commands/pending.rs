//! # Pending Command
//!
//! List pending transactions.

use crate::commands::create_spinner;
use crate::context::CliContext;
use crate::{print_info, print_table};
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

/// Execute pending command
pub async fn execute(ctx: Arc<CliContext>) -> Result<()> {
    println!("{}", "⏳ Pending Transactions".bold().cyan());
    println!();
    
    let spinner = create_spinner("Fetching pending transactions...");
    let pending = ctx.steward.get_pending().await?;
    spinner.finish_with_message(format!("Found {} pending", pending.len()));
    
    if pending.is_empty() {
        print_info("No pending transactions.");
        return Ok(());
    }
    
    println!();
    
    // Build table
    let mut rows = Vec::new();
    
    for tx in &pending {
        let status = match tx.status {
            kamuy_steward::types::TransactionStatus::AwaitingApproval => "🔄 Awaiting Approval".yellow(),
            kamuy_steward::types::TransactionStatus::Pending => "⏳ Pending".to_string().normal(),
            _ => format!("{:?}", tx.status).normal(),
        };
        
        rows.push(vec![
            tx.id.to_string(),
            tx.request.value.clone(),
            tx.request.token.clone(),
            crate::commands::format_address(&tx.request.to),
            status.to_string(),
        ]);
    }
    
    print_table(
        vec!["ID", "Amount", "Token", "To", "Status"],
        rows,
    );
    
    println!();
    println!("Use 'kamuy approve <id>' or 'kamuy reject <id>' to act on transactions.");
    
    Ok(())
}
