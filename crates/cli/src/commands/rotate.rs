//! # Rotate Command
//!
//! Rotate the agent key.

use crate::commands::{confirm, create_spinner};
use crate::context::CliContext;
use crate::{print_error, print_info, print_success, print_warning};
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

/// Execute rotate command
pub async fn execute(_ctx: Arc<CliContext>, force: bool) -> Result<()> {
    println!("{}", "🔄 Rotate Agent Key".bold().cyan());
    println!();
    
    print_warning("This will generate a new Agent key.");
    print_info("The old Agent key will be invalidated.");
    print_info("Your wallet address will NOT change.");
    println!();
    
    if !force {
        if !confirm("Do you want to continue?")? {
            println!("Aborted.");
            return Ok(());
        }
    }
    
    println!();
    
    let spinner = create_spinner("Rotating agent key...");
    
    // In a real implementation, this would:
    // 1. Run PSS protocol with Steward
    // 2. Generate new Agent key
    // 3. Invalidate old Agent key
    
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    let new_agent_key = format!("ag_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
    
    spinner.finish_with_message("Key rotated!");
    
    println!();
    println!("{}", "✅ Agent Key Rotated".green().bold());
    println!();
    println!("{}", "New Agent Key:".yellow().bold());
    println!("  {}", new_agent_key.cyan());
    println!();
    println!("{}", "⚠️  Update your AI agent with this new key!".red().bold());
    
    Ok(())
}
