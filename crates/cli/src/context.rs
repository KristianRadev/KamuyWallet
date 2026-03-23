//! # CLI Context
//!
//! Shared context for CLI commands including Steward client.

use crate::config::CliConfig;
use anyhow::{Context, Result};

/// CLI context shared across commands
pub struct CliContext {
    /// Configuration
    pub config: CliConfig,
    /// Steward client
    pub steward: StewardClient,
}

impl CliContext {
    /// Create new CLI context
    pub async fn new(config: CliConfig) -> Result<Self> {
        let steward = StewardClient::new(&config.steward_url, config.api_key.clone());

        Ok(Self {
            config,
            steward,
        })
    }

    /// Check if user key exists
    pub fn has_user_key(&self) -> bool {
        self.config.user_key_path().exists()
    }
}

/// Steward API client
pub struct StewardClient {
    /// Base URL
    base_url: String,
    /// HTTP client
    client: reqwest::Client,
    /// API key
    api_key: Option<String>,
}

impl StewardClient {
    /// Create new Steward client with timeout configuration
    pub fn new(base_url: &str, api_key: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
            api_key,
        }
    }

    /// Build request with auth
    fn build_request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.request(method, &url);

        if let Some(key) = &self.api_key {
            req = req.header("x-api-key", key);
        }

        req
    }

    /// Health check
    pub async fn health(&self) -> Result<StewardHealth> {
        let resp = self.build_request(reqwest::Method::GET, "/health")
            .send()
            .await
            .context("Failed to connect to Steward service")?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Steward health check failed: {}", resp.status()));
        }

        let health: StewardHealth = resp.json().await
            .context("Failed to parse health response")?;

        Ok(health)
    }

    /// Get transaction
    /// SECURITY: Validates TX ID format before making request
    pub async fn get_transaction(
        &self,
        id: &str,
    ) -> Result<Option<kamuy_steward::types::TransactionRecord>> {
        // Validate TX ID is valid UUID format
        if !is_valid_uuid(id) {
            return Err(anyhow::anyhow!("Invalid transaction ID format: must be valid UUID"));
        }

        let resp = self.build_request(reqwest::Method::GET, &format!("/api/v1/transactions/{}", id))
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get transaction: {}", resp.status()));
        }

        let response: kamuy_steward::types::ApiResponse<kamuy_steward::types::TransactionRecord> =
            resp.json().await?;

        Ok(response.data)
    }

    /// Get pending transactions
    pub async fn get_pending(&self) -> Result<Vec<kamuy_steward::types::TransactionRecord>> {
        let resp = self.build_request(reqwest::Method::GET, "/api/v1/transactions/pending")
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get pending: {}", resp.status()));
        }

        let response: kamuy_steward::types::ApiResponse<Vec<kamuy_steward::types::TransactionRecord>> =
            resp.json().await?;

        Ok(response.data.unwrap_or_default())
    }

    /// Approve transaction
    /// SECURITY: Validates TX ID format before making request
    pub async fn approve_transaction(&self, id: &str) -> Result<()> {
        // Validate TX ID is valid UUID format
        if !is_valid_uuid(id) {
            return Err(anyhow::anyhow!("Invalid transaction ID format: must be valid UUID"));
        }

        let resp = self.build_request(
            reqwest::Method::POST,
            &format!("/api/v1/transactions/{}/approve", id)
        )
            .send()
            .await?;

        if !resp.status().is_success() {
            let err: StewardError = resp.json().await?;
            return Err(anyhow::anyhow!("Approval failed: {}", err.error));
        }

        Ok(())
    }

    /// Reject transaction
    /// SECURITY: Validates TX ID format before making request
    pub async fn reject_transaction(&self, id: &str) -> Result<()> {
        // Validate TX ID is valid UUID format
        if !is_valid_uuid(id) {
            return Err(anyhow::anyhow!("Invalid transaction ID format: must be valid UUID"));
        }

        let resp = self.build_request(
            reqwest::Method::POST,
            &format!("/api/v1/transactions/{}/reject", id)
        )
            .send()
            .await?;

        if !resp.status().is_success() {
            let err: StewardError = resp.json().await?;
            return Err(anyhow::anyhow!("Rejection failed: {}", err.error));
        }

        Ok(())
    }

    /// Approve transaction with password (TerminalPassword approval level)
    /// SECURITY: Validates TX ID format before making request
    pub async fn approve_transaction_with_password(&self, id: &str, password: &str) -> Result<()> {
        // Validate TX ID is valid UUID format
        if !is_valid_uuid(id) {
            return Err(anyhow::anyhow!("Invalid transaction ID format: must be valid UUID"));
        }

        let resp = self.build_request(
            reqwest::Method::POST,
            &format!("/api/v1/transactions/{}/approve-with-password", id)
        )
            .json(&serde_json::json!({ "password": password, "approve": true }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let err: StewardError = resp.json().await?;
            return Err(anyhow::anyhow!("Approval failed: {}", err.error));
        }

        Ok(())
    }

    /// Get a specific policy change request
    pub async fn get_policy_change_request(&self, id: &str) -> Result<Option<kamuy_steward::types::PolicyChangeRecord>> {
        // Validate ID is valid UUID format
        if !is_valid_uuid(id) {
            return Err(anyhow::anyhow!("Invalid policy change ID format: must be valid UUID"));
        }

        let resp = self.build_request(reqwest::Method::GET, &format!("/api/v1/policy/requests/{}", id))
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get policy change request: {}", resp.status()));
        }

        let response: kamuy_steward::types::ApiResponse<kamuy_steward::types::PolicyChangeRecord> =
            resp.json().await?;

        Ok(response.data)
    }

    /// Approve policy change request with password (TerminalPassword approval level)
    pub async fn approve_policy_change(&self, id: &str, password: &str) -> Result<()> {
        // Validate ID is valid UUID format
        if !is_valid_uuid(id) {
            return Err(anyhow::anyhow!("Invalid policy change ID format: must be valid UUID"));
        }

        let resp = self.build_request(
            reqwest::Method::POST,
            &format!("/api/v1/policy/requests/{}/approve", id)
        )
            .json(&serde_json::json!({ "password": password, "approve": true }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let err: StewardError = resp.json().await?;
            return Err(anyhow::anyhow!("Policy change approval failed: {}", err.error));
        }

        Ok(())
    }

    /// Get policy
    pub async fn get_policy(&self) -> Result<kamuy_steward::policy::PolicyRules> {
        let resp = self.build_request(reqwest::Method::GET, "/api/v1/policy")
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get policy: {}", resp.status()));
        }

        let response: kamuy_steward::types::ApiResponse<kamuy_steward::policy::PolicyRules> =
            resp.json().await?;

        response.data.ok_or_else(|| anyhow::anyhow!("No data in response"))
    }

    /// Update policy
    pub async fn update_policy(&self, rules: &kamuy_steward::policy::PolicyRules) -> Result<()> {
        let resp = self.build_request(reqwest::Method::PUT, "/api/v1/policy")
            .json(rules)
            .send()
            .await?;

        if !resp.status().is_success() {
            let err: StewardError = resp.json().await?;
            return Err(anyhow::anyhow!("Policy update failed: {}", err.error));
        }

        Ok(())
    }

    /// Get wallet info
    pub async fn get_wallet(&self) -> Result<Option<kamuy_steward::storage::WalletInfo>> {
        let resp = self.build_request(reqwest::Method::GET, "/api/v1/wallet")
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get wallet: {}", resp.status()));
        }

        let response: kamuy_steward::types::ApiResponse<kamuy_steward::storage::WalletInfo> =
            resp.json().await?;

        Ok(response.data)
    }

    /// Get balances
    pub async fn get_balances(&self) -> Result<std::collections::HashMap<String, String>> {
        let resp = self.build_request(reqwest::Method::GET, "/api/v1/balances")
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get balances: {}", resp.status()));
        }

        let response: kamuy_steward::types::ApiResponse<std::collections::HashMap<String, String>> =
            resp.json().await?;

        Ok(response.data.unwrap_or_default())
    }

    /// Unlock wallet (load Steward key)
    pub async fn unlock(&self, password: &str) -> Result<()> {
        let resp = self.build_request(reqwest::Method::POST, "/api/v1/unlock")
            .json(&serde_json::json!({ "password": password }))
            .send()
            .await
            .context("Failed to connect to Steward service")?;

        if !resp.status().is_success() {
            let err: StewardError = resp.json().await?;
            return Err(anyhow::anyhow!("Unlock failed: {}", err.error));
        }

        Ok(())
    }

    /// Create wallet with password (stores encrypted steward key and auto-unlocks)
    pub async fn create_wallet(
        &self,
        address: &str,
        chain_id: u64,
        agent_key: &str,
        user_key: &str,
        email: Option<&str>,
        password: &str,
    ) -> Result<CreateWalletResponse> {
        let resp = self.build_request(reqwest::Method::POST, "/api/v1/wallet/create")
            .json(&serde_json::json!({
                "address": address,
                "chain_id": chain_id,
                "agent_key": agent_key,
                "user_key": user_key,
                "email": email,
                "password": password,
            }))
            .send()
            .await
            .context("Failed to connect to Steward service")?;

        if !resp.status().is_success() {
            // Try to parse error message from response body
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());

            // Try to extract error from JSON response
            let error_msg = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body_text) {
                json.get("error")
                    .and_then(|e| e.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("HTTP {} - {}", status, body_text))
            } else {
                format!("HTTP {} - {}", status, body_text)
            };

            return Err(anyhow::anyhow!("Wallet creation failed: {}", error_msg));
        }

        // Parse response
        let response: ApiResponse<CreateWalletResponse> = resp.json().await
            .context("Failed to parse wallet creation response")?;

        response.data.ok_or_else(|| anyhow::anyhow!("No data in response"))
    }

    /// Check if steward key is loaded (wallet is unlocked)
    pub async fn is_unlocked(&self) -> Result<bool> {
        let resp = self.build_request(reqwest::Method::GET, "/api/v1/unlock")
            .send()
            .await
            .context("Failed to connect to Steward service")?;

        if !resp.status().is_success() {
            return Ok(false);
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(result["data"]["key_loaded"].as_bool().unwrap_or(false))
    }

    /// Get recovery key with password authentication
    /// SECURITY: Password is required - user key is never stored on disk
    pub async fn get_recovery_key(&self, password: &str) -> Result<String> {
        let resp = self.build_request(reqwest::Method::POST, "/api/v1/recovery-key")
            .json(&serde_json::json!({ "password": password }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let err: StewardError = resp.json().await?;
            return Err(anyhow::anyhow!("Failed to get recovery key: {}", err.error));
        }

        let response: ApiResponse<RecoveryKeyResponse> = resp.json().await?;
        response.data
            .map(|d| d.user_key)
            .ok_or_else(|| anyhow::anyhow!("No data in response"))
    }

    /// Get agent key with password authentication
    /// SECURITY: Password is required - ensures only wallet owner can retrieve agent config
    pub async fn get_agent_key(&self, password: &str) -> Result<String> {
        let resp = self.build_request(reqwest::Method::POST, "/api/v1/agent-key")
            .json(&serde_json::json!({ "password": password }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let err: StewardError = resp.json().await?;
            return Err(anyhow::anyhow!("Failed to get agent key: {}", err.error));
        }

        let response: ApiResponse<AgentKeyResponse> = resp.json().await?;
        response.data
            .map(|d| d.agent_key)
            .ok_or_else(|| anyhow::anyhow!("No data in response"))
    }
}

/// Steward health response
#[derive(Debug, serde::Deserialize)]
pub struct StewardHealth {
    pub status: String,
    pub version: String,
}

/// Steward error response
#[derive(Debug, serde::Deserialize)]
pub struct StewardError {
    pub error: String,
}

/// API response wrapper
#[derive(Debug, serde::Deserialize)]
pub struct ApiResponse<T> {
    pub data: Option<T>,
    pub request_id: Option<String>,
}

/// Response from wallet creation
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CreateWalletResponse {
    pub address: String,
    pub chain_id: u64,
    pub created: bool,
    pub unlocked: bool,
    pub email_backup: Option<EmailBackupResult>,
}

/// Email backup result from wallet creation
#[derive(Debug, Clone, serde::Deserialize)]
pub struct EmailBackupResult {
    pub sent: bool,
    pub message: String,
}

/// Response from recovery key retrieval
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RecoveryKeyResponse {
    pub user_key: String,
}

/// Response from agent key retrieval
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AgentKeyResponse {
    pub agent_key: String,
}

/// Validate UUID format
/// SECURITY: Prevents injection attacks via TX ID
fn is_valid_uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok()
}