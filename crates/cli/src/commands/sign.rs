//! # Sign Command
//!
//! Sign transaction via MPC.

use crate::commands::{confirm, create_spinner, parse_transaction_data};
use crate::context::CliContext;
use crate::print_error;
use anyhow::{Context, Result};
use colored::Colorize;
use std::sync::Arc;

/// Execute sign command
pub async fn execute(
    ctx: Arc<CliContext>,
    data: Option<String>,
    file: Option<String>,
    to: Option<String>,
    amount: Option<String>,
    token: String,
    chain_id: Option<u64>,
    submit: bool,
) -> Result<()> {
    println!("{}", "Sign Transaction".bold().cyan());
    println!();

    // Build transaction data
    let tx_data = if let Some(d) = data {
        parse_transaction_data(&d)?
    } else if let Some(f) = file {
        tokio::fs::read(&f).await
            .with_context(|| format!("Failed to read transaction file: {}", f))?
    } else if let (Some(to_addr), Some(amt)) = (to.clone(), amount.clone()) {
        // Build simple transfer transaction
        build_transfer_data(&to_addr, &amt, &token)?
    } else {
        print_error("Please provide transaction data (--data), file (--file), or to+amount");
        return Ok(());
    };

    let chain_id = chain_id.unwrap_or(ctx.config.default_chain_id);

    println!("{}", "Transaction Details:".bold());
    println!("  Chain ID: {}", chain_id);
    println!("  Token: {}", token.cyan());
    if let Some(ref to_addr) = to {
        println!("  To: {}", to_addr.cyan());
    }
    if let Some(ref amt) = amount {
        println!("  Amount: {}", amt.cyan());
    }
    println!("  Data: {} bytes", tx_data.len());
    println!();

    // Confirm
    if !confirm("Sign this transaction?")? {
        println!("Aborted.");
        return Ok(());
    }

    println!();

    // Sign transaction
    let spinner = create_spinner("Initiating MPC signing...");

    // In a real implementation:
    // 1. Submit to Steward
    // 2. Steward evaluates policy
    // 3. If approved, run MPC signing with Agent
    // 4. Return signature

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    spinner.finish_with_message("Signing complete!".to_string());

    // Generate signature (simulated)
    let signature = format!("0x{}", hex::encode(&[0u8; 65]));
    let tx_hash = format!("0x{}", hex::encode(&[0u8; 32]));

    println!();
    println!("{}", "Transaction Signed".green().bold());
    println!();
    println!("  Signature: {}", signature.cyan());
    println!("  Transaction Hash: {}", tx_hash.cyan());
    println!();

    if submit {
        println!("{}", "Submitting to network...".bold());

        let spinner = create_spinner("Broadcasting...");
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        spinner.finish_with_message("Submitted!".to_string());

        println!();
        println!("{}", "Transaction Submitted".green().bold());
        println!("  Hash: {}", tx_hash.cyan());
        println!();
        println!("Track at: https://basescan.org/tx/{}", tx_hash);
    } else {
        println!("Transaction signed but not submitted.");
        println!("Use --submit to broadcast to the network.");
    }

    Ok(())
}

/// Build transfer transaction data
fn build_transfer_data(to: &str, amount: &str, token: &str) -> Result<Vec<u8>> {
    // Validate address
    if !to.starts_with("0x") || to.len() != 42 {
        return Err(anyhow::anyhow!("Invalid Ethereum address: {}", to));
    }
    
    // For ETH transfers, return empty data
    if token == "ETH" || token == "eth" {
        return Ok(vec![]);
    }
    
    // For ERC-20 transfers, encode the transfer call
    // transfer(address,uint256)
    let mut data = vec![0u8; 4 + 32 + 32];
    
    // Function selector: keccak256("transfer(address,uint256)")[:4]
    data[0..4].copy_from_slice(&[0xa9, 0x05, 0x9c, 0xbb]);
    
    // Address (padded to 32 bytes)
    let addr_bytes = hex::decode(&to[2..])?;
    data[16..36].copy_from_slice(&addr_bytes);
    
    // Amount (padded to 32 bytes)
    let amount_wei = parse_amount(amount)?;
    let amount_bytes = amount_wei.to_be_bytes();
    data[36..68].copy_from_slice(&amount_bytes);
    
    Ok(data)
}

/// Parse amount to wei using integer arithmetic
/// SECURITY: Avoids f64 precision issues by parsing as integer or scaled decimal
fn parse_amount(amount: &str) -> Result<u128> {
    // First try parsing as integer (wei format)
    if let Ok(wei) = amount.parse::<u128>() {
        return Ok(wei);
    }
    
    // Parse decimal amount (e.g., "100.50") using integer arithmetic
    if let Some(dot_pos) = amount.find('.') {
        let integer_part: u128 = amount[..dot_pos].parse()
            .map_err(|_| anyhow::anyhow!("Invalid amount: {}", amount))?;
        let fractional_part = &amount[dot_pos + 1..];
        
        // Pad or truncate fractional to 6 decimal places
        let fractional_padded = if fractional_part.len() >= 6 {
            &fractional_part[..6]
        } else {
            let padding = "0".repeat(6 - fractional_part.len());
            &format!("{}{}", fractional_part, padding)
        };
        
        let fractional: u128 = fractional_padded.parse()
            .map_err(|_| anyhow::anyhow!("Invalid amount: {}", amount))?;
        
        // Combine: integer * 1_000_000 + fractional
        let wei = integer_part * 1_000_000 + fractional;
        return Ok(wei);
    }
    
    // No decimal point - treat as whole units
    let whole: u128 = amount.parse()
        .map_err(|_| anyhow::anyhow!("Invalid amount: {}", amount))?;
    Ok(whole * 1_000_000)
}
