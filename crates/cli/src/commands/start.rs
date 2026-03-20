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
            Ok::<_, anyhow::Error>(exe_dir.join("kamuy-steward"))
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