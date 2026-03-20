# Kamuy Wallet

> Gasless USDC wallet for AI agents. Give your agent spending power in 30 seconds, control it forever.

[![Version](https://img.shields.io/badge/version-2.0.0-blue.svg)](https://github.com/kamuy/kamuy-wallet)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Why This Exists

AI agents need to spend money to be useful - paying for APIs, services, and resources. But giving an agent direct access to your wallet is dangerous. Kamuy Wallet solves this with:

- **Spending limits** - Cap how much your agent can spend
- **Whitelist controls** - Only send to approved addresses
- **Approval workflows** - Higher-security operations require your confirmation
- **Gasless transactions** - No ETH needed, sponsored via Pimlico

The wallet uses MPC (multi-party computation) to split the private key into three shares. No single party ever has the complete key, making it non-custodial and secure by design.

---

## Quick Start

### Prerequisites

- Linux or macOS
- OpenClaw agent installed
- Email address for backup delivery

### Installation

```bash
# Install the skill via OpenClaw
openclaw skill install kamuy-wallet

# Initialize your wallet
kamuy init --email your@email.com

# Enter and confirm your password when prompted
# Your backup will be sent to your email
```

That's it. Your agent can now make USDC payments within your defined policy limits.

### First Payment

Tell your agent:
> "Pay OpenAI $47 for API credits"

If the address is whitelisted and under your limits, the payment executes immediately. No approval needed.

---

## Installation

### Via OpenClaw (Recommended)

```bash
openclaw skill install kamuy-wallet
```

This installs:
- `kamuy-cli` - Command-line interface
- `kamuy-steward` - Policy engine (runs in background)
- Skill handlers for natural language processing

### From Source

```bash
git clone https://github.com/kamuy/kamuy-wallet
cd kamuy-wallet
cargo build --release

# Install binaries
sudo cp target/release/kamuy-cli /usr/local/bin/kamuy
sudo cp target/release/kamuy-steward /usr/local/bin/kamuy-steward
```

### System Requirements

| Requirement | Minimum | Recommended |
|-------------|---------|-------------|
| OS | Linux, macOS | Linux |
| RAM | 512 MB | 1 GB |
| Disk | 100 MB | 500 MB |
| Rust | 1.70+ | Latest stable |

---

## CLI Command Reference

### kamuy init

Initialize a new wallet. Creates key shares and sends backup to your email.

```bash
kamuy init --email <email>

# Example
kamuy init --email user@example.com
```

**Prompts:**
1. Password (entered twice for confirmation)
2. Optional: Telegram setup for mobile approvals

**Output:**
- Wallet address (e.g., `0xABC...1234`)
- Backup file sent to email
- Steward service started and unlocked

---

### kamuy unlock

Unlock the wallet after a restart. Required before the agent can make payments.

```bash
kamuy unlock

# Prompts for password
```

**What it does:**
- Decrypts the Steward key share
- Loads policy into memory
- Enables transaction signing

**When to use:**
- After system restart
- After `kamuy lock`
- When you see "Wallet locked" errors

---

### kamuy status

Check wallet status and health.

```bash
kamuy status
```

**Output:**
```
Wallet: 0xABC...1234
Status: Unlocked
Token: USDC
Gasless: Enabled

Policy:
  Max per tx: $100.00
  Max daily: $500.00
  Max weekly: $2,000.00
  Auto-add threshold: $50.00

Spending:
  Today: $47.00 / $500.00
  This week: $247.00 / $2,000.00

Whitelist: 3 addresses
```

---

### kamuy approve

Approve pending operations. Three types of approvals exist:

#### Approve Policy Changes

```bash
kamuy approve policy <id>

# Example
kamuy approve policy pol-abc123
# Prompts for password
```

**When required:**
- Increasing spending limits
- Changing auto-add threshold
- Any policy modification

**Security:** Password always required. Policy changes cannot be auto-approved.

#### Approve New Addresses

```bash
kamuy approve address <id>

# Example
kamuy approve address addr-xyz789
# Prompts for password
```

**When required:**
- Adding address to whitelist
- Payment to new address over auto-add threshold

#### Approve Transactions

```bash
kamuy approve tx <id>

# Example
kamuy approve tx tx-def456
# No password needed if within policy
```

**When required:**
- Transaction exceeds per-tx limit
- Daily/weekly limit would be exceeded
- Address not in whitelist and over threshold

---

### kamuy policy

View or update spending policy.

```bash
# View current policy
kamuy policy

# View specific field
kamuy policy get max_daily

# Update policy (requires approval)
kamuy policy set max_daily 1000.00
```

**Policy Fields:**

| Field | Description | Default |
|-------|-------------|---------|
| `max_per_tx` | Maximum per transaction | $100.00 |
| `max_daily` | Maximum per day | $500.00 |
| `max_weekly` | Maximum per week | $2,000.00 |
| `auto_add_threshold` | Auto-whitelist threshold | $50.00 |

**Note:** Policy changes always require terminal password confirmation. The agent cannot change policies without your explicit approval.

---

### kamuy pending

View pending approvals waiting for your action.

```bash
kamuy pending
```

**Output:**
```
Pending Approvals (2):

1. [tx-abc123] $200.00 to 0x1234...5678
   Reason: Amount exceeds $100 per-tx limit
   Status: Awaiting approval

2. [pol-def456] Policy change: max_daily -> $1,000.00
   Reason: User requested increase
   Status: Awaiting password

Run 'kamuy approve tx <id>' or 'kamuy approve policy <id>'
```

---

### kamuy history

View transaction history.

```bash
# Recent transactions
kamuy history

# Last N transactions
kamuy history --limit 20

# Filter by status
kamuy history --status confirmed
kamuy history --status pending

# Export to file
kamuy history --output transactions.json
```

**Output:**
```
Transaction History (15 total):

[CONFIRMED] $47.00 USDC to 0x1234...5678 (OpenAI)
  Date: 2026-03-20 14:32:00 UTC
  Tx: 0xabcd...1234

[CONFIRMED] $23.00 USDC to 0x5678...9abc (AWS)
  Date: 2026-03-20 12:15:00 UTC
  Tx: 0xef01...5678

[PENDING] $150.00 USDC to 0x9abc...def0
  Date: 2026-03-20 15:45:00 UTC
  Status: Awaiting approval
```

---

### kamuy lock

Lock the wallet. Prevents all transactions until unlocked.

```bash
kamuy lock
```

**When to use:**
- Leaving computer unattended
- Suspected security issue
- Before system maintenance

---

### kamuy recover

Restore wallet from backup file.

```bash
kamuy recover <backup-file>

# Example
kamuy recover backup-2026-03-20.enc
# Prompts for password used during init
```

**When to use:**
- Lost device
- Corrupted local storage
- Setting up on new machine

---

## Natural Language Examples

### Making Payments

| You Say | What Happens |
|---------|--------------|
| "Pay OpenAI $47 for API credits" | Sends $47 to OpenAI's whitelisted address |
| "Send 100 USDC to 0x1234567890abcdef..." | Sends to specific address (if whitelisted or under threshold) |
| "Spend $50 on AWS" | Pays AWS if address is known |
| "Pay $25 to 0xabcd... for services" | Sends with reason attached |

### Checking Balance

| You Say | Response |
|---------|----------|
| "What's my wallet balance?" | Shows USDC balance across chains |
| "How much do I have?" | Same as balance |
| "What's my USDC balance on Base?" | Chain-specific balance |

### Managing Policy

| You Say | What Happens |
|---------|--------------|
| "What's my spending limit?" | Shows current policy limits |
| "Show my policy" | Full policy display |
| "Increase daily limit to $1000" | Requests policy change (requires terminal approval) |
| "What's my auto-add threshold?" | Shows threshold for auto-whitelisting |

### Viewing History

| You Say | Response |
|---------|----------|
| "Show recent transactions" | Last 10 transactions |
| "What did I spend today?" | Today's spending summary |
| "Transaction history" | Paginated history |

### Whitelist Management

| You Say | Response |
|---------|----------|
| "Who's in my whitelist?" | Lists all whitelisted addresses |
| "Show trusted addresses" | Same as whitelist |
| "What addresses can I pay?" | Whitelist with labels |

### Pending Approvals

| You Say | Response |
|---------|----------|
| "What's pending?" | Lists pending transactions/approvals |
| "Show pending approvals" | Detailed pending list |
| "What needs my attention?" | Pending items summary |

---

## Security Model

### The Three Key Shares

Kamuy uses a 2-of-3 threshold signature scheme (MPC). The private key is split into three shares:

```
+------------------+     +------------------+     +------------------+
|   AGENT KEY      |     |   STEWARD KEY    |     |    USER KEY      |
|    (Share #1)    |     |    (Share #2)    |     |    (Share #3)    |
+------------------+     +------------------+     +------------------+
|                  |     |                  |     |                  |
| Held by AI agent |     | Held by Steward  |     | Held by user     |
| for initiating   |     | service for      |     | (backup file)    |
| transactions     |     | policy checks    |     | for recovery     |
|                  |     | & co-signing     |     |                  |
+------------------+     +------------------+     +------------------+
```

**How signing works:**
1. Agent initiates transaction with Share #1
2. Steward validates policy and signs with Share #2
3. Both signatures combined = valid transaction
4. Share #3 only needed for recovery

**Security guarantee:** No single party can sign alone. Agent cannot bypass policy. Steward cannot initiate transactions. User has ultimate recovery control.

---

### Approval Levels

Kamuy uses three approval channels based on security requirements:

| Level | Channel | When Used | Security |
|-------|---------|-----------|----------|
| **AutoApprove** | Automatic | Within limits + whitelisted address | No interaction needed |
| **TelegramButton** | Telegram chat | Over limits or new address under threshold | One-click in existing chat |
| **TerminalPassword** | Terminal | Policy changes or new address over threshold | Password required |

**The "higher-security-wins" principle:** When multiple rules apply, the most secure approval method is used.

### Security Matrix

| Operation | Within Limits | Over Limits | New Address |
|-----------|--------------|-------------|-------------|
| Payment (under threshold) | Auto | Telegram | Auto if under threshold |
| Payment (over threshold) | Auto | Telegram | Telegram or Terminal |
| Policy change | Terminal | Terminal | N/A |
| Add address (under threshold) | N/A | N/A | Telegram |
| Add address (over threshold) | N/A | N/A | Terminal |
| Wallet unlock | Terminal | Terminal | Terminal |

### Why Terminal for Passwords?

Passwords are NEVER sent through Telegram. Here's why:

1. **Telegram is not E2EE by default** - Messages can be read by Telegram
2. **Chat history persists** - Passwords would remain in history
3. **Multiple device access** - Others with chat access could see passwords
4. **Agent memory** - The AI agent sees chat contents

**Our solution:** When a sensitive operation is needed, the agent tells you to run a terminal command. You enter your password locally, where it never touches the network in plaintext.

---

### Threat Mitigation

| Threat | Mitigation |
|--------|------------|
| **Agent compromised** | Spending limits cap damage; whitelist restricts destinations |
| **Password leaked** | Terminal-only entry; password never in Telegram |
| **Device stolen** | Wallet encrypted; password required to unlock |
| **Steward compromised** | Agent key still required; user key for recovery |
| **Malicious transaction** | Policy validation before co-signing |
| **Key extraction** | Keys never exist in full; shares distributed |

---

## Troubleshooting

### Common Issues

#### "Wallet locked" Error

**Symptom:** Agent reports wallet is locked when trying to pay.

**Solution:**
```bash
kamuy unlock
# Enter your password
```

**Why it happens:** Wallet locks after system restart or explicit lock.

---

#### "Steward not running" Error

**Symptom:** CLI commands fail to connect to Steward.

**Solution:**
```bash
# Check if Steward is running
ps aux | grep kamuy-steward

# If not, start it
kamuy-steward &

# Or unlock (which starts Steward if needed)
kamuy unlock
```

---

#### "Password incorrect" Error

**Symptom:** Unlock or approval fails with password error.

**Solutions:**
1. Check caps lock and keyboard layout
2. Try the password in a text editor to verify
3. If truly forgotten, use backup recovery:
   ```bash
   kamuy recover backup.enc
   ```

---

#### Transaction Stuck in "Pending"

**Symptom:** Transaction shows pending but never completes.

**Solution:**
```bash
# Check pending transactions
kamuy pending

# If approval needed, approve it
kamuy approve tx <tx-id>

# If stuck without reason, check Steward logs
tail -f ~/.kamuy/steward.log
```

---

#### "Address not whitelisted" Error

**Symptom:** Payment rejected because address is not in whitelist.

**Solution:**

1. **If under auto-add threshold:**
   - Agent should ask "Add address and pay?"
   - Click "Add & Pay" in Telegram

2. **If over auto-add threshold:**
   - Agent provides terminal command
   - Run `kamuy approve address <id>`
   - Enter password

3. **Manually add address:**
   ```bash
   kamuy whitelist add 0x... --label "Service Name"
   ```

---

#### Daily/Weekly Limit Reached

**Symptom:** "Daily spending limit reached" or similar.

**Solutions:**

1. **Wait until reset** - Limits reset at midnight UTC

2. **Increase limit temporarily:**
   ```bash
   kamuy policy set max_daily 1000.00
   kamuy approve policy <id>
   ```

3. **Check remaining budget:**
   ```bash
   kamuy status
   ```

---

#### Telegram Approvals Not Working

**Symptom:** Approval buttons don't appear or don't work.

**Solutions:**

1. **Verify Telegram setup:**
   ```bash
   kamuy config get telegram_enabled
   ```

2. **Restart bot:**
   ```bash
   kamuy config set telegram_enabled true
   kamuy unlock  # Reloads config
   ```

3. **Use terminal instead:**
   ```bash
   kamuy pending
   kamuy approve tx <id>
   ```

---

### Getting Help

1. **Check logs:**
   ```bash
   tail -f ~/.kamuy/steward.log
   tail -f ~/.kamuy/cli.log
   ```

2. **Verify setup:**
   ```bash
   kamuy status
   kamuy policy
   ```

3. **Report issues:**
   - GitHub Issues: https://github.com/kamuy/kamuy-wallet/issues
   - Include: `kamuy status` output and relevant logs

---

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `KAMUY_DATA_DIR` | `~/.kamuy` | Data directory location |
| `KAMUY_STEWARD_URL` | `http://localhost:8080` | Steward API URL |
| `KAMUY_LOG_LEVEL` | `info` | Log verbosity (debug, info, warn, error) |

### Config File

Location: `~/.kamuy/config.json`

```json
{
  "version": "2.0",
  "steward_url": "http://localhost:8080",
  "telegram_enabled": true,
  "default_chain": 8453,
  "backup_email": "user@example.com"
}
```

---

## Supported Chains

| Chain | Chain ID | Status |
|-------|----------|--------|
| Base | 8453 | Primary (recommended) |
| Base Sepolia | 84532 | Testnet |
| Polygon | 137 | Supported |
| Arbitrum | 42161 | Supported |
| Optimism | 10 | Supported |

**Note:** Only USDC spending is supported. Wallet can receive any token, but the agent can only spend USDC.

---

## API Reference

For programmatic access, see the [API Reference](./api-reference.md).

### Quick Example

```bash
curl -X POST http://localhost:8080/api/v1/transactions \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-api-key" \
  -d '{
    "to": "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb",
    "value": "47000000",
    "token": "USDC",
    "chain_id": 8453
  }'
```

---

## Contributing

See [CONTRIBUTING.md](../../../CONTRIBUTING.md) for development setup and contribution guidelines.

---

## License

MIT OR Apache-2.0