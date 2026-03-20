# Kamuy Wallet v2.0 — Zero-Friction Local Setup

**Version:** 2.1
**Date:** 2026-03-20
**Status:** Draft
**Author:** Product Team

---

## Problem Statement

Current setup requires too many manual steps:
```bash
export PATH="$HOME/.local/bin:$PATH"
export STEWARD_URL="http://localhost:8080"
export STEWARD_API_KEY="..."
kamuy init --email user@example.com --output ~/.kamuy/wallet.json
kamuy unlock
kamuy status
```

This violates the v2.0 design goal: **one-command setup**.

---

## Proposed Solution

### Simplified Flow
```
$ kamuy init --email your@email.com
Password: ********
✓ Wallet created, Steward running at localhost:8080

Your wallet address: 0xABC...1234
```

**One command.** No env vars, no manual unlock, no visible API key.

---

## Design Details

### 1. Auto-Generated API Key

**Behavior:**
- On `kamuy init`, generate a random 32-byte API key
- Store in `~/.kamuy/config.json`
- Steward started with that key
- CLI reads key from config automatically

**Security:**
- API key provides authentication layer
- Key is never shown to user (no copy-paste)
- Key rotates on each `kamuy init` (new wallet = new key)

**Config file (`~/.kamuy/config.json`):**
```json
{
  "version": "2.0",
  "steward_url": "http://127.0.0.1:8080",
  "api_key": "a3f8b2c1d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1",
  "wallet_path": "~/.kamuy/wallet.json",
  "steward_log": "~/.kamuy/steward.log",
  "steward_pid_file": "~/.kamuy/steward.pid"
}
```

**Note on config location:** Current codebase uses `~/.config/kamuy/` with TOML format. This spec standardizes on `~/.kamuy/` with JSON for simplicity. The `kamuy init` command will handle migration if an old config exists.

### 2. Defaults Eliminate Env Vars

| Current (env var) | New (default) |
|-------------------|---------------|
| `STEWARD_URL` | `http://127.0.0.1:8080` (from config) |
| `STEWARD_API_KEY` | Auto-generated, read from config |
| `STEWARD_DATABASE_URL` | `~/.kamuy/steward.db` (from config) |
| `PATH` | Install script adds to shell rc |

### 3. Auto-Unlock After Init

**Current:**
1. `kamuy init` → wallet created, locked
2. `kamuy unlock` → user enters password again
3. Wallet usable

**New:**
1. `kamuy init` → wallet created, unlocked immediately (password just entered)
2. Wallet usable

The password is already in memory from the init prompt. Use it to unlock.

### 4. Steward Lifecycle Management

**`kamuy init` responsibilities:**
1. Create `~/.kamuy/` directory
2. Generate API key
3. Write config to `~/.kamuy/config.json`
4. Generate MPC key shares
5. Save encrypted wallet to `~/.kamuy/wallet.json`
6. Start `kamuy-steward` as background daemon
7. Write PID to `~/.kamuy/steward.pid`
8. Unlock wallet with password
9. Print wallet address

**Wallet already exists check:**
- If `~/.kamuy/wallet.json` exists, show message and exit:
  ```
  A wallet already exists at ~/.kamuy/wallet.json

  To check your wallet: kamuy status
  To create a new wallet: kamuy init --reset
  ```
- `kamuy init --reset` will delete the old wallet and generate a new API key
- API key only changes on first init or `--reset` (not on accidental re-runs)

**Error handling during init:**
| Failure Mode | Behavior |
|--------------|----------|
| Port 8080 already in use | Error: "Port 8080 in use. Stop existing steward or use `--port` flag" |
| Binary not in PATH | Error: "kamuy-steward not found. Re-run install script" |
| Daemon fails to start | Error with steward log path, exit code 1 |
| Stale PID file exists | Check if process running; if not, remove stale file and proceed |

**Stale PID file handling:**
- Before starting, check if PID in `steward.pid` is running (`kill -0 $PID`)
- If not running, remove stale file and proceed
- If running, check if it's actually kamuy-steward
- If different process, error with suggestion to use different port

**New CLI commands:**
```bash
kamuy start     # Start steward daemon
kamuy stop      # Stop steward daemon
kamuy status    # Show steward status + wallet info
```

**`kamuy unlock` remains available** for manual unlock after:
- `kamuy stop` followed by `kamuy start`
- Computer reboot
- Steward crash/restart

### 5. Config Priority

CLI reads config in this order:
1. `--config <path>` flag (explicit)
2. `KAMUY_CONFIG` env var
3. `~/.kamuy/config.json` (default)

Individual env vars still work for overrides:
- `KAMUY_API_KEY` overrides config
- `KAMUY_STEWARD_URL` overrides config

### 6. Agent Integration

For external agents (OpenClaw), provide a helper:

```bash
# Get API key for agent configuration
kamuy config get api_key
# Output: a3f8b2c1d4e5...

# Get steward URL
kamuy config get steward_url
# Output: http://127.0.0.1:8080
```

Or read config directly:
```bash
cat ~/.kamuy/config.json | jq -r '.api_key'
```

---

## Implementation Changes

### Files to Modify

| File | Change |
|------|--------|
| `crates/cli/src/config.rs` | Add config file reading from `~/.kamuy/config.json`, auto-generate API key |
| `crates/cli/src/commands/init.rs` | Auto-start steward, auto-unlock, handle stale PID |
| `crates/cli/src/commands/mod.rs` | Add `start`, `stop` subcommands |
| `crates/cli/src/commands/config_cmd.rs` | Add `get` subcommand for API key retrieval |
| `crates/steward/src/config.rs` | Support config file input |
| `install.sh` | Simplify, no env var instructions |

### New Files

| File | Purpose |
|------|---------|
| `crates/cli/src/commands/start.rs` | Start steward daemon |
| `crates/cli/src/commands/stop.rs` | Stop steward daemon |

**Note:** `config_cmd.rs` already exists but needs modification to support `kamuy config get <key>`.

---

## Backward Compatibility

- Existing env vars still work (`STEWARD_API_KEY`, etc.)
- Old installations with env vars continue to function
- Migration: If `~/.config/kamuy/` exists, `kamuy init` will migrate config to `~/.kamuy/`

---

## Success Criteria

After this change:

```
$ curl -sSL .../install.sh | bash
$ kamuy init --email user@example.com
Password: ********
✓ Wallet created, Steward running at localhost:8080
Your wallet address: 0xABC...1234

$ kamuy status
Steward: running (PID 12345)
Wallet: 0xABC...1234
Balance: 0 USDC
```

**Two commands to full functionality.**