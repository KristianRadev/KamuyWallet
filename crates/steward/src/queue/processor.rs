//! # Queue Processor
//!
//! Background task that processes transactions from the queue.
//! Supports long-polling via the transaction notifier.

use crate::error::Result;
use crate::queue::TransactionQueue;
use crate::queue::notifier::{TransactionResult, TransactionFinalStatus};
use crate::types::{PolicyCheck, PolicyDecision, PolicyResult, TransactionRecord, TransactionStatus};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

/// Type alias for the queue
pub type QueueRef = Arc<RwLock<TransactionQueue>>;

/// Start the queue processor
pub async fn start(state: Arc<crate::AppState>) -> Result<()> {
    info!("Starting queue processor...");

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));

    loop {
        interval.tick().await;

        // Clear expired transactions
        let queue_guard: tokio::sync::RwLockWriteGuard<'_, TransactionQueue> = state.queue.write().await;
        if let Err(e) = queue_guard.clear_expired().await {
            error!("Failed to clear expired transactions: {}", e);
        }
        drop(queue_guard);

        // Process next transaction
        // SECURITY FIX: Use write lock for queue mutation to prevent race conditions
        let next_tx = {
            let queue: tokio::sync::RwLockWriteGuard<'_, TransactionQueue> = state.queue.write().await;
            queue.next().await
        };

        match next_tx {
            Ok(Some(record)) => {
                if let Err(e) = process_transaction(&state, record).await {
                    error!("Transaction processing error: {}", e);
                }
            }
            Ok(None) => {
                // No transactions to process
            }
            Err(e) => {
                error!("Failed to get next transaction: {}", e);
            }
        }
    }
}

/// Process a single transaction
async fn process_transaction(
    state: &crate::AppState,
    mut record: TransactionRecord,
) -> Result<()> {
    info!(
        transaction_id = %record.id,
        "Processing transaction"
    );

    // Update status to evaluating
    record.set_status(TransactionStatus::Evaluating);
    state.storage.update_transaction(&record).await?;

    // Load policy rules
    let policy_engine: tokio::sync::RwLockReadGuard<'_, crate::policy::PolicyEngine> = state.policy_engine.read().await;
    let mut rules = policy_engine.rules().await;
    drop(policy_engine);

    // Evaluate against policy (v2.0)
    let policy_result = evaluate_transaction_v2(&record.request, &mut rules)?;
    record.policy_result = Some(policy_result.clone());

    match policy_result.decision {
        PolicyDecision::AutoApprove => {
            info!(
                transaction_id = %record.id,
                "Transaction auto-approved by policy"
            );

            record.set_status(TransactionStatus::Approved);
            state.storage.update_transaction(&record).await?;

            // Trigger signing
            if let Err(e) = sign_and_submit(state, &mut record).await {
                error!("Signing failed: {}", e);
                record.error = Some(e.to_string());
                record.set_status(TransactionStatus::Failed);
                state.storage.update_transaction(&record).await?;

                // Notify of failure
                notify_completion(state, &record).await;
            } else {
                // Notify of success
                notify_completion(state, &record).await;
            }
        }
        PolicyDecision::RequireApproval => {
            info!(
                transaction_id = %record.id,
                "Transaction requires user approval"
            );

            record.set_status(TransactionStatus::AwaitingApproval);
            state.storage.update_transaction(&record).await?;

            // Try approval channels
            let approval_result = state.approval_channel.request_approval(&record).await;

            match approval_result {
                Ok(crate::approval::ApprovalDecision::Approved) => {
                    info!(
                        transaction_id = %record.id,
                        "Transaction approved via approval channel"
                    );

                    record.set_status(TransactionStatus::UserApproved);
                    state.storage.update_transaction(&record).await?;

                    // Sign and submit
                    if let Err(e) = sign_and_submit(state, &mut record).await {
                        error!("Signing failed after approval: {}", e);
                        record.error = Some(e.to_string());
                        record.set_status(TransactionStatus::Failed);
                    }

                    state.storage.update_transaction(&record).await?;
                    notify_completion(state, &record).await;
                }
                Ok(crate::approval::ApprovalDecision::Rejected) => {
                    warn!(
                        transaction_id = %record.id,
                        "Transaction rejected by user"
                    );

                    record.set_status(TransactionStatus::UserRejected);
                    record.error = Some("Rejected by user".to_string());
                    state.storage.update_transaction(&record).await?;

                    notify_completion(state, &record).await;
                }
                Ok(crate::approval::ApprovalDecision::TimedOut) => {
                    warn!(
                        transaction_id = %record.id,
                        "Transaction approval timed out"
                    );

                    // Don't change status - leave as awaiting approval
                    // Agent can poll later
                    // Notify of pending status
                    notify_completion(state, &record).await;
                }
                Err(e) => {
                    error!(
                        transaction_id = %record.id,
                        error = %e,
                        "Approval channel error"
                    );

                    // Leave as awaiting approval for later polling
                    notify_completion(state, &record).await;
                }
            }
        }
        PolicyDecision::Reject => {
            warn!(
                transaction_id = %record.id,
                reason = %policy_result.reason,
                "Transaction rejected by policy"
            );

            record.set_status(TransactionStatus::Rejected);
            record.error = Some(policy_result.reason);
            state.storage.update_transaction(&record).await?;

            // Notify of rejection (v2.0: always notify)
            #[cfg(feature = "telegram")]
            if state.config.telegram_enabled() {
                if let Err(e) = notify_rejection(state, &record).await {
                    warn!("Failed to send rejection notification: {}", e);
                }
            }

            notify_completion(state, &record).await;
        }
    }

    // Update spending tracker in rules if confirmed
    if record.status == TransactionStatus::Confirmed {
        // v2.0: Use u64 amounts (USDC micros)
        let amount = record.request.value.parse::<u64>()
            .map_err(|_| crate::error::StewardError::Validation(
                "Invalid amount format: must be integer in USDC micros".to_string()
            ))?;

        // Record spending in the policy rules
        let policy_engine = state.policy_engine.read().await;
        let mut rules = policy_engine.rules().await;
        rules.record_spending(amount, &record.request.to);
        drop(policy_engine);
    }

    // Clear processing flag so queue can process next transaction
    state.queue.write().await.clear_processing().await;

    Ok(())
}

/// Evaluate transaction against v2.0 policy rules
fn evaluate_transaction_v2(
    request: &crate::types::TransactionRequest,
    rules: &mut crate::policy::PolicyRules,
) -> Result<PolicyResult> {
    let mut checks = Vec::new();
    let mut all_passed = true;
    let mut require_approval = false;

    // Parse amount (v2.0: u64 in USDC micros)
    let amount: u64 = request.value.parse()
        .map_err(|_| crate::error::StewardError::Validation(
            "Invalid amount format: must be integer in USDC micros".to_string()
        ))?;

    // Check 1: Max per transaction
    let per_tx_passed = amount <= rules.max_per_tx;
    checks.push(PolicyCheck {
        name: "max_per_tx".to_string(),
        passed: per_tx_passed,
        value: request.value.clone(),
        limit: rules.max_per_tx.to_string(),
        message: if per_tx_passed {
            format!("Amount {} is within limit {}", request.value, rules.max_per_tx)
        } else {
            format!("Amount {} exceeds limit {}", request.value, rules.max_per_tx)
        },
    });
    if !per_tx_passed {
        all_passed = false;
    }

    // Check 2: Daily spending limit
    rules.spending_tracker.check_and_reset();
    let daily_passed = !rules.spending_tracker.would_exceed_daily(amount, rules.max_daily);
    checks.push(PolicyCheck {
        name: "max_daily".to_string(),
        passed: daily_passed,
        value: rules.spending_tracker.daily_spent.to_string(),
        limit: rules.max_daily.to_string(),
        message: if daily_passed {
            format!("Daily spending within limit")
        } else {
            format!("Would exceed daily limit")
        },
    });
    if !daily_passed {
        all_passed = false;
    }

    // Check 3: Weekly spending limit
    let weekly_passed = !rules.spending_tracker.would_exceed_weekly(amount, rules.max_weekly);
    checks.push(PolicyCheck {
        name: "max_weekly".to_string(),
        passed: weekly_passed,
        value: rules.spending_tracker.weekly_spent.to_string(),
        limit: rules.max_weekly.to_string(),
        message: if weekly_passed {
            format!("Weekly spending within limit")
        } else {
            format!("Would exceed weekly limit")
        },
    });
    if !weekly_passed {
        all_passed = false;
    }

    // Check 4: Whitelist
    let whitelist_passed = rules.is_whitelisted(&request.to);
    checks.push(PolicyCheck {
        name: "whitelist".to_string(),
        passed: whitelist_passed,
        value: request.to.clone(),
        limit: if rules.whitelist.is_empty() {
            "(any)".to_string()
        } else {
            format!("{} addresses", rules.whitelist.len())
        },
        message: if whitelist_passed {
            format!("Address {} is whitelisted", request.to)
        } else {
            format!("Address {} is not whitelisted", request.to)
        },
    });

    // Non-whitelisted address requires approval
    if !whitelist_passed {
        if amount > rules.auto_add_threshold {
            // Over threshold requires terminal password (higher security)
            require_approval = true;
        } else {
            // Under threshold: Telegram button to add and pay
            require_approval = true;
        }
    }

    // Check 5: Token is USDC (v2.0 only supports USDC)
    let token_passed = request.token.to_uppercase() == "USDC";
    checks.push(PolicyCheck {
        name: "token".to_string(),
        passed: token_passed,
        value: request.token.clone(),
        limit: "USDC".to_string(),
        message: if token_passed {
            "Token is USDC".to_string()
        } else {
            format!("Token {} is not supported (v2.0 only supports USDC)", request.token)
        },
    });
    if !token_passed {
        all_passed = false;
    }

    // Determine decision
    let decision = if !all_passed {
        PolicyDecision::Reject
    } else if require_approval {
        PolicyDecision::RequireApproval
    } else {
        PolicyDecision::AutoApprove
    };

    let reason = if !all_passed {
        "Policy violation detected".to_string()
    } else if require_approval {
        "Transaction requires user approval".to_string()
    } else {
        "All policy checks passed".to_string()
    };

    Ok(PolicyResult {
        passed: all_passed,
        checks,
        decision,
        reason,
        evaluated_at: chrono::Utc::now(),
    })
}

/// Notify waiters of transaction completion
async fn notify_completion(state: &crate::AppState, record: &TransactionRecord) {
    let final_status = match record.status {
        TransactionStatus::Confirmed => TransactionFinalStatus::Confirmed,
        TransactionStatus::Submitted => TransactionFinalStatus::Signed,
        TransactionStatus::Rejected => TransactionFinalStatus::Rejected,
        TransactionStatus::UserRejected => TransactionFinalStatus::UserRejected,
        TransactionStatus::Failed => TransactionFinalStatus::Failed,
        TransactionStatus::Expired => TransactionFinalStatus::Expired,
        TransactionStatus::AwaitingApproval => TransactionFinalStatus::PendingApproval,
        _ => return, // Don't notify for intermediate states
    };

    let result = TransactionResult {
        tx_id: record.id,
        status: final_status,
        signature: record.signature.as_ref().map(|s| format!("0x{}{}", s.r, s.s)),
        tx_hash: record.tx_hash.clone(),
        error: record.error.clone(),
        reason: record.policy_result.as_ref().map(|p| p.reason.clone()),
    };

    state.notifier.notify(result).await;
}

/// Sign and submit a transaction
async fn sign_and_submit(
    state: &crate::AppState,
    record: &mut TransactionRecord,
) -> Result<()> {
    record.set_status(TransactionStatus::Signing);
    state.storage.update_transaction(record).await?;

    // Check if key is loaded (skip in test mode)
    if !state.config.api.test_mode && !state.is_key_loaded().await {
        return Err(crate::error::StewardError::KeyNotLoaded);
    }

    // Get key share (skip in test mode)
    if !state.config.api.test_mode {
        let key_share_guard: tokio::sync::RwLockReadGuard<'_, Option<kamuy_mpc_core::AgentKeyShare>> = state.key_share.read().await;
        let _key_share = key_share_guard.clone()
            .ok_or(crate::error::StewardError::KeyNotLoaded)?;
        drop(key_share_guard);
    }

    // Compute message hash
    let message = record.request.hash();

    // Create partial signature using MPC
    // This would integrate with the MPC core signing protocol
    // For now, we simulate the signing process
    info!(
        transaction_id = %record.id,
        "Creating partial signature"
    );

    // In a real implementation, this would:
    // 1. Initiate signing protocol with Agent
    // 2. Exchange nonce commitments
    // 3. Create partial signature
    // 4. Combine with Agent's partial
    // 5. Return final signature

    // Simulate successful signing
    let signature = crate::types::TransactionSignature {
        r: hex::encode(&[1u8; 32]),
        s: hex::encode(&[2u8; 32]),
        recid: 0,
        signed_at: chrono::Utc::now(),
    };

    record.signature = Some(signature);
    record.set_status(TransactionStatus::Submitted);
    record.tx_hash = Some(format!("0x{}", hex::encode(&message[..20])));

    state.storage.update_transaction(record).await?;

    info!(
        transaction_id = %record.id,
        tx_hash = %record.tx_hash.as_ref().unwrap(),
        "Transaction submitted"
    );

    // In a real implementation, this would:
    // 1. Submit to relayer (Pimlico)
    // 2. Wait for confirmation
    // 3. Update status to Confirmed

    // Simulate confirmation
    record.set_status(TransactionStatus::Confirmed);
    state.storage.update_transaction(record).await?;

    Ok(())
}

/// Notify user that approval is required
#[cfg(feature = "telegram")]
async fn notify_approval_required(
    state: &crate::AppState,
    record: &TransactionRecord,
) -> Result<()> {
    use crate::telegram::notifications;

    notifications::send_approval_request(
        &state.config.telegram,
        record,
    ).await?;

    Ok(())
}

/// Notify user of rejection
#[cfg(feature = "telegram")]
async fn notify_rejection(
    state: &crate::AppState,
    record: &TransactionRecord,
) -> Result<()> {
    use crate::telegram::notifications;

    notifications::send_rejection(
        &state.config.telegram,
        record,
    ).await?;

    Ok(())
}

/// Handle user approval (called from API or Telegram)
pub async fn handle_user_approval(
    state: &crate::AppState,
    transaction_id: crate::types::TransactionId,
    approved: bool,
    user_id: String,
) -> Result<()> {
    // Load transaction
    let tx_record: Option<TransactionRecord> = state.storage.get_transaction(transaction_id).await?;
    let mut record: TransactionRecord = tx_record
        .ok_or(crate::error::StewardError::NotFound(
            format!("Transaction {} not found", transaction_id)
        ))?;

    // Verify status
    if record.status != TransactionStatus::AwaitingApproval {
        return Err(crate::error::StewardError::Transaction(
            format!("Transaction is not awaiting approval (status: {})", record.status)
        ));
    }

    // Record user decision
    record.user_approval = Some(crate::types::UserApproval {
        approved,
        user_id,
        approved_at: chrono::Utc::now(),
        comment: None,
    });

    if approved {
        info!(
            transaction_id = %transaction_id,
            "Transaction approved by user"
        );

        record.set_status(TransactionStatus::UserApproved);
        state.storage.update_transaction(&record).await?;

        // Sign and submit directly (don't requeue)
        if let Err(e) = sign_and_submit(state, &mut record).await {
            error!("Signing failed after approval: {}", e);
            record.error = Some(e.to_string());
            record.set_status(TransactionStatus::Failed);
            state.storage.update_transaction(&record).await?;
        }

        // Notify completion
        notify_completion(state, &record).await;
    } else {
        info!(
            transaction_id = %transaction_id,
            "Transaction rejected by user"
        );

        record.set_status(TransactionStatus::UserRejected);
        state.storage.update_transaction(&record).await?;

        // Notify rejection
        notify_completion(state, &record).await;
    }

    Ok(())
}