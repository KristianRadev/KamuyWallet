# Zero-Friction Local Setup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Simplify wallet setup to one command: `kamuy init --email user@example.com`

**Architecture:** Auto-generate API key stored in `~/.kamuy/config.json`, steward auto-starts as daemon, auto-unlock after init. Config file replaces env vars with env var override support.

**Tech Stack:** Rust, JSON config, daemon process management, SQLite

---

## File Structure

| File | Purpose |
|------|---------|
| `crates/cli/src/config.rs` | Load/save JSON config, generate API key, env var overrides |
| `crates/cli/src/commands/init.rs` | Wallet exists check, steward start, auto-unlock |
| `crates/cli/src/commands/start.rs` | NEW - Start steward daemon |
| `crates/cli/src/commands/stop.rs` | NEW - Stop steward daemon |
| `crates/cli/src/commands/status.rs` | Update to show steward PID status |
| `crates/cli/src/commands/config_cmd.rs` | Add `get` subcommand |
| `crates/cli/src/commands/mod.rs` | Wire up start/stop commands |
| `crates/cli/src/main.rs` | Add Start/Stop to Commands enum, update Init |
| `crates/cli/Cargo.toml` | Add rand, which, libc dependencies |

---

## Task 1: Update Config for JSON, Auto-Generated API Key, and Env Var Overrides

**Files:**
- Modify: `crates/cli/src/config.rs`
- Modify: `crates/cli/Cargo.toml`

- [ ] **Step 1: Add new config struct after CliConfig implementation**

Add after line 161 in `config.rs` (after the closing `}` of `impl CliConfig`):

```rust

// ============================================================================
// SimpleConfig - v2.0 Zero-Friction Configuration
// ============================================================================

/// Simplified v2.0 config stored at ~/.kamuy/config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleConfig {
    /// Config version
    pub version: String,
    /// Steward service URL
    pub steward_url: String,
    /// Auto-generated API key
    pub api_key: String,
    /// Path to wallet file
    pub wallet_path: PathBuf,
    /// Path to steward log
    pub steward_log: PathBuf,
    /// Path to steward PID file
    pub steward_pid_file: PathBuf,
}

impl SimpleConfig {
    /// Default config path: ~/.kamuy/config.json
    pub fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(".kamuy").join("config.json"))
    }

    /// Wallet path: ~/.kamuy/wallet.json
    pub fn wallet_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(".kamuy").join("wallet.json"))
    }

    /// Data directory: ~/.kamuy/
    pub fn data_dir() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(".kamuy"))
    }

    /// Load config with priority: KAMUY_CONFIG env -> ~/.kamuy/config.json
    /// Also applies env var overrides for api_key and steward_url
    pub fn load() -> Result<Option<Self>> {
        // Check for explicit config path
        let path = if let Ok(custom_path) = std::env::var("KAMUY_CONFIG") {
            PathBuf::from(custom_path)
        } else {
            Self::config_path()?
        };

        if !path.exists() {
            return Ok(None);
        }

        Self::load_from_path(&path)
    }

    /// Load config from a specific path (for --config flag support)
    pub fn load_from_path(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config: {:?}", path))?;

        let mut config: SimpleConfig = serde_json::from_str(&content)
            .with_context(|| "Failed to parse config.json")?;

        // Apply env var overrides
        if let Ok(api_key) = std::env::var("KAMUY_API_KEY") {
            config.api_key = api_key;
        }
        if let Ok(steward_url) = std::env::var("KAMUY_STEWARD_URL") {
            config.steward_url = steward_url;
        }

        Ok(Some(config))
    }

    /// Save config to ~/.kamuy/config.json
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let dir = path.parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid config path"))?;

        std::fs::create_dir_all(dir)
            .with_context(|| "Failed to create ~/.kamuy directory")?;

        let content = serde_json::to_string_pretty(self)
            .with_context(|| "Failed to serialize config")?;

        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write config: {:?}", path))?;

        // Set restrictive permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&path, perms)?;
        }

        Ok(())
    }

    /// Generate new config with random API key
    pub fn generate() -> Result<Self> {
        let api_key = Self::generate_api_key();
        let data_dir = Self::data_dir()?;

        Ok(Self {
            version: "2.0".to_string(),
            steward_url: "http://127.0.0.1:8080".to_string(),
            api_key,
            wallet_path: data_dir.join("wallet.json"),
            steward_log: data_dir.join("steward.log"),
            steward_pid_file: data_dir.join("steward.pid"),
        })
    }

    /// Generate random 32-byte API key (hex encoded)
    fn generate_api_key() -> String {
        use rand::RngCore;
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        hex::encode(bytes)
    }

    /// Check if wallet exists
    pub fn wallet_exists() -> Result<bool> {
        let path = Self::wallet_path()?;
        Ok(path.exists())
    }

    /// Migrate from old ~/.config/kamuy/ config if it exists
    pub fn migrate_from_old_config() -> Result<()> {
        let old_config_dir = dirs::config_dir()
            .map(|d| d.join("kamuy"))
            .unwrap_or_else(|| PathBuf::from(".kamuy"));

        let old_config_path = old_config_dir.join("config.toml");
        if !old_config_path.exists() {
            return Ok(());
        }

        // Old config exists, read it
        let content = std::fs::read_to_string(&old_config_path)?;
        let old_config: toml::Value = toml::from_str(&content)?;

        // Extract values and create new config
        let new_config = Self {
            version: "2.0".to_string(),
            steward_url: old_config.get("steward_url")
                .and_then(|v| v.as_str())
                .unwrap_or("http://127.0.0.1:8080")
                .to_string(),
            api_key: old_config.get("api_key")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| Self::generate_api_key()),
            wallet_path: Self::wallet_path()?,
            steward_log: Self::data_dir()?.join("steward.log"),
            steward_pid_file: Self::data_dir()?.join("steward.pid"),
        };

        new_config.save()?;
        println!("Migrated config from {} to ~/.kamuy/config.json", old_config_dir.display());

        Ok(())
    }
}
```

- [ ] **Step 2: Add dependencies to Cargo.toml**

In `crates/cli/Cargo.toml`, add after line 90 (after `toml = "0.8"`, before `[dev-dependencies]`):

```toml

# Random number generation for API key generation
rand = "0.8"

# Process management
which = "6.0"
libc = "0.2"
```

Note: `serde_json = "1.0"` already exists at line 36, no need to add it. The `toml` crate is already present and used via fully-qualified path (e.g., `toml::Value`) in the migration code.

- [ ] **Step 3: Run cargo check**

Run: `cargo check -p kamuy-cli`
Expected: No compilation errors

- [ ] **Step 4: Add unit tests for SimpleConfig**

Add to the `#[cfg(test)]` module in `config.rs`:

```rust
    #[test]
    fn test_simple_config_generate() {
        let config = super::SimpleConfig::generate().unwrap();
        assert_eq!(config.version, "2.0");
        // API key is 32 bytes hex-encoded = 64 chars (no 0x prefix)
        assert_eq!(config.api_key.len(), 64);
        assert_eq!(config.steward_url, "http://127.0.0.1:8080");
    }

    #[test]
    fn test_simple_config_wallet_exists() {
        // Should return false when no wallet exists
        let result = super::SimpleConfig::wallet_exists();
        assert!(result.is_ok());
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p kamuy-cli config::tests`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/cli/src/config.rs crates/cli/Cargo.toml
git commit -m "feat(cli): add SimpleConfig for ~/.kamuy/config.json

- Auto-generated 32-byte API key
- Config priority: KAMUY_CONFIG env -> ~/.kamuy/config.json
- Env var overrides: KAMUY_API_KEY, KAMUY_STEWARD_URL
- Migration from old ~/.config/kamuy/ config
- Unit tests for generate() and wallet_exists()

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 2: Add Start and Stop Commands

**Files:**
- Create: `crates/cli/src/commands/start.rs`
- Create: `crates/cli/src/commands/stop.rs`

- [ ] **Step 1: Create start.rs**

```rust
//! # Start Command
//!
//! Start the steward daemon.

use crate::commands::create_spinner;
use crate::config::SimpleConfig;
use crate::{print_error, print_success, print_info};
use anyhow::Result;
use colored::Colorize;
use std::process::{Command, Stdio};

#[cfg(unix)]
use libc::{kill, SIGTERM};

/// Start steward daemon
pub async fn execute(port: Option<u16>) -> Result<()> {
    println!("{}", "Starting Steward...".bold().cyan());

    // Load config
    let config = match SimpleConfig::load()? {
        Some(c) => c,
        None => {
            print_error("No wallet found. Run 'kamuy init' first.");
            return Err(anyhow::anyhow!("No wallet found"));
        }
    };

    // Check for stale PID file
    let pid_path = &config.steward_pid_file;
    if pid_path.exists() {
        let pid_str = std::fs::read_to_string(pid_path)?;
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            // Check if process is running
            #[cfg(unix)]
            {
                let result = unsafe { kill(pid, 0) };
                if result == 0 {
                    print_error(&format!("Steward already running (PID {})", pid));
                    print_info("Run 'kamuy stop' first or use 'kamuy status'");
                    return Err(anyhow::anyhow!("Steward already running"));
                } else {
                    // Stale PID file, remove it
                    std::fs::remove_file(pid_path)?;
                    println!("Removed stale PID file");
                }
            }
        }
    }

    // Find steward binary
    let steward_path = which::which("kamuy-steward")
        .or_else(|_| {
            // Check in same directory as kamuy binary
            let exe_dir = std::env::current_exe()?
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Cannot determine binary directory"))?
                .to_path_buf();
            Ok(exe_dir.join("kamuy-steward"))
        })?;

    if !steward_path.exists() {
        print_error("kamuy-steward not found. Re-run the install script.");
        return Err(anyhow::anyhow!("kamuy-steward binary not found"));
    }

    // Check port availability
    let bind_port = port.unwrap_or(8080);
    if !is_port_available(bind_port) {
        print_error(&format!("Port {} is already in use", bind_port));
        print_info("Stop the existing process or use --port flag");
        return Err(anyhow::anyhow!("Port in use"));
    }

    // Start steward in background
    let spinner = create_spinner("Launching steward daemon...");

    let mut child = Command::new(&steward_path)
        .env("STEWARD_API_KEY", &config.api_key)
        .env("STEWARD_DATABASE_URL", format!("sqlite://{}/steward.db?mode=rwc", SimpleConfig::data_dir()?.display()))
        .env("STEWARD_API_PORT", bind_port.to_string())
        .stdout(Stdio::from(std::fs::File::create(&config.steward_log)?))
        .stderr(Stdio::from(std::fs::File::create(&config.steward_log)?))
        .spawn()?;

    let pid = child.id();

    // Write PID file
    std::fs::write(pid_path, pid.to_string())?;

    // Wait a moment and check if process is still running
    std::thread::sleep(std::time::Duration::from_millis(500));

    match child.try_wait() {
        Ok(Some(status)) => {
            spinner.finish_with_message("Failed to start".red().to_string());
            print_error(&format!("Steward exited with status: {}", status));
            print_error(&format!("Check logs at: {}", config.steward_log.display()));
            return Err(anyhow::anyhow!("Steward failed to start"));
        }
        Ok(None) => {
            // Still running, good!
            spinner.finish_with_message("Started!".green().to_string());
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to check steward status: {}", e));
        }
    }

    print_success(&format!("Steward running at http://127.0.0.1:{}", bind_port));
    print_success(&format!("PID: {} (saved to {})", pid, pid_path.display()));

    Ok(())
}

/// Check if a port is available
fn is_port_available(port: u16) -> bool {
    std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_port_available() {
        // Port 1 is typically not available (requires root)
        // Port 8080 might be available
        let _ = is_port_available(8080);
    }
}
```

- [ ] **Step 2: Create stop.rs**

```rust
//! # Stop Command
//!
//! Stop the steward daemon.

use crate::config::SimpleConfig;
use crate::{print_error, print_success, print_info};
use anyhow::Result;
use colored::Colorize;

#[cfg(unix)]
use libc::{kill, SIGTERM, SIGKILL};

/// Stop steward daemon
pub async fn execute() -> Result<()> {
    println!("{}", "Stopping Steward...".bold().cyan());

    // Load config
    let config = match SimpleConfig::load()? {
        Some(c) => c,
        None => {
            print_error("No wallet found.");
            return Err(anyhow::anyhow!("No wallet found"));
        }
    };

    let pid_path = &config.steward_pid_file;

    if !pid_path.exists() {
        print_info("Steward is not running (no PID file found)");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(pid_path)?;
    let pid: i32 = pid_str.trim().parse()
        .map_err(|_| anyhow::anyhow!("Invalid PID in file"))?;

    // Send SIGTERM to the process
    #[cfg(unix)]
    {
        let result = unsafe { kill(pid, SIGTERM) };
        if result != 0 {
            print_error(&format!("Failed to stop process {}: {}", pid, std::io::Error::last_os_error()));
            return Err(anyhow::anyhow!("Failed to stop steward"));
        }
    }

    // Wait for process to terminate
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(100));

        #[cfg(unix)]
        {
            if unsafe { kill(pid, 0) } != 0 {
                // Process has terminated
                break;
            }
        }
    }

    // Force kill if still running
    #[cfg(unix)]
    {
        if unsafe { kill(pid, 0) } == 0 {
            print_info("Process didn't stop gracefully, forcing...");
            unsafe { kill(pid, SIGKILL) };
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    // Remove PID file
    std::fs::remove_file(pid_path)?;

    print_success("Steward stopped");
    Ok(())
}
```

- [ ] **Step 3: Run cargo check**

Run: `cargo check -p kamuy-cli`
Expected: No compilation errors

- [ ] **Step 4: Commit**

```bash
git add crates/cli/src/commands/start.rs crates/cli/src/commands/stop.rs
git commit -m "feat(cli): add start and stop commands for steward daemon

- start: Check stale PID, port availability, launch daemon
- stop: SIGTERM with SIGKILL fallback, PID cleanup

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 3: Update Config Command with Get Subcommand

**Files:**
- Modify: `crates/cli/src/commands/config_cmd.rs`

- [ ] **Step 1: Add get function to config_cmd.rs**

Insert before the final closing `}` of the module (after line 81, before line 82's `}`):

```rust

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
```

- [ ] **Step 2: Run cargo check**

Run: `cargo check -p kamuy-cli`
Expected: No compilation errors

- [ ] **Step 3: Commit**

```bash
git add crates/cli/src/commands/config_cmd.rs
git commit -m "feat(cli): add 'config get' subcommand for retrieving config values

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 4: Update Init Command with Steward Start and Auto-Unlock

**Files:**
- Modify: `crates/cli/src/commands/init.rs`

**Overview of changes:**
- Replace lines 14-31: New function signature + wallet exists check + reset handling + config migration
- DELETE lines 32-46: The old Steward connection check (v2.0 starts Steward, doesn't connect to existing)
- Keep lines 47-114: Chain selection, password prompts, MPC key generation (these define `chain_id`, `user_password`, `wallet_address`, `agent_key`)
- Replace lines 115-165: Config save, Steward start, auto-unlock, simplified output

- [ ] **Step 1: Update function signature and add wallet exists check**

**IMPORTANT:** The existing init code (lines 32-46) checks if Steward is already running and fails if not. In v2.0, init STARTS Steward rather than connecting to an existing one. This check must be removed.

**Lines 31-91 (password prompts through MPC key generation) remain mostly unchanged** but the Steward connection check (lines 32-46) is removed. The variables `chain_id`, `user_password`, `wallet_address`, and `agent_key` from those lines are used in Step 2.

Replace lines 14-31 with:

```rust
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

    // NOTE: Skip lines 32-46 (old Steward connection check) - v2.0 starts Steward
    // Continue with chain selection from line 47+

    // Step 3: Get chain ID (from old lines 48-51)
    let chain_id = crate::config::chain_id_from_name(&chain).unwrap_or(8453);
    println!();
    println!("Chain: {} ({})", chain.cyan(), chain_id);
```

- [ ] **Step 2: Add steward start and auto-unlock after wallet creation**

Replace lines 115-165 with:

```rust
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
    let steward_client = crate::steward::StewardClient::new(
        &simple_config.steward_url,
        Some(&simple_config.api_key),
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
```

**Note:** The code uses `libc::kill()` and `which::which()` directly without import statements since these are external crates listed in Cargo.toml. Rust allows using external crate functions via their crate path.

- [ ] **Step 3: Run cargo check**

Run: `cargo check -p kamuy-cli`
Expected: No compilation errors

- [ ] **Step 4: Commit**

```bash
git add crates/cli/src/commands/init.rs
git commit -m "feat(cli): auto-start steward and auto-unlock in init command

- Add wallet exists check with friendly message
- Add --reset flag to recreate wallet
- Generate and save SimpleConfig with auto API key
- Start steward daemon automatically with port/stale PID checks
- Auto-unlock wallet after creation
- Simplify output for v2.0 flow

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 5: Update Status Command to Show Steward PID

**Files:**
- Modify: `crates/cli/src/commands/status.rs`

- [ ] **Step 1: Add steward PID status check**

Add after line 10:

```rust
use crate::config::SimpleConfig;
```

Replace lines 14-36 with:

```rust
/// Execute status command
pub async fn execute(ctx: Arc<CliContext>, detailed: bool) -> Result<()> {
    println!("{}", "📊 Wallet Status".bold().cyan());
    println!();

    // Check steward process status from PID file
    let steward_status = check_steward_status();
    println!("{}", "Steward:".bold());
    match steward_status {
        Ok((pid, running)) => {
            if running {
                println!("  Status: {}", "running".green());
                println!("  PID: {}", pid);
            } else {
                println!("  Status: {} (stale PID: {})", "stopped".yellow(), pid);
            }
        }
        Err(_) => {
            println!("  Status: {}", "not running".red());
        }
    }
    println!();

    // Check Steward connection
    let spinner = create_spinner("Connecting to Steward API...");

    match ctx.steward.health().await {
        Ok(health) => {
            spinner.finish_with_message(format!("API: Steward v{}", health.version));

            if health.status != "healthy" {
                print_warning(&format!("Steward status: {}", health.status));
            }
        }
        Err(e) => {
            spinner.finish_with_message("API: Connection failed".to_string());
            print_error(&format!("Cannot connect to Steward API: {}", e));
            println!();
            println!("To start steward:");
            println!("  kamuy start");
            return Ok(());
        }
    }
```

- [ ] **Step 2: Add check_steward_status helper function**

Add before the closing `}` at line 95:

```rust

/// Check steward status from PID file
fn check_steward_status() -> Result<(i32, bool)> {
    let config = SimpleConfig::load()?
        .ok_or_else(|| anyhow::anyhow!("No config found"))?;

    let pid_path = &config.steward_pid_file;
    if !pid_path.exists() {
        return Err(anyhow::anyhow!("No PID file"));
    }

    let pid_str = std::fs::read_to_string(pid_path)?;
    let pid: i32 = pid_str.trim().parse()
        .map_err(|_| anyhow::anyhow!("Invalid PID"))?;

    #[cfg(unix)]
    {
        use libc::kill;
        let running = unsafe { kill(pid, 0) } == 0;
        Ok((pid, running))
    }

    #[cfg(not(unix))]
    {
        Ok((pid, false))
    }
}
```

- [ ] **Step 3: Run cargo check**

Run: `cargo check -p kamuy-cli`
Expected: No compilation errors

- [ ] **Step 4: Commit**

```bash
git add crates/cli/src/commands/status.rs
git commit -m "feat(cli): show steward PID status in status command

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 6: Wire Up Commands in mod.rs and main.rs

**Files:**
- Modify: `crates/cli/src/commands/mod.rs`
- Modify: `crates/cli/src/main.rs`

- [ ] **Step 1: Add module declarations to mod.rs**

Add after line 18 in `mod.rs`:

```rust
pub mod start;
pub mod stop;
```

- [ ] **Step 2: Update ConfigAction enum in main.rs**

Replace the ConfigAction enum (lines 214-229) with:

```rust
#[derive(Subcommand)]
enum ConfigAction {
    /// Show current configuration
    Show,

    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,
        /// Configuration value
        value: String,
    },

    /// Get a configuration value
    Get {
        /// Configuration key (api_key, steward_url, wallet_path)
        key: String,
    },

    /// Initialize configuration file
    Init,
}
```

- [ ] **Step 3: Add Start and Stop to Commands enum**

In the `Commands` enum, add after the `Lock` variant:

```rust

    /// Start the steward daemon
    Start {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },

    /// Stop the steward daemon
    Stop,
```

- [ ] **Step 4: Add reset flag to Init command**

In the `Commands` enum, replace the `Init` variant with:

```rust
    /// Initialize a new wallet (recommended for new users)
    Init {
        /// Chain to create wallet on
        #[arg(short, long, default_value = "base")]
        chain: String,

        /// Email for encrypted backup
        #[arg(short, long)]
        email: Option<String>,

        /// Output file for keys (JSON format)
        #[arg(short, long)]
        output: Option<String>,

        /// Reset wallet (delete existing and create new)
        #[arg(long)]
        reset: bool,
    },
```

- [ ] **Step 5: Update match handlers in main.rs**

In the `match cli.command` block, make these changes:

**1. Replace the `Commands::Init` handler with:**

```rust
        Commands::Init { chain, email, output, reset } => {
            progress_tracker.set_current_command("init")?;
            init::execute(ctx, chain, email, output, reset).await?;
            progress_tracker.command_completed("init")?;
        }
```

**2. Add `Start` and `Stop` handlers after the `Commands::Lock` handler:**

```rust
        Commands::Start { port } => {
            progress_tracker.set_current_command("start")?;
            // SimpleConfig::load() already respects KAMUY_CONFIG env var
            start::execute(Some(port)).await?;
            progress_tracker.command_completed("start")?;
        }
        Commands::Stop => {
            progress_tracker.set_current_command("stop")?;
            stop::execute().await?;
            progress_tracker.command_completed("stop")?;
        }
```

**3. Replace the `Commands::Config` handler with:**

```rust
        Commands::Config { action } => {
            progress_tracker.set_current_command("config")?;
            match action {
                ConfigAction::Show => {
                    config_cmd::show(ctx).await?;
                }
                ConfigAction::Set { key, value } => {
                    config_cmd::set(ctx, key, value).await?;
                }
                ConfigAction::Get { key } => {
                    config_cmd::get(ctx, key).await?;
                }
                ConfigAction::Init => {
                    config_cmd::init(ctx).await?;
                }
            }
            progress_tracker.command_completed("config")?;
        }
```

- [ ] **Step 6: Add --config flag integration for SimpleConfig**

The existing `--config` flag in main.rs is passed to CliConfig. For commands that use SimpleConfig (start, stop, config get), we need to check the flag too.

Add this helper function in main.rs before `fn main()`:

```rust
/// Get SimpleConfig with --config flag support
fn get_simple_config(cli: &Cli) -> Option<crate::config::SimpleConfig> {
    // Priority: --config flag > KAMUY_CONFIG env > ~/.kamuy/config.json
    if let Some(config_path) = &cli.config {
        crate::config::SimpleConfig::load_from_path(&std::path::PathBuf::from(config_path))
            .ok()
            .flatten()
    } else {
        crate::config::SimpleConfig::load().ok().flatten()
    }
}
```

Then update the Start/Stop handlers to use it (optional - SimpleConfig::load() already handles env vars):

```rust
        Commands::Start { port } => {
            progress_tracker.set_current_command("start")?;
            // SimpleConfig::load() already respects KAMUY_CONFIG env var
            start::execute(Some(port)).await?;
            progress_tracker.command_completed("start")?;
        }
```

**Note:** Since `SimpleConfig::load()` already checks `KAMUY_CONFIG` env var, the --config flag integration is primarily for explicit override. The start/stop commands load their own config via `SimpleConfig::load()`.

- [ ] **Step 7: Run cargo check**

Run: `cargo check -p kamuy-cli`
Expected: No compilation errors

- [ ] **Step 8: Commit**

```bash
git add crates/cli/src/commands/mod.rs crates/cli/src/main.rs
git commit -m "feat(cli): wire up start, stop, config get commands and reset flag

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 7: Update Install Script

**Files:**
- Modify: `/home/santo6500/kamuy-wallet/install.sh` (root install script)

- [ ] **Step 1: Update install.sh to remove env var instructions**

Replace the entire file with:

```bash
#!/bin/bash
# Kamuy Wallet Installer
# Usage: curl -sSL https://raw.githubusercontent.com/KristianRadev/KamuyWallet/main/install.sh | bash

set -e

REPO="KristianRadev/KamuyWallet"
INSTALL_DIR="$HOME/.kamuy"
BIN_DIR="$HOME/.local/bin"

echo "🔐 Installing Kamuy Wallet..."

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case $ARCH in
    x86_64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *) echo "❌ Unsupported architecture: $ARCH"; exit 1 ;;
esac

case $OS in
    linux) OS="linux" ;;
    darwin) OS="macos" ;;
    *) echo "❌ Unsupported OS: $OS"; exit 1 ;;
esac

# Create directories
mkdir -p "$INSTALL_DIR"
mkdir -p "$BIN_DIR"

# Download binaries from latest release
RELEASE_URL="https://github.com/$REPO/releases/download/v0.1.0"

echo "📥 Downloading kamuy CLI..."
curl -sSL "$RELEASE_URL/kamuy-$OS-$ARCH" -o "$INSTALL_DIR/kamuy"
chmod +x "$INSTALL_DIR/kamuy"

echo "📥 Downloading kamuy-steward..."
curl -sSL "$RELEASE_URL/kamuy-steward-$OS-$ARCH" -o "$INSTALL_DIR/kamuy-steward"
chmod +x "$INSTALL_DIR/kamuy-steward"

# Create symlinks in bin
ln -sf "$INSTALL_DIR/kamuy" "$BIN_DIR/kamuy"
ln -sf "$INSTALL_DIR/kamuy-steward" "$BIN_DIR/kamuy-steward"

# Add to PATH if needed
if ! echo "$PATH" | grep -q "$BIN_DIR"; then
    echo ""
    echo "⚠️  Add this to your ~/.bashrc or ~/.zshrc:"
    echo "   export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
    echo "Then run: source ~/.bashrc"
fi

echo ""
echo "✅ Kamuy Wallet installed!"
echo ""
echo "🚀 Quick Start:"
echo "   kamuy init --email your@email.com"
echo ""
```

- [ ] **Step 2: Commit**

```bash
git add install.sh
git commit -m "docs: simplify install script for v2.0 zero-friction setup

Remove env var instructions - config is auto-generated by init

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 8: Build and Test

**Note:** Test steps use isolated test directories (`/tmp/kamuy-test-*`) to avoid affecting production wallet data. The tests set temporary HOME and KAMUY_CONFIG environment variables.

- [ ] **Step 1: Build release binaries**

Run: `cargo build --release -p kamuy-cli -p kamuy-steward`
Expected: Successful build

- [ ] **Step 2: Run all tests**

Run: `cargo test -p kamuy-cli`
Expected: All tests pass

- [ ] **Step 3: Test init command locally (uses test directory)**

```bash
# Set up test environment - doesn't affect production wallet
export KAMUY_CONFIG="/tmp/kamuy-test-config.json"
export HOME="/tmp/kamuy-test-home"
mkdir -p "$HOME"

# Run init
./target/release/kamuy init --email test@example.com

# Check wallet was created
ls -la /tmp/kamuy-test-home/.kamuy/

# Check config
cat /tmp/kamuy-test-home/.kamuy/config.json

# Cleanup test environment
unset KAMUY_CONFIG
unset HOME
rm -rf /tmp/kamuy-test-home /tmp/kamuy-test-config.json
```

- [ ] **Step 4: Test config get command**

```bash
./target/release/kamuy config get api_key
./target/release/kamuy config get steward_url
```

- [ ] **Step 5: Test status command shows PID**

```bash
./target/release/kamuy status
# Should show: Steward: running (PID 12345) or similar
```

- [ ] **Step 6: Test stop and start commands**

```bash
./target/release/kamuy stop
./target/release/kamuy status
# Should show: Steward: not running

./target/release/kamuy start
./target/release/kamuy status
# Should show: Steward: running
```

- [ ] **Step 7: Test reset flag**

```bash
./target/release/kamuy init --email test2@example.com
# Should show: "A wallet already exists at ~/.kamuy/wallet.json"

./target/release/kamuy init --email test3@example.com --reset
# Should create new wallet with new API key
```

- [ ] **Step 8: Test env var overrides**

```bash
KAMUY_API_KEY=test-override ./target/release/kamuy config get api_key
# Should print: test-override
```

- [ ] **Step 9: Test migration from old config (uses test directory)**

```bash
# Set up test environment
export HOME="/tmp/kamuy-migration-test"
mkdir -p "$HOME"

# Create old config structure
mkdir -p "$HOME/.config/kamuy"
echo 'steward_url = "http://localhost:9000"
api_key = "old-test-key"' > "$HOME/.config/kamuy/config.toml"

# Run init - should migrate
./target/release/kamuy init --email test@example.com

# Verify migration happened
cat "$HOME/.kamuy/config.json"
# Should show migrated steward_url or new API key

# Cleanup
unset HOME
rm -rf /tmp/kamuy-migration-test
```

- [ ] **Step 10: Final commit**

```bash
git add -A
git commit -m "test: verify zero-friction setup flow works end-to-end

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Verification

After all tasks complete:

```bash
# Clean slate
rm -rf ~/.kamuy

# Install
curl -sSL https://raw.githubusercontent.com/KristianRadev/KamuyWallet/main/install.sh | bash

# One-command setup
kamuy init --email user@example.com
Password: ********

# Verify
kamuy status
```

Expected output:
```
📊 Wallet Status

Steward:
  Status: running
  PID: 12345

API: Steward v0.1.0

Wallet Information:
  Address: 0xABC...1234
  Chain ID: 8453
```