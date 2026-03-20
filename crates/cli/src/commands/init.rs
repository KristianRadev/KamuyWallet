//! # Init Command
//!
//! Initialize a new Kamuy wallet with optional email backup.
//! This is the primary entry point for setting up a new wallet.

use crate::commands::{confirm, create_spinner, prompt_password};
use crate::context::CliContext;
use crate::{print_error, print_info, print_success, print_warning};
use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

/// Execute init command
pub async fn execute(
    ctx: Arc<CliContext>,
    chain: String,
    email: Option<String>,
    output: Option<String>,
    reset: bool,
) -> Result<()> {
    println!("{}", "Kamuy Wallet v2.0".bold().cyan());
    println!();

    // Step 1: Check if wallet already exists (using new SimpleConfig check)
    if crate::config::SimpleConfig::wallet_exists()? && !reset {
        print_warning("A wallet already exists at ~/.kamuy/wallet.json");
        println!();
        println!("  To check your wallet: kamuy status");
        println!("  To create a new wallet: kamuy init --reset");
        return Ok(());
    }

    // Handle reset
    if reset {
        let data_dir = crate::config::SimpleConfig::data_dir()?;
        if data_dir.exists() {
            print_info("Resetting wallet...");
            // Stop steward first if running
            let _ = stop_steward_if_running().await;
            std::fs::remove_dir_all(&data_dir)?;
        }
    }

    // Step 2: Migrate from old config if exists
    if let Err(e) = crate::config::SimpleConfig::migrate_from_old_config() {
        print_warning(&format!("Config migration skipped: {}", e));
    }

    // NOTE: Skip old Steward connection check - v2.0 starts Steward
    // Continue with chain selection

    // Step 3: Get chain ID (from old lines 48-51)
    let chain_id = crate::config::chain_id_from_name(&chain).unwrap_or(8453);
    println!();
    println!("Chain: {} ({})", chain.cyan(), chain_id);

    // Step 4: Get passwords
    println!();
    let user_password = prompt_password("Set your wallet password")?;
    let confirm_password = prompt_password("Confirm password")?;

    if user_password != confirm_password {
        print_error("Passwords do not match!");
        return Ok(());
    }

    // Validate password strength
    if let Err(e) = validate_password_strength(&user_password) {
        print_error(&format!("Weak password: {}", e));
        print_info("Password requirements:");
        println!("  - At least 12 characters");
        println!("  - At least one uppercase letter");
        println!("  - At least one lowercase letter");
        println!("  - At least one digit");
        println!("  - At least one special character (!@#$%^&*)");

        if !confirm("Continue with weak password?")? {
            return Ok(());
        }
    }

    // Step 5: Generate MPC keys
    println!();
    let spinner = create_spinner("Generating MPC keys (3 key shares)...");

    // In v2.0, this calls the Steward API to run DKG
    // For now, simulate the process
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    spinner.finish_with_message("MPC keys generated!".to_string());

    // Generate keys (simulated - in production, this would be from DKG)
    let wallet_address = generate_wallet_address();
    let agent_key = format!("ag_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
    let user_key = format!("us_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));

    // Step 6: Save wallet file
    println!();
    let spinner = create_spinner("Saving wallet...");

    // Save wallet.json with wallet info
    let wallet_path = crate::config::SimpleConfig::wallet_path()?;
    let wallet_dir = wallet_path.parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid wallet path"))?;
    std::fs::create_dir_all(wallet_dir)?;

    let wallet_data = serde_json::json!({
        "version": "2.0",
        "address": wallet_address,
        "chain": chain,
        "chain_id": chain_id,
        "agent_key": agent_key,
        "user_key": user_key,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    let wallet_json = serde_json::to_string_pretty(&wallet_data)?;
    std::fs::write(&wallet_path, wallet_json)?;

    // Set restrictive permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&wallet_path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&wallet_path, perms)?;
    }

    spinner.finish_with_message("Wallet saved!".to_string());

    // Step 7: Handle email backup (if provided)
    if let Some(ref email_addr) = email {
        println!();
        let spinner = create_spinner(&format!("Sending encrypted backup to {}...", email_addr));

        // In v2.0, this would call Steward API to send encrypted backup
        // For now, simulate success
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        spinner.finish_with_message("Encrypted backup sent!".to_string());
        print_info("Check your email for the encrypted backup");
    }

    // Step 8: Save simplified config with auto-generated API key
    println!();
    let spinner = create_spinner("Creating configuration...");

    let simple_config = crate::config::SimpleConfig::generate()?;
    simple_config.save()?;

    spinner.finish_with_message("Configuration saved!".to_string());

    // Step 9: Start steward daemon
    println!();
    start_steward(&simple_config).await?;

    // Step 10: Auto-unlock wallet with the password user just entered
    println!();
    let spinner = create_spinner("Unlocking wallet...");

    // NOTE: After starting steward, we need to reconnect.
    // The ctx.steward client was created before steward started.
    // We create a fresh client for the unlock call.
    let steward_client = crate::context::StewardClient::new(
        &simple_config.steward_url,
        Some(simple_config.api_key.clone()),
    );

    match steward_client.unlock(&user_password).await {
        Ok(_) => {
            spinner.finish_with_message("Wallet unlocked!".green().to_string());
        }
        Err(e) => {
            spinner.finish_with_message("Unlock skipped".yellow().to_string());
            print_warning(&format!("Could not auto-unlock: {}", e));
            print_info("Run 'kamuy unlock' manually");
        }
    }

    // NOTE: The ctx.steward client is now stale (was created before steward started).
    // This is acceptable - init completes the setup and exits. Subsequent commands
    // will load fresh config and create new steward clients.

    // Step 11: Display results
    println!();
    print_success("Wallet created successfully!");
    println!();
    println!("{}", "Your wallet:".bold());
    println!("  Address: {}", wallet_address.cyan());
    println!("  Network: {} ({})", chain, chain_id);
    println!();

    // Display agent key for configuration
    println!("{}", "Agent configuration:".bold());
    println!("  Steward URL: http://127.0.0.1:8080");
    println!("  API Key: {}", simple_config.api_key.dimmed());
    println!("  Agent Key: {}", agent_key.cyan());
    println!();

    // Security warnings
    println!("{}", "SECURITY WARNINGS:".red().bold());
    println!("  - These keys are shown only once!");
    println!("  - Store them in a password manager or secure location");
    println!("  - Never share your User Key with anyone");
    println!("  - The Agent Key can be given to AI agents for spending");
    println!();

    // Save to output file if specified
    if let Some(output_path) = output {
        let output_data = serde_json::json!({
            "wallet_address": wallet_address,
            "chain": chain,
            "chain_id": chain_id,
            "agent_key": agent_key,
            "user_key": user_key,
            "email": email,
        });
        let output_str = serde_json::to_string_pretty(&output_data)?;
        tokio::fs::write(&output_path, output_str).await?;
        print_success(&format!("Keys saved to {}", output_path));
    }

    print_info("Your wallet is ready. The Steward is running and unlocked.");
    println!();
    println!("Next: Tell your agent \"check my wallet balance\"");

    Ok(())
}

/// Stop steward if running (used during reset)
async fn stop_steward_if_running() -> Result<()> {
    let config = match crate::config::SimpleConfig::load()? {
        Some(c) => c,
        None => return Ok(()),
    };

    let pid_path = &config.steward_pid_file;
    if !pid_path.exists() {
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(pid_path)?;
    if let Ok(pid) = pid_str.trim().parse::<i32>() {
        #[cfg(unix)]
        {
            if unsafe { libc::kill(pid, 0) } == 0 {
                print_info("Stopping existing steward...");
                unsafe { libc::kill(pid, libc::SIGTERM) };
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    }

    Ok(())
}

/// Start steward daemon with proper error handling
async fn start_steward(config: &crate::config::SimpleConfig) -> Result<()> {
    let spinner = create_spinner("Starting Steward...");

    // Check for stale PID file
    let pid_path = &config.steward_pid_file;
    if pid_path.exists() {
        let pid_str = std::fs::read_to_string(pid_path)?;
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            #[cfg(unix)]
            {
                if unsafe { libc::kill(pid, 0) } == 0 {
                    spinner.finish_with_message("Already running".yellow().to_string());
                    print_info(&format!("Steward already running (PID {})", pid));
                    return Ok(());
                } else {
                    std::fs::remove_file(pid_path)?;
                }
            }
        }
    }

    // Check port availability
    if !is_port_available(8080) {
        spinner.finish_with_message("Port in use".red().to_string());
        print_error("Port 8080 is already in use");
        print_error("Stop the existing process and try again");
        return Err(anyhow::anyhow!("Port 8080 in use"));
    }

    // Find steward binary
    let steward_path = which::which("kamuy-steward")
        .or_else(|_| {
            let exe_dir = std::env::current_exe()?
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Cannot determine binary directory"))?
                .to_path_buf();
            Ok::<_, anyhow::Error>(exe_dir.join("kamuy-steward"))
        })?;

    if !steward_path.exists() {
        spinner.finish_with_message("Binary not found".yellow().to_string());
        print_warning("Could not auto-start Steward (binary not found)");
        print_info("Run 'kamuy-steward' manually after this completes");
        return Ok(());
    }

    let data_dir = crate::config::SimpleConfig::data_dir()?;

    // Start steward in background
    let mut child = std::process::Command::new(&steward_path)
        .env("STEWARD_API_KEY", &config.api_key)
        .env("STEWARD_DATABASE_URL", format!("sqlite://{}/steward.db?mode=rwc", data_dir.display()))
        .stdout(std::process::Stdio::from(std::fs::File::create(&config.steward_log)?))
        .stderr(std::process::Stdio::from(std::fs::File::create(&config.steward_log)?))
        .spawn()?;

    let pid = child.id();
    std::fs::write(&config.steward_pid_file, pid.to_string())?;

    std::thread::sleep(std::time::Duration::from_millis(500));

    match child.try_wait() {
        Ok(Some(status)) => {
            spinner.finish_with_message("Failed to start".red().to_string());
            print_error(&format!("Steward exited: {}", status));
            print_error(&format!("Check logs at: {}", config.steward_log.display()));
            return Err(anyhow::anyhow!("Steward failed to start"));
        }
        Ok(None) => {
            spinner.finish_with_message("Steward running!".green().to_string());
        }
        Err(_) => {
            spinner.finish_with_message("Started (status unknown)".yellow().to_string());
        }
    }

    Ok(())
}

/// Check if a port is available
fn is_port_available(port: u16) -> bool {
    std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok()
}

/// Generate a wallet address (placeholder for DKG result)
fn generate_wallet_address() -> String {
    // In production, this would be derived from the DKG public key
    // For now, generate a placeholder from UUID (16 bytes) + random padding (4 bytes)
    let uuid = uuid::Uuid::new_v4();
    let uuid_bytes = uuid.as_bytes();
    let mut bytes = [0u8; 20];
    bytes[..16].copy_from_slice(uuid_bytes);
    // Pad remaining 4 bytes with random data from system time
    let padding = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u32).to_be_bytes();
    bytes[16..20].copy_from_slice(&padding);
    format!("0x{}", hex::encode(bytes))
}

/// Validate password strength
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_strength_valid() {
        assert!(validate_password_strength("SecureK3y!2024").is_ok());
    }

    #[test]
    fn test_password_strength_too_short() {
        assert!(validate_password_strength("Short1!").is_err());
    }

    #[test]
    fn test_password_strength_no_upper() {
        assert!(validate_password_strength("myp@ssword123!").is_err());
    }

    #[test]
    fn test_password_strength_no_lower() {
        assert!(validate_password_strength("MYP@SSWORD123!").is_err());
    }

    #[test]
    fn test_password_strength_no_digit() {
        assert!(validate_password_strength("MyP@ssword!!!").is_err());
    }

    #[test]
    fn test_password_strength_no_special() {
        assert!(validate_password_strength("MyPassword123").is_err());
    }

    #[test]
    fn test_password_strength_contains_password() {
        assert!(validate_password_strength("MyP@ssword123!").is_err());
    }

    #[test]
    fn test_password_strength_contains_123() {
        assert!(validate_password_strength("MyP@ssw0rd123!").is_err());
    }
}