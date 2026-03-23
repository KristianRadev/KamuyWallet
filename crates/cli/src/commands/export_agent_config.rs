//! # Export Agent Config Command
//!
//! Export agent configuration for AI agent setup.
//! SECURITY: Only exports Agent Key and API Key, NOT password or User Key.

use crate::commands::prompt_password;
use crate::context::CliContext;
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

pub async fn execute(ctx: Arc<CliContext>) -> Result<()> {
    println!("{}", "Export Agent Configuration".bold());
    println!();
    println!("This will display your agent configuration.");
    println!("Give this to your AI agent so it can interact with your wallet.");
    println!();
    println!("{}", "This does NOT include your password or recovery key.".yellow());
    println!();

    // Get wallet info to confirm wallet exists
    let wallet = ctx.steward.get_wallet().await?;
    if wallet.is_none() {
        println!("{}", "No wallet found. Run 'kamuy init' first.".red());
        return Ok(());
    }

    // Prompt for password to verify identity
    let password = prompt_password("Wallet password")?;

    // Verify password by trying to unlock (or call a verify endpoint)
    // This ensures only the wallet owner can export the config
    if ctx.steward.unlock(&password).await.is_err() {
        println!("{}", "Invalid password.".red());
        return Ok(());
    }

    // Get the config
    let config = crate::config::SimpleConfig::load()?.ok_or_else(|| anyhow::anyhow!("Config not found"))?;

    // Get agent key from steward (password protected endpoint)
    let agent_key = ctx.steward.get_agent_key(&password).await?;

    println!();
    println!("{}", "================================================================".green().bold());
    println!("{}", "  AGENT CONFIGURATION".green().bold());
    println!("{}", "================================================================".green().bold());
    println!();
    println!("Copy this to your AI agent's configuration:");
    println!();
    println!("  {{");
    println!("    \"kamuy\": {{");
    println!("      \"steward_url\": \"{}\",", config.steward_url);
    println!("      \"api_key\": \"{}\",", config.api_key);
    println!("      \"agent_key\": \"{}\"", agent_key.cyan());
    println!("    }}");
    println!("  }}");
    println!();
    println!("{}", "----------------------------------------------------------------".dimmed());
    println!();
    println!("{}", "What your AI agent can do with this config:".bold());
    println!("  [OK] Check your wallet balance");
    println!("  [OK] Send payments to whitelisted addresses (within limits)");
    println!("  [OK] Request policy changes (requires your approval)");
    println!();
    println!("{}", "What your AI agent CANNOT do:".red().bold());
    println!("  [X] Change your password");
    println!("  [X] Access your recovery key");
    println!("  [X] Approve high-risk transactions without your password");
    println!();
    println!("{}", "Keep your password and recovery key secret!".yellow());
    println!();

    Ok(())
}