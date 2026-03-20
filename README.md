# Kamuy Wallet

A non-custodial, MPC-based wallet infrastructure designed specifically for AI agents to execute financial transactions on behalf of users.

## Overview

Kamuy Wallet uses a **2-of-3 threshold signature scheme** where:

- **Agent** (Key #1): The AI agent that executes tasks and initiates transactions. This is external software provided by the user - Kamuy only provides the key share.
- **Steward** (Key #2): An automated policy engine that validates and co-signs compliant transactions. Runs as a separate Rust service.
- **User** (Key #3): The ultimate owner who retains control for recovery and high-risk approvals.

### Key Insight: 2-of-3 Auto-Signing

When a transaction **passes all policy checks** (whitelisted address + within limits):
- **Steward + Agent** can sign together = **2 keys**
- This satisfies the 2-of-3 threshold
- **No user interaction required**

The User key is only needed for:
1. New addresses over the auto-add threshold (Tier 3)
2. Policy changes (Tier 3)
3. Wallet recovery

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

### 3. USDC-Only (v2.0)

Kamuy Wallet v2.0 supports **USDC only**:
- Simplified policy enforcement
- Gasless transactions via Pimlico sponsorship
- 6 decimal precision

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

## Transaction Flow & Approval Levels

The Steward uses a **3-tier approval system** based on policy evaluation:

### Tier 1: Auto-Approved (No User Interaction)

When a transaction passes all policy checks:
- Address is **whitelisted**
- Amount is **within limits** (per-tx, daily, weekly)

```
┌──────┐                    ┌─────────┐                    ┌──────────┐
│Agent │ ──POST /tx/sign──► │ Steward │ ──Validate Policy──┤  PASS    │
│      │                    │         │                    │          │
│      │                    │         │   Steward + Agent  │  SIGN    │
│      │ ◄──Signature───── │         │   = 2-of-3 keys   │          │
└──────┘                    └─────────┘                    └──────────┘

→ User sees: "Purchased $47 in OpenAI API credits. ✓"
```

**No user action required.** Steward and Agent keys together satisfy the 2-of-3 threshold.

### Tier 2: Telegram Button (Click Approval)

When a transaction violates policy but is **low risk**:
- Over per-tx limit (but to **whitelisted** address)
- New address **under** auto-add threshold

```
┌──────┐                    ┌─────────┐                    ┌───────────────┐
│Agent │ ──POST /tx/sign──► │ Steward │ ──Notify User─────►│ Telegram      │
│      │                    │         │                    │               │
│      │                    │         │   (No password)   │ [Approve] [Reject] │
│      │ ◄──Signature───── │         │ ◄──User clicks──────┤               │
└──────┘                    └─────────┘                    └───────────────┘

→ User sees: "This $150 exceeds your $100 limit. Approve anyway?"
```

**No password entered.** User just clicks [Approve] or [Reject].

### Tier 3: Terminal Password (Highest Security)

For **high-risk** operations:
- New address **over** auto-add threshold
- Policy changes

```
┌──────┐                    ┌─────────┐                    ┌───────────────┐
│Agent │ ──POST /tx/sign──► │ Steward │ ──Request Terminal─►│ CLI Terminal  │
│      │                    │         │                    │               │
│      │                    │         │   User runs:       │ $ kamuy approve│
│      │                    │         │   kamuy approve    │ Password: ****│
│      │ ◄──Signature───── │         │ ◄──After password───┤               │
└──────┘                    └─────────┘                    └───────────────┘

→ User sees: "Run 'kamuy approve address <id>' in terminal"
```

**Password ALWAYS in terminal, NEVER in Telegram.**

### Approval Level Summary

| Scenario | Address | Amount | User Action | Location |
|----------|---------|--------|-------------|----------|
| Within policy | Whitelisted | Under limit | **NONE** | Auto-approved |
| Over per-tx limit | Whitelisted | Any | Click button | Telegram |
| New address | Not whitelisted | Under threshold | Click button | Telegram |
| New address | Not whitelisted | Over threshold | Enter password | **Terminal** |
| Policy change | N/A | N/A | Enter password | **Terminal** |

## Project Structure

```
kamuy-wallet/
├── crates/
│   ├── mpc-core/           # CGGMP24 MPC implementation (Rust)
│   ├── steward/            # Steward service - policy engine & API (Rust)
│   │   ├── src/
│   │   │   ├── api/        # HTTP API routes (synchronous)
│   │   │   ├── approval/   # Approval channels (terminal + inline)
│   │   │   ├── config/     # Configuration management
│   │   │   ├── policy/     # Policy engine (USDC only in v2.0)
│   │   │   ├── queue/      # Transaction queue
│   │   │   ├── signing/    # MPC signing coordination
│   │   │   └── storage/    # SQLite/PostgreSQL storage
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
# v2.0: One-command setup with terminal password
./target/release/kamuy init --email user@example.com

Email for backup: user@example.com
Set wallet password: ********
Confirm password:  ********

✓ Wallet created
✓ Backup sent to user@example.com
✓ Steward running (unlocked)

Your wallet address: 0xABC...1234
```

This generates:
- Agent Key (give this to your AI agent software)
- Steward Key (encrypted, used by Steward service)
- User Key (encrypted, for recovery and Tier 3 approvals)

### 3. Set Policy (Conversational)

Tell your agent:
> "Set up my wallet policy"

The agent will guide you through:
- Max per transaction
- Daily limit
- Weekly limit
- Auto-add threshold

Then approve in terminal:
```bash
kamuy approve policy setup-001
Password: ********
✓ Policy confirmed. Wallet active.
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
| `STEWARD_POLICY_FILE` | No | Path to policy JSON file |
| `STEWARD_APPROVAL_TIMEOUT_SECS` | No | Approval timeout in seconds (default: 300) |
| `STEWARD_CHAIN_ID` | No | Chain ID (default: 84532 for Base Sepolia) |
| `STEWARD_PIMLICO_API_KEY` | No | Pimlico API key for gas sponsorship |

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

The Agent can request policy changes, but they **always require terminal password** (Tier 3):

```
┌──────┐                    ┌─────────┐                    ┌───────────────┐
│Agent │ ──POST /policy/request──► │ Steward │ ──Tell User──────►│ Agent Chat    │
│      │                    │         │                    │               │
│      │                    │         │   "Run kamuy       │               │
│      │                    │         │    approve policy │               │
│      │                    │         │    <id>"          │               │
└──────┘                    └─────────┘                    └───────────────┘

                                                                  ↓
                                                           ┌───────────────┐
                                                           │ Terminal      │
                                                           │               │
                                                           │ $ kamuy approve│
                                                           │ Password: ****│
                                                           │ ✓ Updated     │
                                                           └───────────────┘
```

**Security:**
- Agent cannot bypass this flow
- Password **always** entered in terminal, never in Telegram
- This is Tier 3 (TerminalPassword) approval

## Approval Channels

### v2.0: Inline in Agent's Chat

All communication happens in the user's **existing agent chat** (e.g., OpenClaw Telegram bot). There is **no separate Kamuy Telegram bot**.

For Tier 2 (Telegram Button):
- Agent sends message with inline buttons: [Approve] [Reject]
- User clicks button, transaction proceeds
- No password entered in Telegram

For Tier 3 (Terminal Password):
- Agent tells user: "Run 'kamuy approve address <id>' in terminal"
- User opens terminal, enters password there
- Password **never** sent to Telegram

### Terminal Commands

```bash
# Unlock wallet after restart
kamuy unlock
Password: ********
✓ Wallet unlocked. Agent can now spend.

# Approve policy change (Tier 3)
kamuy approve policy <id>
Password: ********
✓ Policy updated.

# Approve new address over threshold (Tier 3)
kamuy approve address <id>
Password: ********
✓ Address added to whitelist.
```

## License

MIT OR Apache-2.0
