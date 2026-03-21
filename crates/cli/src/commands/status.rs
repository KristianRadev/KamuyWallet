//! # Status Command
//!
//! Check wallet status and balances.

use crate::commands::create_spinner;
use crate::context::CliContext;
use crate::{print_error, print_success, print_warning};
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

use crate::config::SimpleConfig;

/// Execute status command
pub async fn execute(ctx: Arc<CliContext>, detailed: bool) -> Result<()> {
    println!("{}", "📊 Wallet Status".bold().cyan());
    println!();

    // Check steward process status from PID file
    let steward_status = check_steward_status();
    println!("{}", "Steward:".bold());
    match steward_status {
        Ok((pid, running)) => {
            if running {
                println!("  Status: {}", "running".green());
                println!("  PID: {}", pid);
            } else {
                println!("  Status: {} (stale PID: {})", "stopped".yellow(), pid);
            }
        }
        Err(_) => {
            println!("  Status: {}", "not running".red());
        }
    }
    println!();

    // Check Steward connection
    let spinner = create_spinner("Connecting to Steward API...");

    match ctx.steward.health().await {
        Ok(health) => {
            spinner.finish_with_message(format!("API: Steward v{}", health.version));

            if health.status != "healthy" {
                print_warning(&format!("Steward status: {}", health.status));
            }
        }
        Err(e) => {
            spinner.finish_with_message("API: Connection failed".to_string());
            print_error(&format!("Cannot connect to Steward API: {}", e));
            println!();
            println!("To start steward:");
            println!("  kamuy start");
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

/// Check steward status from PID file
fn check_steward_status() -> Result<(i32, bool)> {
    let config = SimpleConfig::load()?
        .ok_or_else(|| anyhow::anyhow!("No config found"))?;

    let pid_path = &config.steward_pid_file;
    if !pid_path.exists() {
        return Err(anyhow::anyhow!("No PID file"));
    }

    let pid_str = std::fs::read_to_string(pid_path)?;
    let pid: i32 = pid_str.trim().parse()
        .map_err(|_| anyhow::anyhow!("Invalid PID"))?;

    #[cfg(unix)]
    {
        use libc::kill;
        let running = unsafe { kill(pid, 0) } == 0;
        Ok((pid, running))
    }

    #[cfg(not(unix))]
    {
        Ok((pid, false))
    }
}
