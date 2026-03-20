//! # Config Command
//!
//! Manage CLI configuration.

use crate::commands::create_spinner;
use crate::context::CliContext;
use crate::{print_error, print_info, print_success};
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

/// Show configuration
pub async fn show(ctx: Arc<CliContext>) -> Result<()> {
    println!("{}", "⚙️  Configuration".bold().cyan());
    println!();
    
    println!("{}", "Current Settings:".bold());
    println!("  Steward URL: {}", ctx.config.steward_url.cyan());
    println!("  API Key: {}", 
        if ctx.config.api_key.is_some() { "✓ Set".green() } else { "✗ Not set".red() }
    );
    println!("  Default Chain: {} ({})", 
        ctx.config.default_chain.cyan(),
        ctx.config.default_chain_id
    );
    println!("  Config Directory: {:?}", ctx.config.config_dir);
    println!("  Data Directory: {:?}", ctx.config.data_dir);
    
    Ok(())
}

/// Set configuration value
pub async fn set(ctx: Arc<CliContext>, key: String, value: String) -> Result<()> {
    println!("{}", "⚙️  Update Configuration".bold().cyan());
    println!();
    
    print_info(&format!("Setting {} = {}", key, value));
    
    // Update config
    let mut config = ctx.config.clone();
    
    match key.as_str() {
        "steward_url" => config.steward_url = value,
        "api_key" => config.api_key = Some(value),
        "default_chain" => {
            config.default_chain = value.clone();
            if let Some(id) = crate::config::chain_id_from_name(&value) {
                config.default_chain_id = id;
            }
        }
        _ => {
            print_error(&format!("Unknown configuration key: {}", key));
            return Ok(());
        }
    }
    
    // Save
    config.save()?;
    
    print_success(&format!("Updated {}", key));
    
    Ok(())
}

/// Initialize configuration
pub async fn init(_ctx: Arc<CliContext>) -> Result<()> {
    println!("{}", "⚙️  Initialize Configuration".bold().cyan());
    println!();
    
    let spinner = create_spinner("Creating configuration...");
    
    let config = crate::config::CliConfig::init()?;
    
    spinner.finish_with_message("Configuration created!");
    
    println!();
    print_success(&format!("Configuration initialized at {:?}", config.config_dir));
    println!();
    println!("Edit {:?} to customize settings.", config.config_dir.join("config.toml"));

    Ok(())
}

/// Get a configuration value
pub async fn get(ctx: Arc<CliContext>, key: String) -> Result<()> {
    // First try to load from simple config
    if let Some(simple) = crate::config::SimpleConfig::load()? {
        match key.as_str() {
            "api_key" => println!("{}", simple.api_key),
            "steward_url" | "url" => println!("{}", simple.steward_url),
            "wallet_path" => println!("{}", simple.wallet_path.display()),
            "steward_log" => println!("{}", simple.steward_log.display()),
            "steward_pid_file" => println!("{}", simple.steward_pid_file.display()),
            _ => {
                print_error(&format!("Unknown config key: {}", key));
                print_info("Available keys: api_key, steward_url, wallet_path, steward_log, steward_pid_file");
                return Err(anyhow::anyhow!("Unknown config key"));
            }
        }
    } else {
        // Fall back to legacy config
        match key.as_str() {
            "api_key" => {
                match &ctx.config.api_key {
                    Some(k) => println!("{}", k),
                    None => println!("(not set)"),
                }
            }
            "steward_url" | "url" => println!("{}", ctx.config.steward_url),
            _ => {
                print_error(&format!("Unknown config key: {}", key));
                return Err(anyhow::anyhow!("Unknown config key"));
            }
        }
    }

    Ok(())
}
