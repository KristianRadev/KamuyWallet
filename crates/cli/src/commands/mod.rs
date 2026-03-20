//! # CLI Commands
//!
//! Command implementations for the Kamuy CLI.

pub mod approve;
pub mod completions;
pub mod config_cmd;
pub mod create_wallet;
pub mod history;
pub mod init;
pub mod lock;
pub mod pending;
pub mod policy;
pub mod recover;
pub mod rotate;
pub mod sign;
pub mod status;
pub mod unlock;

use crate::context::CliContext;
use anyhow::Result;
use std::sync::Arc;

/// Command trait for all CLI commands
#[async_trait::async_trait]
pub trait Command: Send + Sync {
    async fn execute(&self, ctx: Arc<CliContext>) -> Result<()>;
}

/// Prompt for password securely
pub fn prompt_password(prompt: &str) -> Result<String> {
    use dialoguer::Password;
    
    let password = Password::new()
        .with_prompt(prompt)
        .interact()
        .map_err(|e| anyhow::anyhow!("Failed to read password: {}", e))?;
    
    Ok(password)
}

/// Prompt for confirmation
pub fn confirm(prompt: &str) -> Result<bool> {
    use dialoguer::Confirm;
    
    let confirmed = Confirm::new()
        .with_prompt(prompt)
        .default(false)
        .interact()
        .map_err(|e| anyhow::anyhow!("Failed to read confirmation: {}", e))?;
    
    Ok(confirmed)
}

/// Prompt for input
pub fn prompt_input(prompt: &str) -> Result<String> {
    use dialoguer::Input;
    
    let input: String = Input::new()
        .with_prompt(prompt)
        .interact_text()
        .map_err(|e| anyhow::anyhow!("Failed to read input: {}", e))?;
    
    Ok(input)
}

/// Print a table
pub fn print_table(headers: Vec<&str>, rows: Vec<Vec<String>>) {
    use comfy_table::{Table, ContentArrangement};
    
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(headers);
    
    for row in rows {
        table.add_row(row);
    }
    
    println!("{}", table);
}

/// Format address for display
pub fn format_address(addr: &str) -> String {
    if addr.len() == 42 && addr.starts_with("0x") {
        format!("{}...{}", &addr[..10], &addr[addr.len()-8..])
    } else {
        addr.to_string()
    }
}

/// Format amount for display
pub fn format_amount(amount: &str, decimals: u8) -> String {
    if let Ok(wei) = amount.parse::<u128>() {
        let divisor = 10u128.pow(decimals as u32);
        let integer = wei / divisor;
        let fractional = wei % divisor;
        format!("{}.{:0>width$}", integer, fractional, width = decimals as usize)
    } else {
        amount.to_string()
    }
}

/// Parse transaction data
pub fn parse_transaction_data(data: &str) -> Result<Vec<u8>> {
    // Remove 0x prefix if present
    let data = data.trim_start_matches("0x").trim_start_matches("0X");
    
    // Validate hex
    if data.len() % 2 != 0 {
        return Err(anyhow::anyhow!("Invalid hex data: odd length"));
    }
    
    hex::decode(data)
        .map_err(|e| anyhow::anyhow!("Invalid hex data: {}", e))
}

/// Spinner for long-running operations
pub fn create_spinner(msg: &str) -> indicatif::ProgressBar {
    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_style(
        indicatif::ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap()
    );
    spinner.set_message(msg.to_string());
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));
    spinner
}
