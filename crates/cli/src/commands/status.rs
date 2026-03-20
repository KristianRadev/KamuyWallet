//! # Status Command
//!
//! Check wallet status and balances.

use crate::commands::create_spinner;
use crate::context::CliContext;
use crate::{print_error, print_info, print_success, print_warning};
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

/// Execute status command
pub async fn execute(ctx: Arc<CliContext>, detailed: bool) -> Result<()> {
    println!("{}", "📊 Wallet Status".bold().cyan());
    println!();
    
    // Check Steward connection
    let spinner = create_spinner("Connecting to Steward...");
    
    match ctx.steward.health().await {
        Ok(health) => {
            spinner.finish_with_message(format!("Connected to Steward v{}", health.version));

            if health.status != "healthy" {
                print_warning(&format!("Steward status: {}", health.status));
            }
        }
        Err(e) => {
            spinner.finish_with_message("Connection failed".to_string());
            print_error(&format!("Cannot connect to Steward: {}", e));
            println!();
            println!("Make sure the Steward service is running:");
            println!("  kamuy-steward");
            return Ok(());
        }
    }
    
    println!();
    
    // Get wallet info
    match ctx.steward.get_wallet().await? {
        Some(wallet) => {
            println!("{}", "Wallet Information:".bold());
            println!("  Address: {}", wallet.address.cyan());
            println!("  Chain ID: {}", wallet.chain_id);
            println!();
            
            // Get balances
            let spinner = create_spinner("Fetching balances...");
            let balances = ctx.steward.get_balances().await?;
            spinner.finish_with_message("Balances fetched".to_string());
            
            if balances.is_empty() {
                println!("No token balances found.");
            } else {
                println!("{}", "Balances:".bold());
                for (token, balance) in balances {
                    println!("  {}: {}", token.cyan(), balance);
                }
            }
            
            println!();
            
            // Get pending transactions
            let pending = ctx.steward.get_pending().await?;
            if !pending.is_empty() {
                print_warning(&format!("You have {} pending transaction(s)", pending.len()));
                println!("  Run 'kamuy pending' to view them");
            } else {
                print_success("No pending transactions");
            }
            
            if detailed {
                println!();
                println!("{}", "Configuration:".bold());
                println!("  Steward URL: {}", ctx.config.steward_url);
                println!("  Default Chain: {} ({})", 
                    ctx.config.default_chain, 
                    ctx.config.default_chain_id
                );
                println!("  User Key: {}", 
                    if ctx.has_user_key() { "✓ Present".green() } else { "✗ Missing".red() }
                );
            }
        }
        None => {
            print_error("No wallet configured.");
            println!();
            println!("Create a wallet with:");
            println!("  kamuy create-wallet");
        }
    }
    
    Ok(())
}
