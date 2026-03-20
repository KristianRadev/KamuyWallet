//! Transaction types and builder

use serde::{Deserialize, Serialize};

/// Transaction status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionStatus {
    /// Pending approval
    Pending,
    /// Approved, waiting to be signed
    Approved,
    /// Signing in progress
    Signing,
    /// Signed, waiting to be broadcast
    Signed,
    /// Broadcast to network
    Broadcast,
    /// Confirmed on chain
    Confirmed,
    /// Failed
    Failed,
    /// Rejected by policy
    Rejected,
}

/// Transaction request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Transaction ID
    pub id: String,
    /// Request ID
    pub request_id: String,
    /// Destination address
    pub to: String,
    /// Amount
    pub amount: String,
    /// Token symbol
    pub token: String,
    /// Chain ID
    pub chain_id: u64,
    /// Nonce
    pub nonce: u64,
    /// Gas price
    pub gas_price: Option<String>,
    /// Gas limit
    pub gas_limit: Option<u64>,
    /// Data payload
    pub data: Option<Vec<u8>>,
    /// Status
    pub status: TransactionStatus,
    /// Created timestamp
    pub created_at: i64,
    /// Updated timestamp
    pub updated_at: i64,
}

impl Transaction {
    /// Create a new transaction
    pub fn new(
        id: impl Into<String>,
        request_id: impl Into<String>,
        to: impl Into<String>,
        amount: impl Into<String>,
        token: impl Into<String>,
        chain_id: u64,
    ) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id: id.into(),
            request_id: request_id.into(),
            to: to.into(),
            amount: amount.into(),
            token: token.into(),
            chain_id,
            nonce: 0,
            gas_price: None,
            gas_limit: None,
            data: None,
            status: TransactionStatus::Pending,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update status
    pub fn set_status(&mut self, status: TransactionStatus) {
        self.status = status;
        self.updated_at = chrono::Utc::now().timestamp();
    }
}

/// Transaction builder
#[derive(Debug, Clone)]
pub struct TransactionBuilder {
    to: Option<String>,
    amount: Option<String>,
    token: Option<String>,
    chain_id: Option<u64>,
    nonce: Option<u64>,
    gas_price: Option<String>,
    gas_limit: Option<u64>,
    data: Option<Vec<u8>>,
}

impl TransactionBuilder {
    /// Create a new transaction builder
    pub fn new() -> Self {
        Self {
            to: None,
            amount: None,
            token: None,
            chain_id: None,
            nonce: None,
            gas_price: None,
            gas_limit: None,
            data: None,
        }
    }

    /// Set destination address
    pub fn to(mut self, to: impl Into<String>) -> Self {
        self.to = Some(to.into());
        self
    }

    /// Set amount
    pub fn amount(mut self, amount: impl Into<String>) -> Self {
        self.amount = Some(amount.into());
        self
    }

    /// Set token
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Set chain ID
    pub fn chain_id(mut self, chain_id: u64) -> Self {
        self.chain_id = Some(chain_id);
        self
    }

    /// Set nonce
    pub fn nonce(mut self, nonce: u64) -> Self {
        self.nonce = Some(nonce);
        self
    }

    /// Set gas price
    pub fn gas_price(mut self, gas_price: impl Into<String>) -> Self {
        self.gas_price = Some(gas_price.into());
        self
    }

    /// Set gas limit
    pub fn gas_limit(mut self, gas_limit: u64) -> Self {
        self.gas_limit = Some(gas_limit);
        self
    }

    /// Set data
    pub fn data(mut self, data: Vec<u8>) -> Self {
        self.data = Some(data);
        self
    }

    /// Build the transaction
    pub fn build(self) -> anyhow::Result<Transaction> {
        let id = uuid::Uuid::new_v4().to_string();
        let request_id = uuid::Uuid::new_v4().to_string();

        Ok(Transaction::new(
            id,
            request_id,
            self.to.ok_or_else(|| anyhow::anyhow!("Missing 'to' address"))?,
            self.amount.ok_or_else(|| anyhow::anyhow!("Missing amount"))?,
            self.token.ok_or_else(|| anyhow::anyhow!("Missing token"))?,
            self.chain_id.ok_or_else(|| anyhow::anyhow!("Missing chain_id"))?,
        ))
    }
}

impl Default for TransactionBuilder {
    fn default() -> Self {
        Self::new()
    }
}
