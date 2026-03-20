# Kamuy Wallet Skill

A gasless USDC wallet for AI agents. This skill enables OpenClaw agents to make payments and manage wallet operations using natural language commands.

## Overview

Kamuy Wallet gives AI agents the ability to spend USDC on behalf of users within strict policy boundaries. It uses MPC (multi-party computation) for security - the private key is split into three shares, and no single party ever has the complete key.

### Key Features

- **Gasless transactions** - No ETH needed, sponsored via Pimlico
- **Spending limits** - Per-transaction, daily, and weekly caps
- **Whitelist controls** - Only send to approved addresses
- **Approval workflows** - Higher-security operations require user confirmation
- **Natural language interface** - Tell your agent what to pay in plain English

## Installation

```bash
openclaw skill install kamuy-wallet
```

After installation, initialize your wallet:

```bash
kamuy init --email your@email.com
```

## Commands

### Payment Commands

The skill handles natural language payment requests:

| Trigger Words | Example |
|---------------|---------|
| `pay`, `send`, `spend`, `buy`, `purchase` | "Pay OpenAI $47 for API credits" |

**Handler:** `handlers.payment.handle_payment`

### Query Commands

Query wallet status, balances, and configuration:

| Trigger Words | Example |
|---------------|---------|
| `wallet`, `balance`, `policy` | "What's my wallet balance?" |

**Handler:** `handlers.query.handle_query`

## Natural Language Examples

### Payments

```
User: "Pay OpenAI $47 for API credits"
Agent: "Payment sent. $47 to OpenAI. Transaction: 0xabc..."

User: "Send $100 to 0x1234567890abcdef1234567890abcdef12345678"
Agent: "Address not in whitelist. This is under your $50 auto-add threshold. Add address and pay?"
       [Add & Pay] [Cancel]

User: "Spend $50 on AWS"
Agent: "$50 sent to AWS. Transaction: 0xdef..."
```

### Balance Queries

```
User: "What's my wallet balance?"
Agent: "Your wallet balances:
        - 150.00 USDC on Base
        - 50.00 USDC on Polygon"

User: "How much can I spend?"
Agent: "Your policy limits:
        - Max per transaction: $100
        - Max per day: $500 (you've spent $47 today)
        - Max per week: $2,000 (you've spent $247 this week)"
```

### History and Status

```
User: "Show recent transactions"
Agent: "Recent transactions (5 total):
        [CONFIRMED] $47.00 to OpenAI (0x1234...)
        [CONFIRMED] $23.00 to AWS (0x5678...)
        [PENDING] $150.00 to 0x9abc... - Awaiting approval"

User: "Who's in my whitelist?"
Agent: "Your whitelisted addresses (3 total):
        - OpenAI: 0x1234...5678
        - AWS: 0x5678...9abc
        - Anthropic: 0x9abc...def0"

User: "What's pending?"
Agent: "Pending approvals (1):
        - $150.00 to 0x9abc...def0
          Reason: Amount exceeds $100 per-tx limit
          ID: tx-abc123
        Approve in Telegram or run: kamuy approve tx tx-abc123"
```

## Security Model

### Three Key Shares

The wallet uses a 2-of-3 threshold signature scheme:

| Share | Holder | Purpose |
|-------|--------|---------|
| Agent Key (#1) | AI Agent | Initiates transactions |
| Steward Key (#2) | Steward Service | Policy validation and co-signing |
| User Key (#3) | User (backup) | Recovery and override |

**Security Guarantee:** No single party can sign alone. The agent cannot bypass policy.

### Approval Levels

| Level | When Used | Security |
|-------|-----------|----------|
| AutoApprove | Within limits + whitelisted address | No interaction |
| TelegramButton | Over limits or new address under threshold | One-click approval |
| TerminalPassword | Policy changes or sensitive operations | Password required |

### Password Security

Passwords are **NEVER** entered in Telegram. When a sensitive operation is needed:

1. Agent informs you via Telegram/chat
2. You run a terminal command: `kamuy approve ...`
3. Password is entered locally in the terminal

This prevents password exposure in chat history or to the AI agent.

## CLI Reference

| Command | Description |
|---------|-------------|
| `kamuy init --email <email>` | Create new wallet |
| `kamuy unlock` | Unlock wallet for transactions |
| `kamuy status` | Show wallet status and policy |
| `kamuy approve policy <id>` | Approve policy changes |
| `kamuy approve address <id>` | Approve new addresses |
| `kamuy approve tx <id>` | Approve pending transactions |
| `kamuy policy` | View/update spending policy |
| `kamuy pending` | View pending approvals |
| `kamuy history` | Transaction history |
| `kamuy lock` | Lock wallet |
| `kamuy recover <file>` | Restore from backup |

## Configuration

The skill expects the following dependencies:
- `kamuy-cli` - Command-line interface
- `kamuy-steward` - Policy engine (runs as background service)

### Environment Variables

| Variable | Description |
|----------|-------------|
| `KAMUY_STEWARD_URL` | Steward API URL (default: localhost:8080) |
| `STEWARD_API_KEY` | API key for agent authentication |

### Skill Hooks

| Hook | Command |
|------|---------|
| `on_install` | `kamuy init` |
| `on_start` | `kamuy unlock` |

## Supported Chains

| Chain | Chain ID | Status |
|-------|----------|--------|
| Base | 8453 | Primary |
| Base Sepolia | 84532 | Testnet |
| Polygon | 137 | Supported |
| Arbitrum | 42161 | Supported |

**Note:** Only USDC spending is supported. The wallet can receive any token, but the agent can only spend USDC.

## Troubleshooting

### Wallet Locked

```bash
kamuy unlock
```

### Steward Not Running

```bash
kamuy unlock  # Starts Steward if needed
```

### Transaction Pending

```bash
kamuy pending
kamuy approve tx <id>
```

### Address Not Whitelisted

- Under auto-add threshold: Agent prompts for one-click approval
- Over threshold: Run `kamuy approve address <id>` in terminal

## Full Documentation

For comprehensive documentation including:
- Detailed CLI reference
- Security model deep-dive
- Configuration options
- Troubleshooting guide

See: [README.md](./README.md)

## Version

- **Version:** 2.0.0
- **Author:** Kamuy
- **License:** MIT OR Apache-2.0