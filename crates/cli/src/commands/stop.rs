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