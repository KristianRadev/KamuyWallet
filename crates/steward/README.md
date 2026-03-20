# Kamuy Steward Service

The Steward Service is the policy engine and transaction validator for Kamuy Wallet. It runs as a separate process from the AI Agent, providing security through process isolation.

## Features

- **Policy Engine**: Validate transactions against user-defined rules (spending limits, whitelists, time windows, rate limiting)
- **Synchronous API**: Agent submits transaction and waits for signature or rejection
- **Pluggable Approval Channels**: Terminal for development, Telegram for production
- **Stablecoin-Only**: Supports only USDC, USDT, and DAI
- **MPC Signing**: Co-signs transactions with the Agent (Key #2)
- **Queue Management**: Manages transactions requiring approval

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           STEWARD SERVICE                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                         HTTP API (Axum)                             │   │
│  │  POST /api/v1/transactions  →  Policy Check  →  Sign or Queue       │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│       ┌────────────────────────────┼────────────────────────────┐          │
│       │                            │                            │          │
│       ▼                            ▼                            ▼          │
│  ┌─────────┐                ┌─────────────┐                ┌──────────┐   │
│  │  Policy │                │   Approval  │                │  Queue   │   │
│  │ Engine  │───VIOLATION──►│   Channels  │                │ Processor│   │
│  │         │                │             │                │          │   │
│  │• Limits │                │• Terminal   │                │• Pending │   │
│  │• Tokens │                │• Telegram  │                │• Timeout │   │
│  │• Times  │                │             │                │• Retry   │   │
│  └─────────┘                └─────────────┘                └──────────┘   │
│       │                                                            │       │
│       │    ┌────────────────────────────────────────────────────────┘       │
│       │    │                                                               │
│       ▼    ▼                                                               │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                         StewardStorage                              │   │
│  │              (SQLite/PostgreSQL + Encrypted Key)                    │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Key Design Decisions

### Synchronous API Pattern

Unlike async callback-based systems, Kamuy Steward uses a synchronous request/response pattern:

1. **Agent sends**: `POST /api/v1/transactions` with transaction details
2. **Steward evaluates**: Checks policy rules (limits, tokens, whitelist, etc.)
3. **If compliant**: Returns signature immediately
4. **If violation**: Requests user approval via available channels, then returns result
5. **Agent receives**: Either `{signature, tx_hash}` or `{rejection_reason}`

Benefits:
- Simple HTTP client - no webhook infrastructure needed
- Predictable flow - Agent knows immediately if transaction proceeds
- Compatible with any agent framework
- Configurable timeout (default: 5 minutes)

### Stablecoin-Only Design

Kamuy only supports stablecoin transactions:
- **USDC** (Ethereum, Base, Polygon, Arbitrum)
- **USDT** (Ethereum, Base, Polygon)
- **DAI** (Ethereum, Base, Arbitrum)

This simplifies policy enforcement (no price volatility) and reduces attack surface.

### Pluggable Approval Channels

When a transaction violates policy, the Steward uses `CompositeApprovalChannel` to request user approval:

```rust
pub trait ApprovalChannel: Send + Sync {
    async fn request_approval(&self, tx: &TransactionRecord) -> Result<ApprovalDecision>;
    fn is_available(&self) -> bool;
}
```

**Channels (in order):**
1. **Telegram** (if enabled and configured)
2. **Terminal** (if running in interactive terminal)

The first available channel is used. If none are available, the transaction is rejected.

## Security Model

- **Process Isolation**: Steward runs as separate service, Agent cannot access its files
- **Key in Memory**: Steward Key (#2) never written to disk unencrypted (loaded at runtime)
- **API Authentication**: Agent needs valid API key (constant-time comparison)
- **Fail-Closed**: Denies requests when no API key is configured
- **Encrypted Storage**: Key shares and policies encrypted at rest
- **Sanitized Errors**: Internal errors hidden in production builds
- **Audit Logging**: All API calls and decisions logged

## Quick Start

### Configuration

Set environment variables:

```bash
# Required: API Configuration
export STEWARD_API_KEY="your-secret-api-key"
export STEWARD_API_PORT=8080

# Long-Polling Configuration
export STEWARD_LONG_POLL_TIMEOUT=30      # Max wait for transaction completion
export STEWARD_DEFAULT_WAIT_TIMEOUT=30   # Default wait if agent doesn't specify

# Database
export STEWARD_DATABASE_URL="sqlite://./steward.db"

# Approval Settings
export STEWARD_APPROVAL_TIMEOUT=300  # 5 minutes for user to respond
export STEWARD_TERMINAL_ENABLED=true      # Enable terminal approval

# Telegram Bot (optional but recommended for production)
export STEWARD_TELEGRAM_TOKEN="your-bot-token"
export STEWARD_TELEGRAM_ENABLED=true

# Policy
export STEWARD_POLICY_FILE="./policy.json"
```

### Running

```bash
# Build
cargo build --release

# Run
./target/release/kamuy-steward

# With logging
RUST_LOG=info ./target/release/kamuy-steward
```

### Docker

```bash
docker run -d \
  -e STEWARD_API_KEY="your-api-key" \
  -e STEWARD_DATABASE_URL="sqlite://./steward.db" \
  -e STEWARD_TELEGRAM_TOKEN="your-token" \
  -e STEWARD_TELEGRAM_ENABLED=true \
  -p 8080:8080 \
  kamuy/steward:latest
```

## Hybrid Long-Polling API

Kamuy uses a **hybrid approach** that provides the best of both worlds:

1. **Fast transactions** (auto-approved): Response in 1-2 seconds
2. **Quick approvals**: If user responds within timeout, still in same request
3. **Slow approvals**: Returns pending status, agent can poll later

```
POST /api/v1/transactions
{
  "to": "0x...",
  "value": "50000000",  // Amount in wei
  "token": "USDC",
  "chain_id": 8453,
  "wait": true,          // Wait for completion (default: true)
  "timeout_secs": 60     // Custom timeout (optional)
}
```

**Fast Path (auto-approved):**
```
Response (1-2 seconds):
{
  "success": true,
  "data": {
    "status": "confirmed",
    "signature": "0x...",
    "tx_hash": "0x..."
  }
}
```

**Timeout Path (user takes longer):**
```
Response (after timeout):
{
  "success": true,
  "data": {
    "status": "pending_approval",
    "tx_id": "uuid",
    "message": "Poll GET /transactions/{tx_id} for status"
  }
}
```

**Fire-and-forget (`wait=false`):**
```
Response (immediate):
{
  "success": true,
  "data": {
    "status": "pending",
    "tx_id": "uuid"
  }
}
```

## API Endpoints

### POST /api/v1/transactions

Submit a transaction for signing. Uses hybrid long-polling.

**Request Headers:**
```
Content-Type: application/json
x-api-key: your-secret-api-key
```

**Request Body:**
```json
{
  "to": "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb",
  "value": "50000000",
  "token": "USDC",
  "chain_id": 8453,
  "request_id": "optional-client-id",
  "wait": true,
  "timeout_secs": 60
}
```

**Note**: Amounts must be in **wei** (integer format), not decimal strings.
- 50 USDC = `"50000000"` (50 * 10^6)
- 1 USDC = `"1000000"` (1 * 10^6)
- 1 DAI = `"1000000000000000000"` (1 * 10^18)

**Response (Completed):**
```json
{
  "success": true,
  "data": {
    "tx_id": "550e8400-e29b-41d4-a716-446655440000",
    "status": "confirmed",
    "signature": "0x...",
    "tx_hash": "0x..."
  },
  "request_id": "optional-client-id"
}
```

**Response (Pending Approval):**
```json
{
  "success": true,
  "data": {
    "tx_id": "550e8400-e29b-41d4-a716-446655440000",
    "status": "pending_approval",
    "message": "Transaction still processing. Poll GET /transactions/{tx_id} for status."
  },
  "request_id": "optional-client-id"
}
  "request_id": "optional-client-id"
}
```

**Response (Rejected):**
```json
{
  "success": false,
  "error": "Token XYZ is not a supported stablecoin",
  "request_id": "optional-client-id"
}
```

### GET /api/v1/transactions/:id

Get transaction status and details.

### GET /api/v1/transactions/pending

List transactions awaiting approval.

### POST /api/v1/transactions/:id/approve

Approve a pending transaction (admin/steward endpoint).

### POST /api/v1/transactions/:id/reject

Reject a pending transaction.

### GET /api/v1/policy

Get current policy configuration.

### PUT /api/v1/policy

Update policy (requires authentication).

### GET /api/v1/wallet

Get wallet information (address, status).

### GET /health

Health check with component status.

## Policy Configuration

### Example `policy.json`

```json
{
  "version": "1.0",
  "max_per_tx": "100.00",
  "max_daily": "1000.00",
  "max_weekly": "5000.00",
  "max_monthly": "20000.00",
  "require_approval_above": "50.00",
  "allowed_tokens": ["USDC", "USDT", "DAI"],
  "whitelist": ["0x742d35...", "openai.com", "anthropic.com"],
  "blocklist": [],
  "time_windows": [
    {
      "days": ["mon", "tue", "wed", "thu", "fri"],
      "start": "09:00",
      "end": "18:00"
    }
  ],
  "rate_limit_per_hour": 10,
  "rate_limit_per_day": 100,
  "notify_on_approval": true,
  "notify_on_rejection": true
}
```

### Policy Rules

| Rule | Description |
|------|-------------|
| `max_per_tx` | Maximum amount per transaction |
| `max_daily` | Maximum daily spending |
| `max_weekly` | Maximum weekly spending |
| `max_monthly` | Maximum monthly spending |
| `require_approval_above` | Amount threshold requiring user approval |
| `allowed_tokens` | Supported tokens (USDC, USDT, DAI only) |
| `whitelist` | Allowed destinations (addresses or domains) |
| `blocklist` | Blocked destinations |
| `time_windows` | Allowed transaction times |
| `rate_limit_per_hour` | Max transactions per hour |
| `rate_limit_per_day` | Max transactions per day |

### Policy Evaluation Order

1. **Token validation** - Must be USDC, USDT, or DAI
2. **Amount validation** - Check per-transaction limit
3. **Spending tracking** - Check daily/weekly/monthly limits
4. **Whitelist/Blocklist** - Check destination
5. **Time windows** - Check current time
6. **Rate limits** - Check transaction frequency
7. **Auto-approve check** - Amount below threshold → auto-sign
8. **Approval required** - Otherwise, request user approval

## Approval Channels

### Terminal Approval Channel

Used during development and testing when running the Steward in an interactive terminal.

**Requirements:**
- `STEWARD_TERMINAL_ENABLED=true`
- Stdin is a TTY (`atty::is(Stream::Stdin)`)

**Flow:**
```
╔═══════════════════════════════════════════════════════════╗
║           TRANSACTION REQUIRES APPROVAL                   ║
╠═══════════════════════════════════════════════════════════╣
║ Transaction ID: 550e8400-e29b-41d4-a716-446655440000     ║
║ To:           0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb ║
║ Amount:       150.00 USDC                                 ║
║ Chain:        Base (ID: 8453)                             ║
║ Reason:       Amount exceeds auto-approve limit           ║
╚═══════════════════════════════════════════════════════════╝

Approve this transaction? [y/N/timeout]:
```

User types `y` or `yes` to approve, anything else to reject.

### Telegram Approval Channel

Used in production for mobile notifications.

**Setup:**
1. Create a bot via [@BotFather](https://t.me/botfather)
2. Set `STEWARD_TELEGRAM_TOKEN` and `STEWARD_TELEGRAM_ENABLED=true`
3. User starts bot with `/start` and links their wallet

**Notification:**
```
🚨 Transaction Requires Approval

💰 Amount: 150.00 USDC
📍 To: 0x742d...0bEb
⛓ Chain: Base
⚠️ Reason: Exceeds auto-approve limit

[Approve] [Reject]
```

**Timeout:** If user doesn't respond within `STEWARD_APPROVAL_TIMEOUT_SECS`, the transaction is rejected.

### Composite Channel Behavior

```rust
CompositeApprovalChannel::new(
    vec![
        Box::new(TelegramApprovalChannel::new(tg_config)),  // Try first
        Box::new(TerminalApprovalChannel::new()),            // Fallback
    ],
    Duration::from_secs(300)
)
```

The channel iterates through available channels and uses the first one that reports `is_available() == true`.

## Transaction Flow

### Auto-Approved Transaction

```
┌───────┐      POST /tx/sign      ┌─────────┐
│ Agent │ ───────────────────────►│ Steward │
│       │                         │         │
│       │◄────────────────────────│         │
│       │     {signature, hash}   │         │
└───────┘                         └────┬────┘
                                       │
                                  ┌────┴────┐
                                  │ Policy  │
                                  │ Engine  │
                                  └────┬────┘
                                       │
                                  ┌────┴────┐
                                  │  PASS   │
                                  │ Auto-sign
                                  └─────────┘
```

### Approval-Required Transaction

```
┌───────┐      POST /tx/sign      ┌─────────┐
│ Agent │ ───────────────────────►│ Steward │
│       │                         │         │
│       │◄───Wait/Pending─────────│         │
│       │                         │         │
│       │◄───Signature/Rejection──│         │
│       │                         └────┬────┘
└───────┘                              │
                                  ┌────┴────┐
                                  │ Policy  │
                                  │ Engine  │
                                  └────┬────┘
                                       │ VIOLATION
                                  ┌────┴────────────────────┐
                                  │   Approval Channels     │
                                  │ ┌─────────────────────┐ │
                                  │ │  Telegram or        │ │
                                  │ │  Terminal prompt    │ │
                                  │ └─────────────────────┘ │
                                  └────┬────────────────────┘
                                       │ User response
                                  ┌────┴────┐
                                  │ Sign or │
                                  │ Reject  │
                                  └─────────┘
```

## Development

### Building

```bash
cargo build --release
```

### Testing

```bash
cargo test
```

### Running with Full Logging

```bash
RUST_LOG=debug cargo run
```

### Testing Terminal Approval

1. Run steward: `cargo run`
2. Submit transaction that exceeds limits
3. See terminal prompt
4. Type `y` to approve or `n` to reject

## Environment Reference

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `STEWARD_API_KEY` | **Yes** | - | Secret key for Agent authentication |
| `STEWARD_API_PORT` | No | 8080 | HTTP API port |
| `STEWARD_DATABASE_URL` | No | `sqlite://./steward.db` | Database connection string |
| `STEWARD_TELEGRAM_TOKEN` | No | - | Telegram bot token |
| `STEWARD_TELEGRAM_ENABLED` | No | false | Enable Telegram bot |
| `STEWARD_TERMINAL_ENABLED` | No | true | Enable terminal approval |
| `STEWARD_APPROVAL_TIMEOUT_SECS` | No | 300 | Approval timeout in seconds |
| `STEWARD_POLICY_FILE` | No | `./policy.json` | Path to policy file |
| `RUST_LOG` | No | info | Logging level (error, warn, info, debug, trace) |

## License

MIT OR Apache-2.0
