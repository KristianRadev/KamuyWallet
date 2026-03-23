//! # Core Types for Steward Service
//!
//! Defines all the data structures used throughout the Steward service.
#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Unique identifier for transactions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransactionId(pub Uuid);

impl TransactionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl Default for TransactionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TransactionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for TransactionId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

/// Supported stablecoins for payments
pub const SUPPORTED_STABLECOINS: &[&str] = &["USDC", "USDT", "DAI"];

/// Token decimals (for amount formatting)
pub const TOKEN_DECIMALS: &[(&str, u32)] = &[
    ("USDC", 6),
    ("USDT", 6),
    ("DAI", 18),
];

/// Check if token is a supported stablecoin
pub fn is_supported_stablecoin(token: &str) -> bool {
    let upper = token.to_uppercase();
    SUPPORTED_STABLECOINS.iter().any(|&t| t == upper)
}

/// Get token decimals
pub fn get_token_decimals(token: &str) -> u32 {
    let upper = token.to_uppercase();
    TOKEN_DECIMALS
        .iter()
        .find(|(t, _)| t == &upper)
        .map(|(_, d)| *d)
        .unwrap_or(18) // Default to 18 decimals
}

/// Format amount from wei/smallest unit to human-readable
pub fn format_amount(amount_wei: &str, token: &str) -> String {
    let decimals = get_token_decimals(token);
    match amount_wei.parse::<u128>() {
        Ok(value) => {
            let divisor = 10u128.pow(decimals);
            let whole = value / divisor;
            let frac = value % divisor;
            if frac == 0 {
                format!("{} {}", whole, token.to_uppercase())
            } else {
                let frac_str = format!("{:0>width$}", frac, width = decimals as usize);
                let frac_trimmed = frac_str.trim_end_matches('0');
                if frac_trimmed.is_empty() {
                    format!("{} {}", whole, token.to_uppercase())
                } else {
                    format!("{}.{}", whole, token.to_uppercase())
                }
            }
        }
        Err(_) => format!("{} {}", amount_wei, token.to_uppercase()),
    }
}

/// Unique identifier for policy change requests
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PolicyChangeRequestId(pub Uuid);

impl PolicyChangeRequestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PolicyChangeRequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PolicyChangeRequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for PolicyChangeRequestId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

/// Policy change request status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyChangeStatus {
    /// Pending user approval
    Pending,
    /// Approved by user
    Approved,
    /// Rejected by user
    Rejected,
    /// Expired (timeout)
    Expired,
}

impl std::fmt::Display for PolicyChangeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyChangeStatus::Pending => write!(f, "pending"),
            PolicyChangeStatus::Approved => write!(f, "approved"),
            PolicyChangeStatus::Rejected => write!(f, "rejected"),
            PolicyChangeStatus::Expired => write!(f, "expired"),
        }
    }
}

/// Policy change request from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyChangeRequest {
    /// Unique request ID
    pub id: PolicyChangeRequestId,
    /// Policy field to change (e.g., "max_daily", "require_approval_above")
    pub field: String,
    /// Current value (for display purposes)
    pub current_value: String,
    /// New value requested
    pub new_value: String,
    /// Human-readable reason for the change
    pub reason: String,
    /// Agent ID that made the request
    pub agent_id: String,
    /// When the request was made
    pub created_at: DateTime<Utc>,
    /// When the request expires
    pub expires_at: DateTime<Utc>,
}

impl PolicyChangeRequest {
    /// Create a new policy change request
    pub fn new(
        field: impl Into<String>,
        current_value: impl Into<String>,
        new_value: impl Into<String>,
        reason: impl Into<String>,
        agent_id: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: PolicyChangeRequestId::new(),
            field: field.into(),
            current_value: current_value.into(),
            new_value: new_value.into(),
            reason: reason.into(),
            agent_id: agent_id.into(),
            created_at: now,
            expires_at: now + chrono::Duration::hours(24),
        }
    }

    /// Check if request is expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

/// Policy change request record stored in database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyChangeRecord {
    /// Request data
    pub request: PolicyChangeRequest,
    /// Current status
    pub status: PolicyChangeStatus,
    /// Who approved/rejected (chat ID)
    pub resolved_by: Option<String>,
    /// When it was resolved
    pub resolved_at: Option<DateTime<Utc>>,
    /// Updated timestamp
    pub updated_at: DateTime<Utc>,
}

impl PolicyChangeRecord {
    /// Create a new record from a request
    pub fn new(request: PolicyChangeRequest) -> Self {
        Self {
            request,
            status: PolicyChangeStatus::Pending,
            resolved_by: None,
            resolved_at: None,
            updated_at: Utc::now(),
        }
    }

    /// Mark as approved
    pub fn approve(&mut self, resolved_by: String) {
        self.status = PolicyChangeStatus::Approved;
        self.resolved_by = Some(resolved_by);
        self.resolved_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// Mark as rejected
    pub fn reject(&mut self, resolved_by: String) {
        self.status = PolicyChangeStatus::Rejected;
        self.resolved_by = Some(resolved_by);
        self.resolved_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}

/// Transaction status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionStatus {
    /// Received but not yet processed
    Pending,
    /// Currently being evaluated by policy engine
    Evaluating,
    /// Policy check passed, awaiting signature
    Approved,
    /// Policy check failed, awaiting user approval
    AwaitingApproval,
    /// User approved, processing
    UserApproved,
    /// User rejected
    UserRejected,
    /// Currently signing
    Signing,
    /// Signed and submitted to relayer
    Submitted,
    /// Confirmed on-chain
    Confirmed,
    /// Failed during execution
    Failed,
    /// Rejected by policy
    Rejected,
    /// Expired (too old)
    Expired,
}

impl std::fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionStatus::Pending => write!(f, "pending"),
            TransactionStatus::Evaluating => write!(f, "evaluating"),
            TransactionStatus::Approved => write!(f, "approved"),
            TransactionStatus::AwaitingApproval => write!(f, "awaiting_approval"),
            TransactionStatus::UserApproved => write!(f, "user_approved"),
            TransactionStatus::UserRejected => write!(f, "user_rejected"),
            TransactionStatus::Signing => write!(f, "signing"),
            TransactionStatus::Submitted => write!(f, "submitted"),
            TransactionStatus::Confirmed => write!(f, "confirmed"),
            TransactionStatus::Failed => write!(f, "failed"),
            TransactionStatus::Rejected => write!(f, "rejected"),
            TransactionStatus::Expired => write!(f, "expired"),
        }
    }
}

/// Transaction request from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRequest {
    /// Unique transaction ID
    pub id: TransactionId,
    /// Request ID from agent
    pub request_id: String,
    /// Destination address
    pub to: String,
    /// Amount as string (for precision)
    pub value: String,
    /// Token symbol (e.g., "USDC", "USDT")
    pub token: String,
    /// Chain ID
    pub chain_id: u64,
    /// Nonce
    pub nonce: u64,
    /// Gas price (optional)
    pub gas_price: Option<String>,
    /// Gas limit (optional)
    pub gas_limit: Option<u64>,
    /// Data payload (optional)
    pub data: Option<Vec<u8>>,
    /// Timestamp when request was received
    pub received_at: DateTime<Utc>,
    /// Agent ID that made the request
    pub agent_id: String,
    /// Agent signature (proves request came from agent)
    pub agent_signature: Option<String>,
}

impl TransactionRequest {
    /// Create a new transaction request
    pub fn new(
        request_id: impl Into<String>,
        to: impl Into<String>,
        value: impl Into<String>,
        token: impl Into<String>,
        chain_id: u64,
        agent_id: impl Into<String>,
    ) -> Self {
        Self {
            id: TransactionId::new(),
            request_id: request_id.into(),
            to: to.into(),
            value: value.into(),
            token: token.into(),
            chain_id,
            nonce: 0,
            gas_price: None,
            gas_limit: None,
            data: None,
            received_at: Utc::now(),
            agent_id: agent_id.into(),
            agent_signature: None,
        }
    }

    /// Compute hash for signing
    pub fn hash(&self) -> [u8; 32] {
        use sha3::{Digest, Keccak256};

        let mut hasher = Keccak256::new();
        hasher.update(self.request_id.as_bytes());
        hasher.update(self.to.as_bytes());
        hasher.update(self.value.as_bytes());
        hasher.update(self.token.as_bytes());
        hasher.update(&self.chain_id.to_be_bytes());
        hasher.update(&self.nonce.to_be_bytes());
        if let Some(data) = &self.data {
            hasher.update(data);
        }

        hasher.finalize().into()
    }
}

/// Transaction record stored in database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    /// Transaction ID
    pub id: TransactionId,
    /// Current status
    pub status: TransactionStatus,
    /// Transaction request data
    pub request: TransactionRequest,
    /// Policy evaluation result
    pub policy_result: Option<PolicyResult>,
    /// User approval data (if required)
    pub user_approval: Option<UserApproval>,
    /// Signature data (once signed)
    pub signature: Option<TransactionSignature>,
    /// Transaction hash on-chain
    pub tx_hash: Option<String>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
    /// Updated timestamp
    pub updated_at: DateTime<Utc>,
    /// Expires at
    pub expires_at: DateTime<Utc>,
}

impl TransactionRecord {
    /// Create a new transaction record
    pub fn new(request: TransactionRequest) -> Self {
        let now = Utc::now();
        Self {
            id: request.id,
            status: TransactionStatus::Pending,
            request,
            policy_result: None,
            user_approval: None,
            signature: None,
            tx_hash: None,
            error: None,
            created_at: now,
            updated_at: now,
            expires_at: now + chrono::Duration::hours(24),
        }
    }

    /// Update status
    pub fn set_status(&mut self, status: TransactionStatus) {
        self.status = status;
        self.updated_at = Utc::now();
    }

    /// Check if transaction is expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

/// Policy evaluation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyResult {
    /// Whether the transaction passed all policy checks
    pub passed: bool,
    /// Individual check results
    pub checks: Vec<PolicyCheck>,
    /// Overall decision
    pub decision: PolicyDecision,
    /// Reason for decision
    pub reason: String,
    /// Timestamp
    pub evaluated_at: DateTime<Utc>,
}

/// Individual policy check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyCheck {
    /// Name of the check
    pub name: String,
    /// Whether it passed
    pub passed: bool,
    /// Value that was checked
    pub value: String,
    /// Limit or threshold
    pub limit: String,
    /// Message explaining the result
    pub message: String,
}

/// Policy decision
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    /// Auto-approve (all checks passed)
    AutoApprove,
    /// Require user approval
    RequireApproval,
    /// Reject (violation)
    Reject,
}

/// Approval level required for an operation
/// Used to implement "higher-security-wins" logic
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalLevel {
    /// Auto-approved (within all limits)
    AutoApprove,
    /// Telegram button approval required
    TelegramButton,
    /// Terminal password required (highest security)
    TerminalPassword,
}

impl std::fmt::Display for ApprovalLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApprovalLevel::AutoApprove => write!(f, "auto_approve"),
            ApprovalLevel::TelegramButton => write!(f, "telegram_button"),
            ApprovalLevel::TerminalPassword => write!(f, "terminal_password"),
        }
    }
}

/// User approval data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserApproval {
    /// Whether user approved
    pub approved: bool,
    /// User ID (Telegram chat ID or user identifier)
    pub user_id: String,
    /// Approval timestamp
    pub approved_at: DateTime<Utc>,
    /// Optional comment
    pub comment: Option<String>,
}

/// Transaction signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionSignature {
    /// r component
    pub r: String,
    /// s component
    pub s: String,
    /// Recovery ID
    pub recid: u8,
    /// Signed at
    pub signed_at: DateTime<Utc>,
}

/// Token information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    /// Token symbol
    pub symbol: String,
    /// Token name
    pub name: String,
    /// Decimals
    pub decimals: u8,
    /// Contract address
    pub contract_address: String,
    /// Chain ID
    pub chain_id: u64,
}

/// Spending limits for a time period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendingLimits {
    /// Amount spent today
    pub daily_spent: String,
    /// Daily limit
    pub daily_limit: String,
    /// Amount spent this week
    pub weekly_spent: String,
    /// Weekly limit
    pub weekly_limit: String,
    /// Amount spent this month
    pub monthly_spent: String,
    /// Monthly limit
    pub monthly_limit: String,
    /// Transaction count today
    pub daily_count: u32,
    /// Transaction count limit per day
    pub daily_count_limit: u32,
}

/// Wallet information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletInfo {
    /// Wallet address
    pub address: String,
    /// Chain ID
    pub chain_id: u64,
    /// Current balance
    pub balance: String,
    /// Token balances
    pub token_balances: HashMap<String, String>,
    /// Pending transaction count
    pub pending_count: u32,
}

/// API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// Whether the request was successful
    pub success: bool,
    /// Response data
    pub data: Option<T>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Request ID for tracking
    pub request_id: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

impl<T> ApiResponse<T> {
    /// Create a successful response
    pub fn success(data: T, request_id: impl Into<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            request_id: request_id.into(),
            timestamp: Utc::now(),
        }
    }

    /// Create an error response
    pub fn error(error: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error.into()),
            request_id: request_id.into(),
            timestamp: Utc::now(),
        }
    }
}

/// Pagination parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    /// Page number (0-indexed)
    pub page: u32,
    /// Items per page
    pub per_page: u32,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            page: 0,
            per_page: 20,
        }
    }
}

/// Paginated response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    /// Items
    pub items: Vec<T>,
    /// Total count
    pub total: u64,
    /// Page number
    pub page: u32,
    /// Items per page
    pub per_page: u32,
    /// Total pages
    pub total_pages: u32,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Service status
    pub status: String,
    /// Version
    pub version: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Component health
    pub components: HashMap<String, ComponentHealth>,
}

/// Component health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    /// Status: ok, degraded, error
    pub status: String,
    /// Last checked
    pub last_checked: DateTime<Utc>,
    /// Optional message
    pub message: Option<String>,
    /// Latency in ms
    pub latency_ms: u64,
}

/// Request body for recovery key retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryKeyRequest {
    /// User password for authentication
    pub password: String,
}

/// Response containing the decrypted recovery key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryKeyResponse {
    /// The decrypted user key (hex-encoded)
    pub user_key: String,
}

/// Request body for agent key retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentKeyRequest {
    /// User password for authentication
    pub password: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_id() {
        let id = TransactionId::new();
        let id2 = TransactionId::new();
        assert_ne!(id, id2);
    }

    #[test]
    fn test_transaction_status_display() {
        assert_eq!(TransactionStatus::Pending.to_string(), "pending");
        assert_eq!(TransactionStatus::Confirmed.to_string(), "confirmed");
    }

    #[test]
    fn test_transaction_request_hash() {
        let req = TransactionRequest::new(
            "req1",
            "0x123",
            "100",
            "USDC",
            1,
            "agent1",
        );
        let hash = req.hash();
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_api_response() {
        let response: ApiResponse<String> = ApiResponse::success("data".to_string(), "req1");
        assert!(response.success);
        assert_eq!(response.data, Some("data".to_string()));
        assert!(response.error.is_none());
    }

    #[test]
    fn test_approval_level_ordering() {
        // Higher security = higher value
        assert!(ApprovalLevel::TerminalPassword > ApprovalLevel::TelegramButton);
        assert!(ApprovalLevel::TelegramButton > ApprovalLevel::AutoApprove);
        assert!(ApprovalLevel::AutoApprove < ApprovalLevel::TerminalPassword);
    }
}
