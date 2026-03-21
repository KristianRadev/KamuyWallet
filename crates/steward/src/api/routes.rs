//! # API Routes
//!
//! HTTP route handlers for the Steward API.
//! Implements hybrid long-polling for transaction submission.

use super::{ApiState, PaginationQuery, error, error_response, success, validate_api_key, extract_api_key};
use crate::types::{
    ComponentHealth, PaginatedResponse, PolicyChangeRecord, PolicyChangeRequest, PolicyChangeRequestId,
    TransactionId, TransactionRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use std::collections::HashMap;
use std::time::Instant;
use tracing::{info, warn};
use tokio::time::{timeout, Duration};

/// Request body for transaction submission with wait options
/// This matches the documented API in README.md
#[derive(serde::Deserialize)]
pub struct SubmitTransactionRequest {
    /// Destination address (0x...)
    pub to: String,
    /// Amount in USDC micros (integer string, e.g., 100000000 = 1 USDC)
    pub value: String,
    /// Token symbol (USDC only in v2.0)
    pub token: String,
    /// Chain ID (1=Ethereum, 8453=Base, 137=Polygon, 42161=Arbitrum, 10=Optimism)
    pub chain_id: u64,
    /// Optional request ID from agent for idempotency
    #[serde(default)]
    pub request_id: Option<String>,
    /// Whether to wait for completion (long-polling)
    /// If true, waits up to long_poll_timeout_secs for signature
    /// If false or not specified, returns immediately with pending status
    #[serde(default = "default_wait")]
    pub wait: bool,
    /// Custom timeout in seconds (overrides server default)
    /// Maximum is limited by server's long_poll_timeout_secs
    pub timeout_secs: Option<u64>,
}

fn default_wait() -> bool {
    true // Default to waiting (long-polling)
}

impl SubmitTransactionRequest {
    /// Convert to internal TransactionRequest
    fn into_transaction_request(self) -> TransactionRequest {
        TransactionRequest::new(
            self.request_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            self.to,
            self.value,
            self.token,
            self.chain_id,
            "api", // agent_id for API submissions
        )
    }
}

/// Submit a new transaction with optional long-polling
pub async fn submit_transaction(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(body): Json<SubmitTransactionRequest>,
) -> impl IntoResponse {
    let request_id = body.request_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let should_wait = body.wait;
    let custom_timeout = body.timeout_secs;

    // Validate API key
    if let Some(key) = extract_api_key(&headers) {
        if !validate_api_key(&state, &key) {
            return error(StatusCode::UNAUTHORIZED, "Invalid API key", request_id).into_response();
        }
    } else if state.config.api.api_key.is_some() {
        return error(StatusCode::UNAUTHORIZED, "API key required", request_id).into_response();
    }

    // Validate request fields
    if let Err(e) = validate_transaction_fields(&body) {
        return error(StatusCode::BAD_REQUEST, e, request_id).into_response();
    }

    // Check if key is loaded (skip in test mode)
    if !state.config.api.test_mode && !state.is_key_loaded().await {
        return error(StatusCode::SERVICE_UNAVAILABLE, "Steward key not loaded", request_id)
            .into_response();
    }

    // Convert to internal TransactionRequest
    let tx_request = body.into_transaction_request();

    // Submit to queue
    let record = match state.queue.read().await.submit(tx_request).await {
        Ok(record) => {
            info!(
                transaction_id = %record.id,
                request_id = %request_id,
                wait = should_wait,
                "Transaction submitted via API"
            );
            record
        }
        Err(e) => {
            warn!(
                request_id = %request_id,
                error = %e,
                "Failed to submit transaction"
            );
            return error_response(e, request_id).into_response();
        }
    };

    // If not waiting, return immediately with pending status
    if !should_wait {
        return success(
            serde_json::json!({
                "tx_id": record.id.to_string(),
                "status": "pending",
                "message": "Transaction queued for processing"
            }),
            request_id
        ).into_response();
    }

    // Long-polling: wait for transaction completion
    let max_wait = custom_timeout
        .unwrap_or(state.config.api.default_wait_timeout_secs)
        .min(state.config.api.long_poll_timeout_secs);

    info!(
        transaction_id = %record.id,
        timeout_secs = max_wait,
        "Waiting for transaction completion"
    );

    // Subscribe to completion notification
    let rx = state.notifier.subscribe(record.id).await;

    // Wait with timeout
    match timeout(Duration::from_secs(max_wait), rx).await {
        Ok(Ok(result)) => {
            // Transaction completed within timeout
            info!(
                transaction_id = %record.id,
                status = ?result.status,
                "Transaction completed within wait timeout"
            );

            success(
                serde_json::json!({
                    "tx_id": result.tx_id.to_string(),
                    "status": result.status,
                    "signature": result.signature,
                    "tx_hash": result.tx_hash,
                    "error": result.error,
                    "reason": result.reason
                }),
                request_id
            ).into_response()
        }
        Ok(Err(_)) => {
            // Channel closed unexpectedly
            warn!(
                transaction_id = %record.id,
                "Notification channel closed"
            );

            // Return current status from storage
            match get_current_status(&state, record.id).await {
                Some(status_json) => success(status_json, request_id).into_response(),
                None => error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to get transaction status", request_id).into_response()
            }
        }
        Err(_) => {
            // Timeout - transaction still pending
            info!(
                transaction_id = %record.id,
                timeout_secs = max_wait,
                "Transaction still pending after wait timeout"
            );

            // Unsubscribe since we're returning
            state.notifier.unsubscribe(&record.id).await;

            success(
                serde_json::json!({
                    "tx_id": record.id.to_string(),
                    "status": "pending_approval",
                    "message": "Transaction still processing. Poll GET /transactions/{tx_id} for status.",
                    "estimated_wait_secs": state.queue.read().await.estimated_wait_seconds(record.id).await
                }),
                request_id
            ).into_response()
        }
    }
}

/// Get current transaction status from storage
async fn get_current_status(state: &ApiState, tx_id: TransactionId) -> Option<serde_json::Value> {
    match state.storage.get_transaction(tx_id).await {
        Ok(Some(record)) => Some(serde_json::json!({
            "tx_id": record.id.to_string(),
            "status": record.status.to_string(),
            "signature": record.signature.as_ref().map(|s| format!("0x{}{}", s.r, s.s)),
            "tx_hash": record.tx_hash,
            "error": record.error
        })),
        _ => None
    }
}

/// Get transaction by ID
pub async fn get_transaction(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();
    
    let tx_id = match parse_transaction_id(&id) {
        Ok(id) => id,
        Err(e) => return error(StatusCode::BAD_REQUEST, e, request_id).into_response(),
    };
    
    match state.storage.get_transaction(tx_id).await {
        Ok(Some(record)) => success(record, request_id).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "Transaction not found", request_id)
            .into_response(),
        Err(e) => error_response(e, request_id).into_response(),
    }
}

/// List transactions
pub async fn list_transactions(
    State(state): State<ApiState>,
    Query(query): Query<PaginationQuery>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();
    let pagination = query.to_pagination();
    
    match state.storage.get_recent_transactions(pagination.per_page as i64).await {
        Ok(transactions) => {
            let total = transactions.len() as u64;
            let response = PaginatedResponse {
                items: transactions,
                total,
                page: pagination.page,
                per_page: pagination.per_page,
                total_pages: ((total as f64) / (pagination.per_page as f64)).ceil() as u32,
            };
            success(response, request_id).into_response()
        }
        Err(e) => error_response(e, request_id).into_response(),
    }
}

/// Get pending transactions
pub async fn get_pending(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();
    
    match state.storage.get_pending_transactions().await {
        Ok(transactions) => success(transactions, request_id).into_response(),
        Err(e) => error_response(e, request_id).into_response(),
    }
}

/// Approve a transaction
pub async fn approve_transaction(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();
    
    let tx_id = match parse_transaction_id(&id) {
        Ok(id) => id,
        Err(e) => return error(StatusCode::BAD_REQUEST, e, request_id).into_response(),
    };
    
    match crate::queue::processor::handle_user_approval(
        &state,
        tx_id,
        true,
        "api".to_string(),
    ).await {
        Ok(()) => success(serde_json::json!({"approved": true}), request_id).into_response(),
        Err(e) => error_response(e, request_id).into_response(),
    }
}

/// Reject a transaction
pub async fn reject_transaction(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();
    
    let tx_id = match parse_transaction_id(&id) {
        Ok(id) => id,
        Err(e) => return error(StatusCode::BAD_REQUEST, e, request_id).into_response(),
    };
    
    match crate::queue::processor::handle_user_approval(
        &state,
        tx_id,
        false,
        "api".to_string(),
    ).await {
        Ok(()) => success(serde_json::json!({"rejected": true}), request_id).into_response(),
        Err(e) => error_response(e, request_id).into_response(),
    }
}

/// Get current policy (v2.0 format)
pub async fn get_policy(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();

    let rules = state.policy_engine.read().await.rules().await;

    let policy_response = serde_json::json!({
        "version": rules.version,
        "max_per_tx": rules.max_per_tx,
        "max_daily": rules.max_daily,
        "max_weekly": rules.max_weekly,
        "auto_add_threshold": rules.auto_add_threshold,
        "token": rules.token,
        "gasless": rules.gasless,
        "whitelist": rules.whitelist.entries(),
        "spending_tracker": {
            "daily_spent": rules.spending_tracker.daily_spent,
            "weekly_spent": rules.spending_tracker.weekly_spent,
            "last_reset_daily": rules.spending_tracker.last_reset_daily,
            "last_reset_weekly": rules.spending_tracker.last_reset_weekly,
        }
    });

    success(policy_response, request_id).into_response()
}

/// Update policy
pub async fn update_policy(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(rules): Json<crate::policy::rules::PolicyRules>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();
    
    // Validate API key (policy changes require auth)
    if let Some(key) = extract_api_key(&headers) {
        if !validate_api_key(&state, &key) {
            return error(StatusCode::UNAUTHORIZED, "Invalid API key", request_id).into_response();
        }
    } else {
        return error(StatusCode::UNAUTHORIZED, "API key required", request_id).into_response();
    }
    
    // Validate rules
    if let Err(e) = rules.validate() {
        return error(StatusCode::BAD_REQUEST, e.to_string(), request_id).into_response();
    }
    
    // Update policy
    match state.policy_engine.write().await.update_rules(rules).await {
        Ok(()) => {
            // Save to file
            if let Err(e) = state.policy_engine.read().await.save().await {
                return error_response(e, request_id).into_response();
            }
            success(serde_json::json!({"updated": true}), request_id).into_response()
        }
        Err(e) => error_response(e, request_id).into_response(),
    }
}

/// Get wallet info
pub async fn get_wallet(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();
    
    match state.storage.get_wallet().await {
        Ok(Some(wallet)) => success(wallet, request_id).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "No wallet configured", request_id)
            .into_response(),
        Err(e) => error_response(e, request_id).into_response(),
    }
}

/// Get balances
pub async fn get_balances(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();
    
    match state.storage.get_balances().await {
        Ok(balances) => success(balances, request_id).into_response(),
        Err(e) => error_response(e, request_id).into_response(),
    }
}

/// Get queue status
pub async fn get_queue_status(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();
    
    let queue_size = state.queue.read().await.size().await;
    let processing = state.queue.read().await.current().await.is_some() as u32;
    let pending_count = state.storage.count_pending_transactions().await.unwrap_or(0);
    
    let status = serde_json::json!({
        "queue_size": queue_size,
        "processing": processing,
        "pending_transactions": pending_count,
    });
    
    success(status, request_id).into_response()
}

/// Check component health
pub async fn check_components(state: &ApiState) -> HashMap<String, ComponentHealth> {
    let mut components = HashMap::new();
    let now = chrono::Utc::now();
    
    // Check database
    let db_start = Instant::now();
    let db_status = match state.storage.get_wallet().await {
        Ok(_) => "ok",
        Err(_) => "error",
    };
    components.insert(
        "database".to_string(),
        ComponentHealth {
            status: db_status.to_string(),
            last_checked: now,
            message: None,
            latency_ms: db_start.elapsed().as_millis() as u64,
        },
    );
    
    // Check key loaded
    components.insert(
        "key_share".to_string(),
        ComponentHealth {
            status: if state.is_key_loaded().await { "ok" } else { "degraded" }.to_string(),
            last_checked: now,
            message: if state.is_key_loaded().await {
                None
            } else {
                Some("Key not loaded".to_string())
            },
            latency_ms: 0,
        },
    );
    
    // Check queue
    components.insert(
        "queue".to_string(),
        ComponentHealth {
            status: "ok".to_string(),
            last_checked: now,
            message: None,
            latency_ms: 0,
        },
    );
    
    components
}

/// Validate transaction request fields (v2.0: USDC only)
fn validate_transaction_fields(request: &SubmitTransactionRequest) -> Result<(), String> {
    if request.value.is_empty() {
        return Err("Amount required".to_string());
    }

    // SECURITY: Parse and validate amount
    let amount = request.value.parse::<u128>()
        .map_err(|_| "Invalid amount format: must be integer in micros".to_string())?;

    // SECURITY: Reject zero amounts
    if amount == 0 {
        return Err("Amount must be greater than zero".to_string());
    }

    if !is_valid_ethereum_address(&request.to) {
        return Err("Invalid Ethereum address format".to_string());
    }

    // v2.0: Only USDC allowed
    if request.token.to_uppercase() != "USDC" {
        return Err("Only USDC is supported in v2.0".to_string());
    }

    if request.chain_id == 0 {
        return Err("Invalid chain ID".to_string());
    }

    let known_chains = [1u64, 8453, 137, 42161, 10, 11155111, 84532];
    if !known_chains.contains(&request.chain_id) {
        return Err("Unsupported chain ID".to_string());
    }

    Ok(())
}

/// Validate Ethereum address format
/// Must be: 42 characters, start with 0x, followed by 40 hex characters
fn is_valid_ethereum_address(addr: &str) -> bool {
    // Check length (0x + 40 hex chars = 42 total)
    if addr.len() != 42 {
        return false;
    }
    
    // Check prefix
    if !addr.starts_with("0x") && !addr.starts_with("0X") {
        return false;
    }
    
    // Check all characters after 0x are valid hex
    addr[2..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Parse transaction ID
fn parse_transaction_id(id: &str) -> Result<TransactionId, String> {
    uuid::Uuid::parse_str(id)
        .map(TransactionId::from)
        .map_err(|e| format!("Invalid transaction ID: {}", e))
}

/// Request body for policy update
#[derive(serde::Deserialize)]
pub struct PolicyUpdateRequest {
    pub key: String,
    pub value: String,
}

/// Request body for policy change
#[derive(serde::Deserialize)]
pub struct PolicyChangeRequestInput {
    /// Policy field to change (e.g., "max_daily", "require_approval_above")
    pub field: String,
    /// New value for the field
    pub new_value: String,
    /// Reason for the change
    #[serde(default)]
    pub reason: String,
    /// Request ID for idempotency
    #[serde(default)]
    pub request_id: Option<String>,
}

/// Request a policy change (Agent proposes, User approves via Telegram)
pub async fn request_policy_change(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(body): Json<PolicyChangeRequestInput>,
) -> impl IntoResponse {
    let request_id = body.request_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Validate API key (policy changes require auth)
    if let Some(key) = extract_api_key(&headers) {
        if !validate_api_key(&state, &key) {
            return error(StatusCode::UNAUTHORIZED, "Invalid API key", request_id).into_response();
        }
    } else {
        return error(StatusCode::UNAUTHORIZED, "API key required", request_id).into_response();
    }

    // Validate field name
    let valid_fields = [
        "max_per_tx", "max_daily", "max_weekly", "auto_add_threshold",
    ];
    if !valid_fields.contains(&body.field.as_str()) {
        return error(
            StatusCode::BAD_REQUEST,
            format!("Invalid field '{}'. Valid fields: {:?}", body.field, valid_fields),
            request_id,
        ).into_response();
    }

    // Get current policy value
    let rules = state.policy_engine.read().await.rules().await;
    let current_value = match body.field.as_str() {
        "max_per_tx" => rules.max_per_tx.to_string(),
        "max_daily" => rules.max_daily.to_string(),
        "max_weekly" => rules.max_weekly.to_string(),
        "auto_add_threshold" => rules.auto_add_threshold.to_string(),
        _ => "unknown".to_string(),
    };

    // Validate the new value
    if let Err(e) = validate_policy_value(&body.field, &body.new_value) {
        return error(StatusCode::BAD_REQUEST, e, request_id).into_response();
    }

    // Create policy change request
    let reason = if body.reason.is_empty() {
        format!("Agent requested change to {}", body.field)
    } else {
        body.reason.clone()
    };
    let policy_request = PolicyChangeRequest::new(
        body.field.clone(),
        current_value,
        body.new_value.clone(),
        reason,
        "api", // agent_id
    );

    // Save to database
    let record = PolicyChangeRecord::new(policy_request.clone());
    if let Err(e) = state.storage.save_policy_change_request(&record).await {
        return error_response(e, request_id).into_response();
    }

    info!(
        policy_change_id = %record.request.id,
        field = %body.field,
        new_value = %body.new_value,
        "Policy change request submitted"
    );

    // Note: Telegram notification will be sent when the bot is running
    // Users can check pending policy changes via /policy command in Telegram

    success(
        serde_json::json!({
            "policy_change_id": record.request.id.to_string(),
            "status": "pending_approval",
            "message": "Policy change request submitted. Check Telegram to approve.",
            "field": body.field,
            "current_value": record.request.current_value,
            "new_value": record.request.new_value,
        }),
        request_id,
    ).into_response()
}

/// Validate a policy field value
fn validate_policy_value(field: &str, value: &str) -> Result<(), String> {
    match field {
        "max_per_tx" | "max_daily" | "max_weekly" | "auto_add_threshold" => {
            // Must be a valid positive u64 integer (USDC micros)
            if value.parse::<u64>().is_err() {
                return Err(format!("{} must be a valid positive integer (USDC micros)", field));
            }
        }
        _ => {}
    }
    Ok(())
}

/// Get pending policy change requests
pub async fn get_pending_policy_changes(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();

    match state.storage.get_pending_policy_change_requests().await {
        Ok(requests) => success(requests, request_id).into_response(),
        Err(e) => error_response(e, request_id).into_response(),
    }
}

/// Get a specific policy change request
pub async fn get_policy_change_request(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();

    let policy_id = match uuid::Uuid::parse_str(&id) {
        Ok(u) => PolicyChangeRequestId::from(u),
        Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid policy change ID", request_id).into_response(),
    };

    match state.storage.get_policy_change_request(policy_id).await {
        Ok(Some(record)) => success(record, request_id).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "Policy change request not found", request_id).into_response(),
        Err(e) => error_response(e, request_id).into_response(),
    }
}

/// Update a single policy value
pub async fn update_policy_value(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(update): Json<PolicyUpdateRequest>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();
    
    // Validate API key
    if let Some(key) = extract_api_key(&headers) {
        if !validate_api_key(&state, &key) {
            return error(StatusCode::UNAUTHORIZED, "Invalid API key", request_id).into_response();
        }
    } else {
        return error(StatusCode::UNAUTHORIZED, "API key required", request_id).into_response();
    }
    
    // Update policy
    let update_result = {
        let engine: tokio::sync::RwLockWriteGuard<'_, crate::policy::PolicyEngine> = state.policy_engine.write().await;
        engine.update_rule(&update.key, &update.value).await
    };

    match update_result {
        Ok(()) => {
            // Save to file
            let engine: tokio::sync::RwLockReadGuard<'_, crate::policy::PolicyEngine> = state.policy_engine.read().await;
            if let Err(e) = engine.save().await {
                return error_response(e, request_id).into_response();
            }
            drop(engine);
            success(serde_json::json!({"updated": true, "key": update.key, "value": update.value}), 
                   request_id).into_response()
        }
        Err(e) => error_response(e, request_id).into_response(),
    }
}

/// Request to load signing keys for testing
#[derive(serde::Deserialize)]
pub struct LoadSigningKeysRequest {
    /// Agent private key (hex, with or without 0x prefix)
    pub agent_key: String,
    /// Steward private key (hex, with or without 0x prefix)
    pub steward_key: String,
}

/// Load signing keys for MPC signing (admin endpoint)
/// This is for testing purposes - in production, keys are loaded from encrypted storage
pub async fn load_signing_keys(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(body): Json<LoadSigningKeysRequest>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();
    
    // Validate API key
    if let Some(key) = extract_api_key(&headers) {
        if !validate_api_key(&state, &key) {
            return error(StatusCode::UNAUTHORIZED, "Invalid API key", request_id).into_response();
        }
    } else {
        return error(StatusCode::UNAUTHORIZED, "API key required", request_id).into_response();
    }
    
    // Load keys into signing coordinator
    match state.signing_coordinator.load_keys_from_hex(&body.steward_key, &body.agent_key, None).await {
        Ok(()) => {
            tracing::info!("Signing keys loaded successfully");
            success(serde_json::json!({"loaded": true, "message": "Agent and Steward keys loaded for MPC signing"}), request_id).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to load signing keys: {}", e);
            error(StatusCode::BAD_REQUEST, &format!("Failed to load keys: {}", e), request_id).into_response()
        }
    }
}

/// Check if signing keys are loaded
pub async fn check_signing_keys(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();
    
    // Validate API key
    if let Some(key) = extract_api_key(&headers) {
        if !validate_api_key(&state, &key) {
            return error(StatusCode::UNAUTHORIZED, "Invalid API key", request_id).into_response();
        }
    } else {
        return error(StatusCode::UNAUTHORIZED, "API key required", request_id).into_response();
    }
    
    let keys_loaded = state.signing_coordinator.is_keys_loaded().await;
    let stats = state.signing_coordinator.stats().await;
    
    success(serde_json::json!({"keys_loaded": keys_loaded, "completed_signatures": stats.completed_signatures}), request_id).into_response()
}

/// Request body for policy change approval with password
#[derive(serde::Deserialize)]
pub struct ApprovePolicyChangeRequest {
    /// User password for verification
    pub password: String,
    /// Whether to approve (true) or reject (false)
    #[serde(default = "default_approve")]
    pub approve: bool,
}

fn default_approve() -> bool {
    true
}

/// Approve or reject a policy change request with password
pub async fn approve_policy_change_request(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(body): Json<ApprovePolicyChangeRequest>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();

    // Parse policy change ID
    let policy_id = match uuid::Uuid::parse_str(&id) {
        Ok(u) => PolicyChangeRequestId::from(u),
        Err(_) => return error(StatusCode::BAD_REQUEST, "Invalid policy change ID", request_id).into_response(),
    };

    // Get the policy change request
    let mut record = match state.storage.get_policy_change_request(policy_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return error(StatusCode::NOT_FOUND, "Policy change request not found", request_id).into_response(),
        Err(e) => return error_response(e, request_id).into_response(),
    };

    // Check if already resolved
    if record.status != crate::types::PolicyChangeStatus::Pending {
        return error(StatusCode::BAD_REQUEST, "Policy change request already processed", request_id).into_response();
    }

    // Verify password
    if !state.storage.verify_user_password(&body.password).await.unwrap_or(false) {
        return error(StatusCode::UNAUTHORIZED, "Invalid password", request_id).into_response();
    }

    if body.approve {
        // Apply the policy change
        let field = record.request.field.clone();
        let new_value = record.request.new_value.clone();

        // Update the policy
        if let Err(e) = state.policy_engine.write().await.update_rule(&field, &new_value).await {
            return error_response(e, request_id).into_response();
        }
        if let Err(e) = state.policy_engine.read().await.save().await {
            return error_response(e, request_id).into_response();
        }

        // Mark as approved
        record.approve("cli".to_string());
        if let Err(e) = state.storage.update_policy_change_request(&record).await {
            return error_response(e, request_id).into_response();
        }

        info!(
            policy_change_id = %policy_id,
            field = %record.request.field,
            new_value = %record.request.new_value,
            "Policy change approved via CLI"
        );

        success(serde_json::json!({
            "approved": true,
            "field": record.request.field,
            "new_value": record.request.new_value
        }), request_id).into_response()
    } else {
        // Mark as rejected
        record.reject("cli".to_string());
        if let Err(e) = state.storage.update_policy_change_request(&record).await {
            return error_response(e, request_id).into_response();
        }

        info!(
            policy_change_id = %policy_id,
            "Policy change rejected via CLI"
        );

        success(serde_json::json!({"rejected": true}), request_id).into_response()
    }
}

/// Request body for wallet creation with password
#[derive(serde::Deserialize)]
pub struct CreateWalletWithPasswordRequest {
    /// Wallet address (computed from CREATE2 or placeholder)
    pub address: String,
    /// Chain ID
    pub chain_id: u64,
    /// Agent key (hex encoded, for user to give to AI)
    pub agent_key: String,
    /// User key (hex encoded, for user backup)
    pub user_key: String,
    /// Email for backup (optional)
    pub email: Option<String>,
    /// Password to encrypt the steward key
    pub password: String,
}

/// Create wallet from CLI with password (stores everything and auto-unlocks)
pub async fn create_wallet_with_password(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(body): Json<CreateWalletWithPasswordRequest>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();

    // Validate API key
    if let Some(key) = extract_api_key(&headers) {
        if !validate_api_key(&state, &key) {
            return error(StatusCode::UNAUTHORIZED, "Invalid API key", request_id).into_response();
        }
    } else if state.config.api.api_key.is_some() {
        return error(StatusCode::UNAUTHORIZED, "API key required", request_id).into_response();
    }

    // Save wallet info to storage using existing method
    // Use agent_key as the public_key placeholder (in production, this would be from DKG)
    if let Err(e) = state.storage.set_wallet(&body.address, body.chain_id, &body.agent_key).await {
        return error_response(e, request_id).into_response();
    }

    // Generate a random steward private key (32 bytes)
    let steward_key_bytes: [u8; 32] = {
        use rand::RngCore;
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        bytes
    };

    // Create an AgentKeyShare from the steward key bytes for encryption
    let steward_key_share = {
        use kamuy_mpc_core::types::{AgentKeyShare, PartyRole};
        use kamuy_mpc_core::utils::math::{bytes_to_scalar, generator, point_mul};

        let secret_share = match bytes_to_scalar(&steward_key_bytes) {
            Ok(s) => s,
            Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Invalid key bytes: {}", e), request_id).into_response(),
        };
        let public_key = point_mul(&generator(), &secret_share);

        AgentKeyShare::new(
            1, // party_id for Steward
            PartyRole::Steward,
            secret_share,
            public_key,
            vec![public_key], // public_shares (just our own)
            [0u8; 32], // chain_code (placeholder)
        )
    };

    // Encrypt and save steward key with password
    let encrypted_steward = match kamuy_mpc_core::encrypt_key_share(&steward_key_share, &body.password) {
        Ok(enc) => enc,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Key encryption failed: {}", e), request_id).into_response(),
    };

    if let Err(e) = state.storage.save_steward_key(&encrypted_steward).await {
        return error_response(e, request_id).into_response();
    }

    // Hash password and save user key with password hash for verification
    use sha3::{Keccak256, Digest};
    let mut hasher = Keccak256::new();
    hasher.update(body.password.as_bytes());
    let password_hash = hex::encode(hasher.finalize());

    // Create a user key share from the user_key string (placeholder for demo)
    let user_key_share = {
        use kamuy_mpc_core::types::{AgentKeyShare, PartyRole};
        use kamuy_mpc_core::utils::math::{bytes_to_scalar, generator, point_mul};

        // Use the provided user_key string as seed for the key share
        let user_key_bytes = hex::decode(body.user_key.trim_start_matches("0x"))
            .unwrap_or_else(|_| body.user_key.as_bytes().to_vec());
        let mut bytes = [0u8; 32];
        bytes[..user_key_bytes.len().min(32)].copy_from_slice(&user_key_bytes[..user_key_bytes.len().min(32)]);

        let secret_share = match bytes_to_scalar(&bytes) {
            Ok(s) => s,
            Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Invalid user key: {}", e), request_id).into_response(),
        };
        let public_key = point_mul(&generator(), &secret_share);

        AgentKeyShare::new(
            2, // party_id for User
            PartyRole::User,
            secret_share,
            public_key,
            vec![public_key],
            [0u8; 32],
        )
    };

    let encrypted_user = match kamuy_mpc_core::encrypt_key_share(&user_key_share, &body.password) {
        Ok(enc) => enc,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, &format!("User key encryption failed: {}", e), request_id).into_response(),
    };

    if let Err(e) = state.storage.save_user_key(&encrypted_user, &password_hash).await {
        return error_response(e, request_id).into_response();
    }

    // Load keys into signing coordinator for immediate signing capability
    if let Err(e) = state.signing_coordinator.load_keys_from_hex(
        &hex::encode(steward_key_bytes),
        &body.agent_key,
        Some(&body.user_key),
    ).await {
        return error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Failed to load signing keys: {}", e), request_id).into_response();
    }

    // Mark the key as loaded in state
    {
        let mut key_guard = state.key_share.write().await;
        *key_guard = Some(steward_key_share);
    }

    info!(
        address = %body.address,
        chain_id = body.chain_id,
        "Wallet created and unlocked from CLI"
    );

    success(serde_json::json!({
        "address": body.address,
        "chain_id": body.chain_id,
        "created": true,
        "unlocked": true
    }), request_id).into_response()
}

/// Request body for unlock
#[derive(serde::Deserialize)]
pub struct UnlockRequest {
    /// User password to decrypt the steward key
    pub password: String,
}

/// Unlock the steward key (load into memory)
pub async fn unlock_steward(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(body): Json<UnlockRequest>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();

    // Validate API key
    if let Some(key) = extract_api_key(&headers) {
        if !validate_api_key(&state, &key) {
            return error(StatusCode::UNAUTHORIZED, "Invalid API key", request_id).into_response();
        }
    } else if state.config.api.api_key.is_some() {
        return error(StatusCode::UNAUTHORIZED, "API key required", request_id).into_response();
    }

    // Load the key share
    match state.load_key_share(&body.password).await {
        Ok(()) => {
            info!("Steward key unlocked successfully");
            success(serde_json::json!({"unlocked": true}), request_id).into_response()
        }
        Err(e) => {
            warn!("Failed to unlock steward key: {}", e);
            error(StatusCode::UNAUTHORIZED, &format!("Unlock failed: {}", e), request_id).into_response()
        }
    }
}

/// Check if steward key is loaded
pub async fn check_unlocked(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();

    // Validate API key
    if let Some(key) = extract_api_key(&headers) {
        if !validate_api_key(&state, &key) {
            return error(StatusCode::UNAUTHORIZED, "Invalid API key", request_id).into_response();
        }
    } else if state.config.api.api_key.is_some() {
        return error(StatusCode::UNAUTHORIZED, "API key required", request_id).into_response();
    }

    let is_loaded = state.is_key_loaded().await;
    success(serde_json::json!({"key_loaded": is_loaded}), request_id).into_response()
}

/// Request body for transaction approval with password
#[derive(serde::Deserialize)]
pub struct ApproveTransactionWithPasswordRequest {
    /// User password for verification
    pub password: String,
    /// Whether to approve (true) or reject (false)
    #[serde(default = "default_approve")]
    pub approve: bool,
}

/// Approve or reject a transaction with password (for TerminalPassword approval level)
pub async fn approve_transaction_with_password(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(body): Json<ApproveTransactionWithPasswordRequest>,
) -> impl IntoResponse {
    let request_id = uuid::Uuid::new_v4().to_string();

    // Parse transaction ID
    let tx_id = match parse_transaction_id(&id) {
        Ok(id) => id,
        Err(e) => return error(StatusCode::BAD_REQUEST, e, request_id).into_response(),
    };

    // Verify password
    if !state.storage.verify_user_password(&body.password).await.unwrap_or(false) {
        return error(StatusCode::UNAUTHORIZED, "Invalid password", request_id).into_response();
    }

    // Process approval/rejection
    match crate::queue::processor::handle_user_approval(
        &state,
        tx_id,
        body.approve,
        "cli".to_string(),
    ).await {
        Ok(()) => {
            if body.approve {
                success(serde_json::json!({"approved": true}), request_id).into_response()
            } else {
                success(serde_json::json!({"rejected": true}), request_id).into_response()
            }
        }
        Err(e) => error_response(e, request_id).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_request() -> SubmitTransactionRequest {
        SubmitTransactionRequest {
            to: "0x1234567890123456789012345678901234567890".to_string(),
            value: "100000000".to_string(), // 1 USDC in micros
            token: "USDC".to_string(),
            chain_id: 1,
            request_id: None,
            wait: true,
            timeout_secs: None,
        }
    }

    #[test]
    fn test_validate_transaction_fields_valid() {
        let request = valid_request();
        assert!(validate_transaction_fields(&request).is_ok());
    }

    #[test]
    fn test_validate_usdc_case_insensitive() {
        let mut request = valid_request();
        request.token = "usdc".to_string();
        assert!(validate_transaction_fields(&request).is_ok());

        request.token = "UsDc".to_string();
        assert!(validate_transaction_fields(&request).is_ok());
    }

    #[test]
    fn test_validate_rejects_non_usdc() {
        let mut request = valid_request();
        request.token = "USDT".to_string();
        assert!(validate_transaction_fields(&request).is_err());

        request.token = "DAI".to_string();
        assert!(validate_transaction_fields(&request).is_err());

        request.token = "ETH".to_string();
        assert!(validate_transaction_fields(&request).is_err());
    }

    #[test]
    fn test_validate_rejects_zero_amount() {
        let mut request = valid_request();
        request.value = "0".to_string();
        let result = validate_transaction_fields(&request);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("greater than zero"));
    }

    #[test]
    fn test_validate_rejects_empty_amount() {
        let mut request = valid_request();
        request.value = "".to_string();
        let result = validate_transaction_fields(&request);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Amount required"));
    }

    #[test]
    fn test_validate_rejects_invalid_amount_format() {
        let mut request = valid_request();
        request.value = "not_a_number".to_string();
        let result = validate_transaction_fields(&request);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be integer"));
    }

    #[test]
    fn test_validate_rejects_invalid_address() {
        let mut request = valid_request();
        request.to = "invalid".to_string();
        assert!(validate_transaction_fields(&request).is_err());
    }
}
