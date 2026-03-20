//! # Kamuy Steward Service
//!
//! The Steward Service is the policy engine and transaction validator for Kamuy Wallet.
//!
//! ## Architecture
//!
//! ```ignore
//! ┌─────────────────────────────────────────┐
//! │         Steward Service               │
//! ├─────────────────────────────────────────┤
//! │  ┌─────────┐  ┌─────────┐  ┌────────┐ │
//! │  │   API   │  │ Telegram│  │  Queue │ │
//! │  │ Server  │  │   Bot   │  │Processor│ │
//! │  └────┬────┘  └────┬────┘  └────┬───┘ │
//! │       │            │            │     │
//! │       └────────────┼────────────┘     │
//! │                    │                  │
//! │       ┌────────────┴────────────┐     │
//! │       │      Policy Engine      │     │
//! │       │  (Validation + Rules)   │     │
//! │       └────────────┬────────────┘     │
//! │                    │                  │
//! │       ┌────────────┴────────────┐     │
//! │       │      StewardStorage    │     │
//! │       │  (SQLite/PostgreSQL)    │     │
//! │       └─────────────────────────┘     │
//! └─────────────────────────────────────────┘
//! ```
//!
//! ## Features
//!
//! - **Policy Engine**: Validate transactions against user-defined rules
//! - **Request Queue**: Queue and manage transaction requests
//! - **Approval Flow**: Auto-approve compliant transactions, escalate violations
//! - **Telegram Bot**: User interface for approvals and wallet management
//! - **MPC Signing**: Co-sign transactions with the Agent
//!
//! ## Security
//!
//! - Runs as separate process from AI Agent
//! - Steward Key (#2) never leaves service memory
//! - Policies encrypted at rest
//! - All input validated before processing
//! - Audit logging for all operations

pub mod api;
pub mod approval;
pub mod config;
pub mod error;
pub mod policy;
pub mod queue;
pub mod signing;
pub mod storage;
pub mod telegram;
pub mod transaction;
pub mod types;

/// Application state shared across all components
pub struct AppState {
    /// Configuration
    pub config: StewardConfig,
    /// Policy engine
    pub policy_engine: std::sync::Arc<tokio::sync::RwLock<policy::engine::PolicyEngine>>,
    /// Transaction queue
    pub queue: std::sync::Arc<tokio::sync::RwLock<queue::TransactionQueue>>,
    /// Storage backend
    pub storage: std::sync::Arc<storage::StewardStorage>,
    /// Steward key share (loaded from encrypted storage)
    pub key_share: std::sync::Arc<tokio::sync::RwLock<Option<kamuy_mpc_core::AgentKeyShare>>>,
    /// Signing coordinator for MPC signing (Agent + Steward keys)
    pub signing_coordinator: std::sync::Arc<signing::SigningCoordinator>,
    /// Approval channel for user confirmations
    pub approval_channel: approval::CompositeApprovalChannel,
    /// Transaction completion notifier (for long-polling)
    pub notifier: std::sync::Arc<queue::TransactionNotifier>,
    /// Pending approvals (shared between Telegram callbacks and approval channel)
    pub pending_approvals: approval::PendingApprovals,
    /// Temporary private keys for wallet creation flow (cleared after password set)
    pub temp_private_keys: std::sync::Arc<tokio::sync::Mutex<TempPrivateKeys>>,
}

/// Temporary storage for private keys during wallet creation
#[derive(Debug, Default)]
pub struct TempPrivateKeys {
    pub agent: Option<String>,
    pub user: Option<String>,
    pub awaiting_password: bool,
    /// FIX #2: First password entry for confirmation flow
    /// Stores the first password while user enters confirmation
    pub pending_password_confirm: Option<String>,
    /// FIX #1: Pending approval action waiting for password verification
    /// (tx_id, approved: bool) - if Some, user needs to enter password to complete the action
    pub pending_approval_action: Option<(crate::types::TransactionId, bool)>,
    /// Pending policy change approval waiting for password verification
    /// (policy_change_id, approved: bool) - if Some, user needs to enter password to complete the action
    pub pending_policy_change_action: Option<(crate::types::PolicyChangeRequestId, bool)>,
}

impl AppState {
    /// Create new AppState from components
    pub fn new(
        config: StewardConfig,
        policy_engine: std::sync::Arc<tokio::sync::RwLock<policy::engine::PolicyEngine>>,
        queue: std::sync::Arc<tokio::sync::RwLock<queue::TransactionQueue>>,
        storage: std::sync::Arc<storage::StewardStorage>,
        approval_channel: approval::CompositeApprovalChannel,
    ) -> Self {
        Self {
            config,
            policy_engine,
            queue,
            storage,
            key_share: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            signing_coordinator: std::sync::Arc::new(signing::SigningCoordinator::new()),
            approval_channel,
            notifier: std::sync::Arc::new(queue::TransactionNotifier::new()),
            pending_approvals: approval::PendingApprovals::new(),
            temp_private_keys: std::sync::Arc::new(tokio::sync::Mutex::new(TempPrivateKeys::default())),
        }
    }

    /// Create new AppState with shared pending approvals
    pub fn with_pending_approvals(
        config: StewardConfig,
        policy_engine: std::sync::Arc<tokio::sync::RwLock<policy::engine::PolicyEngine>>,
        queue: std::sync::Arc<tokio::sync::RwLock<queue::TransactionQueue>>,
        storage: std::sync::Arc<storage::StewardStorage>,
        approval_channel: approval::CompositeApprovalChannel,
        pending_approvals: approval::PendingApprovals,
    ) -> Self {
        Self {
            config,
            policy_engine,
            queue,
            storage,
            key_share: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            signing_coordinator: std::sync::Arc::new(signing::SigningCoordinator::new()),
            approval_channel,
            notifier: std::sync::Arc::new(queue::TransactionNotifier::new()),
            pending_approvals,
            temp_private_keys: std::sync::Arc::new(tokio::sync::Mutex::new(TempPrivateKeys::default())),
        }
    }

    /// Check if key share is loaded
    pub async fn is_key_loaded(&self) -> bool {
        self.key_share.read().await.is_some()
    }

    /// Load the Steward key share (requires password)
    pub async fn load_key_share(&self, password: &str) -> anyhow::Result<()> {
        let encrypted = self.storage.load_steward_key().await?;
        let encrypted = encrypted.ok_or_else(|| anyhow::anyhow!("No key share found"))?;
        let key_share = kamuy_mpc_core::decrypt_key_share(&encrypted, password)?;

        let mut guard = self.key_share.write().await;
        *guard = Some(key_share);

        tracing::info!("Steward key share loaded successfully");
        Ok(())
    }
}

// Re-export main types
pub use approval::{ApprovalChannelConfig, CompositeApprovalChannel, ApprovalDecision, PendingApprovals};
pub use config::StewardConfig;
pub use error::{StewardError, Result};
pub use types::{
    ApiResponse, HealthResponse, PaginatedResponse, Pagination,
    PolicyChangeRecord, PolicyChangeStatus, PolicyDecision, PolicyResult,
    TransactionId, TransactionRecord, TransactionRequest, TransactionStatus, WalletInfo,
};

// Re-export policy types
pub use policy::{PolicyEngine, PolicyRules, SpendingTracker};

// Re-export storage
pub use storage::StewardStorage;

/// Protocol version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default API port
pub const DEFAULT_API_PORT: u16 = 8080;

/// Default database URL
pub const DEFAULT_DATABASE_URL: &str = "sqlite://./steward.db";

/// Default policy file
pub const DEFAULT_POLICY_FILE: &str = "./policy.json";

/// Maximum transactions per page
pub const MAX_PER_PAGE: u32 = 100;

/// Default transaction expiration (hours)
pub const DEFAULT_TX_EXPIRATION_HOURS: i64 = 24;
