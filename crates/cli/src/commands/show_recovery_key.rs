//! # Show Recovery Key Command
//!
//! Display the User Key (recovery key) with password authentication.
//! SECURITY: User Key is never shown in init output, must be retrieved separately.

use crate::commands::prompt_password;
use crate::context::CliContext;
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

pub async fn execute(ctx: Arc<CliContext>) -> Result<()> {
    println!("{}", "Recovery Key Retrieval".bold());
    println!();
    println!("This will display your User Key (recovery key).");
    println!("You will need your wallet password.");
    println!();

    // Prompt for password
    let password = prompt_password("Wallet password")?;

    // Call Steward API to get recovery key
    let recovery_key = ctx.steward.get_recovery_key(&password).await?;

    // Display with security warnings
    println!();
    println!("{}", "═══════════════════════════════════════════════════════════".red().bold());
    println!("{}", "  WARNING: USER KEY - SAVE THIS SECURELY - SHOWN ONLY ONCE  ".red().bold());
    println!("{}", "═══════════════════════════════════════════════════════════".red().bold());
    println!();
    println!("  User Key: {}", recovery_key.yellow().bold());
    println!();
    println!("{}", "This is your RECOVERY key.".yellow());
    println!("{}", "If you lose access to this device, you can use this key to recover your wallet.".yellow());
    println!();
    println!("{}", "SECURITY:".red().bold());
    println!("  - Write this down or save in a password manager NOW");
    println!("  - NEVER share this key with anyone");
    println!("  - This key is NOT stored anywhere on disk");
    println!();

    Ok(())
}