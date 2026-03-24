# Kamuy Wallet Security Model

**Version:** 2.0
**Last Updated:** March 2026
**Status:** Production Ready

---

## Table of Contents

1. [Security Model Overview](#1-security-model-overview)
2. [The Agent Problem](#2-the-agent-problem)
3. [Why Browser Flows and External Devices Work](#3-why-browser-flows-and-external-devices-work)
4. [The Native OS Dialog Solution](#4-the-native-os-dialog-solution)
5. [Threat Model](#5-threat-model)
6. [Responsible Disclosure](#6-responsible-disclosure)

---

## 1. Security Model Overview

Kamuy Wallet implements a **2-of-3 Multi-Party Computation (MPC) threshold signature scheme** based on the CGGMP24 protocol. This architecture ensures that no single party ever possesses the complete private key, and at least two parties must collaborate to produce a valid signature.

### Key Share Distribution

| Share | Holder | Location | Purpose |
|-------|--------|----------|---------|
| **Agent Key (#1)** | AI Agent | Steward database (encrypted) | Initiates transactions, participates in signing |
| **Steward Key (#2)** | Steward Service | Memory (while unlocked) | Policy enforcement, co-signs compliant transactions |
| **User Key (#3)** | End User | Encrypted backup file | Recovery, high-risk approvals, policy changes |

### Who Holds What and Why

**Agent Key (#1)**
- Held by the Steward service on behalf of the AI agent
- Stored encrypted in the Steward's SQLite database
- The AI agent (external software) requests signing operations via authenticated API calls
- Never exposed directly to the agent process

**Steward Key (#2)**
- Held by the Steward service (separate Rust process)
- Exists only in memory while the wallet is unlocked
- Used to co-sign transactions that pass policy validation
- Never written to disk in plaintext

**User Key (#3)**
- Held by the user in an encrypted backup file
- Required for wallet recovery and high-risk operations
- User must enter their password in the terminal to unlock this key
- Sent to user's email as encrypted backup during setup

### Auto-Signing Flow

When a transaction passes all policy checks:
1. Agent initiates signing request via API
2. Steward validates against policy (whitelist, limits)
3. If approved, Steward contributes its key share
4. Agent key share is retrieved and used
5. Two signatures satisfy the 2-of-3 threshold
6. No user interaction required

### Security Properties

- **Non-custodial**: Full private key never exists in one place
- **Threshold security**: Compromise of any single key is insufficient
- **Process isolation**: Steward runs as separate process from the AI agent
- **Encrypted at rest**: All key shares are encrypted using Argon2id + ChaCha20-Poly1305
- **Forward secrecy planning**: Architecture supports key refresh (planned feature)
- **Fail-closed design**: Default deny on any policy violation or authentication failure

---

## 2. The Agent Problem

### The Fundamental Challenge

AI agents present a unique security challenge: **they run with user privileges**. When you grant an AI agent access to your system, it inherits your permissions, your file access, and your authentication context.

### The Man-in-the-Middle Reality

In a CLI-based workflow, the AI agent is effectively a man-in-the-middle on user input:

```
User types command --> Agent processes --> Agent executes
```

The agent can:
- Read what you type
- Modify commands before execution
- Intercept and read program output
- Make decisions on your behalf

### Why Software-Only Rules Cannot Solve This

No software-based rule, permission system, or sandbox can fully protect against a compromised agent because:

1. **Shared privilege level**: The agent runs with your user permissions. Any file you can read, the agent can read. Any process you can spawn, the agent can spawn.

2. **Input interception**: The agent controls the terminal interface. It can see passwords, intercept confirmations, and modify inputs before they reach security software.

3. **Output manipulation**: The agent can modify or hide security warnings, confirmation dialogs, or error messages that would alert the user.

4. **Memory access on same machine**: If the agent runs on your local machine, it may be able to access memory of other processes depending on OS configuration.

5. **Environment control**: The agent can set environment variables, modify PATH, and potentially redirect API calls.

### The Trust Assumption

Kamuy Wallet's design acknowledges this reality. We assume:
- The AI agent may be compromised
- The agent has full access to the user's terminal session
- Any password entered in the terminal could be observed by the agent

This is why our security model does not rely solely on software protections within the terminal.

---

## 3. Why Browser Flows and External Devices Work

### The Separate Channel Principle

The only robust defense against a compromised agent is a **separate communication channel** that the agent cannot control or observe:

```
┌─────────────────────────────────────────────────────────────────┐
│                     COMPROMISED AGENT                           │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ Agent controls terminal, reads all input/output         │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ But CANNOT access:                                       │   │
│  │                                                          │   │
│  │   • Browser session (different process space)            │   │
│  │   • Hardware wallet (USB device, separate MCU)           │   │
│  │   • Mobile phone (different device entirely)             │   │
│  │   • OS-level secure enclave (hardware-isolated)          │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### Examples of Secure Channels

1. **Browser-based confirmation**: User is redirected to a web page where they enter a password or confirm a transaction. The browser process is isolated from the CLI agent.

2. **Hardware wallet**: User confirms on a Ledger or Trezor device. The agent cannot access the hardware's secure element.

3. **Mobile push notification**: User receives a push notification on their phone and confirms there. The phone is a completely separate device.

4. **Email/SMS verification**: A code is sent to a channel the agent cannot access.

### Why We're Not Using Them (Yet)

Despite their security benefits, we chose not to require browser flows or external devices in the initial version because:

1. **Adoption friction**: Every additional step reduces user adoption. Requiring a hardware wallet or mobile app creates a barrier to entry.

2. **Setup complexity**: Users would need to acquire, configure, and maintain separate hardware or apps.

3. **Target audience**: Early adopters of AI-agent wallets are often developers and power users who understand the tradeoffs.

4. **Risk calibration**: The combination of MPC, spending limits, and whitelist policies provides meaningful security for moderate-value use cases.

5. **Gradual security path**: Users can upgrade to hardware-backed security later without changing their wallet address.

---

## 4. The Native OS Dialog Solution

### The Future: OS-Level Process Isolation

The most promising path forward for software-only security is **native OS security dialogs**. Modern operating systems provide hardware-enforced process isolation that even root-level malware cannot easily bypass.

### How They Work

**macOS Keychain**
- Prompts are rendered by the `securityd` process
- UI runs in a separate, protected address space
- Even admin/root processes cannot read Keychain password input
- Touch ID integration provides biometric confirmation

**Windows Hello**
- Prompts are rendered by the Windows security processor
- Isolated in a Trusted Execution Environment (TEE)
- Supports biometric (face, fingerprint) and PIN authentication
- Hardware-backed protection via TPM

**Linux Polkit / systemd-ask-password**
- PolicyKit prompts run in a separate, privileged process
- `systemd-ask-password` can use graphical frontends with isolation
- Snap and Flatpak applications can be sandboxed with restricted access

### Why They're Secure

```
┌─────────────────────────────────────────────────────────────────┐
│                     SECURITY ARCHITECTURE                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────────┐                                           │
│  │  AI Agent        │   Can spawn dialog request, but...        │
│  │  (compromised?)  │                                           │
│  └────────┬─────────┘                                           │
│           │                                                      │
│           │ Request dialog                                       │
│           ▼                                                      │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    OS KERNEL                              │   │
│  │  ┌────────────────────────────────────────────────────┐  │   │
│  │  │  Secure Dialog Process                              │  │   │
│  │  │  • Runs in separate address space                   │  │   │
│  │  │  • Protected by hardware memory isolation           │  │   │
│  │  │  • Cannot be read by user-space processes           │  │   │
│  │  │  • Input goes directly to secure enclave            │  │   │
│  │  └────────────────────────────────────────────────────┘  │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  Agent CANNOT:                                                   │
│  - Read the password as it is typed                             │
│  - Modify the dialog message                                    │
│  - Intercept the result before encryption                       │
│  - Inject fake approval responses                               │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Implementation Roadmap

| Phase | Feature | Platform | Security Level |
|-------|---------|----------|----------------|
| Current | Terminal password | All | Baseline (agent can observe) |
| Phase 5 | Polkit integration | Linux | High (process isolation) |
| Phase 5 | Keychain integration | macOS | High (hardware-backed) |
| Phase 5 | Windows Hello | Windows | High (TPM-backed) |
| Future | Hardware wallet support | All | Maximum (air-gapped) |

### Why This Is Better Than Browser Flows

1. **No additional device**: Uses hardware already present in the computer
2. **Native integration**: No need to switch contexts to a browser
3. **Biometric support**: Touch ID, Windows Hello face/fingerprint
4. **Zero friction**: User experience remains seamless
5. **Same security**: Hardware-enforced isolation equivalent to separate device

---

## 5. Threat Model

### What We Protect Against

| Threat | Mitigation | Effectiveness |
|--------|------------|---------------|
| **Agent compromise** (moderate) | Spending limits cap damage; whitelist prevents transfers to attacker addresses | Partial - agent can still drain up to daily limit |
| **Steward process compromise** | Agent key encrypted; User key required for recovery | High - attacker gets only one key share |
| **Database theft** | All key shares encrypted with Argon2id | High - brute force requires substantial time |
| **Replay attacks** | Session IDs bound to message + timestamp + randomness | High |
| **Rogue key attacks** | Schnorr proofs on nonce commitments | High |
| **Malicious signing party** | Partial signature verification | High |
| **SQL injection** | Parameterized queries throughout | High |
| **Timing attacks on API** | Constant-time comparison for API keys | High |
| **Invalid shares during DKG** | Complaint mechanism with public verification | High |

### What We Do NOT Protect Against (Yet)

| Threat | Current Status | Future Mitigation |
|--------|----------------|-------------------|
| **Agent observing terminal password** | Agent can see password input | OS dialog integration (Phase 5) |
| **Agent modifying policy requests** | Agent could hide policy change warnings | OS dialog integration |
| **Full system compromise** (root/kernel) | All bets are off | Hardware wallet support |
| **Zero-day OS vulnerabilities** | Cannot defend against unknown exploits | Defense in depth, monitoring |
| **Social engineering** | User might approve malicious transaction | Better UX warnings, contextual info |
| **Key share exfiltration** (memory dump) | Keys exist in memory while unlocked | Forward secrecy via key refresh |
| **Rate limiting** | Not implemented in code | Reverse proxy or tower middleware |

### Compromise Scenarios

**Scenario 1: Malicious Agent**
- Impact: Agent can spend up to daily limit to whitelisted addresses
- Mitigation: Set conservative daily limits; use auto-add threshold
- Recovery: Revoke agent access; use User key to move funds

**Scenario 2: Steward Binary Compromised**
- Impact: Attacker gains Steward key share (1 of 3)
- Mitigation: Cannot sign alone; needs another key share
- Recovery: Use User key to regenerate wallet with new shares

**Scenario 3: Backup File Leaked**
- Impact: Attacker has encrypted User key share
- Mitigation: Argon2id makes brute force expensive
- Recovery: Strong password makes attack infeasible

**Scenario 4: Password Compromised**
- Impact: Attacker can decrypt User key share
- Mitigation: Still need access to encrypted backup file
- Recovery: Immediately move funds to new wallet

**Scenario 5: Agent + Password Compromised**
- Impact: Attacker has Agent key (from Steward DB) + User key (via password)
- Result: Can reconstruct full private key
- Mitigation: None currently - this is the worst case
- Recovery: Move funds immediately if detected

### Security Assumptions

1. **User's machine is not fully compromised**: We assume the attacker does not have root/kernel access
2. **User's password is strong**: We assume at least 16 characters with complexity
3. **User verifies transactions**: We assume user reads Telegram notifications and policy change prompts
4. **Backup is stored securely**: We assume user protects their encrypted backup file

---

## 6. Responsible Disclosure

### Reporting Security Issues

We take security seriously. If you discover a vulnerability, please report it responsibly.

**DO:**
- Report via email to: security@kamuywallet.io
- Include detailed steps to reproduce
- Provide proof-of-concept code if applicable
- Allow 90 days for response before public disclosure

**DO NOT:**
- Post vulnerabilities in public issues or forums
- Exploit vulnerabilities beyond what's needed for proof of concept
- Demand payment for disclosure (we do not have a bug bounty program at this time)

### What to Include in Your Report

1. **Summary**: Brief description of the vulnerability
2. **Impact**: What an attacker could achieve
3. **Steps to reproduce**: Detailed instructions
4. **Proof of concept**: Code or screenshots demonstrating the issue
5. **Suggested fix**: If you have ideas for remediation
6. **Disclosure timeline**: Your preferred timeline for public disclosure

### Example Report

```
Subject: [Security] Potential timing attack in API key validation

Summary:
The API key validation may be vulnerable to timing attacks due to
early-exit comparison in certain code paths.

Impact:
An attacker could recover the API key through careful timing analysis,
gaining unauthorized access to the signing API.

Steps to reproduce:
1. Start Steward service with API key "test123"
2. Send requests with incrementally longer prefixes
3. Measure response times with microsecond precision
4. Observe statistically significant timing differences

Proof of concept:
[Code snippet or repository link]

Suggested fix:
Use constant-time comparison (subtle::ConstantTimeEq) for all
comparison paths, not just the main validation function.

Timeline:
I request 90 days to fix before public disclosure.
```

### Our Commitment

- **Acknowledgment**: We will acknowledge receipt within 48 hours
- **Assessment**: We will assess and classify within 7 days
- **Fix timeline**: Critical vulnerabilities will be patched within 14 days
- **Credit**: We will credit you in release notes (unless you prefer anonymity)
- **Communication**: We will keep you informed throughout the process

### Security Advisories

Security advisories will be published:
- On our GitHub Security Advisories page
- In release notes for affected versions
- Via email to registered users (for critical issues)

---

## Security Audit History

| Date | Auditor | Scope | Status |
|------|---------|-------|--------|
| March 2026 | Tao + Esteban | MPC Core + Steward | All issues resolved |
| March 2026 | Internal | Full audit | Production ready |

For detailed audit reports, see the `audits/` directory.

---

## Security Configuration Recommendations

### Production Deployment

1. **Use environment variables** for all secrets (API keys, database URLs)
2. **Enable TLS** for all API communication
3. **Use a reverse proxy** with rate limiting (nginx, Caddy, or Cloudflare)
4. **Configure CORS** appropriately for your deployment
5. **Enable audit logging** (planned feature)
6. **Use PostgreSQL** instead of SQLite for production workloads
7. **Implement monitoring** for suspicious activity (failed auth, unusual signing patterns)

### User Recommendations

1. **Use a strong password** (16+ characters, password manager recommended)
2. **Store backup securely** (offline, encrypted, multiple locations)
3. **Monitor Telegram notifications** for unexpected transactions
4. **Review whitelist periodically** for unauthorized additions
5. **Set conservative limits** initially, increase as needed
6. **Keep software updated** to receive security patches

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 2.0 | March 2026 | Initial release with MPC threshold signatures |

---

*This document is maintained by the Kamuy Wallet security team. For questions, contact security@kamuywallet.io.*