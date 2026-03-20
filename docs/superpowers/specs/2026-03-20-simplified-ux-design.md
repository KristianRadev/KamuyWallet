# Kamuy Wallet v2.0 — Simplified User Experience Design

**Version:** 2.0
**Date:** 2026-03-20
**Status:** Approved Design
**Author:** Product Team

---

## Executive Summary

This document defines the redesigned Kamuy Wallet experience focused on **one-command setup** and **minimal user friction**. The wallet becomes an invisible plugin that users install into their existing AI agent setup, not a separate product they manage.

### Key Changes from v1.x

| Aspect | v1.x (Old) | v2.0 (New) |
|--------|------------|------------|
| **Setup** | Multiple env vars, separate Telegram bot | One terminal command |
| **Approval channel** | Separate Telegram bot | Inline in agent's existing chat |
| **Password entry** | In Telegram (insecure) | Terminal only (hidden input) |
| **Policy setup** | JSON file or API | Conversational with agent |
| **Token support** | USDC, USDT, DAI | USDC only (gasless) |
| **Distribution** | Build from source | OpenClaw skill installer |
| **Whitelist** | Pre-populated or manual | Empty by default, grows with use |

---

## Target User

**Primary:** End users who want to give their AI agent spending power (Segment C)
**Secondary:** AI agent operators (Segment B)
**Tertiary:** Developers building AI agents (Segment A)

---

## Architecture

### System Overview

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

1. **Local-first**: Everything runs on user's machine. No hosted servers required.
2. **Plugin model**: Kamuy is installed as a skill into existing agent platforms (OpenClaw).
3. **Single chat**: All communication happens in the user's existing agent chat. No separate bot.
4. **Terminal for sensitive operations**: Password never enters Telegram.

---

## User Journey

### Phase 1: One-Time Setup (Terminal)

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

**What happens internally:**
1. Generates 3 key shares (Agent, Steward, User)
2. Encrypts all shares with user's password (Argon2 KDF)
3. Sends encrypted backup to provided email
4. Starts Steward in background (unlocked state)
5. Registers Kamuy skill with OpenClaw agent

**Security guarantees:**
- Password never leaves terminal (hidden input)
- Agent never sees password
- Keys encrypted before backup

---

### Phase 2: Policy Setup (Telegram Conversation)

The user has a natural conversation with their agent to set spending limits:

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

**Policy summary shown to user:**
- Clear explanations for each setting
- Explicit statement that agent can ONLY send to whitelisted addresses
- Auto-add threshold explained with example

---

### Phase 3: Daily Use — Auto-Approved Transactions

Transactions within policy happen automatically. User isn't interrupted:

```
Agent: "Purchased $47 in OpenAI API credits. ✓"
```

**No approval needed because:**
- Amount ($47) < max_per_tx ($100)
- Address (OpenAI) already in whitelist
- Within daily limit

---

### Phase 4: Daily Use — New Address (Under Threshold)

```
Agent: "I need to pay OpenAI $23 for API credits.
       Address: 0x1234567890abcdef1234567890abcdef12345678 isn't whitelisted.

       This is under your $50 auto-add threshold.
       Add address and pay?"

       [Add & Pay]  [Cancel]

User: [Add & Pay]

Agent: "✓ Done. OpenAI added to whitelist, $23 sent."
```

**Flow:**
- User clicks button in Telegram
- Address added to whitelist (labeled "OpenAI")
- Transaction executes
- No terminal interaction needed

---

### Phase 5: Daily Use — New Address (Over Threshold)

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

**Flow:**
- Agent recognizes threshold exceeded
- Asks for terminal confirmation
- User enters password in terminal (secure)
- Address added, transaction sent

---

### Phase 6: Daily Use — Transaction Over Limit

```
Agent: "This $150 purchase exceeds your $100 per-transaction limit.
       Approve anyway?"

       [Approve]  [Reject]

User: [Approve]

Agent: "✓ Approved. $150 sent."
```

**Flow:**
- Single transaction override
- No password needed for moderate overages
- Policy updated temporarily for this transaction only

---

### Phase 7: Daily Limit Alert

When daily limit is reached, user gets notified:

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

**Key features:**
- Full addresses shown (not truncated) for verification
- Clear breakdown of spending
- Simple instruction to increase limit

---

### Phase 8: Policy Change Request

```
User: "Increase daily limit to $1000"

Agent: "I'll request to increase your daily limit from $500 to $1,000.

       Reason: User requested increase

       To confirm, run in terminal:
       kamuy approve policy change-003"
```

```
$ kamuy approve policy change-003
Password: ********
✓ Policy updated. New daily limit: $1,000.
```

---

### Phase 9: Wallet Unlock (After Restart)

If Steward restarts (computer reboot, service restart):

```
$ kamuy unlock
Password: ********
✓ Wallet unlocked. Agent can now spend.
```

**Alternative notification in Telegram:**

```
Agent: "I need to make a payment but your wallet is locked.
       Run 'kamuy unlock' on your device to continue."
```

---

### Phase 10: Recovery

If user loses their device:

1. Open email → Download encrypted backup file
2. Install Kamuy on new device
3. Run recovery command

```
$ kamuy recover backup.enc
Password: ********
✓ Wallet restored from backup.
```

---

## Policy System

### Policy Schema (v2.0)

```json
{
  "version": "2.0",
  "max_per_tx": "100.00",
  "max_daily": "500.00",
  "max_weekly": "2000.00",
  "auto_add_threshold": "50.00",
  "token": "USDC",
  "gasless": true,
  "whitelist": {},
  "spending_tracker": {
    "daily_spent": "0.00",
    "weekly_spent": "0.00",
    "last_reset_daily": "2026-03-20T00:00:00Z",
    "last_reset_weekly": "2026-03-16T00:00:00Z"
  }
}
```

### Whitelist Structure

```json
{
  "whitelist": {
    "0x1234567890abcdef1234567890abcdef12345678": {
      "label": "OpenAI",
      "added_at": "2026-03-20T10:30:00Z",
      "total_sent": "47.00"
    },
    "0x567890abcdef1234567890abcdef1234567890ab": {
      "label": "AWS",
      "added_at": "2026-03-20T11:00:00Z",
      "total_sent": "200.00"
    }
  }
}
```

### Policy Rules

| Rule | Description | Enforced By |
|------|-------------|-------------|
| `max_per_tx` | Maximum single transaction amount | Steward (rejects if exceeded) |
| `max_daily` | Maximum total spending per day | Steward (blocks after limit) |
| `max_weekly` | Maximum total spending per week | Steward (blocks after limit) |
| `auto_add_threshold` | Amount under which new addresses auto-add | Steward (prompts in Telegram) |
| `token` | Only USDC can be spent | Steward (hardcoded, rejects others) |
| `whitelist` | Only whitelisted addresses receive funds | Steward (rejects non-whitelisted) |

### Edge Case: Multiple Policy Violations

When a transaction triggers multiple policy violations (e.g., both "new address over threshold" AND "per-transaction limit exceeded"), the **higher-security requirement wins**:

```
Transaction: $200 to new address
    ├── Amount ($200) > max_per_tx ($100) → Requires approval
    └── Address not whitelisted + $200 > auto_add_threshold ($50) → Requires password

Resolution: Require terminal password confirmation
Rationale: Higher security takes precedence
```

**Priority order (highest to lowest security):**
1. Terminal password required
2. Telegram button approval
3. Auto-approved (within all limits)

### Policy Change Flow

```
User request in Telegram
        ↓
Agent creates policy change request
        ↓
Request stored in database (pending_approval)
        ↓
User runs: kamuy approve policy <id>
        ↓
Terminal: Password prompt
        ↓
Password verified → Policy updated
        ↓
Agent notified → Continues operation
```

---

## Security Model

### Password Security

| Operation | Password Required? | Location |
|-----------|-------------------|----------|
| Unlock wallet | Yes | Terminal |
| Approve policy change | Yes | Terminal |
| Approve new address (over threshold) | Yes | Terminal |
| Approve transaction | No | Telegram (button) |
| Reject transaction | No | Telegram (button) |
| Check balance | No | Telegram |
| View history | No | Telegram |

### Key Separation

| Key | Stored Where | Who Has Access |
|-----|--------------|----------------|
| Agent Key (#1) | Steward database (encrypted) | Steward service only |
| Steward Key (#2) | Steward memory | Steward service only |
| User Key (#3) | User's backup file (encrypted) | User only |

### Threat Mitigations

| Threat | Mitigation |
|--------|------------|
| Agent compromised | Spending limits cap damage; whitelist prevents arbitrary sends |
| Password entered in Telegram | Password prompts ONLY in terminal |
| Agent sees user's password | Password never sent to agent's chat |
| Device lost | Encrypted backup in email; password required to restore |
| Steward memory dump | Keys in memory only while unlocked |

---

## Monetization

### Gas Sponsorship Model

- All transactions are gasless (user pays no ETH)
- Pimlico sponsors gas via Paymaster
- Fee collected: X% of gas costs (not transaction value)

### Revenue Flow

```
User sends $100 USDC
    ↓
Transaction uses ~$0.02 gas
    ↓
Pimlico pays gas (sponsored)
    ↓
Paymaster deducts fee (e.g., $0.01)
    ↓
User pays $0, receives full $100
    ↓
Kamuy earns $0.01
```

---

## Distribution

### Installation Method

```bash
# OpenClaw skill installation
$ openclaw skill install kamuy-wallet
```

**What gets installed:**
- `kamuy` CLI binary
- Steward binary (runs in background)
- OpenClaw skill definition

### Skill Definition

```yaml
# kamuy-wallet-skill.yaml
name: kamuy-wallet
version: 2.0.0
description: Gasless USDC wallet for AI agents
commands:
  - trigger: "pay|send|spend|buy|purchase"
    handler: kamuy_skill::handle_payment
  - trigger: "wallet|balance|policy"
    handler: kamuy_skill::handle_query
hooks:
  - on_install: kamuy init
  - on_start: kamuy unlock
```

---

## Migration from v1.x

### Breaking Changes

1. **Telegram bot removed** — All communication via agent's existing bot
2. **Token support narrowed** — USDC only (no USDT/DAI spending)
3. **Password entry location** — Terminal only, never in Telegram
4. **Installation method** — Skill installer, not manual build

### Data Migration

- Existing wallets: Compatible (same MPC keys)
- Existing policies: Auto-migrate to new schema
- Transaction history: Preserved

---

## Implementation Priorities

### Phase 1: Core Infrastructure
1. Refactor Steward to support inline Telegram (no separate bot)
2. Implement terminal-only password flow
3. Add auto-add threshold logic
4. Update policy schema to v2.0

### Phase 2: OpenClaw Integration
1. Create skill definition
2. Bundle CLI + Steward binaries
3. Test installation flow
4. Write skill documentation

### Phase 3: User Experience
1. Conversational policy setup
2. Spending limit alerts
3. Whitelist management UI
4. Recovery flow

---

## Open Questions

| Question | Status | Decision Date |
|----------|--------|---------------|
| Exact auto-add threshold default | TBD | Before beta |
| Fee percentage on gas | TBD | Before launch |
| Support for multiple chains at launch | TBD | Before beta |
| Email provider for backups | TBD | Before launch |

---

**Document Status:** Approved
**Last Updated:** 2026-03-20