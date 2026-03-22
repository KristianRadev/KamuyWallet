//! # Storage Module
//!
//! Database storage for transactions, wallet info, and configuration.

use crate::error::{StewardError, Result};
use crate::types::{PolicyChangeRequestId, PolicyChangeRecord, TransactionId, TransactionRecord, TransactionStatus};
use chrono::{DateTime, Utc};
use hex;
use kamuy_mpc_core::EncryptedKeyShare;
use sha3::{Keccak256, Digest};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Row, Sqlite};
use std::collections::HashMap;
use tracing::info;

/// Maximum length for address strings
const MAX_ADDRESS_LEN: usize = 128;
/// Maximum length for token symbols
const MAX_TOKEN_LEN: usize = 32;
/// Maximum length for JSON data
const MAX_JSON_LEN: usize = 1024 * 1024; // 1MB
/// Maximum length for error messages
const MAX_ERROR_LEN: usize = 4096;

/// Steward storage backend
pub struct StewardStorage {
    /// Database pool
    pool: Pool<Sqlite>,
}

impl StewardStorage {
    /// Create new storage with database connection
    pub async fn new(database_url: &str) -> Result<Self> {
        // Normalize and enhance SQLite URL with create mode if needed
        let url = if database_url.starts_with("sqlite:") {
            // Already has sqlite: prefix - add mode=rwc if not present
            if database_url.contains('?') {
                database_url.to_string()
            } else {
                format!("{}?mode=rwc", database_url)
            }
        } else if database_url == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            // File path without prefix - add sqlite: and mode=rwc
            format!("sqlite:{}?mode=rwc", database_url)
        };

        info!("Connecting to database: {}", url.split('?').next().unwrap_or(&url));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .map_err(|e| StewardError::Database(format!("Failed to connect: {}", e)))?;

        let storage = Self { pool };
        storage.init().await?;

        info!("Storage initialized");
        Ok(storage)
    }
    
    /// Initialize database schema
    async fn init(&self) -> Result<()> {
        // Create transactions table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS transactions (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                request_json TEXT NOT NULL,
                policy_result_json TEXT,
                user_approval_json TEXT,
                signature_json TEXT,
                tx_hash TEXT,
                error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                expires_at TEXT NOT NULL
            )
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to create transactions table: {}", e)))?;
        
        // Create wallet table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS wallet (
                id INTEGER PRIMARY KEY,
                address TEXT NOT NULL,
                chain_id INTEGER NOT NULL,
                public_key TEXT NOT NULL,
                email TEXT,
                created_at TEXT NOT NULL
            )
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to create wallet table: {}", e)))?;
        
        // Create steward_key table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS steward_key (
                id INTEGER PRIMARY KEY,
                encrypted_key_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            )
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to create steward_key table: {}", e)))?;
        
        // Create user_key table (encrypted with user's password)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_key (
                id INTEGER PRIMARY KEY,
                encrypted_key_json TEXT NOT NULL,
                password_hash TEXT NOT NULL,
                created_at TEXT NOT NULL
            )
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to create user_key table: {}", e)))?;
        
        // Create balances table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS balances (
                token TEXT PRIMARY KEY,
                balance TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to create balances table: {}", e)))?;

        // Create policy_change_requests table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS policy_change_requests (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                request_json TEXT NOT NULL,
                resolved_by TEXT,
                resolved_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                expires_at TEXT NOT NULL
            )
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to create policy_change_requests table: {}", e)))?;

        // Create spending_tracker table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS spending_tracker (
                id INTEGER PRIMARY KEY,
                daily_spent TEXT NOT NULL,
                weekly_spent TEXT NOT NULL,
                last_reset_daily TEXT NOT NULL,
                last_reset_weekly TEXT NOT NULL
            )
            "#
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to create spending_tracker table: {}", e)))?;

        info!("Database schema initialized");
        Ok(())
    }
    
    /// Save a transaction
    pub async fn save_transaction(&self, record: &TransactionRecord) -> Result<()> {
        // SECURITY FIX: Validate input lengths to prevent resource exhaustion
        Self::validate_record(record)?;

        let request_json = serde_json::to_string(&record.request)
            .map_err(|e| StewardError::Serialization(e.to_string()))?;
        let policy_result_json = record.policy_result.as_ref()
            .map(|p| serde_json::to_string(p).unwrap_or_default());
        let user_approval_json = record.user_approval.as_ref()
            .map(|u| serde_json::to_string(u).unwrap_or_default());
        let signature_json = record.signature.as_ref()
            .map(|s| serde_json::to_string(s).unwrap_or_default());

        // Validate JSON lengths
        if request_json.len() > MAX_JSON_LEN {
            return Err(StewardError::Validation(
                format!("Request JSON exceeds maximum size of {} bytes", MAX_JSON_LEN)
            ));
        }

        sqlx::query(
            r#"
            INSERT INTO transactions (
                id, status, request_json, policy_result_json, user_approval_json,
                signature_json, tx_hash, error, created_at, updated_at, expires_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(record.id.to_string())
        .bind(record.status.to_string())
        .bind(request_json)
        .bind(policy_result_json)
        .bind(user_approval_json)
        .bind(signature_json)
        .bind(record.tx_hash.as_ref())
        .bind(record.error.as_ref().map(|e| &e[..e.len().min(MAX_ERROR_LEN)]))
        .bind(record.created_at.to_rfc3339())
        .bind(record.updated_at.to_rfc3339())
        .bind(record.expires_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to save transaction: {}", e)))?;

        Ok(())
    }
    
    /// Get a transaction by ID
    pub async fn get_transaction(&self, id: TransactionId) -> Result<Option<TransactionRecord>> {
        let row = sqlx::query(
            r#"
            SELECT * FROM transactions WHERE id = ?
            "#
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to get transaction: {}", e)))?;
        
        match row {
            Some(row) => Ok(Some(self.row_to_transaction(row)?)),
            None => Ok(None),
        }
    }
    
    /// Update a transaction
    pub async fn update_transaction(&self, record: &TransactionRecord) -> Result<()> {
        // SECURITY FIX: Validate input lengths
        Self::validate_record(record)?;

        let policy_result_json = record.policy_result.as_ref()
            .map(|p| serde_json::to_string(p).unwrap_or_default());
        let user_approval_json = record.user_approval.as_ref()
            .map(|u| serde_json::to_string(u).unwrap_or_default());
        let signature_json = record.signature.as_ref()
            .map(|s| serde_json::to_string(s).unwrap_or_default());

        // Truncate error message if too long
        let error_truncated = record.error.as_ref()
            .map(|e| e[..e.len().min(MAX_ERROR_LEN)].to_string());

        sqlx::query(
            r#"
            UPDATE transactions SET
                status = ?,
                policy_result_json = ?,
                user_approval_json = ?,
                signature_json = ?,
                tx_hash = ?,
                error = ?,
                updated_at = ?
            WHERE id = ?
            "#
        )
        .bind(record.status.to_string())
        .bind(policy_result_json)
        .bind(user_approval_json)
        .bind(signature_json)
        .bind(record.tx_hash.as_ref())
        .bind(error_truncated)
        .bind(record.updated_at.to_rfc3339())
        .bind(record.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to update transaction: {}", e)))?;

        Ok(())
    }
    
    /// Get pending transactions
    pub async fn get_pending_transactions(&self) -> Result<Vec<TransactionRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM transactions 
            WHERE status IN ('pending', 'awaiting_approval', 'user_approved')
            ORDER BY created_at ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to get pending transactions: {}", e)))?;
        
        rows.into_iter()
            .map(|row| self.row_to_transaction(row))
            .collect()
    }
    
    /// Get recent transactions
    pub async fn get_recent_transactions(&self, limit: i64) -> Result<Vec<TransactionRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM transactions 
            ORDER BY created_at DESC
            LIMIT ?
            "#
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to get recent transactions: {}", e)))?;
        
        rows.into_iter()
            .map(|row| self.row_to_transaction(row))
            .collect()
    }
    
    /// Count pending transactions
    pub async fn count_pending_transactions(&self) -> Result<u32> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM transactions 
            WHERE status IN ('pending', 'awaiting_approval', 'user_approved')
            "#
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to count pending: {}", e)))?;
        
        Ok(count as u32)
    }
    
    /// Save Steward key
    pub async fn save_steward_key(&self, encrypted: &EncryptedKeyShare) -> Result<()> {
        let json = encrypted.to_bytes()
            .map_err(|e| StewardError::Serialization(e.to_string()))?;
        
        // Delete existing key
        sqlx::query("DELETE FROM steward_key")
            .execute(&self.pool)
            .await
            .ok();
        
        sqlx::query(
            r#"
            INSERT INTO steward_key (encrypted_key_json, created_at)
            VALUES (?, ?)
            "#
        )
        .bind(String::from_utf8_lossy(&json))
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to save key: {}", e)))?;
        
        Ok(())
    }
    
    /// Load Steward key
    pub async fn load_steward_key(&self) -> Result<Option<EncryptedKeyShare>> {
        let row: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT encrypted_key_json FROM steward_key LIMIT 1
            "#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to load key: {}", e)))?;
        
        match row {
            Some((json,)) => {
                let encrypted = EncryptedKeyShare::from_bytes(json.as_bytes())
                    .map_err(|e| StewardError::Deserialization(e.to_string()))?;
                Ok(Some(encrypted))
            }
            None => Ok(None),
        }
    }
    
    /// Save User key (encrypted with user's password)
    pub async fn save_user_key(&self, encrypted: &EncryptedKeyShare, password_hash: &str) -> Result<()> {
        let json = encrypted.to_bytes()
            .map_err(|e| StewardError::Serialization(e.to_string()))?;
        
        // Delete existing key
        sqlx::query("DELETE FROM user_key")
            .execute(&self.pool)
            .await
            .ok();
        
        sqlx::query(
            r#"
            INSERT INTO user_key (encrypted_key_json, password_hash, created_at)
            VALUES (?, ?, ?)
            "#
        )
        .bind(String::from_utf8_lossy(&json))
        .bind(password_hash)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to save user key: {}", e)))?;
        
        Ok(())
    }
    
    /// Load User key (caller must provide password to decrypt)
    pub async fn load_user_key(&self) -> Result<Option<(Vec<u8>, String)>> {
        let row: Option<(String, String)> = sqlx::query_as(
            r#"
            SELECT encrypted_key_json, password_hash FROM user_key LIMIT 1
            "#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to load user key: {}", e)))?;
        
        match row {
            Some((json, password_hash)) => {
                Ok(Some((json.into_bytes(), password_hash)))
            }
            None => Ok(None),
        }
    }
    
    /// Check if user key exists
    pub async fn has_user_key(&self) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM user_key
            "#
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to check user key: {}", e)))?;
        
        Ok(count > 0)
    }
    
    /// Verify user password against stored hash
    pub async fn verify_user_password(&self, password: &str) -> Result<bool> {
        let row: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT password_hash FROM user_key LIMIT 1
            "#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to load password hash: {}", e)))?;
        
        match row {
            Some((stored_hash,)) => {
                // Hash the provided password
                let mut hasher = Keccak256::new();
                hasher.update(password.as_bytes());
                let hash = hasher.finalize();
                let computed_hash = hex::encode(hash);
                
                Ok(computed_hash == stored_hash)
            }
            None => Ok(false),
        }
    }
    
    /// Get wallet info
    pub async fn get_wallet(&self) -> Result<Option<WalletInfo>> {
        let row = sqlx::query(
            r#"
            SELECT address, chain_id, public_key, email FROM wallet LIMIT 1
            "#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to get wallet: {}", e)))?;

        match row {
            Some(row) => {
                Ok(Some(WalletInfo {
                    address: row.get("address"),
                    chain_id: row.get::<i64, _>("chain_id") as u64,
                    public_key: row.get("public_key"),
                    email: row.get("email"),
                }))
            }
            None => Ok(None),
        }
    }
    
    /// Save wallet info (create or update)
    pub async fn set_wallet(&self, address: &str, chain_id: u64, public_key: &str, email: Option<&str>) -> Result<()> {
        let created_at = chrono::Utc::now().to_rfc3339();

        // Delete existing wallet first (we only want one)
        sqlx::query("DELETE FROM wallet")
            .execute(&self.pool)
            .await
            .map_err(|e| StewardError::Database(format!("Failed to delete existing wallet: {}", e)))?;

        // Insert new wallet
        sqlx::query(
            r#"
            INSERT INTO wallet (address, chain_id, public_key, email, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#
        )
        .bind(address)
        .bind(chain_id as i64)
        .bind(public_key)
        .bind(email)
        .bind(&created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to save wallet: {}", e)))?;

        Ok(())
    }

    /// Update wallet email
    pub async fn set_wallet_email(&self, email: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE wallet SET email = ? WHERE id = 1
            "#
        )
        .bind(email)
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to update wallet email: {}", e)))?;

        Ok(())
    }
    
    /// Get balances
    pub async fn get_balances(&self) -> Result<HashMap<String, String>> {
        let rows = sqlx::query("SELECT token, balance FROM balances")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StewardError::Database(format!("Failed to get balances: {}", e)))?;

        let mut balances = HashMap::new();
        for row in rows {
            let token: String = row.get("token");
            let balance: String = row.get("balance");
            balances.insert(token, balance);
        }

        Ok(balances)
    }

    // ========== Policy Change Requests ==========

    /// Save a policy change request
    pub async fn save_policy_change_request(&self, record: &PolicyChangeRecord) -> Result<()> {
        let request_json = serde_json::to_string(&record.request)
            .map_err(|e| StewardError::Serialization(e.to_string()))?;

        if request_json.len() > MAX_JSON_LEN {
            return Err(StewardError::Validation(
                format!("Request JSON exceeds maximum size of {} bytes", MAX_JSON_LEN)
            ));
        }

        sqlx::query(
            r#"
            INSERT INTO policy_change_requests (
                id, status, request_json, resolved_by, resolved_at,
                created_at, updated_at, expires_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(record.request.id.to_string())
        .bind(record.status.to_string())
        .bind(request_json)
        .bind(record.resolved_by.as_ref())
        .bind(record.resolved_at.as_ref().map(|t| t.to_rfc3339()))
        .bind(record.request.created_at.to_rfc3339())
        .bind(record.updated_at.to_rfc3339())
        .bind(record.request.expires_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to save policy change request: {}", e)))?;

        Ok(())
    }

    /// Get a policy change request by ID
    pub async fn get_policy_change_request(&self, id: PolicyChangeRequestId) -> Result<Option<PolicyChangeRecord>> {
        let row = sqlx::query(
            r#"
            SELECT * FROM policy_change_requests WHERE id = ?
            "#
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to get policy change request: {}", e)))?;

        match row {
            Some(row) => Ok(Some(self.row_to_policy_change_record(row)?)),
            None => Ok(None),
        }
    }

    /// Update a policy change request
    pub async fn update_policy_change_request(&self, record: &PolicyChangeRecord) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE policy_change_requests SET
                status = ?,
                resolved_by = ?,
                resolved_at = ?,
                updated_at = ?
            WHERE id = ?
            "#
        )
        .bind(record.status.to_string())
        .bind(record.resolved_by.as_ref())
        .bind(record.resolved_at.as_ref().map(|t| t.to_rfc3339()))
        .bind(record.updated_at.to_rfc3339())
        .bind(record.request.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to update policy change request: {}", e)))?;

        Ok(())
    }

    /// Get pending policy change requests
    pub async fn get_pending_policy_change_requests(&self) -> Result<Vec<PolicyChangeRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM policy_change_requests
            WHERE status = 'pending'
            ORDER BY created_at ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to get pending policy change requests: {}", e)))?;

        rows.into_iter()
            .map(|row| self.row_to_policy_change_record(row))
            .collect()
    }

    /// Delete a policy change request
    pub async fn delete_policy_change_request(&self, id: PolicyChangeRequestId) -> Result<()> {
        sqlx::query("DELETE FROM policy_change_requests WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| StewardError::Database(format!("Failed to delete policy change request: {}", e)))?;

        Ok(())
    }

    // ========== Spending Tracker ==========

    /// Save spending tracker state
    pub async fn save_spending_tracker(&self, tracker: &crate::policy::rules::SpendingTracker) -> Result<()> {
        let daily_spent = tracker.daily_spent.to_string();
        let weekly_spent = tracker.weekly_spent.to_string();
        let last_reset_daily = tracker.last_reset_daily.to_rfc3339();
        let last_reset_weekly = tracker.last_reset_weekly.to_rfc3339();

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO spending_tracker (id, daily_spent, weekly_spent, last_reset_daily, last_reset_weekly)
            VALUES (1, ?, ?, ?, ?)
            "#
        )
        .bind(daily_spent)
        .bind(weekly_spent)
        .bind(last_reset_daily)
        .bind(last_reset_weekly)
        .execute(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to save spending tracker: {}", e)))?;

        Ok(())
    }

    /// Load spending tracker state
    pub async fn load_spending_tracker(&self) -> Result<crate::policy::rules::SpendingTracker> {
        use crate::policy::rules::SpendingTracker;

        let row = sqlx::query(
            r#"
            SELECT daily_spent, weekly_spent, last_reset_daily, last_reset_weekly
            FROM spending_tracker WHERE id = 1
            "#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StewardError::Database(format!("Failed to load spending tracker: {}", e)))?;

        match row {
            Some(row) => {
                let daily_spent: String = row.get("daily_spent");
                let weekly_spent: String = row.get("weekly_spent");
                let last_reset_daily: String = row.get("last_reset_daily");
                let last_reset_weekly: String = row.get("last_reset_weekly");

                Ok(SpendingTracker {
                    daily_spent: daily_spent.parse().unwrap_or(0),
                    weekly_spent: weekly_spent.parse().unwrap_or(0),
                    last_reset_daily: DateTime::parse_from_rfc3339(&last_reset_daily)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    last_reset_weekly: DateTime::parse_from_rfc3339(&last_reset_weekly)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            }
            None => Ok(SpendingTracker::new()),
        }
    }

    /// Convert database row to policy change record
    fn row_to_policy_change_record(&self, row: sqlx::sqlite::SqliteRow) -> Result<PolicyChangeRecord> {
        let request_json: String = row.get("request_json");
        let request: crate::types::PolicyChangeRequest = serde_json::from_str(&request_json)
            .map_err(|e| StewardError::Deserialization(format!("Failed to parse request: {}", e)))?;

        let status_str: String = row.get("status");
        let status = parse_policy_change_status(&status_str)?;

        Ok(PolicyChangeRecord {
            request,
            status,
            resolved_by: row.get("resolved_by"),
            resolved_at: row.get::<Option<String>, _>("resolved_at")
                .map(|t| DateTime::parse_from_rfc3339(&t))
                .transpose()
                .map_err(|e| StewardError::Deserialization(e.to_string()))?
                .map(|t| t.with_timezone(&Utc)),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("updated_at"))
                .map_err(|e| StewardError::Deserialization(e.to_string()))?
                .with_timezone(&Utc),
        })
    }
    
    /// Convert database row to transaction record
    fn row_to_transaction(&self, row: sqlx::sqlite::SqliteRow) -> Result<TransactionRecord> {
        // Parse JSON fields
        let request_json: String = row.get("request_json");
        let request = serde_json::from_str(&request_json)
            .map_err(|e| StewardError::Deserialization(format!("Failed to parse request: {}", e)))?;
        
        let policy_result_json: Option<String> = row.get("policy_result_json");
        let policy_result = policy_result_json
            .and_then(|j| serde_json::from_str(&j).ok());
        
        let user_approval_json: Option<String> = row.get("user_approval_json");
        let user_approval = user_approval_json
            .and_then(|j| serde_json::from_str(&j).ok());
        
        let signature_json: Option<String> = row.get("signature_json");
        let signature = signature_json
            .and_then(|j| serde_json::from_str(&j).ok());
        
        let status_str: String = row.get("status");
        let status = parse_status(&status_str)?;
        
        Ok(TransactionRecord {
            id: TransactionId::from(
                uuid::Uuid::parse_str(&row.get::<String, _>("id"))
                    .map_err(|e| StewardError::Deserialization(e.to_string()))?
            ),
            status,
            request,
            policy_result,
            user_approval,
            signature,
            tx_hash: row.get("tx_hash"),
            error: row.get("error"),
            created_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))
                .map_err(|e| StewardError::Deserialization(e.to_string()))?
                .with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("updated_at"))
                .map_err(|e| StewardError::Deserialization(e.to_string()))?
                .with_timezone(&Utc),
            expires_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("expires_at"))
                .map_err(|e| StewardError::Deserialization(e.to_string()))?
                .with_timezone(&Utc),
        })
    }

    /// Validate transaction record input lengths
    fn validate_record(record: &TransactionRecord) -> Result<()> {
        // Validate address length
        if record.request.to.len() > MAX_ADDRESS_LEN {
            return Err(StewardError::Validation(
                format!("Destination address exceeds maximum length of {} characters", MAX_ADDRESS_LEN)
            ));
        }

        // Validate token symbol length
        if record.request.token.len() > MAX_TOKEN_LEN {
            return Err(StewardError::Validation(
                format!("Token symbol exceeds maximum length of {} characters", MAX_TOKEN_LEN)
            ));
        }

        // Validate agent_id length
        if record.request.agent_id.len() > MAX_ADDRESS_LEN {
            return Err(StewardError::Validation(
                format!("Agent ID exceeds maximum length of {} characters", MAX_ADDRESS_LEN)
            ));
        }

        // Validate request_id length
        if record.request.request_id.len() > MAX_ADDRESS_LEN {
            return Err(StewardError::Validation(
                format!("Request ID exceeds maximum length of {} characters", MAX_ADDRESS_LEN)
            ));
        }

        // Validate tx_hash length if present
        if let Some(tx_hash) = &record.tx_hash {
            if tx_hash.len() > MAX_ADDRESS_LEN {
                return Err(StewardError::Validation(
                    format!("Transaction hash exceeds maximum length of {} characters", MAX_ADDRESS_LEN)
                ));
            }
        }

        // Validate error message length
        if let Some(error) = &record.error {
            if error.len() > MAX_ERROR_LEN {
                return Err(StewardError::Validation(
                    format!("Error message exceeds maximum length of {} characters", MAX_ERROR_LEN)
                ));
            }
        }

        Ok(())
    }
}

/// Wallet info
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WalletInfo {
    pub address: String,
    pub chain_id: u64,
    pub public_key: String,
    pub email: Option<String>,
}

/// Parse status string
fn parse_status(s: &str) -> Result<TransactionStatus> {
    match s {
        "pending" => Ok(TransactionStatus::Pending),
        "evaluating" => Ok(TransactionStatus::Evaluating),
        "approved" => Ok(TransactionStatus::Approved),
        "awaiting_approval" => Ok(TransactionStatus::AwaitingApproval),
        "user_approved" => Ok(TransactionStatus::UserApproved),
        "user_rejected" => Ok(TransactionStatus::UserRejected),
        "signing" => Ok(TransactionStatus::Signing),
        "submitted" => Ok(TransactionStatus::Submitted),
        "confirmed" => Ok(TransactionStatus::Confirmed),
        "failed" => Ok(TransactionStatus::Failed),
        "rejected" => Ok(TransactionStatus::Rejected),
        "expired" => Ok(TransactionStatus::Expired),
        _ => Err(StewardError::Deserialization(format!("Unknown status: {}", s))),
    }
}

/// Parse policy change status string
fn parse_policy_change_status(s: &str) -> Result<crate::types::PolicyChangeStatus> {
    match s {
        "pending" => Ok(crate::types::PolicyChangeStatus::Pending),
        "approved" => Ok(crate::types::PolicyChangeStatus::Approved),
        "rejected" => Ok(crate::types::PolicyChangeStatus::Rejected),
        "expired" => Ok(crate::types::PolicyChangeStatus::Expired),
        _ => Err(StewardError::Deserialization(format!("Unknown policy change status: {}", s))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TransactionRequest;

    async fn create_test_storage() -> StewardStorage {
        StewardStorage::new("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn test_save_and_get_transaction() {
        let storage = create_test_storage().await;

        // SECURITY: Use wei format (integer) not decimal strings
        let request = TransactionRequest::new(
            "req1", "0x123", "100000000", "USDC", 1, "agent1"
        );
        let record = TransactionRecord::new(request);

        storage.save_transaction(&record).await.unwrap();

        let loaded = storage.get_transaction(record.id).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, record.id);
    }

    #[tokio::test]
    async fn test_update_transaction() {
        let storage = create_test_storage().await;

        // SECURITY: Use wei format (integer) not decimal strings
        let request = TransactionRequest::new("req1", "0x123", "100000000", "USDC", 1, "agent1");
        let mut record = TransactionRecord::new(request);

        storage.save_transaction(&record).await.unwrap();

        record.set_status(TransactionStatus::Approved);
        storage.update_transaction(&record).await.unwrap();

        let loaded = storage.get_transaction(record.id).await.unwrap().unwrap();
        assert_eq!(loaded.status, TransactionStatus::Approved);
    }

    #[tokio::test]
    async fn test_pending_transactions() {
        let storage = create_test_storage().await;

        // Create pending transaction
        // SECURITY: Use wei format (integer) not decimal strings
        let request = TransactionRequest::new("req1", "0x123", "100000000", "USDC", 1, "agent1");
        let record = TransactionRecord::new(request);
        storage.save_transaction(&record).await.unwrap();

        // Create approved transaction
        let request2 = TransactionRequest::new("req2", "0x123", "200000000", "USDC", 1, "agent1");
        let mut record2 = TransactionRecord::new(request2);
        record2.set_status(TransactionStatus::Approved);
        storage.save_transaction(&record2).await.unwrap();

        let pending = storage.get_pending_transactions().await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, record.id);
    }

    #[tokio::test]
    async fn test_spending_tracker_storage() {
        let storage = create_test_storage().await;

        let mut tracker = crate::policy::rules::SpendingTracker::new();
        tracker.add_spending(100_000_000);
        tracker.add_spending(50_000_000);

        storage.save_spending_tracker(&tracker).await.unwrap();

        let loaded = storage.load_spending_tracker().await.unwrap();
        assert_eq!(loaded.daily_spent, 150_000_000);
        assert_eq!(loaded.weekly_spent, 150_000_000);
    }
}

