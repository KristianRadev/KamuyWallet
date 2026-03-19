# Kamuy Wallet PRD

## Product Requirements Document

**Version:** 2.0
**Date:** March 2026
**Project Name:** Kamuy Wallet
**Status:** Approved Design
**Last Updated:** 2026-03-20

**Key Changes in v2.0:**
- **One-command setup**: Install via `openclaw skill install kamuy-wallet`
- **Inline Telegram**: No separate bot — all communication in agent's existing chat
- **Terminal passwords**: Password never enters Telegram (security)
- **Conversational policy**: User sets limits by talking to agent
- **USDC-only spending**: Gasless transactions via Pimlico sponsorship
- **Empty whitelist default**: Addresses added as needed during payments

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Problem Statement](#problem-statement)
3. [Product Vision](#product-vision)
4. [Architecture Overview](#architecture-overview)
5. [Key Definitions](#key-definitions)
6. [Core Components](#core-components)
7. [User Flows](#user-flows)
8. [Policy System](#policy-system)
9. [Transaction Flow](#transaction-flow)
10. [Monetization](#monetization)
11. [Technical Specification](#technical-specification)
12. [EVM Chains Supported](#evm-chains-supported)
13. [Security Model](#security-model)
14. [Roadmap](#roadmap)
15. [Glossary](#glossary)

---

## 1. Executive Summary

**Kamuy Wallet** is a non-custodial, MPC-based wallet designed for AI agents to spend on behalf of users. It installs as a plugin into existing agent platforms (OpenClaw, Claude Code) with minimal friction.

### The Problem
Users want to give their AI agents spending power, but:
- Setting up crypto wallets is complex
- Managing separate bots/apps creates friction
- Security concerns with agent access to funds

### The Solution
Kamuy Wallet becomes invisible infrastructure:
1. **One command install** — `openclaw skill install kamuy-wallet`
2. **One chat for everything** — User's existing agent chat handles approvals
3. **Password protected** — Sensitive operations require terminal password
4. **Gasless USDC** — No ETH needed, sponsored transactions

### 2-of-3 Threshold Signature Scheme

- **Agent** (Key #1): The AI agent that initiates transactions
- **Steward** (Key #2): Policy engine that validates and co-signs
- **User** (Key #3): Recovery and override authority

### Target Users

| Segment | Description | Priority |
|---------|-------------|----------|
| **Primary (C)** | End users who want to give AI agents spending power | First |
| **Secondary (B)** | AI agent operators (non-developers) | Second |
| **Tertiary (A)** | Developers building AI agents | Third |

---

## 2. Problem Statement

### User Problems Solved

| Problem | Kamuy Solution |
|---------|----------------|
| "I don't trust my agent with my money" | Spending limits + whitelist + approval for exceptions |
| "Crypto wallets are too complicated" | One command install, conversational setup |
| "I don't want another app" | Everything in your existing agent chat |
| "I don't have ETH for gas" | Gasless USDC transactions (sponsored) |
| "What if something goes wrong?" | Daily/weekly limits cap damage; instant alerts |

### What We Don't Do

- We don't build AI agents (users bring their own)
- We don't support multiple tokens for spending (USDC only)
- We don't host user funds (non-custodial)
- We don't require users to manage private keys

---

## 3. Product Vision

> **"Give your AI agent a wallet in 30 seconds. Control it forever."**

Kamuy Wallet enables AI agents to autonomously execute USDC transactions within strict policy boundaries, while users maintain ultimate control through their existing agent chat.

### Core Principles

1. **Invisible infrastructure** — Wallet disappears into the agent experience
2. **Policy-bound autonomy** — Agents act freely within defined rules
3. **Non-custodial** — Users always own their keys
4. **Fail-closed** — Default deny on policy violations
5. **Earn through service** — Gas sponsorship revenue

---

## 4. Architecture Overview

### System Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    KAMUY WALLET v2.0 ARCHITECTURE                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  User's Machine:                                                            │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                                                                     │   │
│  │  ┌──────────────────┐    ┌─────────────────────────────────────┐  │   │
│  │  │  OpenClaw Agent  │    │  Kamuy Skill (bundled)              │  │   │
│  │  │  (existing)      │    │  ├── CLI (kamuy init/approve)       │  │   │
│  │  │                  │◄──►│  ├── Steward binary (background)    │  │   │
│  │  │  Telegram Bot    │    │  └── Skill definition               │  │   │
│  │  │                  │    │                                     │  │   │
│  │  └──────────────────┘    └─────────────────────────────────────┘  │   │
│  │                                                                     │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  External:                                                                  │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────────┐   │
│  │ Telegram API    │    │ Pimlico API     │    │ User's Email        │   │
│  │ (agent's bot)   │    │ (gas sponsor)   │    │ (backup delivery)   │   │
│  └─────────────────┘    └─────────────────┘    └─────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Key Architectural Decisions

| Decision | Rationale |
|----------|-----------|
| Local-first (no hosted service) | User sovereignty, no server costs, simpler model |
| Plugin/skill model | Users already have agents; we plug into them |
| Single chat | No separate bot reduces friction and setup steps |
| Terminal for passwords | Passwords never touch Telegram (security) |
| USDC only | Simplifies policy, reduces volatility, enables gasless |

### Process Isolation

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    PROCESS ISOLATION SECURITY                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Same machine, DIFFERENT process:                                           │
│                                                                             │
│  ┌──────────────────┐          ┌──────────────────────────────┐           │
│  │  AI AGENT        │  HTTPS  │    STEWARD SERVICE            │           │
│  │                  │  API    │    (separate process)         │           │
│  │  • Has Agent Key │ ──────▶ │  • Has Steward Key (#2)      │           │
│  │  • Calls API    │         │  • Stores policies (encrypted)│           │
│  │  • No password  │         │  • Validates + co-signs       │           │
│  │    access       │         │                               │           │
│  └──────────────────┘         └──────────────────────────────┘           │
│                                                                             │
│  Agent CANNOT:                                                              │
│  • Read Steward Key                                                         │
│  • Read user's password                                                     │
│  • Change policies without terminal confirmation                           │
│  • Bypass spending limits                                                   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 5. Key Definitions

| Term | Definition |
|------|------------|
| **MPC (Multi-Party Computation)** | Cryptographic technique where a private key is split into shares |
| **Threshold Signature** | Signature requiring M-of-N shares (we use 2-of-3) |
| **Agent Key (#1)** | Key share held by the AI agent for initiating transactions |
| **Steward Key (#2)** | Key share held by Steward service for policy enforcement |
| **User Key (#3)** | Key share held by user for recovery and override |
| **Policy** | User-defined rules governing spending behavior |
| **Whitelist** | Approved addresses the agent can send to |
| **Auto-add threshold** | Amount under which new addresses auto-add to whitelist |
| **Gasless** | User pays no ETH; gas sponsored via Paymaster |
| **Skill** | Plugin/module that extends an AI agent's capabilities |

---

## 6. Core Components

### 6.1 Kamuy CLI

**Commands:**

| Command | Description |
|---------|-------------|
| `kamuy init` | Create wallet, set password, send backup |
| `kamuy unlock` | Unlock wallet after restart |
| `kamuy approve policy <id>` | Confirm policy change with password |
| `kamuy approve address <id>` | Add new address with password |
| `kamuy recover <backup>` | Restore wallet from backup file |
| `kamuy status` | Show wallet status |

### 6.2 Steward Service

**Responsibilities:**
- Policy validation and enforcement
- Key share management (Steward Key #2)
- Transaction co-signing
- Spending limit tracking
- Whitelist management

### 6.3 OpenClaw Skill

**Triggers:**

| User Says | Skill Action |
|-----------|--------------|
| "pay X to Y" | Initiate transaction |
| "send X to Y" | Initiate transaction |
| "set up wallet policy" | Start policy configuration |
| "increase daily limit" | Request policy change |
| "check balance" | Show USDC balance |

### 6.4 Paymaster (Pimlico Integration)

- Sponsors gas fees for all transactions
- Collects fee percentage on gas costs
- User pays nothing (gasless experience)

---

## 7. User Flows

### 7.1 One-Time Setup

```
$ openclaw skill install kamuy-wallet

Installing Kamuy Wallet...
✓ CLI installed
✓ Steward binary installed
✓ Skill registered

$ kamuy init

Email for backup: user@example.com
Set wallet password: ********
Confirm password:  ********

✓ Wallet created
✓ Backup sent to user@example.com
✓ Steward running (unlocked)

Your wallet address: 0xABC...1234

Next: Tell your agent "Set up my wallet policy"
```

### 7.2 Policy Setup (Conversational)

```
Agent: "Let's configure your wallet policy.

       What's your maximum per transaction?
       (This is the most I can spend in a single payment)"

User: "$100"

Agent: "Maximum daily spend?
       (You'll get a notification when this limit is reached)"

User: "$500"

Agent: "Maximum weekly spend?"

User: "$2,000"

Agent: "Auto-add threshold for new addresses?
       (When paying a new address under this amount, I can add it to
       your whitelist automatically. Over this amount requires your
       password confirmation. Default: $50)"

User: "$50"

Agent: "Your policy:

       • Max per transaction: $100
       • Max daily: $500 (you'll be notified when reached)
       • Max weekly: $2,000
       • Auto-add threshold: $50
       • Token: USDC only (gasless via sponsored transactions)
       • Whitelist: Empty (I can ONLY send to whitelisted addresses.
         New addresses can be added during payment.)

       Confirm in terminal: kamuy approve policy setup-001"
```

```
$ kamuy approve policy setup-001
Password: ********
✓ Policy confirmed. Wallet active.
```

### 7.3 Daily Use — Auto-Approved

```
Agent: "Purchased $47 in OpenAI API credits. ✓"
```

(Amount under limit, address whitelisted → automatic)

### 7.4 New Address — Under Threshold

```
Agent: "I need to pay OpenAI $23 for API credits.
       Address: 0x1234567890abcdef1234567890abcdef12345678 isn't whitelisted.

       This is under your $50 auto-add threshold.
       Add address and pay?"

       [Add & Pay]  [Cancel]

User: [Add & Pay]

Agent: "✓ Done. OpenAI added to whitelist, $23 sent."
```

### 7.5 New Address — Over Threshold

```
Agent: "I need to pay AWS $200 for servers.
       Address: 0x567890abcdef1234567890abcdef1234567890ab isn't whitelisted.

       This exceeds your $50 auto-add threshold.
       To approve, run in terminal:
       kamuy approve address aws-001"
```

```
$ kamuy approve address aws-001
Password: ********
✓ Address added. Transaction queued. Send now?
[Y/n]: Y
✓ $200 sent to AWS.
```

### 7.6 Daily Limit Alert

```
Agent: "⚠️ Daily spending limit reached: $500 of $500

       Recent transactions:
       • OpenAI API: $47 → 0x1234567890abcdef1234567890abcdef12345678
       • AWS servers: $200 → 0x567890abcdef1234567890abcdef1234567890ab
       • Cloudflare: $150 → 0x9abcdef01234567890abcdef01234567890abcdef
       • Anthropic API: $103 → 0x2468135790abcdef2468135790abcdef24681357

       I won't make any more payments today unless you
       increase your daily limit. To adjust:
       'Increase daily limit to $1000'"
```

### 7.7 Wallet Unlock

```
$ kamuy unlock
Password: ********
✓ Wallet unlocked. Agent can now spend.
```

### 7.8 Recovery

```
$ kamuy recover backup.enc
Password: ********
✓ Wallet restored from backup.
```

---

## 8. Policy System

### 8.1 Policy Schema (v2.0)

```json
{
  "version": "2.0",
  "max_per_tx": "100.00",
  "max_daily": "500.00",
  "max_weekly": "2000.00",
  "auto_add_threshold": "50.00",
  "token": "USDC",
  "gasless": true,
  "whitelist": {
    "0x1234567890abcdef1234567890abcdef12345678": {
      "label": "OpenAI",
      "added_at": "2026-03-20T10:30:00Z",
      "total_sent": "47.00"
    }
  },
  "spending_tracker": {
    "daily_spent": "0.00",
    "weekly_spent": "0.00",
    "last_reset_daily": "2026-03-20T00:00:00Z",
    "last_reset_weekly": "2026-03-16T00:00:00Z"
  }
}
```

### 8.2 Policy Rules

| Rule | Type | Description |
|------|------|-------------|
| `max_per_tx` | Limit | Maximum single transaction |
| `max_daily` | Limit | Maximum total per day |
| `max_weekly` | Limit | Maximum total per week |
| `auto_add_threshold` | Threshold | Amount under which new addresses auto-add |
| `token` | Fixed | Always "USDC" (hardcoded) |
| `whitelist` | List | Only these addresses can receive funds |

### 8.3 Policy Evaluation Flow

```
Transaction Request
        │
        ▼
┌───────────────────┐
│ Token == USDC?    │──No──▶ REJECT
└───────────────────┘
        │Yes
        ▼
┌───────────────────┐
│ Amount ≤ max_per_tx?│──No──▶ Request user approval
└───────────────────┘
        │Yes
        ▼
┌───────────────────┐
│ Daily limit OK?   │──No──▶ Block + alert user
└───────────────────┘
        │Yes
        ▼
┌───────────────────┐
│ Address in whitelist?│──No──▶ Check auto-add threshold
└───────────────────┘
        │Yes
        ▼
    AUTO-APPROVE
```

---

## 9. Transaction Flow

### 9.1 Auto-Approved Transaction

```
Agent: "Pay OpenAI $47"
        │
        ▼
Steward: Validate policy
        │
        ├── Amount: $47 ≤ $100 ✓
        ├── Daily: $47 + $0 ≤ $500 ✓
        ├── Token: USDC ✓
        └── Address: In whitelist ✓
        │
        ▼
Steward: Co-sign with Agent
        │
        ▼
Pimlico: Sponsor gas, submit to chain
        │
        ▼
Agent: "✓ Paid $47 to OpenAI"
```

### 9.2 Transaction Requiring Approval

```
Agent: "Pay NewService $200"
        │
        ▼
Steward: Validate policy
        │
        ├── Amount: $200 > $100 ✗
        └── Address: NOT in whitelist ✗
        │
        ▼
Agent: "This exceeds your $100 limit and address isn't whitelisted.
       Add address and approve?"

       [Approve]  [Reject]
        │
        ▼
User: [Approve]
        │
        ▼
Agent: "Run: kamuy approve tx tx-001"
        │
        ▼
Terminal: Password: ********
        │
        ▼
Steward: Co-sign, execute
```

---

## 10. Monetization

### Gas Sponsorship Model

```
User sends $100 USDC
        │
        ▼
Gas cost: ~$0.02
        │
        ▼
Pimlico sponsors gas (Paymaster)
        │
        ▼
Kamuy fee: X% of gas (~$0.01)
        │
        ▼
User pays: $0
Kamuy earns: $0.01
```

**Key principle:** Fee is percentage of GAS, not transaction value. Large and small transactions have similar fees.

### Revenue Scaling

| Daily Transactions | Avg Gas | Fee (50%) | Monthly Revenue |
|-------------------|---------|-----------|-----------------|
| 100 | $0.02 | $1/day | $30 |
| 1,000 | $0.02 | $10/day | $300 |
| 10,000 | $0.02 | $100/day | $3,000 |
| 100,000 | $0.02 | $1,000/day | $30,000 |

---

## 11. Technical Specification

### 11.1 Technology Stack

| Component | Technology |
|-----------|------------|
| MPC Core | Rust + CGGMP24 |
| Steward Service | Rust + Axum |
| CLI | Rust |
| Database | SQLite (local) |
| Smart Contracts | Solidity + Foundry |
| Distribution | OpenClaw Skill Registry |

### 11.2 Project Structure

```
kamuy-wallet/
├── crates/
│   ├── mpc-core/           # MPC implementation
│   ├── steward/            # Steward service
│   └── cli/                # CLI binary
├── contracts-eth/          # Smart contracts
├── skill/                  # OpenClaw skill definition
│   ├── skill.yaml
│   └── install.sh
└── docs/
    └── superpowers/
        └── specs/
            └── 2026-03-20-simplified-ux-design.md
```

### 11.3 API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/transactions` | POST | Submit transaction |
| `/api/v1/transactions/:id` | GET | Check status |
| `/api/v1/policy` | GET | Get current policy |
| `/api/v1/policy/request` | POST | Request policy change |
| `/api/v1/whitelist` | POST | Add address to whitelist |
| `/health` | GET | Service health |

### 11.4 Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `STEWARD_API_KEY` | Yes | - | Agent authentication |
| `STEWARD_DATABASE_URL` | No | `sqlite://./steward.db` | Database URL |
| `STEWARD_API_PORT` | No | 8080 | API port |
| `STEWARD_PIMLICO_API_KEY` | Yes | - | Pimlico API key |

---

## 12. EVM Chains Supported

| Chain | Chain ID | Status |
|-------|----------|--------|
| **Base** | 8453 | Primary (recommended) |
| **Base Sepolia** | 84532 | Testnet |
| **Polygon** | 137 | Supported |
| **Arbitrum** | 42161 | Supported |

**Note:** Only USDC spending is supported. Wallet can receive any token, but agent can only spend USDC.

---

## 13. Security Model

### 13.1 Password Protection

| Operation | Password Required | Location |
|-----------|-------------------|----------|
| Unlock wallet | Yes | Terminal |
| Policy change | Yes | Terminal |
| Add address (over threshold) | Yes | Terminal |
| Transaction approval | No | Telegram |
| Check balance | No | Telegram |

### 13.2 Key Security

| Key | Storage | Access |
|-----|---------|--------|
| Agent Key (#1) | Steward DB (encrypted) | Steward only |
| Steward Key (#2) | Memory (while unlocked) | Steward only |
| User Key (#3) | Backup file (encrypted) | User only |

### 13.3 Threat Model

| Threat | Mitigation |
|--------|------------|
| Agent compromised | Spending limits + whitelist cap damage |
| Password in Telegram | Password prompts ONLY in terminal |
| Device lost | Encrypted backup in email |
| Steward compromised | User Key required for recovery |

---

## 14. Roadmap

### Phase 1: Core Refactor (Weeks 1-2)
- [ ] Remove separate Telegram bot requirement
- [ ] Implement inline Telegram approval in agent's chat
- [ ] Terminal-only password flow
- [ ] Auto-add threshold logic

### Phase 2: Policy System (Week 3)
- [ ] Conversational policy setup
- [ ] Policy schema v2.0
- [ ] Whitelist management
- [ ] Spending limit alerts

### Phase 3: OpenClaw Integration (Week 4)
- [ ] Skill definition and installer
- [ ] Bundle CLI + Steward binaries
- [ ] Test installation flow
- [ ] Documentation

### Phase 4: Polish & Launch (Week 5)
- [ ] Email backup integration
- [ ] Error handling
- [ ] User documentation
- [ ] Beta testing

### Future (Post-v2.0)
- [ ] Claude Code skill
- [ ] Additional agent platforms
- [ ] Multi-chain support

---

## 15. Glossary

| Term | Definition |
|------|------------|
| **Agent** | AI agent that executes tasks and initiates transactions |
| **Steward** | Policy engine that validates and co-signs transactions |
| **User** | Wallet owner with override and recovery authority |
| **Skill** | Plugin that extends an agent's capabilities |
| **Whitelist** | Approved addresses the agent can send funds to |
| **Auto-add threshold** | Amount under which new addresses auto-add |
| **Gasless** | Transaction where gas is sponsored (user pays no ETH) |

---

## Appendix: Design Spec

For the complete UX design specification, see:
- `docs/superpowers/specs/2026-03-20-simplified-ux-design.md`

---

**Document Status:** Approved Design
**Last Updated:** 2026-03-20
**Author:** Kamuy Wallet Team