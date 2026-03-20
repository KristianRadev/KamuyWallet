//! Wallet state and management

use crate::policy::{PolicyDecision, PolicyEngine};
use crate::transaction::{Transaction, TransactionStatus};
use kamuy_mpc_core::{AgentKeyShare, PartyRole, PublicKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Wallet configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConfig {
    /// Wallet ID
    pub wallet_id: String,
    /// Default chain ID
    pub default_chain_id: u64,
    /// Supported chains
    pub supported_chains: Vec<u64>,
    /// Policy configuration
    pub policy: crate::policy::PolicyConfig,
}

impl Default for WalletConfig {
    fn default() -> Self {
        Self {
            wallet_id: uuid::Uuid::new_v4().to_string(),
            default_chain_id: 8453, // Base
            supported_chains: vec![8453, 137, 42161, 10, 1], // Base, Polygon, Arbitrum, Optimism, Ethereum
            policy: crate::policy::PolicyConfig::default(),
        }
    }
}

/// Wallet state
#[derive(Debug, Clone)]
pub struct WalletState {
    /// Current nonce for each chain
    pub nonces: HashMap<u64, u64>,
    /// Pending transactions
    pub pending_transactions: Vec<Transaction>,
    /// Transaction history
    pub transaction_history: Vec<Transaction>,
}

impl Default for WalletState {
    fn default() -> Self {
        Self {
            nonces: HashMap::new(),
            pending_transactions: Vec::new(),
            transaction_history: Vec::new(),
        }
    }
}

/// Main wallet struct
#[derive(Debug)]
pub struct Wallet {
    /// Wallet configuration
    pub config: WalletConfig,
    /// Key share (if loaded)
    pub key_share: Option<AgentKeyShare>,
    /// Policy engine
    pub policy_engine: PolicyEngine,
    /// Wallet state
    pub state: WalletState,
}

impl Wallet {
    /// Create a new wallet
    pub fn new(config: WalletConfig) -> Self {
        let policy_engine = PolicyEngine::new(config.policy.clone());

        Self {
            config,
            key_share: None,
            policy_engine,
            state: WalletState::default(),
        }
    }

    /// Load a key share
    pub fn load_key_share(&mut self, key_share: AgentKeyShare) {
        self.key_share = Some(key_share);
    }

    /// Get the wallet address
    pub fn address(&self) -> Option<String> {
        self.key_share.as_ref()?.eth_address().ok()
    }

    /// Get the public key
    pub fn public_key(&self) -> Option<&PublicKey> {
        self.key_share.as_ref().map(|ks| &ks.public_key)
    }

    /// Check if wallet has a loaded key share
    pub fn is_loaded(&self) -> bool {
        self.key_share.is_some()
    }

    /// Get the party role
    pub fn role(&self) -> Option<PartyRole> {
        self.key_share.as_ref().map(|ks| ks.role)
    }

    /// Validate an Ethereum address
    fn validate_address(address: &str) -> Result<String, String> {
        let address = address.trim();

        // Check prefix
        if !address.starts_with("0x") {
            return Err("Address must start with 0x".to_string());
        }

        // Check length (40 hex chars + 0x prefix = 42)
        if address.len() != 42 {
            return Err("Address must be 42 characters (0x + 40 hex)".to_string());
        }

        // Validate hex characters
        let hex_part = &address[2..];
        if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("Address contains invalid hex characters".to_string());
        }

        Ok(address.to_lowercase())
    }

    /// Validate transaction amount
    fn validate_amount(amount: &str) -> Result<f64, String> {
        let amount = amount.trim();

        if amount.is_empty() {
            return Err("Amount cannot be empty".to_string());
        }

        // Try to parse as float
        let parsed: f64 = amount
            .replace(',', "")
            .parse()
            .map_err(|_| "Invalid amount format".to_string())?;

        if parsed < 0.0 {
            return Err("Amount cannot be negative".to_string());
        }

        if parsed.is_infinite() || parsed.is_nan() {
            return Err("Invalid amount".to_string());
        }

        // Check for reasonable precision (max 18 decimal places for ETH)
        let parts: Vec<&str> = amount.split('.').collect();
        if parts.len() > 1 && parts[1].len() > 18 {
            return Err("Amount has too many decimal places (max 18)".to_string());
        }

        Ok(parsed)
    }

    /// Validate token symbol
    fn validate_token(token: &str) -> Result<String, String> {
        let token = token.trim();

        if token.is_empty() {
            return Err("Token cannot be empty".to_string());
        }

        if token.len() > 20 {
            return Err("Token symbol too long (max 20 chars)".to_string());
        }

        if !token.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err("Token must be alphanumeric".to_string());
        }

        Ok(token.to_uppercase())
    }

    /// Validate chain ID
    fn validate_chain_id(chain_id: u64) -> Result<u64, String> {
        // Known chain IDs
        let valid_chains = [1, 10, 137, 42161, 8453, 84532, 421614];

        if !valid_chains.contains(&chain_id) {
            // Allow unknown but warn
            tracing::warn!("Unknown chain ID: {}", chain_id);
        }

        Ok(chain_id)
    }

    /// Create a transaction (does not sign) - with validation
    pub fn create_transaction(
        &self,
        to: impl Into<String>,
        amount: impl Into<String>,
        token: impl Into<String>,
    ) -> anyhow::Result<Transaction> {
        let to = to.into();
        let amount = amount.into();
        let token = token.into();
        let chain_id = self.config.default_chain_id;

        // Validate all inputs before creating transaction
        let validated_to = Self::validate_address(&to).map_err(|e| anyhow::anyhow!("Invalid address: {}", e))?;
        let _validated_amount = Self::validate_amount(&amount).map_err(|e| anyhow::anyhow!("Invalid amount: {}", e))?;
        let validated_token = Self::validate_token(&token).map_err(|e| anyhow::anyhow!("Invalid token: {}", e))?;
        let validated_chain = Self::validate_chain_id(chain_id).map_err(|e| anyhow::anyhow!("Invalid chain: {}", e))?;

        crate::transaction::TransactionBuilder::new()
            .to(validated_to)
            .amount(amount)
            .token(validated_token)
            .chain_id(validated_chain)
            .build()
    }

    /// Submit a transaction for signing (with policy evaluation)
    pub async fn submit_transaction(&mut self, tx: Transaction) -> anyhow::Result<Transaction> {
        // Evaluate policy using the policy engine
        let decision = self.policy_engine.evaluate(&tx).await;

        match decision {
            PolicyDecision::Approve => {
                // Auto-approve, proceed with signing
                let mut tx = tx;
                tx.set_status(TransactionStatus::Approved);
                self.state.pending_transactions.push(tx.clone());
                Ok(tx)
            }
            PolicyDecision::Reject { reason } => {
                anyhow::bail!("Transaction rejected: {}", reason)
            }
            PolicyDecision::RequireAdditionalApproval { reason } => {
                let mut tx = tx;
                tx.set_status(TransactionStatus::Pending);
                self.state.pending_transactions.push(tx.clone());
                // TODO: Send notification for approval
                tracing::info!("Transaction pending approval: {}", reason);
                Ok(tx)
            }
        }
    }

    /// Submit a transaction synchronously (non-async version)
    pub fn submit_transaction_sync(&mut self, tx: Transaction) -> anyhow::Result<Transaction> {
        // Evaluate policy synchronously
        let decision = self.policy_engine.evaluate_sync(&tx);

        match decision {
            PolicyDecision::Approve => {
                // Auto-approve, proceed with signing
                let mut tx = tx;
                tx.set_status(TransactionStatus::Approved);
                self.state.pending_transactions.push(tx.clone());
                Ok(tx)
            }
            PolicyDecision::Reject { reason } => {
                anyhow::bail!("Transaction rejected: {}", reason)
            }
            PolicyDecision::RequireAdditionalApproval { reason } => {
                let mut tx = tx;
                tx.set_status(TransactionStatus::Pending);
                self.state.pending_transactions.push(tx.clone());
                tracing::info!("Transaction pending approval: {}", reason);
                Ok(tx)
            }
        }
    }

    /// Get pending transactions
    pub fn pending_transactions(&self) -> &[Transaction] {
        &self.state.pending_transactions
    }

    /// Get transaction history
    pub fn transaction_history(&self) -> &[Transaction] {
        &self.state.transaction_history
    }
}

impl Default for Wallet {
    fn default() -> Self {
        Self::new(WalletConfig::default())
    }
}
