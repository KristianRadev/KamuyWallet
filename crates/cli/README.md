# Kamuy CLI

Command-line interface for Kamuy Wallet - an MPC-based threshold wallet for AI agents.

## Overview

The Kamuy CLI is the **user-facing** interface for managing wallets and policies. It is used by the wallet owner (not the AI agent) to:
- Create and manage wallets
- Configure spending policies
- Unlock the Steward service
- Approve/reject transactions (when not using Telegram)
- Monitor wallet activity

**Important**: The AI Agent uses the Agent Key independently. The CLI is for the wallet owner, not the agent.

## Architecture

```
┌──────────────────┐          ┌──────────────────┐
│   USER (You)     │          │   AI AGENT       │
│                  │          │   (External)     │
│  ┌────────────┐  │          │                  │
│  │ Kamuy CLI  │──┼──┐       │  ┌────────────┐  │
│  │            │  │  │       │  │ User's     │  │
│  │ • Create   │  │  │       │  │ Agent SW   │  │
│  │ • Policy   │  │  │       │  │            │  │
│  │ • Approve  │  │  │       │  │ • Has Key  │  │
│  │ • Monitor  │  │  │       │  │ • Calls API│  │
│  └────────────┘  │  │       │  └────────────┘  │
└──────────────────┘  │       └────────┬─────────┘
                      │                │
                      └────────┬───────┘
                               │ HTTP API
                      ┌────────┴─────────┐
                      │  STEWARD SERVICE │
                      │  (always running)│
                      └──────────────────┘
```

## Installation

### Option 1: Using Install Script (Recommended)

```bash
# 1. Download the install script
curl -sSL https://raw.githubusercontent.com/KristianRadev/KamuyWallet/master/install.sh -o install.sh

# 2. Review it (optional but recommended)
cat install.sh

# 3. Run the installer
bash install.sh

# 4. Add to PATH if needed
export PATH="$HOME/.local/bin:$PATH"
```

### Option 2: Direct Binary Download

```bash
# Create directories
mkdir -p ~/.local/bin

# Download binary from latest release
curl -sSL https://github.com/KristianRadev/KamuyWallet/releases/download/v0.2.0/kamuy -o ~/.local/bin/kamuy
curl -sSL https://github.com/KristianRadev/KamuyWallet/releases/download/v0.2.0/kamuy-steward -o ~/.local/bin/kamuy-steward

# Make executable
chmod +x ~/.local/bin/kamuy ~/.local/bin/kamuy-steward

# Add to PATH
export PATH="$HOME/.local/bin:$PATH"
```

### Option 3: Build from Source

```bash
# Requires Rust
cargo build --release -p kamuy-cli

# The binary will be available at:
# ./target/release/kamuy

# Install to system
cargo install --path crates/cli
```

## Quick Start

### Create Your Wallet

```bash
# One command creates wallet, starts Steward, and unlocks
kamuy init --email your@email.com

Password: ********
Confirm password: ********

✓ Wallet created, Steward running at localhost:8080
Your wallet address: 0xABC...1234
```

That's it! This single command:
- Generates MPC key shares
- Auto-generates and stores API key
- Starts the Steward daemon
- Unlocks the wallet

### Configure Your AI Agent

Get the config values:
```bash
kamuy config get api_key
kamuy config get steward_url
```

Give these to your AI agent software (OpenClaw, etc.):
- **Steward URL**: From `kamuy config get steward_url`
- **API Key**: From `kamuy config get api_key`

### Set Your Spending Policy

```bash
kamuy policy set max_per_tx 100
kamuy policy set require_approval_above 50
kamuy policy set allowed_tokens USDC
```

Now your AI agent can make payments according to your policy!

## Commands

### Wallet Management

#### `init`
Initialize a new wallet with auto-generated config and auto-start.

```bash
# Create a wallet (default: Base chain)
kamuy init --email your@email.com

# Create on a specific chain
kamuy init --chain ethereum --email your@email.com

# Reset and create new wallet (deletes existing)
kamuy init --email your@email.com --reset
```

**Output**:
```
✓ Wallet created, Steward running at localhost:8080
Your wallet address: 0xABC...1234

Agent configuration:
  Steward URL: http://127.0.0.1:8080
  API Key: a3f8b2c1...
  Agent Key: ag_xxxx...
```

This command:
1. Generates MPC key shares
2. Auto-generates a 32-byte API key
3. Saves config to `~/.kamuy/config.json`
4. Starts Steward daemon
5. Unlocks wallet automatically

#### `create-wallet`
Generate MPC keys and create a smart account wallet.

```bash
# Create a wallet (default: Base chain)
kamuy create-wallet

# Create on a specific chain
kamuy create-wallet --chain ethereum

# Save keys to a file
kamuy create-wallet --output wallet-keys.txt
```

**Output**:
```
✓ Wallet created successfully

Address: 0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb

⚠️  IMPORTANT: Save these securely!

AGENT KEY (give to your AI agent):
ag_key_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx

STEWARD KEY: Encrypted and stored
USER KEY: Encrypted and stored (for recovery)
```

#### `status`
Check wallet status and Steward connection.

```bash
# Basic status (includes steward PID)
kamuy status

# Detailed status
kamuy status --detailed
```

#### `start`
Start the Steward daemon.

```bash
kamuy start

# With custom port
kamuy start --port 8081
```

#### `stop`
Stop the Steward daemon.

```bash
kamuy stop
```

#### `unlock`
Load the STEWARD key with password.

```bash
kamuy unlock
# Enter password: ***
# ✓ Steward key loaded
```

**Note**: After `kamuy init`, the wallet is already unlocked. Use this after `kamuy stop` followed by `kamuy start`, or after a reboot.

#### `lock`
Unload the STEWARD key from memory.

```bash
kamuy lock
```

#### `recover`
Recover wallet access using user key file.

```bash
kamuy recover --key-file ~/backup/user.key
```

### Policy Management

#### `policy`
View and update spending policies.

```bash
# Show current policy
kamuy policy
kamuy policy show

# Update a policy value
kamuy policy set max_per_tx 100
kamuy policy set max_daily 1000
kamuy policy set require_approval_above 50

# Edit policy in default editor
kamuy policy edit

# Reset to defaults
kamuy policy reset
```

**Policy Rules**:
- `max_per_tx`: Maximum per transaction
- `max_daily`: Maximum daily spending
- `max_weekly`: Maximum weekly spending
- `require_approval_above`: Amount requiring your approval
- `allowed_tokens`: USDC, USDT, DAI
- `whitelist`: Allowed destinations

### Transaction Management

#### `pending`
List transactions awaiting approval.

```bash
kamuy pending
```

#### `approve`
Approve a pending transaction.

```bash
kamuy approve <tx-id>
```

#### `reject`
Reject a pending transaction.

```bash
kamuy reject <tx-id>
```

#### `history`
Show transaction history.

```bash
# Show last 10 transactions
kamuy history

# Show more
kamuy history --limit 50
```

### Configuration

#### `config`
Manage CLI configuration.

```bash
# Get a config value
kamuy config get api_key
kamuy config get steward_url

# Show current config
kamuy config show

# Set a value
kamuy config set steward_url http://localhost:8080

# Initialize config file
kamuy config init
```

## Configuration

The CLI stores configuration in `~/.kamuy/config.json`:

```json
{
  "version": "2.0",
  "steward_url": "http://127.0.0.1:8080",
  "api_key": "auto-generated-key"
}
```

### Config Commands

```bash
# Get config values
kamuy config get api_key
kamuy config get steward_url

# Show all config
kamuy config show

# Set a value
kamuy config set steward_url http://localhost:8080
```

### Environment Variables (Optional Overrides)

- `KAMUY_CONFIG` - Custom config file path
- `KAMUY_API_KEY` - Override API key
- `KAMUY_STEWARD_URL` - Override Steward URL

## File Locations

- **Config**: `~/.kamuy/config.json`
- **Steward PID**: `~/.kamuy/steward.pid`
- **Steward Log**: `~/.kamuy/steward.log`
- **Database**: `~/.kamuy/steward.db` (encrypted wallet data)

## Security

- Keys are encrypted with ChaCha20-Poly1305 using Argon2
- Key files have restrictive permissions (0o600)
- Passwords are never displayed or logged
- All sensitive operations require password confirmation
- **User Key is shown ONCE during init - must be saved securely for recovery**
- **No plain-text key storage - user_key is NEVER written to disk**
- Agent Key can be shared with AI agents for spending operations

## Typical Workflow

### First Time Setup

```bash
# One command does it all
kamuy init

# You'll be prompted for:
# 1. Password (for encrypting keys)
# 2. Email (optional, for backup notifications)

# Get API key for your agent
kamuy config get api_key

# Set policy
kamuy policy set max_per_tx 100
kamuy policy set max_daily 1000
```

### Daily Operations

```bash
# Check status (shows steward PID and wallet info)
kamuy status

# Start/stop steward manually if needed
kamuy start
kamuy stop

# Review pending transactions
kamuy pending

# Approve/reject if needed
kamuy approve <tx-id>

# View history
kamuy history
```

## Integration with AI Agents

The CLI is for the **wallet owner**. The AI agent operates independently:

1. **You** (via CLI):
   - Create wallet
   - Set policies
   - Approve exceptional transactions
   - Monitor activity

2. **AI Agent** (external software):
   - Has the Agent Key
   - Calls Steward API directly
   - Gets auto-signed if compliant
   - Gets rejected if non-compliant and you don't approve

## Development

### Running Tests

```bash
# Unit tests
cargo test -p kamuy-cli

# Integration tests
cargo test -p kamuy-cli --test integration_tests
```

### Building

```bash
# Debug build
cargo build -p kamuy-cli

# Release build
cargo build --release -p kamuy-cli
```

## Troubleshooting

### "Steward key not loaded"
Run `kamuy unlock` and enter your password.

### "Cannot connect to Steward"
Ensure the Steward service is running and `STEWARD_URL` is correct.

### "No approval channel available"
Enable terminal approval with `--terminal-enabled` or configure Telegram.

## License

MIT OR Apache-2.0
