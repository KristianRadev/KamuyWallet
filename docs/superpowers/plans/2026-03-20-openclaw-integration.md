# Phase 2: OpenClaw Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Create OpenClaw skill package with CLI integration for one-command setup.

**Architecture:**
- CLI binary (`kamuy`) provides terminal commands: init, unlock, approve, status
- Steward binary runs as background service
- OpenClaw skill definition registers handlers for payment and wallet queries
- Installation bundles both binaries with skill metadata

**Tech Stack:** Rust (CLI), Python (skill handlers), YAML (skill definition)

**Spec Document:** `/home/santo6500/kamuy-wallet/docs/superpowers/specs/2026-03-20-simplified-ux-design.md`

---

## Current State

### CLI Commands (Already Exist in crates/cli)
- `create-wallet` - Generate MPC keys
- `unlock` - Load Steward key with password
- `lock` - Unload Steward key
- `sign` - Sign transaction
- `policy` - View/update policy
- `status` - Check wallet status
- `approve` - Approve pending items
- `pending` - View pending approvals
- `history` - Transaction history
- `rotate` - Rotate agent key
- `recover` - Recover wallet
- `config` - Configure CLI
- `completions` - Shell completions

### Steward API (Already Exists)
- REST API on port 8080
- `/api/v1/policy` - v2.0 format
- `/api/v1/transactions` - Submit/query transactions
- `/api/v1/admin/signing-keys` - Load keys for testing

---

## Task 1: Update CLI init Command for v2.0

**Files:**
- Modify: `crates/cli/src/commands/create_wallet.rs` (or add new init command)
- Modify: `crates/cli/src/main.rs`

**Requirements:**
- Add `init` subcommand (or alias create-wallet to init)
- Support email for backup: `kamuy init --email user@example.com`
- Generate wallet with 3 key shares
- Send encrypted backup to email (via Steward API)
- Start Steward in unlocked state
- Display wallet address

**Implementation:**
1. Check if `init` command exists, if not add it
2. Update to work with v2.0 Steward API
3. Add email backup flow

---

## Task 2: Update CLI approve Command for v2.0

**Files:**
- Modify: `crates/cli/src/commands/approve.rs`

**Requirements:**
- Support: `kamuy approve policy <id>` - Terminal password for policy changes
- Support: `kamuy approve address <id>` - Terminal password for new addresses over threshold
- Support: `kamuy approve tx <id>` - Telegram button style (optional terminal override)
- Always prompt for password in terminal (hidden input)

---

## Task 3: Create OpenClaw Skill Package Structure

**Files:**
- Create: `skills/kamuy-wallet/` directory
- Create: `skills/kamuy-wallet/skill.yaml`
- Create: `skills/kamuy-wallet/skill.md`
- Create: `skills/kamuy-wallet/handlers/payment.py`
- Create: `skills/kamuy-wallet/handlers/query.py`
- Create: `skills/kamuy-wallet/handlers/__init__.py`

**Skill Definition (skill.yaml):**
```yaml
name: kamuy-wallet
version: 2.0.0
description: Gasless USDC wallet for AI agents
author: Kamuy
commands:
  - trigger: "pay|send|spend|buy|purchase"
    handler: handlers.payment.handle_payment
  - trigger: "wallet|balance|policy"
    handler: handlers.query.handle_query
hooks:
  on_install: "kamuy init"
  on_start: "kamuy unlock"
dependencies:
  - kamuy-cli
  - kamuy-steward
```

---

## Task 4: Implement Payment Handler

**Files:**
- Create: `skills/kamuy-wallet/handlers/payment.py`

**Requirements:**
- Parse natural language payment requests
- Extract: recipient, amount, token (default USDC)
- Call Steward API to submit transaction
- Return response with approval status

**Example:**
```python
async def handle_payment(request):
    # "Pay OpenAI $47 for API credits"
    # Extract: to=lookup("OpenAI"), amount=47, reason="API credits"

    # Submit to Steward
    response = await steward_api.submit_transaction(
        to=recipient_address,
        amount=usdc_micros,
        token="USDC",
        chain_id=chain
    )

    if response["status"] == "awaiting_approval":
        return f"Transaction queued. Approval required: {response['reason']}"
    return f"Transaction sent: {response['tx_hash']}"
```

---

## Task 5: Implement Query Handler

**Files:**
- Create: `skills/kamuy-wallet/handlers/query.py`

**Requirements:**
- Handle balance queries: "What's my wallet balance?"
- Handle policy queries: "What's my spending limit?"
- Handle history queries: "Show recent transactions"
- Handle whitelist queries: "Who's in my whitelist?"

---

## Task 6: Create Installation Script

**Files:**
- Create: `scripts/install.sh`
- Create: `scripts/build-release.sh`

**Requirements:**
- Build CLI and Steward binaries
- Package into skill directory
- Create installation manifest
- Support: `openclaw skill install ./kamuy-wallet`

---

## Task 7: Write Skill Documentation

**Files:**
- Create: `skills/kamuy-wallet/README.md`
- Create: `skills/kamuy-wallet/skill.md`

**Requirements:**
- Installation instructions
- CLI command reference
- Natural language examples
- Security model explanation
- Troubleshooting guide

---

## Task 8: Integration Testing

**Files:**
- Create: `tests/integration/skill_test.py`

**Requirements:**
- Test skill installation
- Test payment handler
- Test query handler
- Test full flow: init -> unlock -> pay -> approve

---

## Summary

Phase 2 delivers:

1. **Updated CLI** - v2.0 compatible init, approve commands
2. **OpenClaw Skill** - YAML definition, Python handlers
3. **Installation Package** - Bundled binaries with skill metadata
4. **Documentation** - User-facing guides
5. **Integration Tests** - End-to-end verification