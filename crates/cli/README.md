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

### Pre-built Binary (Recommended)

```bash
# Download latest release
curl -L https://github.com/kris/kamuy-wallet/releases/latest/download/kamuy-linux-amd64 -o kamuy
chmod +x kamuy
sudo mv kamuy /usr/local/bin/
```

### Build from Source

```bash
# Requires Rust
cargo build --release -p kamuy-cli

# The binary will be available at:
# ./target/release/kamuy

# Install to system
cargo install --path crates/cli
```

## Quick Start

### 1. Start the Steward Service

The Steward must be running before using the CLI:

```bash
export STEWARD_API_KEY="your-secret-key"
./kamuy-steward
```

### 2. Create Your Wallet

```bash
kamuy create-wallet

# Enter password for encrypting keys
# Save the AGENT KEY - give this to your AI agent
# Save the USER KEY - keep this for recovery
```

### 3. Configure Your AI Agent

Give these to your AI agent software (OpenClaw, etc.):
- **Agent Key**: From wallet creation
- **Steward URL**: `http://localhost:8080`
- **Steward API Key**: Same as `STEWARD_API_KEY`

### 4. Set Your Spending Policy

```bash
kamuy policy set max_per_tx 100
kamuy policy set require_approval_above 50
kamuy policy set allowed_tokens USDC,USDT,DAI
```

### 5. Unlock the Steward

```bash
kamuy unlock
# Enter Steward password
```

Now your AI agent can make payments according to your policy!

## Commands

### Wallet Management

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
# Basic status
kamuy status

# Detailed status
kamuy status --detailed
```

#### `unlock`
Load the STEWARD key with password.

```bash
kamuy unlock
# Enter password: ***
# ✓ Steward key loaded
```

**Required**: Must be done after starting the Steward service.

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
# Show current config
kamuy config show

# Set a value
kamuy config set STEWARD_url http://localhost:8080
kamuy config set api_key your-api-key
kamuy config set default_chain base

# Initialize config file
kamuy config init
```

## Configuration

The CLI reads configuration from `~/.config/kamuy/config.toml`:

```toml
STEWARD_url = "http://localhost:8080"
api_key = "your-api-key"
default_chain = "base"
default_chain_id = 8453
use_color = true
```

Configuration can also be set via environment variables:
- `STEWARD_URL` - Steward service URL
- `STEWARD_API_KEY` - API key for authentication

## File Locations

- **Config**: `~/.config/kamuy/config.toml`
- **Data**: `~/.local/share/kamuy/`
- **User Key**: `~/.local/share/kamuy/user.key` (backup this!)
- **Agent Key**: Displayed once during wallet creation
- **Steward Key**: Encrypted in database

## Security

- Keys are encrypted with ChaCha20-Poly1305 using Argon2
- Key files have restrictive permissions (0o600)
- Passwords are never displayed or logged
- All sensitive operations require password confirmation
- User Key should be backed up securely (for recovery)
- Agent Key is given to external agent software

## Typical Workflow

### First Time Setup

```bash
# 1. Start Steward (in separate terminal)
export STEWARD_API_KEY="secret-api-key"
./kamuy-steward

# 2. Create wallet
kamuy create-wallet
# Save Agent Key for your AI agent
# Save User Key in secure backup

# 3. Configure policy
kamuy policy set max_per_tx 100
kamuy policy set max_daily 1000
kamuy policy set require_approval_above 50

# 4. Unlock Steward
kamuy unlock
```

### Daily Operations

```bash
# Check status
kamuy status

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
