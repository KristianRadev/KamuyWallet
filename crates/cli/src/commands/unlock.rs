//! # Unlock Command
//!
//! Load Steward key with password.

use crate::commands::{create_spinner, prompt_password};
use crate::context::CliContext;
use crate::{print_error, print_info, print_success};
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

/// Execute unlock command
pub async fn execute(ctx: Arc<CliContext>) -> Result<()> {
    println!("{}", "🔓 Unlock Wallet".bold().cyan());
    println!();
    
    // Check if already unlocked
    match ctx.steward.health().await {
        Ok(health) if health.status == "healthy" => {
            print_info("Steward is already healthy and ready.");
            return Ok(());
        }
        _ => {}
    }
    
    // Get password
    let password = prompt_password("Enter your wallet password")?;
    
    println!();
    
    // Try to unlock
    let spinner = create_spinner("Unlocking wallet...");
    
    match ctx.steward.unlock(&password).await {
        Ok(()) => {
            spinner.finish_with_message("Wallet unlocked!");
            print_success("Wallet is now unlocked and ready");
        }
        Err(e) => {
            spinner.finish_with_message("Unlock failed");
            print_error(&format!("Failed to unlock: {}", e));
            println!();
            println!("Make sure:");
            println!("  • The password is correct");
            println!("  • The Steward service is running");
            println!("  • The wallet has been created");
        }
    }
    
    Ok(())
}
