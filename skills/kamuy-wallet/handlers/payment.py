"""
Payment handler for Kamuy Wallet OpenClaw skill.

Parses natural language payment requests and submits transactions via Steward API.
"""

import re
from typing import Optional

# Use aiohttp for async HTTP calls
try:
    import aiohttp
    HAS_AIOHTTP = True
except ImportError:
    HAS_AIOHTTP = False

# Fallback to httpx if aiohttp not available
if not HAS_AIOHTTP:
    try:
        import httpx
        HAS_HTTPX = True
    except ImportError:
        HAS_HTTPX = False


# Constants
USDC_MICROS_PER_DOLLAR = 1_000_000  # $1 = 1,000,000 USDC micros
DEFAULT_CHAIN_ID = 1  # Ethereum mainnet
STEWARD_API_URL = "http://localhost:8080/api/v1/transactions"

# Known recipients (label to address mapping)
# This could be extended to load from a config file or database
KNOWN_RECIPIENTS = {
    # Common AI service providers
    "openai": "0x3b14dD5D9E8a1B3F7E8C6a5B4d3c2B1a0E9F8D7C",  # Example address
    "anthropic": "0x4c25eE6a0F9B2C4A8D9E7b6c5D4f3A2e1B0d9C8b",  # Example address
    "aws": "0x5d36Ff7b1A0c3D5B9E0F8c7A6b5E4d3C2B1a0F9e",  # Example address
    "google cloud": "0x6e47Gg8c2B1d4E6C0F1A9b8C7d6E5f4A3B2c1D0e",  # Example address
    "azure": "0x7f58Hh9d3C2e5F7D1A2B0c9D8e7F6g5B4C3d2E1f",  # Example address
}


def parse_payment_request(text: str) -> dict:
    """Extract payment details from natural language.

    Parses patterns like:
    - "Pay OpenAI $47 for API credits"
    - "Send 100 USDC to 0x1234..."
    - "Spend $50 on AWS"
    - "Pay $25 to 0xabcd... for services"

    Args:
        text: Natural language payment request.

    Returns:
        dict with keys: to, amount_micros, reason, chain_id, raw_text

    Raises:
        ValueError: If payment details cannot be parsed.
    """
    text = text.strip()
    result = {
        "raw_text": text,
        "to": None,
        "amount_micros": None,
        "reason": None,
        "chain_id": DEFAULT_CHAIN_ID,
    }

    # Pattern 1: "Pay X $Y for Z" or "Pay $Y to X for Z"
    # Examples: "Pay OpenAI $47 for API credits", "Pay $50 to OpenAI for credits"
    pattern1 = r"(?i)pay\s+(?:([^\$]+?)\s+)?\$(\d+(?:\.\d+)?)\s*(?:to\s+([^\s]+))?\s*(?:for\s+(.+))?"
    match1 = re.match(pattern1, text)
    if match1:
        recipient_label = match1.group(1)
        amount = float(match1.group(2))
        recipient_addr = match1.group(3)
        reason = match1.group(4)

        if recipient_addr and is_ethereum_address(recipient_addr):
            result["to"] = recipient_addr
        elif recipient_label:
            result["to"] = resolve_recipient(recipient_label.strip())

        result["amount_micros"] = int(amount * USDC_MICROS_PER_DOLLAR)
        if reason:
            result["reason"] = reason.strip()
        return finalize_result(result)

    # Pattern 2: "Send X USDC to Y" or "Send $X to Y"
    # Examples: "Send 100 USDC to 0x1234...", "Send $50 to 0xabcd..."
    pattern2 = r"(?i)send\s+(\d+(?:\.\d+)?)\s*(?:USDC|usdc)?\s+to\s+(0x[a-fA-F0-9]{40})"
    match2 = re.match(pattern2, text)
    if match2:
        amount = float(match2.group(1))
        result["to"] = match2.group(2)
        result["amount_micros"] = int(amount * USDC_MICROS_PER_DOLLAR)
        return finalize_result(result)

    # Pattern 2b: "Send $X to Y"
    pattern2b = r"(?i)send\s+\$(\d+(?:\.\d+)?)\s+to\s+(.+?)(?:\s+for\s+(.+))?$"
    match2b = re.match(pattern2b, text)
    if match2b:
        amount = float(match2b.group(1))
        recipient = match2b.group(2).strip()
        reason = match2b.group(3)

        if is_ethereum_address(recipient):
            result["to"] = recipient
        else:
            result["to"] = resolve_recipient(recipient)

        result["amount_micros"] = int(amount * USDC_MICROS_PER_DOLLAR)
        if reason:
            result["reason"] = reason.strip()
        return finalize_result(result)

    # Pattern 3: "Spend $X on Y"
    # Examples: "Spend $50 on AWS", "Spend $100 on OpenAI API"
    pattern3 = r"(?i)spend\s+\$(\d+(?:\.\d+)?)\s+on\s+(.+?)(?:\s+for\s+(.+))?$"
    match3 = re.match(pattern3, text)
    if match3:
        amount = float(match3.group(1))
        recipient_label = match3.group(2).strip()
        reason = match3.group(3)

        result["to"] = resolve_recipient(recipient_label)
        result["amount_micros"] = int(amount * USDC_MICROS_PER_DOLLAR)
        if reason:
            result["reason"] = reason.strip()
        return finalize_result(result)

    # Pattern 4: "Buy X for $Y" or "Purchase X for $Y"
    # Examples: "Buy API credits for $25", "Purchase credits for $100 from OpenAI"
    pattern4 = r"(?i)(?:buy|purchase)\s+(.+?)\s+for\s+\$(\d+(?:\.\d+)?)\s*(?:from\s+(.+))?$"
    match4 = re.match(pattern4, text)
    if match4:
        item = match4.group(1).strip()
        amount = float(match4.group(2))
        recipient_label = match4.group(3)

        if recipient_label:
            result["to"] = resolve_recipient(recipient_label.strip())
        result["amount_micros"] = int(amount * USDC_MICROS_PER_DOLLAR)
        result["reason"] = f"Purchase: {item}"
        return finalize_result(result)

    # Pattern 5: Direct address with amount
    # Examples: "0x1234... $50", "Transfer $100 to 0xabcd..."
    pattern5 = r"(?i)(?:transfer\s+)?\$(\d+(?:\.\d+)?)\s+to\s+(0x[a-fA-F0-9]{40})"
    match5 = re.match(pattern5, text)
    if match5:
        amount = float(match5.group(1))
        result["to"] = match5.group(2)
        result["amount_micros"] = int(amount * USDC_MICROS_PER_DOLLAR)
        return finalize_result(result)

    # Pattern 6: "Pay 0xADDRESS $X"
    pattern6 = r"(?i)pay\s+(0x[a-fA-F0-9]{40})\s+\$(\d+(?:\.\d+)?)\s*(?:for\s+(.+))?$"
    match6 = re.match(pattern6, text)
    if match6:
        result["to"] = match6.group(1)
        amount = float(match6.group(2))
        result["amount_micros"] = int(amount * USDC_MICROS_PER_DOLLAR)
        if match6.group(3):
            result["reason"] = match6.group(3).strip()
        return finalize_result(result)

    # If no pattern matched, try to extract basic components
    # Look for dollar amount
    dollar_match = re.search(r"\$(\d+(?:\.\d+)?)", text)
    if dollar_match:
        amount = float(dollar_match.group(1))
        result["amount_micros"] = int(amount * USDC_MICROS_PER_DOLLAR)

    # Look for Ethereum address
    address_match = re.search(r"(0x[a-fA-F0-9]{40})", text)
    if address_match:
        result["to"] = address_match.group(1)
    else:
        # Try to find a known recipient
        text_lower = text.lower()
        for label, addr in KNOWN_RECIPIENTS.items():
            if label in text_lower:
                result["to"] = addr
                break

    # Extract reason if present
    for_match = re.search(r"\bfor\s+(.+?)(?:\s*$|\s+to\s+)", text)
    if for_match:
        result["reason"] = for_match.group(1).strip()

    return finalize_result(result)


def finalize_result(result: dict) -> dict:
    """Validate and finalize the parsed result.

    Args:
        result: Parsed result dictionary.

    Returns:
        Validated result dictionary.

    Raises:
        ValueError: If required fields are missing or invalid.
    """
    if result["amount_micros"] is None or result["amount_micros"] <= 0:
        raise ValueError("Could not parse payment amount from request")

    if result["to"] is None:
        raise ValueError("Could not identify payment recipient")

    if not is_ethereum_address(result["to"]):
        raise ValueError(f"Invalid recipient address: {result['to']}")

    return result


def is_ethereum_address(addr: str) -> bool:
    """Check if string is a valid Ethereum address.

    Args:
        addr: String to check.

    Returns:
        True if valid Ethereum address format.
    """
    if not addr:
        return False
    if len(addr) != 42:
        return False
    if not addr.startswith(("0x", "0X")):
        return False
    return all(c in "0123456789abcdefABCDEF" for c in addr[2:])


def resolve_recipient(label: str) -> Optional[str]:
    """Resolve a recipient label to an Ethereum address.

    Args:
        label: Human-readable recipient label (e.g., "OpenAI", "AWS").

    Returns:
        Ethereum address if found, None otherwise.
    """
    label_lower = label.lower().strip()

    # Check known recipients
    if label_lower in KNOWN_RECIPIENTS:
        return KNOWN_RECIPIENTS[label_lower]

    # Check if it's already an address
    if is_ethereum_address(label):
        return label

    return None


async def handle_payment(request: dict) -> dict:
    """Handle payment requests from OpenClaw agent.

    Parses natural language payment commands like:
    - "Pay OpenAI $47 for API credits"
    - "Send 10 USDC to 0x123..."
    - "Spend $50 on AWS"

    Args:
        request: The payment request containing:
            - text: Natural language payment command
            - api_key: Optional API key for Steward API
            - base_url: Optional Steward API base URL
            - chain_id: Optional chain ID override

    Returns:
        dict: Response with transaction status or error message:
            - response: Human-readable status message
            - success: Boolean indicating if payment was successful
            - tx_id: Transaction ID if submitted
            - tx_hash: Transaction hash if completed
    """
    text = request.get("text", "")
    api_key = request.get("api_key", "")
    base_url = request.get("base_url", "http://localhost:8080")
    chain_id = request.get("chain_id", DEFAULT_CHAIN_ID)

    if not text:
        return {
            "response": "No payment request provided",
            "success": False,
        }

    # Parse the natural language request
    try:
        details = parse_payment_request(text)
    except ValueError as e:
        return {
            "response": f"Could not parse payment request: {e}",
            "success": False,
        }

    # Prepare API request
    endpoint = f"{base_url.rstrip('/')}/api/v1/transactions"
    headers = {
        "Content-Type": "application/json",
    }
    if api_key:
        headers["X-API-Key"] = api_key

    body = {
        "to": details["to"],
        "value": str(details["amount_micros"]),
        "token": "USDC",
        "chain_id": details.get("chain_id", chain_id),
        "wait": True,  # Wait for approval/signing
    }

    # Add reason if provided
    if details.get("reason"):
        body["reason"] = details["reason"]

    # Make async HTTP call
    try:
        if HAS_AIOHTTP:
            result = await _call_with_aiohttp(endpoint, headers, body)
        elif HAS_HTTPX:
            result = await _call_with_httpx(endpoint, headers, body)
        else:
            return {
                "response": "No HTTP client available. Install aiohttp or httpx.",
                "success": False,
            }
    except Exception as e:
        return {
            "response": f"Payment failed: {str(e)}",
            "success": False,
        }

    # Format response based on result
    return format_response(result, details)


async def _call_with_aiohttp(endpoint: str, headers: dict, body: dict) -> dict:
    """Make HTTP call using aiohttp.

    Args:
        endpoint: API endpoint URL.
        headers: Request headers.
        body: Request body.

    Returns:
        JSON response from API.

    Raises:
        Exception: On HTTP or network error.
    """
    async with aiohttp.ClientSession() as session:
        async with session.post(
            endpoint,
            json=body,
            headers=headers,
            timeout=aiohttp.ClientTimeout(total=120),  # Long timeout for approval
        ) as resp:
            result = await resp.json()
            result["_status_code"] = resp.status
            return result


async def _call_with_httpx(endpoint: str, headers: dict, body: dict) -> dict:
    """Make HTTP call using httpx.

    Args:
        endpoint: API endpoint URL.
        headers: Request headers.
        body: Request body.

    Returns:
        JSON response from API.

    Raises:
        Exception: On HTTP or network error.
    """
    async with httpx.AsyncClient(timeout=120.0) as client:
        resp = await client.post(endpoint, json=body, headers=headers)
        result = resp.json()
        result["_status_code"] = resp.status_code
        return result


def format_response(result: dict, details: dict) -> dict:
    """Format the API response into a user-friendly message.

    Args:
        result: API response dictionary.
        details: Parsed payment details.

    Returns:
        Formatted response dictionary.
    """
    status_code = result.get("_status_code", 200)

    # Handle error responses
    if status_code >= 400:
        error_msg = result.get("error", result.get("message", "Unknown error"))
        return {
            "response": f"Payment failed: {error_msg}",
            "success": False,
            "error": error_msg,
        }

    # Handle API error format
    if not result.get("success", True):
        error_msg = result.get("error", result.get("message", "Unknown error"))
        return {
            "response": f"Payment failed: {error_msg}",
            "success": False,
            "error": error_msg,
        }

    # Extract data from successful response
    data = result.get("data", result)
    status = data.get("status", "unknown")

    # Format amount for display
    amount_display = format_amount_display(details["amount_micros"])
    recipient_display = format_recipient_display(details["to"])

    # Handle different statuses
    if status == "awaiting_approval" or status == "pending_approval":
        reason = data.get("reason", data.get("message", "Approval required"))
        tx_id = data.get("tx_id", data.get("id", "unknown"))
        return {
            "response": f"Transaction queued. Approval required: {reason}",
            "success": True,
            "tx_id": tx_id,
            "status": "awaiting_approval",
        }

    if status == "pending" or status == "queued":
        tx_id = data.get("tx_id", data.get("id", "unknown"))
        return {
            "response": f"Transaction queued for processing. Amount: {amount_display} to {recipient_display}",
            "success": True,
            "tx_id": tx_id,
            "status": "pending",
        }

    if status == "completed" or status == "success":
        tx_hash = data.get("tx_hash", "pending")
        tx_id = data.get("tx_id", data.get("id", "unknown"))
        return {
            "response": f"Transaction sent: {tx_hash}",
            "success": True,
            "tx_id": tx_id,
            "tx_hash": tx_hash,
            "status": "completed",
        }

    if status == "failed" or status == "rejected":
        error = data.get("error", "Transaction failed")
        return {
            "response": f"Payment failed: {error}",
            "success": False,
            "error": error,
            "status": status,
        }

    # Unknown status
    tx_id = data.get("tx_id", data.get("id"))
    message = data.get("message", f"Transaction status: {status}")
    return {
        "response": message,
        "success": True,
        "tx_id": tx_id,
        "status": status,
    }


def format_amount_display(amount_micros: int) -> str:
    """Format USDC micros amount for display.

    Args:
        amount_micros: Amount in USDC micros.

    Returns:
        Human-readable amount string (e.g., "$47.00").
    """
    dollars = amount_micros / USDC_MICROS_PER_DOLLAR
    if dollars == int(dollars):
        return f"${int(dollars)}"
    return f"${dollars:.2f}"


def format_recipient_display(address: str) -> str:
    """Format Ethereum address for display.

    Args:
        address: Full Ethereum address.

    Returns:
        Shortened address string (e.g., "0x1234...5678").
    """
    if not address or len(address) < 10:
        return address
    return f"{address[:6]}...{address[-4:]}"


# Synchronous wrapper for non-async contexts
def handle_payment_sync(request: dict) -> dict:
    """Synchronous wrapper for handle_payment.

    Args:
        request: Payment request dictionary.

    Returns:
        Response dictionary.
    """
    import asyncio
    try:
        loop = asyncio.get_running_loop()
    except RuntimeError:
        loop = None

    if loop and loop.is_running():
        import concurrent.futures
        with concurrent.futures.ThreadPoolExecutor() as pool:
            future = pool.submit(asyncio.run, handle_payment(request))
            return future.result()
    else:
        return asyncio.run(handle_payment(request))