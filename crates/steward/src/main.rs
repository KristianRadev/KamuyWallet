//! # Kamuy Steward Service
//!
//! The Steward Service is the policy engine and transaction validator for Kamuy Wallet.
//! It runs as a separate process from the AI Agent, providing security through isolation.
//!
//! ## Key Responsibilities
//!
//! 1. **Policy Engine**: Validate transactions against user-defined rules
//! 2. **Request Queue**: Queue and manage transaction requests from agents
//! 3. **Approval Flow**: Auto-approve compliant transactions, escalate violations
//! 4. **Telegram Bot**: User interface for onboarding and approvals
//! 5. **MPC Signing**: Co-sign transactions with the Agent (Key #2)
//!
//! ## Security Model
//!
//! - Runs as separate process from AI Agent
//! - Steward Key (#2) never leaves service memory
//! - Policies encrypted at rest
//! - All input validated before processing
//! - Audit logging for all operations

use anyhow::Result;
use std::sync::Arc;
use tracing::{info, error};

use kamuy_steward::{
    AppState,
    approval::{ApprovalChannelConfig, PendingApprovals},
    config::StewardConfig,
    api, policy, queue, storage, telegram,
};

// AppState is defined in lib.rs, we just initialize it here

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("kamuy_steward=info".parse()?)
                .add_directive("tower_http=info".parse()?),
        )
        .init();

    info!("Starting Kamuy Steward Service...");

    // Load configuration
    let config = StewardConfig::from_env()?;
    info!("Configuration loaded");

    // Initialize components
    let storage = Arc::new(storage::StewardStorage::new(config.database_url()).await?);
    let policy_engine = Arc::new(tokio::sync::RwLock::new(policy::engine::PolicyEngine::new(config.policy_file())?));
    let queue = Arc::new(tokio::sync::RwLock::new(queue::TransactionQueue::new(storage.clone())));

    // Create shared pending approvals (used by both Telegram callbacks and approval channel)
    let pending_approvals = PendingApprovals::new();

    // Create approval channel based on config (with shared pending approvals)
    let approval_config = ApprovalChannelConfig {
        telegram: if config.telegram.enabled { Some(config.telegram.clone()) } else { None },
        terminal_enabled: config.approval.terminal_enabled,
        timeout_secs: config.approval.timeout_secs,
    };
    let approval_channel = approval_config.create_channels_with_pending(pending_approvals.clone());

    // Initialize application state (with shared pending approvals)
    let state = Arc::new(AppState::with_pending_approvals(
        config,
        policy_engine,
        queue,
        storage,
        approval_channel,
        pending_approvals,
    ));
    info!("Application state initialized");

    // Try to load key share if it exists
    if let Ok(Some(_)) = state.storage.load_steward_key().await {
        info!("Found existing Steward key - use /unlock command to load it");
    }

    // Start services
    let mut handles = vec![];

    // Start API server
    let api_handle = tokio::spawn({
        let state = state.clone();
        async move {
            if let Err(e) = api::server::start(state).await {
                error!("API server error: {}", e);
            }
        }
    });
    handles.push(api_handle);
    info!("API server started");

    // Start Telegram bot (if enabled)
    #[cfg(feature = "telegram")]
    if state.config.telegram_enabled() {
        let telegram_handle = tokio::spawn({
            let state = state.clone();
            async move {
                if let Err(e) = telegram::bot::start(state).await {
                    error!("Telegram bot error: {}", e);
                }
            }
        });
        handles.push(telegram_handle);
        info!("Telegram bot started");
    }

    // Start queue processor
    let queue_handle = tokio::spawn({
        let state = state.clone();
        async move {
            if let Err(e) = queue::processor::start(state).await {
                error!("Queue processor error: {}", e);
            }
        }
    });
    handles.push(queue_handle);
    info!("Queue processor started");

    // Wait for all services
    for handle in handles {
        if let Err(e) = handle.await {
            error!("Service error: {}", e);
        }
    }

    Ok(())
}
