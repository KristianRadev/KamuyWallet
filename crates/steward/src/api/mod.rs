//! # API Module
//!
//! HTTP API for agent communication and external integrations.

pub mod routes;
pub mod server;

use crate::error::StewardError;
use crate::types::{ApiResponse, HealthResponse};
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

/// API state wrapper - using AppState from crate root
pub type ApiState = Arc<crate::AppState>;

/// Query parameters for pagination
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

impl PaginationQuery {
    pub fn to_pagination(&self) -> crate::types::Pagination {
        crate::types::Pagination {
            page: self.page.unwrap_or(0),
            per_page: self.per_page.unwrap_or(20).min(100),
        }
    }
}

/// Extract API key from request
pub fn extract_api_key(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Validate API key
/// SECURITY: Returns false if no API key is configured (fail-closed)
/// Uses constant-time comparison to prevent timing attacks
pub fn validate_api_key(state: &ApiState, key: &str) -> bool {
    state.config.api.api_key.as_ref()
        .map(|expected: &String| {
            // Constant-time comparison to prevent timing attacks
            use subtle::ConstantTimeEq;
            expected.as_bytes().ct_eq(key.as_bytes()).unwrap_u8() == 1
        })
        .unwrap_or(false) // SECURITY FIX: Deny requests when no key configured
}

/// Create success response
pub fn success<T: serde::Serialize>(data: T, request_id: impl Into<String>) -> impl IntoResponse {
    let response = ApiResponse::success(data, request_id);
    (StatusCode::OK, Json(response))
}

/// Create error response
pub fn error(status: StatusCode, error: impl Into<String>, request_id: impl Into<String>) -> impl IntoResponse {
    let response = ApiResponse::<()>::error(error, request_id);
    (status, Json(response))
}

/// Convert StewardError to HTTP response
pub fn error_response(err: StewardError, request_id: impl Into<String>) -> impl IntoResponse {
    let status = StatusCode::from_u16(err.status_code())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    // SECURITY FIX: Sanitize error messages in production to avoid information leakage
    let sanitized_error = if cfg!(debug_assertions) {
        // Development: show full error
        err.to_string()
    } else {
        // Production: sanitize internal errors
        match &err {
            StewardError::Internal(_) |
            StewardError::Database(_) |
            StewardError::Encryption(_) |
            StewardError::Mpc(_) => {
                "Internal server error".to_string()
            }
            _ => err.to_string(),
        }
    };

    error(status, sanitized_error, request_id)
}

/// Health check endpoint handler
pub async fn health_check(State(state): State<ApiState>) -> impl IntoResponse {
    let components = crate::api::routes::check_components(&state).await;
    
    let health = HealthResponse {
        status: if components.values().all(|c| c.status == "ok") {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        },
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: chrono::Utc::now(),
        components,
    };
    
    Json(health)
}

/// Request logging middleware
pub async fn log_request(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> impl IntoResponse {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let start = std::time::Instant::now();
    
    let response = next.run(req).await;
    
    let duration = start.elapsed();
    let status = response.status();
    
    tracing::info!(
        method = %method,
        uri = %uri,
        status = %status,
        duration_ms = %duration.as_millis(),
        "API request"
    );
    
    response
}
