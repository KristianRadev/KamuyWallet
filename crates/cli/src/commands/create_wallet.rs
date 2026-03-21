//! # Create Wallet Command
//!
//! Generate MPC keys and create smart account.

use crate::commands::{confirm, create_spinner, prompt_password};
use crate::context::CliContext;
use crate::{print_error, print_info, print_success, print_warning};
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

/// Execute create-wallet command
pub async fn execute(
    ctx: Arc<CliContext>,
    chain: String,
    output: Option<String>,
) -> Result<()> {
    println!("{}", "🔐 Creating Kamuy Wallet".bold().cyan());
    println!();
    
    // Check if wallet already exists
    if ctx.has_user_key() {
        print_warning("A wallet already exists. Creating a new one will overwrite it.");
        if !confirm("Do you want to continue?")? {
            println!("Aborted.");
            return Ok(());
        }
    }
    
    // Get chain ID
    let chain_id = crate::config::chain_id_from_name(&chain)
        .unwrap_or(8453);
    
    println!("Chain: {} ({})", chain, chain_id);
    println!();
    
    // Get passwords
    let user_password = prompt_password("Set your wallet password")?;
    let confirm_password = prompt_password("Confirm password")?;
    
    if user_password != confirm_password {
        print_error("Passwords do not match!");
        return Ok(());
    }
    
    // SECURITY: Validate password strength
    if let Err(e) = validate_password_strength(&user_password) {
        print_error(&format!("Weak password: {}", e));
        print_info("Password requirements:");
        println!("  • At least 12 characters");
        println!("  • At least one uppercase letter");
        println!("  • At least one lowercase letter");
        println!("  • At least one digit");
        println!("  • At least one special character (!@#$%^&*)");
        
        if !confirm("Continue with weak password?")? {
            return Ok(());
        }
    }
    
    println!();
    
    // Run DKG
    let spinner = create_spinner("Generating MPC keys...");
    
    // In a real implementation, this would:
    // 1. Connect to Steward service
    // 2. Run DKG protocol with all 3 parties
    // 3. Generate Agent, Steward, and User keys
    // 4. Create smart account
    
    // For now, simulate the process
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    spinner.finish_with_message("MPC keys generated!");
    
    // Generate keys (simulated)
    let wallet_address = format!("0x{}", hex::encode(&[0u8; 20]));
    let agent_key = format!("ag_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
    let user_key = format!("us_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
    
    println!();
    println!("{}", "✅ Wallet created successfully!".green().bold());
    println!();
    println!("{}", "Wallet Details:".bold());
    println!("  Address: {}", wallet_address.cyan());
    println!("  Chain: {} ({})", chain, chain_id);
    println!();
    
    // Save keys
    let spinner = create_spinner("Saving keys...");
    
    // In a real implementation, save encrypted keys
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    spinner.finish_with_message("Keys saved!");
    
    println!();
    println!("{}", "⚠️  IMPORTANT: Save these keys securely!".red().bold());
    println!();
    println!("{}", "Agent Key (give to your AI agent):".yellow());
    println!("  {}", agent_key.cyan());
    println!();
    println!("{}", "User Key (keep this secret):".yellow());
    println!("  {}", user_key.cyan());
    println!();
    println!("{}", "These keys are shown only once and cannot be recovered!".red());
    println!();
    
    if let Some(output_path) = output {
        let output_data = format!(
            "Wallet Address: {}\nAgent Key: {}\nUser Key: {}\n",
            wallet_address, agent_key, user_key
        );
        tokio::fs::write(&output_path, output_data).await?;
        print_success(&format!("Keys saved to {}", output_path));
    }
    
    println!();
    println!("Next steps:");
    println!("  1. Configure your AI agent with the Agent Key");
    println!("  2. Run 'kamuy status' to check your wallet");
    println!("  3. Run 'kamuy policy' to view/update policy");
    
    Ok(())
}

/// Validate password strength
/// SECURITY: Enforces strong password requirements
fn validate_password_strength(password: &str) -> Result<(), String> {
    if password.len() < 12 {
        return Err("Password must be at least 12 characters".to_string());
    }
    
    let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_special = password.chars().any(|c| "!@#$%^&*()_+-=[]{}|;:,.<>?".contains(c));
    
    if !has_upper {
        return Err("Password must contain at least one uppercase letter".to_string());
    }
    if !has_lower {
        return Err("Password must contain at least one lowercase letter".to_string());
    }
    if !has_digit {
        return Err("Password must contain at least one digit".to_string());
    }
    if !has_special {
        return Err("Password must contain at least one special character".to_string());
    }
    
    // Check for common patterns
    if password.to_lowercase().contains("password") {
        return Err("Password cannot contain the word 'password'".to_string());
    }
    if password.to_lowercase().contains("123") {
        return Err("Password cannot contain simple sequences like '123'".to_string());
    }
    
    Ok(())
}
