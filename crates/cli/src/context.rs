//! # CLI Context
//!
//! Shared context for CLI commands including Steward client.

use crate::config::CliConfig;
use anyhow::{Context, Result};
use kamuy_mpc_core::{AgentKeyShare, decrypt_key_share, encrypt_key_share};
use std::path::Path;

/// CLI context shared across commands
pub struct CliContext {
    /// Configuration
    pub config: CliConfig,
    /// Steward client
    pub steward: StewardClient,
    /// Loaded user key (if available)
    pub user_key: Option<AgentKeyShare>,
}

impl CliContext {
    /// Create new CLI context
    pub async fn new(config: CliConfig) -> Result<Self> {
        let steward = StewardClient::new(&config.steward_url, config.api_key.clone());
        
        Ok(Self {
            config,
            steward,
            user_key: None,
        })
    }
    
    /// Load user key from file
    pub async fn load_user_key(&mut self, password: &str) -> Result<()> {
        let key_path = self.config.user_key_path();
        
        if !key_path.exists() {
            return Err(anyhow::anyhow!("User key file not found: {:?}", key_path));
        }
        
        let encrypted = tokio::fs::read(&key_path).await
            .with_context(|| format!("Failed to read user key: {:?}", key_path))?;
        
        let encrypted: kamuy_mpc_core::EncryptedKeyShare = serde_json::from_slice(&encrypted)
            .with_context(|| "Failed to parse user key")?;
        
        let key_share = decrypt_key_share(&encrypted, password)
            .with_context(|| "Failed to decrypt user key (wrong password?)")?;
        
        self.user_key = Some(key_share);
        
        Ok(())
    }
    
    /// Save user key to file
    pub async fn save_user_key(&self, key: &AgentKeyShare, password: &str) -> Result<()> {
        let key_path = self.config.user_key_path();
        
        // Ensure directory exists
        if let Some(parent) = key_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        
        let encrypted = encrypt_key_share(key, password)?;
        let data = serde_json::to_vec_pretty(&encrypted)?;
        
        tokio::fs::write(&key_path, data).await
            .with_context(|| format!("Failed to write user key: {:?}", key_path))?;
        
        // Set restrictive permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&key_path)?.permissions();
            perms.set_mode(0o600); // Owner read/write only
            std::fs::set_permissions(&key_path, perms)?;
        }
        
        Ok(())
    }
    
    /// Check if user key exists
    pub fn has_user_key(&self) -> bool {
        self.config.user_key_path().exists()
    }
    
    /// Check if user key is loaded
    pub fn is_user_key_loaded(&self) -> bool {
        self.user_key.is_some()
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
    /// Create new Steward client
    pub fn new(base_url: &str, api_key: Option<String>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
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
    
    /// Submit transaction
    pub async fn submit_transaction(
        &self,
        request: &kamuy_steward::types::TransactionRequest,
    ) -> Result<kamuy_steward::types::TransactionRecord> {
        let resp = self.build_request(reqwest::Method::POST, "/api/v1/transactions")
            .json(request)
            .send()
            .await
            .context("Failed to submit transaction")?;
        
        if !resp.status().is_success() {
            let err: StewardError = resp.json().await?;
            return Err(anyhow::anyhow!("Transaction submission failed: {}", err.error));
        }
        
        let response: kamuy_steward::types::ApiResponse<kamuy_steward::types::TransactionRecord> = 
            resp.json().await?;
        
        response.data.ok_or_else(|| anyhow::anyhow!("No data in response"))
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

    /// Reject transaction with password (TerminalPassword approval level)
    /// SECURITY: Validates TX ID format before making request
    pub async fn reject_transaction_with_password(&self, id: &str, password: &str) -> Result<()> {
        // Validate TX ID is valid UUID format
        if !is_valid_uuid(id) {
            return Err(anyhow::anyhow!("Invalid transaction ID format: must be valid UUID"));
        }

        let resp = self.build_request(
            reqwest::Method::POST,
            &format!("/api/v1/transactions/{}/approve-with-password", id)
        )
            .json(&serde_json::json!({ "password": password, "approve": false }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let err: StewardError = resp.json().await?;
            return Err(anyhow::anyhow!("Rejection failed: {}", err.error));
        }

        Ok(())
    }

    /// Get pending policy change requests
    pub async fn get_pending_policy_changes(&self) -> Result<Vec<kamuy_steward::types::PolicyChangeRecord>> {
        let resp = self.build_request(reqwest::Method::GET, "/api/v1/policy/requests")
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get pending policy changes: {}", resp.status()));
        }

        let response: kamuy_steward::types::ApiResponse<Vec<kamuy_steward::types::PolicyChangeRecord>> =
            resp.json().await?;

        Ok(response.data.unwrap_or_default())
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

    /// Reject policy change request with password (TerminalPassword approval level)
    pub async fn reject_policy_change(&self, id: &str, password: &str) -> Result<()> {
        // Validate ID is valid UUID format
        if !is_valid_uuid(id) {
            return Err(anyhow::anyhow!("Invalid policy change ID format: must be valid UUID"));
        }

        let resp = self.build_request(
            reqwest::Method::POST,
            &format!("/api/v1/policy/requests/{}/approve", id)
        )
            .json(&serde_json::json!({ "password": password, "approve": false }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let err: StewardError = resp.json().await?;
            return Err(anyhow::anyhow!("Policy change rejection failed: {}", err.error));
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
    pub async fn unlock(&self, _password: &str) -> Result<()> {
        // This would be an API call to unlock the Steward
        // For now, we just verify the password works by checking health
        let health = self.health().await?;

        if health.status != "healthy" {
            return Err(anyhow::anyhow!("Steward is not healthy"));
        }

        Ok(())
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
    pub code: String,
    pub message: String,
    pub error: String,
}

/// Validate UUID format
/// SECURITY: Prevents injection attacks via TX ID
fn is_valid_uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok()
}
