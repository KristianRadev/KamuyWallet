# Kamuy Wallet

A non-custodial, MPC-based wallet infrastructure designed specifically for AI agents to execute financial transactions on behalf of users.

## Overview

Kamuy Wallet uses a **2-of-3 threshold signature scheme** where:

- **Agent** (Key #1): The AI agent that executes tasks and initiates transactions. This is external software provided by the user - Kamuy only provides the key share.
- **Steward** (Key #2): An automated policy engine that validates and co-signs compliant transactions. Runs as a separate Rust service.
- **User** (Key #3): The ultimate owner who retains control for recovery and overrides.

The Steward automatically co-signs transactions that comply with user-defined policies, enabling the AI agent to operate autonomously within defined boundaries.

## Key Design Decisions

### 1. External Agent Model
The AI Agent is **not** part of Kamuy Wallet. Users bring their own agent software (OpenClaw, custom agents, etc.) and configure it with the Agent Key share. Kamuy only provides:
- The Steward service for policy enforcement and co-signing
- Key generation and management tools
- The smart contract infrastructure

### 2. Synchronous API Communication
The Agent communicates with the Steward via **synchronous HTTP API calls**:
- Agent submits transaction → Steward evaluates policy → Returns signature or rejection
- Long-polling with configurable timeout (default: 5 minutes)
- No callback URLs or async notification mechanisms required
- Simple request/response pattern compatible with any HTTP client

### 3. Stablecoin-Only Support
Kamuy Wallet is designed exclusively for stablecoin payments:
- **USDC** (primary)
- **USDT**
- **DAI**

This simplifies policy enforcement, reduces volatility risk, and ensures predictable transaction values.

### 4. Pluggable Approval Channels
When a transaction requires user approval (policy violation), the Steward uses a composite approval channel:
- **Terminal** (always available): Interactive console approval for testing/development
- **Telegram** (optional): Mobile notifications for production use

The system tries channels in order until one responds.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           KAMUY WALLET ARCHITECTURE                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────────────┐          ┌─────────────────────────────────────┐     │
│  │  EXTERNAL AGENT  │  HTTP    │    STEWARD SERVICE (Rust)           │     │
│  │  (User's Agent)  │  API     │    (separate process)               │     │
│  │                  │◄────────►│                                     │     │
│  │  • Has Agent Key │  SYNC    │  • Has Steward Key (#2)            │     │
│  │  • Calls API    │          │  • Stores policies (encrypted)      │     │
│  │  • Waits for    │          │  • Validates + co-signs             │     │
│  │    signature    │          │  • Terminal/Telegram approval       │     │
│  │    or rejection │          │  • Only USDC/USDT/DAI supported     │     │
│  └──────────────────┘          └─────────────────────────────────────┘     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Transaction Flow

### Auto-Approved Transaction (Compliant with Policy)

```
┌──────┐                    ┌─────────┐                    ┌──────────┐
│Agent │ ──POST /tx/sign──► │ Steward │ ──Validate Policy──┤  PASS    │
│      │                    │         │                    │  SIGN    │
│      │ ◄──Signature───── │         │ ◄──Co-sign tx─────┤          │
└──────┘                    └─────────┘                    └──────────┘
```

### Approval Required Transaction (Policy Violation)

```
┌──────┐                    ┌─────────┐                    ┌───────────────┐
│Agent │ ──POST /tx/sign──► │ Steward │ ──Validate Policy──┤   VIOLATION   │
│      │                    │         │                    │               │
│      │ ◄──Wait/Pending─── │         │ ──Request Approval─┤ Terminal/TG   │
│      │                    │         │ ◄──User approves───┤               │
│      │ ◄──Signature───── │         │ ──Co-sign tx───────┤   SIGN        │
└──────┘                    └─────────┘                    └───────────────┘
```

## Project Structure

```
kamuy-wallet/
├── crates/
│   ├── mpc-core/           # CGGMP24 MPC implementation (Rust)
│   ├── steward/            # Steward service - policy engine & API (Rust)
│   │   ├── src/
│   │   │   ├── api/        # HTTP API routes (synchronous)
│   │   │   ├── approval/   # Pluggable approval channels
│   │   │   ├── config/     # Configuration management
│   │   │   ├── policy/     # Policy engine (stablecoin-only)
│   │   │   ├── queue/      # Transaction queue
│   │   │   ├── signing/    # MPC signing coordination
│   │   │   ├── storage/    # SQLite/PostgreSQL storage
│   │   │   └── telegram/   # Telegram bot integration
│   │   └── Cargo.toml
│   ├── cli/                # Command-line interface (Rust)
│   └── smart-account/      # Smart contract utilities
├── contracts-eth/          # Ethereum smart contracts (Solidity)
│   ├── src/
│   │   ├── AgentWallet.sol # ERC-4337 smart account
│   │   └── Paymaster.sol   # Fee-collecting paymaster
│   └── test/
└── policies/               # Policy file schemas and examples
```

## Technology Stack

| Component | Technology |
|-----------|------------|
| **MPC Core** | Rust + CGGMP24 |
| **Steward Service** | Rust + Axum |
| **Database** | SQLite (dev) / PostgreSQL (prod) |
| **Smart Contracts** | Solidity + Foundry |
| **CLI** | Rust |

## Distribution

Pre-built binaries will be provided for easy installation. Until then, building from source requires Rust.

## Quick Start

### Installation (From Source)

```bash
git clone https://github.com/kris/kamuy-wallet
cd kamuy-wallet
cargo build --release
```

### 1. Initialize the Steward

```bash
# Set required environment variables
export STEWARD_API_KEY="your-secret-api-key"
export STEWARD_DATABASE_URL="sqlite://./steward.db"

# Optional: Enable Telegram notifications
export STEWARD_TELEGRAM_TOKEN="your-bot-token"
export STEWARD_TELEGRAM_ENABLED=true

# Run the Steward service
./target/release/kamuy-steward
```

### 2. Create a Wallet

```bash
./target/release/kamuy-cli wallet create
```

This generates:
- Agent Key (give this to your AI agent software)
- Steward Key (encrypted, used by Steward service)
- User Key (encrypted, for recovery)

### 3. Set Policy

```bash
./target/release/kamuy-cli policy set --file policy.json
```

Example `policy.json`:
```json
{
  "max_per_tx": "100.00",
  "max_daily": "1000.00",
  "require_approval_above": "50.00",
  "allowed_tokens": ["USDC", "USDT", "DAI"],
  "whitelist": ["0x1234...", "openai.com"]
}
```

### 4. Configure Your Agent

Configure your external AI agent (OpenClaw, etc.) with:
- **Agent Key**: The key share generated in step 2
- **Steward URL**: `http://localhost:8080`
- **Steward API Key**: Same as `STEWARD_API_KEY` above

The agent makes synchronous HTTP calls:

```bash
curl -X POST http://localhost:8080/api/v1/transactions \
  -H "Content-Type: application/json" \
  -H "x-api-key: your-secret-api-key" \
  -d '{
    "to": "0x...",
    "amount": "50.00",
    "token": "USDC",
    "chain_id": 8453
  }'
```

Response:
```json
{
  "success": true,
  "data": {
    "signature": "0x...",
    "tx_hash": "0x..."
  }
}
```

## Security Model

1. **Non-custodial**: Full private key never exists in one place
2. **Threshold**: 2-of-3 required for any signature
3. **Process Isolation**: Steward runs as separate process from Agent
4. **Encrypted storage**: Key shares encrypted at rest (Steward requires unlock)
5. **Fail-closed**: Default deny on policy violations or missing API key
6. **Constant-time validation**: API keys compared in constant time to prevent timing attacks
7. **Sanitized errors**: Internal errors hidden in production mode

## Configuration

### Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `STEWARD_API_KEY` | Yes | Secret key for Agent authentication |
| `STEWARD_DATABASE_URL` | No | Database URL (default: `sqlite://./steward.db`) |
| `STEWARD_API_PORT` | No | API port (default: 8080) |
| `STEWARD_TELEGRAM_TOKEN` | No | Telegram bot token |
| `STEWARD_TELEGRAM_ENABLED` | No | Enable Telegram bot (default: false) |
| `STEWARD_POLICY_FILE` | No | Path to policy JSON file |
| `STEWARD_APPROVAL_TIMEOUT_SECS` | No | Approval timeout in seconds (default: 300) |

## API Reference

### POST /api/v1/transactions

Submit a transaction for signing.

**Request:**
```json
{
  "to": "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb",
  "amount": "50.00",
  "token": "USDC",
  "chain_id": 8453,
  "request_id": "optional-unique-id"
}
```

**Response (Auto-approved):**
```json
{
  "success": true,
  "data": {
    "tx_id": "uuid",
    "status": "signed",
    "signature": "0x...",
    "tx_hash": "0x..."
  },
  "request_id": "..."
}
```

**Response (Pending Approval):**
```json
{
  "success": true,
  "data": {
    "tx_id": "uuid",
    "status": "pending_approval",
    "reason": "Amount exceeds auto-approve limit"
  },
  "request_id": "..."
}
```

### GET /api/v1/transactions/:id

Check transaction status.

### POST /api/v1/policy/request

Request a policy change. The Agent can propose changes, but they require User approval via Telegram.

**Request:**
```json
{
  "field": "max_daily",
  "new_value": "2000000000",
  "reason": "Need higher daily limit for increased transaction volume"
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "policy_change_id": "uuid",
    "status": "pending_approval",
    "message": "Policy change request submitted. Check Telegram to approve.",
    "field": "max_daily",
    "current_value": "1000000000",
    "new_value": "2000000000"
  },
  "request_id": "..."
}
```

**Valid fields for change:**
- `max_per_tx` - Maximum per transaction
- `max_daily` - Maximum daily spending
- `max_weekly` - Maximum weekly spending
- `max_monthly` - Maximum monthly spending
- `require_approval_above` - Amount above which approval is required
- `rate_limit_per_hour` - Maximum transactions per hour
- `rate_limit_per_day` - Maximum transactions per day

### GET /api/v1/policy/requests

List pending policy change requests.

### GET /api/v1/policy/requests/:id

Get a specific policy change request.

### GET /health

Health check endpoint.

## Policy Change Approval Flow

The Agent can request policy changes, but they always require User approval via Telegram:

```
┌──────┐                    ┌─────────┐                    ┌───────────────┐
│Agent │ ──POST /policy/request──► │ Steward │ ──Notification──►│ Telegram      │
│      │                    │         │                    │               │
│      │ ◄──Pending Approval────── │         │ ◄──User approves───┤ [Approve] [Reject] │
│      │                    │         │                    │               │
│      │ ◄──Policy Updated────────── │         │                    │               │
└──────┘                    └─────────┘                    └───────────────┘
```

**Security:** The Agent cannot bypass this flow. Policy changes are always sent to Telegram for User approval with password verification.

## Approval Channels

### Terminal (Development/Testing)

When a transaction requires approval and the Steward is running in an interactive terminal:

```
╔═══════════════════════════════════════════════════════════╗
║           TRANSACTION REQUIRES APPROVAL                   ║
╠═══════════════════════════════════════════════════════════╣
║ Transaction ID: uuid                                      ║
║ To:           0x742d...                                   ║
║ Amount:       50.00 USDC                                  ║
║ Chain:        Base (ID: 8453)                             ║
║ Reason:       Amount exceeds auto-approve limit           ║
╚═══════════════════════════════════════════════════════════╝

Approve this transaction? [y/N/timeout]:
```

### Telegram (Production)

Enable Telegram for mobile notifications:
1. Create a bot via @BotFather
2. Set `STEWARD_TELEGRAM_TOKEN` and `STEWARD_TELEGRAM_ENABLED=true`
3. Start the bot with `/start`
4. Approve transactions via inline buttons

## License

MIT OR Apache-2.0
