//! # CLI Integration Tests
//!
//! Integration tests for the Kamuy CLI commands.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Test the help command
#[test]
fn test_help() {
    let mut cmd = Command::cargo_bin("kamuy").unwrap();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Kamuy Wallet"));
}

/// Test version command
#[test]
fn test_version() {
    let mut cmd = Command::cargo_bin("kamuy").unwrap();
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("0.2.6"));
}

/// Test config show without config file
#[test]
fn test_config_show_no_config() {
    let temp_dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("kamuy").unwrap();
    
    cmd.env("HOME", temp_dir.path());
    cmd.arg("config");
    cmd.arg("show");
    
    // Should succeed with default config
    cmd.assert().success();
}

/// Test completions generation
#[test]
fn test_completions_bash() {
    let mut cmd = Command::cargo_bin("kamuy").unwrap();
    cmd.arg("completions");
    cmd.arg("bash");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("complete"));
}

#[test]
fn test_completions_zsh() {
    let mut cmd = Command::cargo_bin("kamuy").unwrap();
    cmd.arg("completions");
    cmd.arg("zsh");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("#compdef"));
}

#[test]
fn test_completions_fish() {
    let mut cmd = Command::cargo_bin("kamuy").unwrap();
    cmd.arg("completions");
    cmd.arg("fish");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("complete"));
}

/// Test policy show (requires mock Steward)
#[test]
#[ignore = "Requires running Steward service"]
fn test_policy_show() {
    let mut cmd = Command::cargo_bin("kamuy").unwrap();
    cmd.arg("policy");
    cmd.arg("show");
    cmd.assert().success();
}

/// Test status command (requires mock Steward)
#[test]
#[ignore = "Requires running Steward service"]
fn test_status() {
    let mut cmd = Command::cargo_bin("kamuy").unwrap();
    cmd.arg("status");
    cmd.assert().success();
}

/// Test pending command (requires mock Steward)
#[test]
#[ignore = "Requires running Steward service"]
fn test_pending() {
    let mut cmd = Command::cargo_bin("kamuy").unwrap();
    cmd.arg("pending");
    cmd.assert().success();
}

/// Test history command (requires mock Steward)
#[test]
#[ignore = "Requires running Steward service"]
fn test_history() {
    let mut cmd = Command::cargo_bin("kamuy").unwrap();
    cmd.arg("history");
    cmd.assert().success();
}

/// Test config init
#[test]
fn test_config_init() {
    let temp_dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("kamuy").unwrap();
    
    cmd.env("HOME", temp_dir.path());
    cmd.arg("config");
    cmd.arg("init");
    
    cmd.assert().success();
}

/// Test that all subcommands are documented in help
#[test]
fn test_all_commands_in_help() {
    let mut cmd = Command::cargo_bin("kamuy").unwrap();
    cmd.arg("--help");
    
    let output = cmd.output().expect("Failed to execute");
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Check that all major commands are documented
    assert!(stdout.contains("create-wallet"));
    assert!(stdout.contains("sign"));
    assert!(stdout.contains("policy"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("unlock"));
    assert!(stdout.contains("lock"));
    assert!(stdout.contains("rotate"));
    assert!(stdout.contains("recover"));
    assert!(stdout.contains("pending"));
    assert!(stdout.contains("approve"));
    assert!(stdout.contains("reject"));
    assert!(stdout.contains("history"));
    assert!(stdout.contains("config"));
    assert!(stdout.contains("completions"));
}
