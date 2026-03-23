//! # API Server
//!
//! HTTP server setup and configuration.

use super::routes;
use super::ApiState;
use axum::{
    middleware,
    routing::{get, post, put},
    Router,
};
use std::net::SocketAddr;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::info;

/// Start the API server
pub async fn start(state: ApiState) -> anyhow::Result<()> {
    let config = &state.config.api;
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;

    info!("Starting API server on {}", addr);

    // Build router
    let app = build_router(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Build the API router
fn build_router(state: ApiState) -> Router {
    // CORS configuration
    let cors = if state.config.api.cors_enabled {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::PUT, axum::http::Method::DELETE])
            .allow_headers([
                axum::http::header::CONTENT_TYPE,
                axum::http::header::AUTHORIZATION,
                axum::http::header::HeaderName::from_static("x-api-key"),
            ])
    } else {
        CorsLayer::new()
    };

    // Build routes
    Router::new()
        // Health check
        .route("/health", get(super::health_check))
        // Transaction routes
        .route("/api/v1/transactions", post(routes::submit_transaction))
        .route("/api/v1/transactions", get(routes::list_transactions))
        .route("/api/v1/transactions/pending", get(routes::get_pending))
        .route("/api/v1/transactions/:id", get(routes::get_transaction))
        .route("/api/v1/transactions/:id/approve", post(routes::approve_transaction))
        .route("/api/v1/transactions/:id/reject", post(routes::reject_transaction))
        // Policy routes
        .route("/api/v1/policy", get(routes::get_policy))
        .route("/api/v1/policy", put(routes::update_policy))
        .route("/api/v1/policy", post(routes::update_policy_value))
        // Policy change request routes (Agent proposes, User approves via Telegram)
        .route("/api/v1/policy/request", post(routes::request_policy_change))
        .route("/api/v1/policy/requests", get(routes::get_pending_policy_changes))
        .route("/api/v1/policy/requests/:id", get(routes::get_policy_change_request))
        .route("/api/v1/policy/requests/:id/approve", post(routes::approve_policy_change_request))
        // Transaction approval routes (with password for TerminalPassword level)
        .route("/api/v1/transactions/:id/approve-with-password", post(routes::approve_transaction_with_password))
        // Wallet routes
        .route("/api/v1/wallet", get(routes::get_wallet))
        .route("/api/v1/wallet/create", post(routes::create_wallet_with_password))
        .route("/api/v1/balances", get(routes::get_balances))
        // Recovery key route (requires password authentication)
        .route("/api/v1/recovery-key", post(routes::get_recovery_key))
        // Agent key route (requires password authentication)
        .route("/api/v1/agent-key", post(routes::get_agent_key))
        // Unlock routes
        .route("/api/v1/unlock", post(routes::unlock_steward))
        .route("/api/v1/unlock", get(routes::check_unlocked))
        // Queue routes
        .route("/api/v1/queue/status", get(routes::get_queue_status))
        // Admin routes for MPC signing keys (testing)
        .route("/api/v1/admin/signing-keys", post(routes::load_signing_keys))
        .route("/api/v1/admin/signing-keys", get(routes::check_signing_keys))
        // Add middleware
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CompressionLayer::new())
                .layer(cors)
                .layer(middleware::from_fn(super::log_request))
        )
        .with_state(state)
}

/// Build router for testing
#[cfg(test)]
pub fn build_test_router(state: ApiState) -> Router {
    build_router(state)
}
