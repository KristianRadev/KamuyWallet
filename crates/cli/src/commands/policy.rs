//! # Policy Command
//!
//! View and update policy.

use crate::commands::{confirm, create_spinner};
use crate::context::CliContext;
use crate::{print_info, print_success, print_warning};
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

/// Show current policy
pub async fn show(ctx: Arc<CliContext>) -> Result<()> {
    println!("{}", "Current Policy".bold().cyan());
    println!();

    let spinner = create_spinner("Fetching policy...");
    let policy = ctx.steward.get_policy().await?;
    spinner.finish_with_message("Policy loaded".to_string());

    println!();
    println!("{}", "Spending Limits:".bold());
    println!("  Max per transaction: {} USDC", format_usdc(policy.max_per_tx).cyan());
    println!("  Max daily: {} USDC", format_usdc(policy.max_daily).cyan());
    println!("  Max weekly: {} USDC", format_usdc(policy.max_weekly).cyan());
    println!("  Auto-add threshold: {} USDC", format_usdc(policy.auto_add_threshold).cyan());

    println!();
    println!("{}", "Settings:".bold());
    println!("  Token: {}", policy.token.cyan());
    println!("  Gasless: {}", if policy.gasless { "Yes".green() } else { "No".red() });

    println!();
    println!("{}", "Spending Tracker:".bold());
    println!("  Daily spent: {} USDC", format_usdc(policy.spending_tracker.daily_spent).cyan());
    println!("  Weekly spent: {} USDC", format_usdc(policy.spending_tracker.weekly_spent).cyan());

    let whitelist = policy.whitelist.entries();
    if !whitelist.is_empty() {
        println!();
        println!("{}", "Whitelist:".bold());
        for (addr, meta) in whitelist {
            println!("  {} - {} (added: {})",
                addr.cyan(),
                meta.label,
                meta.added_at.format("%Y-%m-%d")
            );
        }
    }

    Ok(())
}

/// Set a policy value
pub async fn set(ctx: Arc<CliContext>, key: String, value: String) -> Result<()> {
    println!("{}", "Update Policy".bold().cyan());
    println!();

    println!("Updating: {} = {}", key.cyan(), value.cyan());

    let spinner = create_spinner("Updating policy...");

    // Get current policy
    let mut policy = ctx.steward.get_policy().await?;

    // Update value based on key
    match key.as_str() {
        "max_per_tx" => {
            let usdc_value = parse_usdc(&value)?;
            policy.max_per_tx = usdc_value;
        }
        "max_daily" => {
            let usdc_value = parse_usdc(&value)?;
            policy.max_daily = usdc_value;
        }
        "max_weekly" => {
            let usdc_value = parse_usdc(&value)?;
            policy.max_weekly = usdc_value;
        }
        "auto_add_threshold" => {
            let usdc_value = parse_usdc(&value)?;
            policy.auto_add_threshold = usdc_value;
        }
        "gasless" => {
            policy.gasless = value.parse().map_err(|e| anyhow::anyhow!("Invalid boolean value: {}", e))?;
        }
        _ => {
            return Err(anyhow::anyhow!("Unknown policy key: {}. Valid keys: max_per_tx, max_daily, max_weekly, auto_add_threshold, gasless", key));
        }
    }

    // Validate
    policy.validate()?;

    // Save
    ctx.steward.update_policy(&policy).await?;

    spinner.finish_with_message("Policy updated!".to_string());
    print_success(&format!("Updated {} to {}", key, value));

    Ok(())
}

/// Edit policy in default editor
pub async fn edit(_ctx: Arc<CliContext>) -> Result<()> {
    print_info("Policy editor not yet implemented.");
    print_info("Use 'kamuy policy set <key> <value>' instead.");
    Ok(())
}

/// Reset to default policy
pub async fn reset(ctx: Arc<CliContext>) -> Result<()> {
    println!("{}", "Reset Policy".bold().cyan());
    println!();

    print_warning("This will reset all policy settings to defaults!");

    if !confirm("Continue?")? {
        println!("Aborted.");
        return Ok(());
    }

    let spinner = create_spinner("Resetting policy...");

    let policy = kamuy_steward::policy::PolicyRules::default();
    ctx.steward.update_policy(&policy).await?;

    spinner.finish_with_message("Policy reset!".to_string());
    print_success("Policy reset to defaults");

    Ok(())
}

/// Format USDC micros to human-readable string
fn format_usdc(micros: u64) -> String {
    let whole = micros / 1_000_000;
    let frac = micros % 1_000_000;
    if frac == 0 {
        format!("{}", whole)
    } else {
        format!("{}.{:06}", whole, frac).trim_end_matches('0').to_string()
    }
}

/// Parse USDC value (supports both micros and decimal format)
fn parse_usdc(value: &str) -> Result<u64> {
    // If it's a plain integer, treat as micros
    if let Ok(micros) = value.parse::<u64>() {
        return Ok(micros);
    }

    // If it contains a decimal point, convert from USDC to micros
    if value.contains('.') {
        let parts: Vec<&str> = value.split('.').collect();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!("Invalid USDC format: {}", value));
        }

        let whole: u64 = parts[0].parse().unwrap_or(0);
        let frac_str = parts[1];
        let frac: u64 = if frac_str.len() <= 6 {
            frac_str.parse::<u64>()? * 10u64.pow(6 - frac_str.len() as u32)
        } else {
            frac_str[..6].parse::<u64>()?
        };

        return Ok(whole * 1_000_000 + frac);
    }

    Err(anyhow::anyhow!("Invalid USDC value: {}", value))
}