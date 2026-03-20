"""
Query handler for Kamuy Wallet OpenClaw skill.

Handles wallet queries including balance, policy, history, whitelist, and pending items.
"""

import aiohttp
from typing import Optional


# Chain ID to name mapping
CHAIN_NAMES = {
    1: "Ethereum",
    8453: "Base",
    137: "Polygon",
    42161: "Arbitrum",
    10: "Optimism",
    11155111: "Sepolia",
    84532: "Base Sepolia",
}


def parse_query_type(text: str) -> str:
    """Determine the type of wallet query from natural language text.

    Args:
        text: Natural language query text.

    Returns:
        Query type: "balance", "policy", "history", "whitelist", "pending", or "status".
    """
    text_lower = text.lower()

    # Balance queries
    if any(keyword in text_lower for keyword in ["balance", "how much", "amount"]):
        # Exclude history-related balance queries
        if not any(kw in text_lower for kw in ["history", "recent", "spent", "transaction"]):
            return "balance"

    # Pending queries (check before policy to catch "pending approval")
    if any(keyword in text_lower for keyword in ["pending", "approval", "awaiting"]):
        return "pending"

    # Policy/limit queries
    if any(keyword in text_lower for keyword in ["policy", "limit", "spending limit", "threshold"]):
        return "policy"

    # History queries
    if any(keyword in text_lower for keyword in ["history", "recent", "spent", "transaction", "activity"]):
        return "history"

    # Whitelist queries
    if any(keyword in text_lower for keyword in ["whitelist", "trusted", "allowed address"]):
        return "whitelist"

    return "status"  # Default to status


def format_usdc_amount(micros: str) -> str:
    """Format USDC micros amount to human-readable string.

    Args:
        micros: Amount in USDC micros (6 decimals).

    Returns:
        Human-readable amount string like "1.50 USDC".
    """
    try:
        value = int(micros)
        whole = value // 1_000_000
        frac = value % 1_000_000
        if frac == 0:
            return f"{whole} USDC"
        else:
            return f"{whole}.{frac:06d}".rstrip("0").rstrip(".") + " USDC"
    except (ValueError, TypeError):
        return f"{micros} USDC"


def format_balance_response(data: dict) -> dict:
    """Format balance API response for user display.

    Args:
        data: Balance data from Steward API.

    Returns:
        Formatted response dict with text and structured data.
    """
    balances = data.get("balances", [])

    if not balances:
        return {
            "text": "Your wallet has no balances yet.",
            "data": {"balances": []}
        }

    lines = ["Your wallet balances:"]
    formatted_balances = []

    for balance in balances:
        chain_id = balance.get("chain_id", 0)
        chain_name = CHAIN_NAMES.get(chain_id, f"Chain {chain_id}")
        amount = format_usdc_amount(balance.get("balance", "0"))
        address = balance.get("address", "unknown")

        lines.append(f"  - {amount} on {chain_name}")
        formatted_balances.append({
            "chain": chain_name,
            "chain_id": chain_id,
            "balance": amount,
            "address": address
        })

    return {
        "text": "\n".join(lines),
        "data": {"balances": formatted_balances}
    }


def format_policy_response(data: dict) -> dict:
    """Format policy API response for user display.

    Args:
        data: Policy data from Steward API.

    Returns:
        Formatted response dict with text and structured data.
    """
    lines = ["Your wallet policy:"]

    # Spending limits
    max_per_tx = format_usdc_amount(data.get("max_per_tx", "0"))
    max_daily = format_usdc_amount(data.get("max_daily", "0"))
    max_weekly = format_usdc_amount(data.get("max_weekly", "0"))
    auto_add_threshold = format_usdc_amount(data.get("auto_add_threshold", "0"))

    lines.append(f"  - Max per transaction: {max_per_tx}")
    lines.append(f"  - Max per day: {max_daily}")
    lines.append(f"  - Max per week: {max_weekly}")
    lines.append(f"  - Auto-add threshold: {auto_add_threshold}")

    # Spending tracker
    tracker = data.get("spending_tracker", {})
    daily_spent = format_usdc_amount(tracker.get("daily_spent", "0"))
    weekly_spent = format_usdc_amount(tracker.get("weekly_spent", "0"))

    lines.append(f"  - Spent today: {daily_spent}")
    lines.append(f"  - Spent this week: {weekly_spent}")

    # Whitelist count
    whitelist = data.get("whitelist", [])
    lines.append(f"  - Whitelisted addresses: {len(whitelist)}")

    # Gasless status
    gasless = data.get("gasless", True)
    lines.append(f"  - Gasless: {'Enabled' if gasless else 'Disabled'}")

    return {
        "text": "\n".join(lines),
        "data": {
            "max_per_tx": max_per_tx,
            "max_daily": max_daily,
            "max_weekly": max_weekly,
            "auto_add_threshold": auto_add_threshold,
            "daily_spent": daily_spent,
            "weekly_spent": weekly_spent,
            "whitelist_count": len(whitelist),
            "gasless": gasless
        }
    }


def format_history_response(data: dict) -> dict:
    """Format transaction history API response for user display.

    Args:
        data: Paginated transaction history from Steward API.

    Returns:
        Formatted response dict with text and structured data.
    """
    items = data.get("items", [])
    total = data.get("total", 0)

    if not items:
        return {
            "text": "No recent transactions found.",
            "data": {"transactions": [], "total": 0}
        }

    lines = [f"Recent transactions ({total} total):"]

    formatted_txs = []
    for tx in items[:10]:  # Show up to 10 most recent
        request = tx.get("request", {})
        status = tx.get("status", "unknown")

        amount = format_usdc_amount(request.get("value", "0"))
        recipient = request.get("to", "unknown")
        # Shorten address for display
        if recipient.startswith("0x") and len(recipient) == 42:
            recipient = f"{recipient[:8]}...{recipient[-4:]}"

        chain_id = request.get("chain_id", 0)
        chain_name = CHAIN_NAMES.get(chain_id, f"Chain {chain_id}")

        status_icon = {
            "confirmed": "[OK]",
            "submitted": "[SENT]",
            "pending": "[PENDING]",
            "awaiting_approval": "[NEEDS APPROVAL]",
            "failed": "[FAILED]",
            "rejected": "[REJECTED]"
        }.get(status, f"[{status.upper()}]")

        lines.append(f"  {status_icon} {amount} to {recipient} on {chain_name}")

        formatted_txs.append({
            "amount": amount,
            "recipient": recipient,
            "chain": chain_name,
            "status": status,
            "tx_id": tx.get("id"),
            "tx_hash": tx.get("tx_hash")
        })

    if total > 10:
        lines.append(f"  ... and {total - 10} more")

    return {
        "text": "\n".join(lines),
        "data": {"transactions": formatted_txs, "total": total}
    }


def format_whitelist_response(data: dict) -> dict:
    """Format whitelist from policy API response for user display.

    Args:
        data: Policy data from Steward API containing whitelist.

    Returns:
        Formatted response dict with text and structured data.
    """
    whitelist = data.get("whitelist", [])

    if not whitelist:
        return {
            "text": "Your whitelist is empty. No addresses are currently trusted.",
            "data": {"whitelist": []}
        }

    lines = [f"Your whitelisted addresses ({len(whitelist)} total):"]

    formatted_entries = []
    for entry in whitelist:
        address = entry.get("address", "unknown")
        label = entry.get("label", "")
        # Shorten address for display
        if address.startswith("0x") and len(address) == 42:
            short_addr = f"{address[:8]}...{address[-4:]}"
        else:
            short_addr = address

        if label:
            lines.append(f"  - {label}: {short_addr}")
        else:
            lines.append(f"  - {short_addr}")

        formatted_entries.append({
            "address": address,
            "label": label
        })

    return {
        "text": "\n".join(lines),
        "data": {"whitelist": formatted_entries}
    }


def format_pending_response(data: dict) -> dict:
    """Format pending transactions API response for user display.

    Args:
        data: Pending transactions data from Steward API.

    Returns:
        Formatted response dict with text and structured data.
    """
    # Handle both list and dict responses
    if isinstance(data, list):
        items = data
    else:
        items = data.get("items", data.get("transactions", []))

    if not items:
        return {
            "text": "No pending transactions. All caught up!",
            "data": {"pending": [], "count": 0}
        }

    lines = [f"Pending approvals ({len(items)} total):"]

    formatted_pending = []
    for tx in items:
        request = tx.get("request", {})
        policy_result = tx.get("policy_result", {})
        reason = policy_result.get("reason", "Requires approval")

        amount = format_usdc_amount(request.get("value", "0"))
        recipient = request.get("to", "unknown")
        # Shorten address for display
        if recipient.startswith("0x") and len(recipient) == 42:
            recipient = f"{recipient[:8]}...{recipient[-4:]}"

        tx_id = tx.get("id", "unknown")

        lines.append(f"  - {amount} to {recipient}")
        lines.append(f"    Reason: {reason}")
        lines.append(f"    ID: {tx_id}")

        formatted_pending.append({
            "id": tx_id,
            "amount": amount,
            "recipient": recipient,
            "reason": reason
        })

    lines.append("\nTo approve, use: kamuy approve tx <ID>")
    lines.append("Or check Telegram for approval buttons.")

    return {
        "text": "\n".join(lines),
        "data": {"pending": formatted_pending, "count": len(items)}
    }


async def handle_query(request: dict) -> dict:
    """Handle wallet queries from OpenClaw agent.

    Parses natural language queries like:
    - "What's my wallet balance?"
    - "How much USDC do I have?"
    - "What's my spending limit?"
    - "Show recent transactions"
    - "Who's in my whitelist?"
    - "What's pending approval?"

    Args:
        request: The query request containing:
            - text: Natural language query string (required)
            - api_key: Steward API key (optional, uses default)
            - steward_url: Steward base URL (optional, defaults to localhost:8080)

    Returns:
        dict: Response with:
            - text: Human-readable response string
            - data: Structured response data
            - error: Error message if request failed

    Example:
        >>> result = await handle_query({"text": "What's my balance?"})
        >>> print(result["text"])
        Your wallet balances:
          - 100.50 USDC on Ethereum
          - 25.00 USDC on Base
    """
    text = request.get("text", "")
    query_type = parse_query_type(text)

    # Configuration
    steward_url = request.get("steward_url", "http://localhost:8080")
    api_key = request.get("api_key", "")

    # Build headers
    headers = {}
    if api_key:
        headers["X-API-Key"] = api_key

    async with aiohttp.ClientSession() as session:
        try:
            if query_type == "balance":
                async with session.get(
                    f"{steward_url}/api/v1/balances",
                    headers=headers,
                    timeout=aiohttp.ClientTimeout(total=10)
                ) as resp:
                    if resp.status != 200:
                        error_text = await resp.text()
                        return {
                            "text": f"Failed to get balances: {resp.status}",
                            "error": error_text
                        }
                    result = await resp.json()
                    data = result.get("data", result)
                    return format_balance_response(data)

            elif query_type == "policy":
                async with session.get(
                    f"{steward_url}/api/v1/policy",
                    headers=headers,
                    timeout=aiohttp.ClientTimeout(total=10)
                ) as resp:
                    if resp.status != 200:
                        error_text = await resp.text()
                        return {
                            "text": f"Failed to get policy: {resp.status}",
                            "error": error_text
                        }
                    result = await resp.json()
                    data = result.get("data", result)
                    return format_policy_response(data)

            elif query_type == "history":
                async with session.get(
                    f"{steward_url}/api/v1/transactions",
                    headers=headers,
                    timeout=aiohttp.ClientTimeout(total=10)
                ) as resp:
                    if resp.status != 200:
                        error_text = await resp.text()
                        return {
                            "text": f"Failed to get transaction history: {resp.status}",
                            "error": error_text
                        }
                    result = await resp.json()
                    data = result.get("data", result)
                    return format_history_response(data)

            elif query_type == "whitelist":
                async with session.get(
                    f"{steward_url}/api/v1/policy",
                    headers=headers,
                    timeout=aiohttp.ClientTimeout(total=10)
                ) as resp:
                    if resp.status != 200:
                        error_text = await resp.text()
                        return {
                            "text": f"Failed to get whitelist: {resp.status}",
                            "error": error_text
                        }
                    result = await resp.json()
                    data = result.get("data", result)
                    return format_whitelist_response(data)

            elif query_type == "pending":
                async with session.get(
                    f"{steward_url}/api/v1/transactions/pending",
                    headers=headers,
                    timeout=aiohttp.ClientTimeout(total=10)
                ) as resp:
                    if resp.status != 200:
                        error_text = await resp.text()
                        return {
                            "text": f"Failed to get pending transactions: {resp.status}",
                            "error": error_text
                        }
                    result = await resp.json()
                    data = result.get("data", result)
                    return format_pending_response(data)

            else:  # status
                # Get basic wallet status
                async with session.get(
                    f"{steward_url}/health",
                    timeout=aiohttp.ClientTimeout(total=5)
                ) as resp:
                    if resp.status == 200:
                        health = await resp.json()
                        status = health.get("status", "unknown")
                        key_status = health.get("components", {}).get("key_share", {}).get("status", "unknown")

                        return {
                            "text": f"Steward status: {status}\nKey loaded: {key_status}",
                            "data": {"status": status, "key_loaded": key_status == "ok"}
                        }
                    else:
                        return {
                            "text": "Unable to reach Steward. Is it running?",
                            "error": f"Health check failed: {resp.status}"
                        }

        except aiohttp.ClientError as e:
            return {
                "text": "Unable to connect to Steward. Please ensure it's running.",
                "error": str(e)
            }
        except Exception as e:
            return {
                "text": f"An error occurred: {e}",
                "error": str(e)
            }