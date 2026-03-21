//! # Progress Tracking
//!
//! Auto-recovery system for resuming work after session interruption.
//! SECURITY: Command names are hashed to avoid leaking implementation details.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::SystemTime;
use tracing::{info, debug};

/// Hash a command name to opaque ID
/// SECURITY: Prevents leaking command names in plaintext progress file
fn hash_command(cmd: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(cmd.as_bytes());
    hasher.update(b"kamuy-progress-salt"); // Add salt
    hex::encode(hasher.finalize())[..16].to_string() // Truncate to 16 chars
}

/// Phase 3 progress tracking
/// SECURITY: Command names stored as hashed opaque IDs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase3Progress {
    /// Current task being worked on
    pub current_task: String,
    /// List of completed files
    pub completed_files: Vec<String>,
    /// Timestamp of last update
    pub last_updated: SystemTime,
    /// Whether task is complete
    pub is_complete: bool,
    /// Current command being implemented (hashed)
    pub current_command_hash: Option<String>,
    /// Commands completed (hashed)
    pub completed_command_hashes: Vec<String>,
    /// Progress percentage (0-100)
    pub progress_percent: u8,
}

impl Default for Phase3Progress {
    fn default() -> Self {
        Self {
            current_task: "starting".to_string(),
            completed_files: Vec::new(),
            last_updated: SystemTime::now(),
            is_complete: false,
            current_command_hash: None,
            completed_command_hashes: Vec::new(),
            progress_percent: 0,
        }
    }
}

impl Phase3Progress {
    /// Get progress file path
    fn progress_file() -> PathBuf {
        let data_dir = dirs::data_dir()
            .map(|d| d.join("kamuy"))
            .unwrap_or_else(|| PathBuf::from("~/.local/share/kamuy"));

        data_dir.join("phase3_progress.json")
    }

    /// Ensure data directory exists
    fn ensure_data_dir() -> Result<PathBuf> {
        let data_dir = dirs::data_dir()
            .map(|d| d.join("kamuy"))
            .unwrap_or_else(|| PathBuf::from("~/.local/share/kamuy"));

        std::fs::create_dir_all(&data_dir)
            .with_context(|| format!("Failed to create data directory: {:?}", data_dir))?;

        Ok(data_dir)
    }

    /// Load progress from file
    pub fn load() -> Result<Self> {
        let file_path = Self::progress_file();

        if !file_path.exists() {
            debug!("No progress file found, starting fresh");
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read progress file: {:?}", file_path))?;

        let progress: Phase3Progress = match serde_json::from_str(&content) {
            Ok(p) => p,
            Err(e) => {
                // If parse fails (old format or corrupt), start fresh
                debug!("Progress file parse error, starting fresh: {}", e);
                return Ok(Self::default());
            }
        };

        info!(
            task = %progress.current_task,
            completed_files = progress.completed_files.len(),
            "Loaded progress from file"
        );

        Ok(progress)
    }

    /// Save progress to file
    pub fn save(&self) -> Result<()> {
        Self::ensure_data_dir()?;

        let file_path = Self::progress_file();
        let content = serde_json::to_string_pretty(self)
            .with_context(|| "Failed to serialize progress")?;

        std::fs::write(&file_path, content)
            .with_context(|| format!("Failed to write progress file: {:?}", file_path))?;

        debug!("Progress saved to {:?}", file_path);
        Ok(())
    }

    /// Mark a command as completed
    /// SECURITY: Stores hashed command name, not plaintext
    pub fn mark_command_completed(&mut self, command: impl Into<String>) {
        let cmd_hash = hash_command(&command.into());
        if !self.completed_command_hashes.contains(&cmd_hash) {
            self.completed_command_hashes.push(cmd_hash);
        }
        self.current_command_hash = None;
        self.update_progress();
        self.last_updated = SystemTime::now();
    }

    /// Set current command being worked on
    /// SECURITY: Stores hashed command name, not plaintext
    pub fn set_current_command(&mut self, command: impl Into<String>) {
        self.current_command_hash = Some(hash_command(&command.into()));
        self.last_updated = SystemTime::now();
    }

    /// Update progress percentage
    fn update_progress(&mut self) {
        // Phase 3 has roughly 15 major components
        let total_components = 15u8;
        let completed = self.completed_files.len() as u8;
        self.progress_percent = (completed * 100 / total_components).min(100);
    }

    /// Check if we should resume from a specific point
    pub fn should_resume(&self) -> bool {
        self.current_task != "starting" && !self.is_complete
    }

    /// Get resume message
    /// SECURITY: Does not leak command names in message
    pub fn resume_message(&self) -> String {
        if self.is_complete {
            return "Phase 3 is already complete.".to_string();
        }

        if self.current_command_hash.is_some() {
            format!(
                "Resuming Phase 3...\nCurrent task: {}\nIn progress\nProgress: {}%",
                self.current_task, self.progress_percent
            )
        } else {
            format!(
                "Resuming Phase 3...\nCurrent task: {}\nProgress: {}%",
                self.current_task, self.progress_percent
            )
        }
    }
}

/// Progress tracker for auto-save
pub struct ProgressTracker {
    progress: Phase3Progress,
}

impl ProgressTracker {
    /// Create new tracker
    pub fn new() -> Result<Self> {
        Ok(Self {
            progress: Phase3Progress::load()?,
        })
    }

    /// Load existing or create new
    pub fn load_or_create() -> Result<Self> {
        Ok(Self::new()?)
    }

    /// Mark command complete and save
    pub fn command_completed(&mut self, command: impl Into<String>) -> Result<()> {
        self.progress.mark_command_completed(command);
        self.progress.save()?;
        Ok(())
    }

    /// Set current command and save
    pub fn set_current_command(&mut self, command: impl Into<String>) -> Result<()> {
        self.progress.set_current_command(command);
        self.progress.save()?;
        Ok(())
    }

    /// Check if should resume
    pub fn should_resume(&self) -> bool {
        self.progress.should_resume()
    }

    /// Get resume message
    pub fn resume_message(&self) -> String {
        self.progress.resume_message()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_hashing() {
        let hash1 = hash_command("create_wallet");
        let hash2 = hash_command("create_wallet");
        let hash3 = hash_command("sign");

        // Same command should produce same hash
        assert_eq!(hash1, hash2);
        // Different commands should produce different hashes
        assert_ne!(hash1, hash3);
        // Hash should be truncated to 16 chars
        assert_eq!(hash1.len(), 16);
    }
}
