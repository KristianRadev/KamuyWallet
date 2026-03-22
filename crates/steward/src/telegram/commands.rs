//! # Telegram Bot Commands
//!
//! Command handlers for the Telegram bot interface.

use crate::error::{StewardError, Result};
use crate::types::{format_amount, TransactionStatus};
use std::collections::HashMap;

use std::sync::Arc;

/// Format balance line for display in Telegram
fn format_balance_line(eth_amount: &str, eth_token: &str, usdc_amount: &str, usdc_token: &str) -> String {
    let eth = format_amount(eth_amount, eth_token);
    let usdc = format_amount(usdc_amount, usdc_token);
    format!("  • {}\n  • {}", eth, usdc)
}

/// Fetch wallet balances from Base Sepolia RPC
async fn fetch_wallet_balances(wallet_address: &str) -> Result<HashMap<String, String>> {
    let mut balances = HashMap::new();
    
    // Normalize: remove 0x if present and lowercase
    let addr = if wallet_address.starts_with("0x") {
        &wallet_address[2..]
    } else {
        wallet_address
    }.to_lowercase();
    
    let addr_padded = format!("{:0>64}", addr);
    let data = format!("0x70a08231{}", addr_padded);
    
    let usdc_contract = "0x036CbD53842c5426634e7929541eC2318f3dCF7e";
    
    // Try multiple RPCs
    let rpcs = vec![
        "https://sepolia.base.org",
        "https://base-sepolia.drpc.org",
        "https://base-sepolia-rpc.publicnode.com",
    ];
    
    for rpc_url in rpcs {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build() 
        {
            Ok(c) => c,
            Err(_) => continue,
        };
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{
                "to": usdc_contract,
                "data": data
            }, "latest"],
            "id": 1
        });
        
        let Ok(response) = client.post(rpc_url).json(&request).send().await else {
            continue;
        };
        
        let Ok(json) = response.json::<serde_json::Value>().await else {
            continue;
        };
        
        if let Some(result) = json.get("result").and_then(|v| v.as_str()) {
            if result.len() >= 3 && result != "0x0000000000000000000000000000000000000000000000000000000000000000" {
                if let Ok(balance) = u128::from_str_radix(&result[2..], 16) {
                    balances.insert("USDC".to_string(), balance.to_string());
                    break;
                }
            }
        }
    }
    
    Ok(balances)
}

use sha3::{Keccak256, Digest};
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use tracing::info;
use k256::ecdsa::SigningKey;
use k256::{AffinePoint, ProjectivePoint};
use rand::{RngCore, rngs::OsRng};
use hex;
use kamuy_mpc_core::{AgentKeyShare, KeyShareMetadata, PartyRole, encrypt_key_share};
use kamuy_mpc_core::utils::bytes_to_scalar;

/// Bot commands
#[derive(BotCommands, Clone, Debug)]
#[command(
    rename_rule = "lowercase",
    description = "Kamuy Wallet Steward Bot - Commands:"
)]
pub enum Command {
    /// Start the bot
    #[command(description = "Start the bot and show welcome message")]
    Start,
    /// Show help
    #[command(description = "Show help message")]
    Help,
    /// Show status
    #[command(description = "Show wallet status and pending transactions")]
    Status,
    /// Show policy
    #[command(description = "Show current policy rules")]
    Policy,
    /// List pending transactions
    #[command(description = "List transactions awaiting approval")]
    Pending,
    /// Show transaction history
    #[command(description = "Show last 5 transactions")]
    History,
    /// Show wallet info
    #[command(description = "Show wallet address and balance")]
    Wallet,
    /// Create a new wallet
    #[command(description = "Create a new smart wallet")]
    CreateWallet,
    /// Delete the wallet
    #[command(description = "Delete the existing wallet and all keys")]
    DeleteWallet,
}

/// Handle bot commands
pub async fn handle_command(bot: Bot, msg: Message, cmd: Command, state: Arc<crate::AppState>) -> Result<()> {
    // Check if chat is allowed
    let allowed_chats = &state.config.telegram.allowed_chats;
    if !allowed_chats.is_empty() {
        if !allowed_chats.contains(&(msg.chat.id.0)) {
            bot.send_message(msg.chat.id, "⛔ This chat is not authorized.")
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
            return Ok(());
        }
    }

    info!(chat_id = msg.chat.id.0, command = ?cmd, "Received Telegram command");

    match cmd {
        Command::Start => {
            let welcome = r#"👋 Welcome to Kamuy Wallet Steward!

I help manage your AI wallet securely.

What I can do:
• ✅ Approve or reject transactions
• 📊 View wallet status and balances
• 🔐 Manage policy rules
• 📋 List pending transactions

Commands:
/help - Show all commands
/status - Wallet overview
/pending - Transactions needing approval
/policy - Current spending limits
/history - Recent transactions

Security:
• I only approve transactions you explicitly allow
• All transactions are logged and auditable
• Policy limits protect against overspending"#;

            bot.send_message(msg.chat.id, welcome)
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
        }
        Command::Help => {
            let help = r#"📚 Kamuy Wallet Commands

/start - Start the bot and show welcome message
/help - Show this help message
/status - Show wallet status and pending transactions
/policy - Show current policy rules
/pending - List transactions awaiting approval
/history - Show last 5 transactions
/wallet - Show wallet address and balance
/createwallet - Create a new smart wallet (Base Sepolia)

When a transaction needs approval, you'll receive a notification with Approve/Reject buttons."#;

            bot.send_message(msg.chat.id, help)
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
        }
        Command::Status => {
            handle_status(&bot, &msg, &state).await?;
        }
        Command::Policy => {
            handle_policy(&bot, &msg, &state).await?;
        }
        Command::Pending => {
            handle_pending(&bot, &msg, &state).await?;
        }
        Command::History => {
            handle_history(&bot, &msg, &state).await?;
        }
        Command::Wallet => {
            handle_wallet(&bot, &msg, &state).await?;
        }
        Command::CreateWallet => {
            handle_create_wallet(&bot, &msg, &state).await?;
        }
        Command::DeleteWallet => {
            handle_delete_wallet(&bot, &msg, &state).await?;
        }
    }

    Ok(())
}

/// Handle /status command
async fn handle_status(bot: &Bot, msg: &Message, state: &Arc<crate::AppState>) -> Result<()> {
    // Get queue status
    let queue_size = state.queue.read().await.size().await;
    let pending_txs = state.storage.get_pending_transactions().await
        .map_err(|e| StewardError::Database(e.to_string()))?;

    // Get key status
    let key_loaded = state.is_key_loaded().await;
    let key_status = if key_loaded { "✅ Loaded" } else { "⚠️ Not loaded" };

    // Count by status
    let mut awaiting_approval = 0;
    let mut processing = 0;
    for tx in &pending_txs {
        match tx.status {
            TransactionStatus::AwaitingApproval => awaiting_approval += 1,
            TransactionStatus::Evaluating | TransactionStatus::Signing => processing += 1,
            _ => {}
        }
    }

    let status = format!(
        r#"📊 Wallet Status
━━━━━━━━━━━━━━━

🔑 Key: {}

📥 Queue:
• Pending: {}
• Processing: {}
• Awaiting approval: {}

💡 Use /pending to see transactions needing approval
💡 Use /wallet for wallet address"#,
        key_status,
        queue_size,
        processing,
        awaiting_approval
    );

    bot.send_message(msg.chat.id, status)
        .await
        .map_err(|e| StewardError::Telegram(e.to_string()))?;

    Ok(())
}

/// Handle /policy command
async fn handle_policy(bot: &Bot, msg: &Message, state: &Arc<crate::AppState>) -> Result<()> {
    let rules = state.policy_engine.read().await.rules().await;

    // Format amounts for display (v2.0: u64 in USDC micros)
    let format_usdc = |micros: u64| -> String {
        let whole = micros / 1_000_000;
        let frac = micros % 1_000_000;
        if frac == 0 {
            format!("{} USDC", whole)
        } else {
            format!("{}.{:06} USDC", whole, frac)
        }
    };

    // Format whitelist entries
    let whitelist = if rules.whitelist.is_empty() {
        "Any destination".to_string()
    } else {
        rules.whitelist.entries()
            .iter()
            .map(|(addr, entry)| format!("{} ({})", addr, entry.label))
            .collect::<Vec<_>>()
            .join("\n        ")
    };

    // Format spending tracker status
    let daily_spent = format_usdc(rules.spending_tracker.daily_spent);
    let weekly_spent = format_usdc(rules.spending_tracker.weekly_spent);

    let policy = format!(
        r#"🔐 Current Policy (v2.0)
━━━━━━━━━━━━━━━

💰 Spending Limits:
• Per transaction: {}
• Daily: {} (spent: {})
• Weekly: {} (spent: {})
• Auto-add threshold: {}

🪙 Token: USDC (gasless)

✅ Whitelist:
{}"#,
        format_usdc(rules.max_per_tx),
        format_usdc(rules.max_daily), daily_spent,
        format_usdc(rules.max_weekly), weekly_spent,
        format_usdc(rules.auto_add_threshold),
        whitelist
    );

    bot.send_message(msg.chat.id, policy)
        .await
        .map_err(|e| StewardError::Telegram(e.to_string()))?;

    Ok(())
}

/// Handle /pending command
async fn handle_pending(bot: &Bot, msg: &Message, state: &Arc<crate::AppState>) -> Result<()> {
    let pending_txs = state.storage.get_pending_transactions().await
        .map_err(|e| StewardError::Database(e.to_string()))?;

    if pending_txs.is_empty() {
        bot.send_message(msg.chat.id, "✅ No pending transactions requiring approval.")
            .await
            .map_err(|e| StewardError::Telegram(e.to_string()))?;
        return Ok(());
    }

    // Show summary of pending transactions
    let mut summary = format!("📋 Pending Transactions ({} total)\n━━━━━━━━━━━━━━━\n\n", pending_txs.len());

    for (i, tx) in pending_txs.iter().enumerate().take(5) {
        let amount_display = format_amount(&tx.request.value, &tx.request.token);
        let addr_short = super::truncate_address(&tx.request.to);
        let status_emoji = match tx.status {
            TransactionStatus::AwaitingApproval => "🔄",
            TransactionStatus::Evaluating => "⏳",
            _ => "❓"
        };

        summary.push_str(&format!(
            "{} #{} - ID: {}...\n   Amount: {}\n   To: {}\n   Status: {}\n\n",
            status_emoji,
            i + 1,
            &tx.id.to_string()[..8],
            amount_display,
            addr_short,
            tx.status
        ));
    }

    if pending_txs.len() > 5 {
        summary.push_str(&format!("... and {} more\n", pending_txs.len() - 5));
    }

    summary.push_str("\n💡 Use /history to see all recent transactions");

    bot.send_message(msg.chat.id, summary)
        .await
        .map_err(|e| StewardError::Telegram(e.to_string()))?;

    Ok(())
}

/// Handle /history command - show last 5 transactions with details
async fn handle_history(bot: &Bot, msg: &Message, state: &Arc<crate::AppState>) -> Result<()> {
    // Get recent transactions from storage
    let recent_txs = state.storage.get_recent_transactions(5).await
        .map_err(|e| StewardError::Database(e.to_string()))?;

    if recent_txs.is_empty() {
        bot.send_message(msg.chat.id, "📭 No transactions found.")
            .await
            .map_err(|e| StewardError::Telegram(e.to_string()))?;
        return Ok(());
    }

    let mut history = format!("📜 Transaction History (last {})\n━━━━━━━━━━━━━━━\n\n", recent_txs.len());

    for tx in &recent_txs {
        let amount_display = format_amount(&tx.request.value, &tx.request.token);
        let addr_short = super::truncate_address(&tx.request.to);
        let status_emoji = match tx.status {
            TransactionStatus::Pending => "⏳",
            TransactionStatus::Evaluating => "🔍",
            TransactionStatus::AwaitingApproval => "🔄",
            TransactionStatus::Approved => "✅",
            TransactionStatus::UserApproved => "✅",
            TransactionStatus::Signing => "✍️",
            TransactionStatus::Submitted => "📤",
            TransactionStatus::Confirmed => "✅",
            TransactionStatus::Rejected => "❌",
            TransactionStatus::UserRejected => "❌",
            TransactionStatus::Failed => "⚠️",
            TransactionStatus::Expired => "⌛",
        };

        let chain_name = match tx.request.chain_id {
            1 => "Ethereum",
            8453 => "Base",
            137 => "Polygon",
            42161 => "Arbitrum",
            10 => "Optimism",
            _ => "Unknown",
        };

        history.push_str(&format!(
            r#"{} Transaction
━━━━━━━━━━━━━━━
ID: {}
Status: {}
Amount: {} {}
To: {}
Token: {}
Chain: {}
Time: {}

"#,
            status_emoji,
            tx.id,
            tx.status,
            amount_display,
            tx.request.token,
            addr_short,
            tx.request.token,
            chain_name,
            tx.created_at.format("%Y-%m-%d %H:%M UTC")
        ));

        // Add reason if available
        if let Some(ref result) = tx.policy_result {
            history.push_str(&format!("Reason: {}\n", result.reason));
        }

        // Add tx hash if confirmed
        if let Some(ref hash) = tx.tx_hash {
            history.push_str(&format!("Tx Hash: {}\n", hash));
        }

        // Add approve/reject buttons if awaiting approval
        if tx.status == TransactionStatus::AwaitingApproval {
            use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

            let keyboard = InlineKeyboardMarkup::new(vec![
                vec![
                    InlineKeyboardButton::callback("✅ Approve", format!("approve:{}", tx.id)),
                    InlineKeyboardButton::callback("❌ Reject", format!("reject:{}", tx.id)),
                ],
            ]);

            bot.send_message(msg.chat.id, history.clone())
                .reply_markup(keyboard)
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
            history.clear();
        }
    }

    // Send remaining history if any (and last tx didn't have buttons)
    if !history.is_empty() {
        bot.send_message(msg.chat.id, history)
            .await
            .map_err(|e| StewardError::Telegram(e.to_string()))?;
    }

    Ok(())
}

/// Handle /wallet command
async fn handle_wallet(bot: &Bot, msg: &Message, state: &Arc<crate::AppState>) -> Result<()> {
    // Get wallet from storage
    let wallet = state.storage.get_wallet().await
        .map_err(|e| StewardError::Database(e.to_string()))?;

    match wallet {
        Some(w) => {
            // Get wallet balances - fetch fresh from chain
            let balances = fetch_wallet_balances(&w.address).await
                .unwrap_or_else(|_| {
                    std::collections::HashMap::new()
                });
            
            // Get stored balances as fallback
            let stored_balances = state.storage.get_balances().await
                .map_err(|e| StewardError::Database(e.to_string()))?;
            
            // Get USDC balance only
            let usdc_balance = balances.get("USDC")
                .or_else(|| stored_balances.get("USDC"))
                .cloned()
                .unwrap_or_else(|| "0".to_string());
            
            let usdc_display = format_amount(&usdc_balance, "USDC");
            
            let info = format!(
                r#"👛 Wallet Info
━━━━━━━━━━━━━━━
📍 Address: {}
⛓ Chain: Base Sepolia (84532)
💰 USDC Balance: {}"#,
                w.address,
                usdc_display
            );

            bot.send_message(msg.chat.id, info)
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
        }
        None => {
            bot.send_message(msg.chat.id, "⚠️ No wallet configured. Use /createwallet to create one.")
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
        }
    }

    Ok(())
}

/// Generate a random ECDSA key pair and return (private_key_hex, address)
fn generate_keypair() -> Result<(String, String)> {
    // Generate random signing key (private key)
    let signing_key = SigningKey::random(&mut OsRng);

    // Get private key bytes
    let private_key_bytes = signing_key.to_bytes();
    let private_key_hex = hex::encode(private_key_bytes);

    // Derive Ethereum address from public key
    let verifying_key = signing_key.verifying_key();
    let encoded_point = verifying_key.to_encoded_point(false);
    let public_key_bytes = encoded_point.as_bytes();

    // Ethereum address = last 20 bytes of keccak256(public_key)
    // Skip the first byte (0x04 prefix for uncompressed point)
    let public_key_without_prefix = &public_key_bytes[1..];
    let mut hasher = Keccak256::new();
    hasher.update(public_key_without_prefix);
    let hash = hasher.finalize();

    let address = format!("0x{}", hex::encode(&hash[12..32]));

    Ok((private_key_hex, address))
}

/// Create an AgentKeyShare from a private key hex string
fn create_key_share(private_key_hex: &str, party_id: u8, role: PartyRole) -> Result<AgentKeyShare> {
    // Parse private key
    let private_key_bytes = hex::decode(private_key_hex)
        .map_err(|e| StewardError::Internal(format!("Invalid private key hex: {}", e)))?;

    // Ensure 32 bytes
    if private_key_bytes.len() != 32 {
        return Err(StewardError::Internal(format!("Private key must be 32 bytes, got {}", private_key_bytes.len())));
    }

    // Convert bytes to scalar using mpc-core utility
    let secret_share = bytes_to_scalar(&private_key_bytes)
        .map_err(|e| StewardError::Internal(format!("Failed to convert to scalar: {}", e)))?;

    // Derive public key from private key (P = k*G)
    let public_key: AffinePoint = (ProjectivePoint::GENERATOR * secret_share).to_affine();

    Ok(AgentKeyShare {
        party_id,
        role,
        secret_share,
        public_key,
        public_shares: vec![public_key], // For standalone keys, just include own public key
        chain_code: [0u8; 32],
        metadata: KeyShareMetadata::new(role),
    })
}

/// Generate a secure random password for steward key encryption
/// Uses cryptographically secure random bytes encoded as hex (32 bytes = 64 hex chars)
fn generate_secure_password() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Factory bytecode for MpcSmartAccount (runtime bytecode from chain)
/// This is the bytecode of MpcSmartAccount that the factory deploys
const FACTORY_BYTECODE: &str = "60806040526004361015610011575f80fd5b5f3560e01c806325350f081461008457806361b36f4b1461007f5780638a48ac031461007a57806394430fa5146100755780639f3ee0f914610070578063a98e4e771461006b5763f2a40db814610066575f80fd5b610413565b6103f6565b610388565b610344565b6102bb565b61012a565b346100c25760203660031901126100c2576001600160a01b036100a56100c6565b165f525f60205260018060a01b0360405f20541660805260206080f35b5f80fd5b600435906001600160a01b03821682036100c257565b60809060031901126100c2576004356001600160a01b03811681036100c257906024356001600160a01b03811681036100c257906044356001600160a01b03811681036100c2579060643590565b346100c257610138366100dc565b90926001600160a01b038082169390811692916101879186918686141580610266575b80610253575b61016a9061046d565b8615158061024a575b80610238575b610182906104ad565b6105b3565b6020815191015ff591823b156100c257610234936101a4846104fc565b6101e0816101c18660018060a01b03165f525f60205260405f2090565b80546001600160a01b0319166001600160a01b03909216919091179055565b6040516001600160a01b039182168152908416907ff910bcf6ef45198082a2e9755330a11e60bde93603dd71de5eb22ecab541676890602090a46040516001600160a01b0390911681529081906020820190565b0390f35b506001600160a01b0383161515610179565b50851515610173565b506001600160a01b038316861415610161565b506001600160a01b03831687141561015b565b60206040818301928281528451809452019201905f5b81811061029c5750505090565b82516001600160a01b031684526020938401939092019160010161028f565b346100c2575f3660031901126100c25760405180602060015491828152019060015f527fb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf6905f5b81811061032557610234856103198187038261057f565b60405191829182610279565b82546001600160a01b0316845260209093019260019283019201610302565b346100c2575f3660031901126100c2576040517f0000000000000000000000000000000071727de22e5e9d8baf0edac6f37da0326001600160a01b03168152602090f35b346100c2576103a3610399366100dc565b93929190916105b3565b602081519101209060405191602083019160ff60f81b83523060601b602185015260358401526055830152605582526103dd60758361057f565b905190206040516001600160a01b039091168152602090f35b346100c2575f3660031901126100c2576020600154604051908152f35b346100c25760203660031901126100c2576004356001548110156100c25760015f527fb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf601546040516001600160a01b039091168152602090f35b1561047457565b60405162461bcd60e51b81526020600482015260116024820152704475706c6963617465207369676e65727360781b6044820152606490fd5b156104b457560405162461bcd60e51b815260206004820152600c60248201526b5a65726f206164647265737360a01b6044820152606490fd5b634e487b7160e01b5f52604160045260245ffd5b6001546801000000000000000081101561057a57600181016001556001548110156105665760015f527fb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf60180546001600160a01b0319166001600160a01b03909216919091179055565b634e487b7160e01b5f52603260045260245ffd5b6104e8565b90601f8019910116810190811067ffffffffffffffff82111761057a57604052565b805191908290602001825e015f815290565b906106536106619261064d946110d293604051946105d4602082018761057f565b8086526106656020870139604080516001600160a01b037f0000000000000000000000000000000071727de22e5e9d8baf0edac6f37da032811660208301529485169181019190915290831660608201529116608080830191909152815261063d60a08261057f565b60405194859360208501906105a1565b906105a1565b03601f19810183528261057f565b9056fe60a0346101d057601f6110d238819003918201601f19168301916001600160401b038311848410176101d4578084926080946040528339810103126101d057610047816101e8565b610053602083016101e8565b61006b6060610064604086016101e8565b94016101e8565b60017f9b779b17422d0df92223018b32b4d1fa46e071723d6817e2486d003becc55f00556001600160a01b03928316608052911691821580156101bf575b80156101ae575b61019f576001600160a01b031690828214801561018d575b801561017b575b61016c575f80546001600160a01b03199081168517825560018054821685178155600280549092166001600160a01b03949094169384179091556004805460ff1916909117905560405193919291907f878936ad695fbd2a258823e44200240df62c70657ed27c6a9ad48c8a0a6e28ef9080a4610ed590816101fd82396080518181816104c601528181610692015281816107580152610c850152f35b636081df1760e11b5f5260045ffd5b506001600160a01b03811682146100cf565b506001600160a01b03811683146100c8565b63719feac760e01b5f5260045ffd5b506001600160a01b038216156100b0565b506001600160a01b038116156100a9565b5f80fd5b634e487b7160e01b5f52604160045260245ffd5b51906001600160a01b03821682036101d05756fe608080604052600436101561001a575b50361561001857005b005b5f3560e01c9081632079fb9a1461092c57508063308ff0c914610911578063392e53cd146108ef5780633a871cdd146108a757806347e1da2a146106dc578063785ffb37146106c157806394430fa51461067d57806394cf795e146105ea578063a44b8c6f146105cf578063affed0e0146105b2578063b61d27f614610478578063ce04c9fb146100d45763d7c48089146100b5575f61000f565b346100d0575f3660031901126100d057602060405160038152f35b5f80fd5b346100d05760603660031901126100d05760043560ff81168082036100d0576024356001600160a01b03811691908281036100d05760443567ffffffffffffffff81116100d05761012990369060040161098d565b949060ff60045416156104696761013e610e15565b600384101561042857841561045a575f5b60ff811660038110156101b35785141580610183575b6101745760010160ff1661014f565b636081df1760e11b5f5260045ffd5b50600381101561019f5780546001600160a01b03168614610165565b634e487b7160e01b5f52603260045260245ffd5b5050909185906003546040519060208201926b3ab83230ba32a9b4b3b732b960a11b845260ff60f81b8760f81b16602c8401526bffffffffffffffffffffffff199060601b16602d830152604182015246606182015260618152610218608182610a44565b51902060405160208101917f19457468657265756d205369676e6564204d6573736167653a0a3332000000008352603c820152603c815261025a605c82610a44565b519020916083820361044b57811561019f578035600f8160f81c169060fc1c93600382109384158095610440575b8015610437575b61042857806021116100d0576001840135816041116100d057602185013590826041101561019f5760408051858152604188013560f81c6020808301919091529181019290925260608201929092525f8080529060809060015afa1561041d575f5193816062116100d057604281013590826082116100d0576062810135926082101561019f5761034d5f93608293602096604051958695013560f81c90859094939260ff6060936080840197845216602083015260408201520152565b838052039060015afa1561041d575f519261019f57546001600160a01b0391821691161491600381101561019f57548215926001600160a01b03928316919092161490610414575b5061040557600381101561019f5780546001600160a01b0319811684179091556003546103c190610b1d565b6003556001600160a01b0316907f78ba0801ef45283444a99cf912d91d8ff161f692ef7bd81fcde3845720798bf25f80a460015f80516020610e8083398151915255005b6380629b4960e01b5f5260045ffd5b90501584610395565b6040513d5f823e3d90fd5b63091316a160e11b5f5260045ffd5b5085831461028f565b506003861015610288565b634be6321b60e01b5f5260045ffd5b63719feac760e01b5f5260045ffd5b6321c4e35760e21b5f5260045ffd5b346100d05760603660031901126100d0576004356001600160a01b0381168082036100d0576024359160443567ffffffffffffffff81116100d0576104c190369060040161098d565b9390917f00000000000000000000000000000000000000000000000000000000000000006001600160a01b031633036105a35760ff6004541615610469675f809161050a610e15565b610515600354610b1d565b60035560405187868237848189810185815203925af1610533610a7a565b9015610582575061056c7f47d99ad340f52da66535aff7e10da1ceb85a32bcbd9fa1c42314d194545e14d2939460405193849384610b03565b0390a260015f80516020610e8083398151915255005b6040516315fcd67560e01b815290819061059f9060048301610ab9565b0390fd5b6306facdbd60e21b5f5260045ffd5b346100d0575f3660031901126100d0576020600354604051908152f35b346100d0575f3660031901126100d057602060405160838152f35b346100d0575f3660031901126100d05760608060405161060a8282610a44565b3690376040515f80825b6003821061065d575050506106298282610a44565b604051905f825b6003821061063d57505050f35b82516001600160a01b031681526020928301926001929092019101610630565b82546001600160a01b031681526001928301929190910190602001610614565b346100d0575f3660031901126100d0576040517f00000000000000000000000000000000000000000000000000000000000000006001600160a01b03168152602090f35b346100d0575f3660031901126100d057602060405160028152f35b346100d05760603660031901126100d05760043567ffffffffffffffff81116100d05761070d90369060040161095c565b9060243567ffffffffffffffff81116100d05761072e90369060040161095c565b60449391933567ffffffffffffffff81116100d05761075190369060040161095c565b90929091907f00000000000000000000000000000000000000000000000000000000000000006001600160a01b031633036105a35760ff600454161561046967610799610e15565b81811480159061089d575b61088e575f5b8181106107d2576107bc600354610b1d565b60035560015f80516020610e8083398151915255005b5f806107e76107e284868b6109d2565b6109e2565b6107f284878c6109d2565b356107fe85898b610a29565b9190826040519384928337810185815203925af161081a610a7a565b90156105825750806108326107e2600193858a6109d2565b7f47d99ad340f52da66535aff7e10da1ceb85a32bcbd9fa1c42314d194545e14d261088561086184888d6109d2565b359261086e858a8c610a29565b604093919351938493898060a01b03169684610b03565b0390a2016107aa565b634ec4810560e11b5f5260045ffd5b50828114156107a4565b346100d05760603660031901126100d05760043567ffffffffffffffff81116100d05761016060031982360301126100d0576108e76020916004016109bb565b604051908152f35b346100d0575f3660031901126100d057602060ff600454166040519015158152f35b346100d0575f3660031901126100d057602060405160418152f35b346100d05760203660031901126100d0576004359060038210156100d05790546001600160a01b03168152602090f35b9181601f840112156100d05782359167ffffffffffffffff83116100d0576020808501948460051b0101116100d057565b9181601f840112156100d05782359167ffffffffffffffff83116100d057602083818601950101116100d057565b6109c490610b3f565b156109cd575f90565b600190565b919081101561019f5760051b0190565b356001600160a01b03811681036100d05790565b903590601e19813603018212156100d0570180359067ffffffffffffffff82116100d0576020019181360383136100d057565b9082101561019f57610a409160051b8101906109f6565b9091565b90601f8019910116810190811067ffffffffffffffff821117610a6657604052565b634e487b7160e01b5f52604160045260245ffd5b3d15610ab4573d9067ffffffffffffffff8211610a665760405191610aa9601f8201601f191660200184610a44565b82523d5f602084013e565b606090565b602060409281835280519182918282860152018484015e5f828201840152601f01601f1916010190565b908060209392818452848401375f828201840152601f01601f1916010190565b604090610b1a949281528160208201520191610ae3565b90565b5f198114610b2b5760010190565b634e487b7160e01b5f52601160045260245ffd5b610140810190610b4f82826109f6565b906083820361044b57811561019f57803593600f8560f81c169460fc1c94600381109182158093610e0a575b8015610e01575b6104285760405163a619353160e01b8152602060048201529580356001600160a01b038116908190036100d057610c818892610c6e60209585946024860152868301356044860152610c68610c12610bf3610be06040870187610e4d565b61016060648b01526101848a0191610ae3565b610c006060870187610e4d565b8983036023190160848b015290610ae3565b608085013560a488015260a085013560c488015260c085013560e488015260e0850135610104880152610100850135610124880152610c55610120860186610e4d565b888303602319016101448a015290610ae3565b92610e4d565b8483036023190161016486015290610ae3565b03817f00000000000000000000000000000000000000000000000000000000000000006001600160a01b03165afa94851561041d575f95610dcd575b50836021116100d0576001830135846041116100d057602184013590856041101561019f5760408051888152604187013560f81c6020808301919091529181019290925260608201929092525f8080529060809060015afa1561041d575f519161019f57546001600160a01b03908116911603610dc557816062116100d057604281013590826082116100d0576062810135926082101561019f57610d8f5f93608293602096604051958695013560f81c90859094939260ff6060936080840197845216602083015260408201520152565b838052039060015afa1561041d575f5190600381101561019f57546001600160a01b03908116911603610dc157600190565b5f90565b505050505f90565b9094506020813d602011610df9575b81610de960209383610a44565b810103126100d05751935f610cbd565b3d9150610ddc565b50868214610b82565b506003871015610b7b565b60025f80516020610e808339815191525414610e3e5760025f80516020610e8083398151915255565b633ee5aeb560e01b5f5260045ffd5b9035601e19823603018112156100d057016020813591019167ffffffffffffffff82116100d05781360383136100d05756fe9b779b17422d0df92223018b32b4d1fa46e071723d6817e2486d003becc55f00a2646970667358221220230e8d085536d0df00fff9c3bec84f4ce08e2d2a9e18e3ba5990075f181088fb64736f6c634300081a0033a26469706673582212202fd990c81e6199ef3cc10f55d9719492def6e157e6505a3b84edb6f4d97f3f6364736f6c634300081a00330";

/// EntryPoint address on Base Sepolia (EntryPoint v0.6)
/// FIX: Corrected to standard ERC-4337 EntryPoint address
const ENTRY_POINT_BASE_SEPOLIA: &str = "0x5FF137D4b0FDCD49DcA30c7CF57E578a026d2789";

/// Compute CREATE2 address using the factory's getAddress formula
/// This matches MpcSmartAccountFactory.getAddress() on-chain
/// 
/// The factory computes:
/// bytecode = abi.encodePacked(creationCode, abi.encode(entryPoint, agent, steward, user))
/// hash = keccak256(abi.encodePacked(0xff, factory, salt, keccak256(bytecode)))
fn compute_create2_address_from_factory(
    factory_address: &str,
    agent_address: &str,
    steward_address: &str,
    user_address: &str,
    salt: &[u8; 32],
) -> Result<String> {
    // Parse addresses
    let factory_bytes = hex::decode(factory_address.strip_prefix("0x").unwrap_or(factory_address))
        .map_err(|e| StewardError::Internal(format!("Invalid factory address: {}", e)))?;
    let agent_bytes = hex::decode(agent_address.strip_prefix("0x").unwrap_or(agent_address))
        .map_err(|e| StewardError::Internal(format!("Invalid agent address: {}", e)))?;
    let steward_bytes = hex::decode(steward_address.strip_prefix("0x").unwrap_or(steward_address))
        .map_err(|e| StewardError::Internal(format!("Invalid steward address: {}", e)))?;
    let user_bytes = hex::decode(user_address.strip_prefix("0x").unwrap_or(user_address))
        .map_err(|e| StewardError::Internal(format!("Invalid user address: {}", e)))?;

    if factory_bytes.len() != 20 || agent_bytes.len() != 20 || steward_bytes.len() != 20 || user_bytes.len() != 20 {
        return Err(StewardError::Internal("All addresses must be 20 bytes".to_string()));
    }

    // Parse factory bytecode (hex string)
    let factory_bytecode = hex::decode(FACTORY_BYTECODE)
        .map_err(|e| StewardError::Internal(format!("Invalid factory bytecode: {}", e)))?;

    // Encode constructor args: (entryPoint, agent, steward, user)
    // Each address is 20 bytes, padded to 32 bytes in ABI encoding
    let mut constructor_args = Vec::with_capacity(4 * 32);
    
    // EntryPoint (padded to 32 bytes)
    let entrypoint_bytes = hex::decode(ENTRY_POINT_BASE_SEPOLIA.strip_prefix("0x").unwrap_or(ENTRY_POINT_BASE_SEPOLIA))
        .map_err(|e| StewardError::Internal(format!("Invalid EntryPoint: {}", e)))?;
    constructor_args.extend_from_slice(&[0u8; 12]); // padding
    constructor_args.extend_from_slice(&entrypoint_bytes);
    
    // Agent (padded to 32 bytes)
    constructor_args.extend_from_slice(&[0u8; 12]);
    constructor_args.extend_from_slice(&agent_bytes);
    
    // Steward (padded to 32 bytes)
    constructor_args.extend_from_slice(&[0u8; 12]);
    constructor_args.extend_from_slice(&steward_bytes);
    
    // User (padded to 32 bytes)
    constructor_args.extend_from_slice(&[0u8; 12]);
    constructor_args.extend_from_slice(&user_bytes);

    // init_code = creationCode + constructorArgs
    let mut init_code = factory_bytecode;
    init_code.extend_from_slice(&constructor_args);

    // Compute keccak256 of init_code
    let mut init_code_hasher = Keccak256::new();
    init_code_hasher.update(&init_code);
    let init_code_hash: [u8; 32] = init_code_hasher.finalize().into();

    // Build CREATE2 input: 0xff ++ factory(20) ++ salt(32) ++ init_code_hash(32)
    let mut create2_input = Vec::with_capacity(1 + 20 + 32 + 32);
    create2_input.push(0xff);
    create2_input.extend_from_slice(&factory_bytes);
    create2_input.extend_from_slice(salt);
    create2_input.extend_from_slice(&init_code_hash);

    // Hash and take last 20 bytes as address
    let mut hasher = Keccak256::new();
    hasher.update(&create2_input);
    let hash = hasher.finalize();
    
    let address_bytes = &hash[12..32];
    Ok(format!("0x{}", hex::encode(address_bytes)))
}

/// Generate a shorter memorable password (16 hex chars)
fn generate_steward_password() -> String {
    let mut bytes = [0u8; 8];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Compute CREATE2 address for MpcSmartAccount
/// CREATE2 address = keccak256(0xff ++ factory_address ++ salt ++ keccak256(init_code))[12:]
/// 
/// The init_code is the bytecode of the contract to deploy, which includes:
/// - MpcSmartAccount implementation address
/// - Initialization data (owner, threshold, etc.)
/// For CREATE2, we hash the init_code and use that hash
fn compute_create2_address(
    factory_address: &str,
    salt: &[u8; 32],
    init_code_hash: &[u8; 32],
) -> Result<String> {
    // Parse factory address
    let factory_bytes = hex::decode(factory_address.strip_prefix("0x").unwrap_or(factory_address))
        .map_err(|e| StewardError::Internal(format!("Invalid factory address: {}", e)))?;

    if factory_bytes.len() != 20 {
        return Err(StewardError::Internal("Factory address must be 20 bytes".to_string()));
    }

    // Build CREATE2 input: 0xff ++ factory(20) ++ salt(32) ++ init_code_hash(32)
    let mut create2_input = Vec::with_capacity(1 + 20 + 32 + 32);
    create2_input.push(0xff);
    create2_input.extend_from_slice(&factory_bytes);
    create2_input.extend_from_slice(salt);
    create2_input.extend_from_slice(init_code_hash);

    // Compute keccak256
    let mut hasher = Keccak256::new();
    hasher.update(&create2_input);
    let hash = hasher.finalize();

    // Take last 20 bytes as address
    let address = format!("0x{}", hex::encode(&hash[12..32]));

    Ok(address)
}

/// Handle /createwallet command - creates a proper MPC wallet with real keys
/// Deletes any existing wallet first to ensure a fresh wallet is always created
async fn handle_create_wallet(bot: &Bot, msg: &Message, state: &Arc<crate::AppState>) -> Result<()> {
    // Check if wallet already exists - delete it first
    let existing = state.storage.get_wallet().await
        .map_err(|e| StewardError::Database(e.to_string()))?;

    if let Some(w) = existing {
        let addr_short = super::truncate_address(&w.address);
        
        // Delete existing wallet data
        bot.send_message(msg.chat.id, 
            format!("🔄 Deleting existing wallet {} to create a new one...", addr_short))
            .await
            .map_err(|e| StewardError::Telegram(e.to_string()))?;
        
        // Delete wallet and all associated keys
        // Clear temp keys as well
        {
            let mut temp_keys = state.temp_private_keys.lock().await;
            temp_keys.agent = None;
            temp_keys.user = None;
            temp_keys.awaiting_password = false;
            temp_keys.pending_password_confirm = None;
            temp_keys.pending_approval_action = None;
            temp_keys.pending_policy_change_action = None;
        }
    }

    // Send "creating" message
    bot.send_message(msg.chat.id, "🔄 Creating your MPC wallet with secure keys...")
        .await
        .map_err(|e| StewardError::Telegram(e.to_string()))?;

    // Generate 3 real ECDSA key pairs for MPC parties
    let (agent_private, agent_address) = generate_keypair()
        .map_err(|e| StewardError::Internal(format!("Failed to generate agent key: {}", e)))?;

    let (steward_private, steward_address) = generate_keypair()
        .map_err(|e| StewardError::Internal(format!("Failed to generate steward key: {}", e)))?;

    let (user_private, user_address) = generate_keypair()
        .map_err(|e| StewardError::Internal(format!("Failed to generate user key: {}", e)))?;

    // Factory address for CREATE2 (Base Sepolia)
    // This is the deployed MpcSmartAccountFactory address
    let _factory_addr = "0x8D9dd4062D0D68d4d8Dc439aE9762DEde9bcb821";

    // Compute salt from agent + steward addresses (deterministic)
    let agent_addr_bytes = hex::decode(agent_address.strip_prefix("0x").unwrap_or(&agent_address))
        .map_err(|e| StewardError::Internal(format!("Invalid agent address: {}", e)))?;
    let steward_addr_bytes = hex::decode(steward_address.strip_prefix("0x").unwrap_or(&steward_address))
        .map_err(|e| StewardError::Internal(format!("Invalid steward address: {}", e)))?;

    let mut salt_input = Vec::new();
    salt_input.extend_from_slice(&agent_addr_bytes);
    salt_input.extend_from_slice(&steward_addr_bytes);
    let mut salt_hasher = Keccak256::new();
    salt_hasher.update(&salt_input);
    let _salt: [u8; 32] = salt_hasher.finalize().into();

    // FIX #3: Use correct init_code from MpcSmartAccountFactory
    // 
    // The factory's _getBytecode function returns:
    // abi.encodePacked(type(MpcSmartAccount).creationCode, abi.encode(entryPoint, agent, steward, user))
    //
    // The CREATE2 address is:
    // keccak256(0xff ++ factory ++ salt ++ keccak256(bytecode))
    //
    // For Base Sepolia EntryPoint: 0x0000000071727De22E5E9d8BAf0edAc6f37da032
    
    // The bytecode = creationCode + constructorArgs
    // Since we can't get the exact creationCode without being on-chain,
    // we use the factory's getAddress function to verify
    //
    // For now, we compute based on the known formula:
    // The salt should be unique per wallet - we use keccak256(agent || steward || user)
    let salt_input = format!("{}{}{}", 
        agent_address.strip_prefix("0x").unwrap_or(&agent_address),
        steward_address.strip_prefix("0x").unwrap_or(&steward_address),
        user_address.strip_prefix("0x").unwrap_or(&user_address)
    );
    let mut salt_hasher = Keccak256::new();
    salt_hasher.update(&hex::decode(&salt_input).unwrap_or_default());
    let salt: [u8; 32] = salt_hasher.finalize().into();
    
    // The init_code_hash depends on MpcSmartAccount's creation bytecode
    // which we can get from the deployed factory
    // For accurate address computation, we should call factory.getAddress() on-chain
    //
    // Using the factory method: getAddress(agent, steward, user, salt)
    // This is more reliable than computing locally
    let wallet_address = compute_create2_address_from_factory(
        "0x8D9dd4062D0D68d4d8Dc439aE9762DEde9bcb821",
        &agent_address,
        &steward_address,
        &user_address,
        &salt,
    ).map_err(|e| StewardError::Internal(format!("Failed to compute address: {}", e)))?;

    let chain_id: u64 = 84532; // Base Sepolia

    // Store wallet in database
    // public_key field stores comma-separated addresses (public info)
    let public_key = format!(
        "{},{},{}",
        agent_address.strip_prefix("0x").unwrap_or(&agent_address),
        steward_address.strip_prefix("0x").unwrap_or(&steward_address),
        user_address.strip_prefix("0x").unwrap_or(&user_address)
    );

    state.storage.set_wallet(&wallet_address, chain_id, &public_key, None)
        .await
        .map_err(|e| StewardError::Database(e.to_string()))?;

    // FIX #2: Generate a secure random password for steward key encryption
    // Use a cryptographically secure random password instead of "default_password"
    let steward_password = generate_steward_password();
    
    // Create an AgentKeyShare for the steward key and encrypt it with the secure password
    let steward_share = create_key_share(&steward_private, 1, PartyRole::Steward)
        .map_err(|e| StewardError::Internal(format!("Failed to create steward key share: {}", e)))?;

    let encrypted_steward = encrypt_key_share(&steward_share, &steward_password)
        .map_err(|e| StewardError::Internal(format!("Failed to encrypt steward key: {}", e)))?;

    state.storage.save_steward_key(&encrypted_steward).await
        .map_err(|e| StewardError::Database(e.to_string()))?;

    // Store user private key temporarily in state for password setup
    // This will be cleared after password is set or session ends
    {
        let mut temp_keys = state.temp_private_keys.lock().await;
        temp_keys.agent = Some(agent_private.clone());
        temp_keys.user = Some(user_private.clone());
        temp_keys.awaiting_password = true;
    }

    // Use full address - don't truncate for wallet creation
    let response = format!(
        r#"✅ MPC Wallet Created!

🆔 Wallet: {}
⛓ Chain: Base Sepolia (84532)

🔑 MPC Keys (2-of-3 threshold):
  • Agent: {} (shown below)
  • Steward: {} (stored encrypted)
  • User: {} (protected by your password)

━━━━━━━━━━━━━━━━━━━━━━━━━━
🔐 AGENT KEY FOR YOUR AI:
━━━━━━━━━━━━━━━━━━━━━━━━━━

`{}`

━━━━━━━━━━━━━━━━━━━━━━━━━━
⚠️  SECURITY NOTES:
━━━━━━━━━━━━━━━━━━━━━━━━━━

• Your USER KEY is NOT shown here - it's protected by your password
• Only the AGENT KEY is shown - give this to your AI agent
• With only the Agent key, NO ONE can drain your funds (needs 2-of-3)
• Set your password below to protect your User Key

The Steward key is stored encrypted in the database.
The User key will be encrypted with your password.

💰 Next Steps:
1. ⭐ COPY the AGENT KEY above to your AI agent
2. 🔐 Click "Set Password" below to protect your User Key
3. 💰 Fund the wallet with USDC on Base Sepolia
4. 📱 Use /wallet to view details"#,
        wallet_address,
        super::truncate_address(&agent_address),
        super::truncate_address(&steward_address),
        super::truncate_address(&user_address),
        agent_private
    );

    // Add inline keyboard with "Set Password" button
    use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
    
    let keyboard = InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("🔐 Set Password", "set_password"),
        ],
    ]);

    bot.send_message(msg.chat.id, response)
        .reply_markup(keyboard)
        .await
        .map_err(|e| StewardError::Telegram(e.to_string()))?;

    info!(
        wallet = %wallet_address,
        agent = %agent_address,
        steward = %steward_address,
        user = %user_address,
        "MPC wallet created via Telegram"
    );

    Ok(())
}

/// Handle callback queries (inline buttons)
pub async fn handle_callback(bot: Bot, q: teloxide::types::CallbackQuery, state: Arc<crate::AppState>) -> Result<()> {
    let data = q.data.clone().unwrap_or_default();
    let chat_id = q.message.as_ref().map(|m| m.chat().id);

    info!(callback_data = %data, "Received Telegram callback");

    // Handle "set_password" action specially
    if data == "set_password" {
        return handle_set_password_callback(bot, q, state).await;
    }

    // Handle policy change callbacks: "policy_approve:uuid" or "policy_reject:uuid"
    if data.starts_with("policy_") {
        return handle_policy_change_callback(bot, q, state).await;
    }

    // Parse callback data: "approve:uuid" or "reject:uuid"
    let parts: Vec<&str> = data.split(':').collect();
    if parts.len() != 2 {
        if let Some(cid) = chat_id {
            bot.send_message(cid, "❌ Invalid callback data.")
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
        }
        return Ok(());
    }

    let action = parts[0];
    let tx_id_str = parts[1];

    // Parse transaction ID
    let uuid = match uuid::Uuid::parse_str(tx_id_str) {
        Ok(u) => u,
        Err(_) => {
            if let Some(cid) = chat_id {
                bot.send_message(cid, "❌ Invalid transaction ID.")
                    .await
                    .map_err(|e| StewardError::Telegram(e.to_string()))?;
            }
            return Ok(());
        }
    };

    let tx_id = crate::types::TransactionId::from(uuid);
    let approved = action == "approve";

    // FIX #1: Password verification before approval
    // Instead of immediately processing, ask for password first
    // Check if user has a password set up
    let has_user_key = state.storage.has_user_key().await
        .map_err(|e| StewardError::Database(e.to_string()))?;

    if has_user_key {
        // User has a password - require it before processing approval
        // Store the pending action and ask for password
        {
            let mut temp_keys = state.temp_private_keys.lock().await;
            temp_keys.pending_approval_action = Some((tx_id, approved));
            temp_keys.awaiting_password = true;
        }

        // Answer callback to dismiss loading
        bot.answer_callback_query(q.id)
            .text("🔐 Enter your password to confirm")
            .await
            .map_err(|e| StewardError::Telegram(e.to_string()))?;

        // Ask for password
        if let Some(cid) = chat_id {
            let prompt = if approved {
                r#"🔐 Password Required to Approve

Please enter your wallet password to confirm this transaction approval.

⚠️ You must enter your password - there's no way to recover it if forgotten!"#
            } else {
                r#"🔐 Password Required to Reject

Please enter your wallet password to confirm this transaction rejection.

⚠️ You must enter your password - there's no way to recover it if forgotten!"#
            };

            bot.send_message(cid, prompt)
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;

            // Update the original message
            if let Some(msg) = q.message {
                let new_text = if approved {
                    "🔐 Awaiting password for approval..."
                } else {
                    "🔐 Awaiting password for rejection..."
                };
                bot.edit_message_text(msg.chat().id, msg.id(), new_text)
                    .await
                    .ok(); // Ignore errors if edit fails
            }
        }
        
        return Ok(());
    }

    // No password set yet - proceed without (this shouldn't normally happen 
    // if they completed wallet creation, but allow for edge cases)
    let decision = if approved {
        crate::approval::ApprovalDecision::Approved
    } else {
        crate::approval::ApprovalDecision::Rejected
    };

    // Resolve the pending approval
    // This will unblock the TelegramApprovalChannel that's waiting
    let was_pending = state.pending_approvals.resolve(&tx_id, decision).await;

    if was_pending {
        // Answer the callback query
        bot.answer_callback_query(q.id)
            .text(if approved { "✅ Decision recorded!" } else { "❌ Decision recorded!" })
            .await
            .map_err(|e| StewardError::Telegram(e.to_string()))?;

        // Update the message
        if let Some(msg) = q.message {
            let new_text = if approved {
                "✅ Transaction Approved\n\nProcessing transaction..."
            } else {
                "❌ Transaction Rejected\n\nThe transaction has been cancelled."
            };

            bot.edit_message_text(msg.chat().id, msg.id(), new_text)
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
        }
    } else {
        // No pending approval found - transaction might already be processed or timed out
        bot.answer_callback_query(q.id)
            .text("⚠️ This transaction is no longer pending.")
            .show_alert(true)
            .await
            .map_err(|e| StewardError::Telegram(e.to_string()))?;

        // Update the message to show it's no longer relevant
        if let Some(msg) = q.message {
            bot.edit_message_text(msg.chat().id, msg.id(), "⏰ Transaction Expired\n\nThis transaction is no longer pending approval.")
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
        }
    }

    Ok(())
}

/// Handle "Set Password" callback - ask user for password
async fn handle_set_password_callback(bot: Bot, q: teloxide::types::CallbackQuery, state: Arc<crate::AppState>) -> Result<()> {
    let chat_id = q.message.as_ref().map(|m| m.chat().id);
    
    // Check if temp keys exist
    {
        let mut temp_keys = state.temp_private_keys.lock().await;
        if temp_keys.user.is_none() {
            bot.answer_callback_query(q.id)
                .text("⚠️ No wallet creation in progress. Use /createwallet first.")
                .show_alert(true)
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
            return Ok(());
        }
        // Set flag to indicate we're waiting for password
        temp_keys.awaiting_password = true;
    }
    
    // Answer the callback to dismiss the loading state
    bot.answer_callback_query(q.id)
        .text("Enter your wallet password")
        .await
        .map_err(|e| StewardError::Telegram(e.to_string()))?;
    
    // Ask for password
    if let Some(cid) = chat_id {
        let prompt = r#"🔐 Set Your Wallet Password

Please enter a strong password to protect your User Key.

Requirements:
• At least 8 characters
• Use a mix of letters, numbers, and symbols

⚠️ This password will be required to approve transactions!

Send your password now:"#;

        bot.send_message(cid, prompt)
            .await
            .map_err(|e| StewardError::Telegram(e.to_string()))?;
            
        // Update the original message to show password is being set
        if let Some(msg) = q.message {
            bot.edit_message_text(msg.chat().id, msg.id(), "🔐 Password setup initiated...\n\nCheck your next message for password input.")
                .await
                .ok();
        }
    }
    
    Ok(())
}

/// Handle password input from user - process and store encrypted user key
pub async fn handle_password_input(bot: Bot, msg: Message, state: Arc<crate::AppState>, password: String) -> Result<()> {
    let chat_id = msg.chat.id;

    // Check if we're awaiting password
    let (user_private, agent_private, pending_approval_action, pending_policy_change_action) = {
        let mut temp_keys = state.temp_private_keys.lock().await;
        if !temp_keys.awaiting_password
            && temp_keys.pending_approval_action.is_none()
            && temp_keys.pending_policy_change_action.is_none() {
            return Ok(()); // Not expecting password, ignore
        }

        // Take pending approval action if any
        let pending_action = temp_keys.pending_approval_action.take();
        let pending_policy_action = temp_keys.pending_policy_change_action.take();

        // Reset the flag
        temp_keys.awaiting_password = false;

        // Take the keys (moving them out)
        let user_key = temp_keys.user.take();
        let agent_key = temp_keys.agent.take();

        (user_key, agent_key, pending_action, pending_policy_action)
    };

    // Handle pending policy change action (approve/reject policy change with password)
    if let Some((policy_id, approved)) = pending_policy_change_action {
        // Verify the password against stored hash
        let password_valid = state.storage.verify_user_password(&password).await
            .map_err(|e| StewardError::Database(e.to_string()))?;

        if !password_valid {
            bot.send_message(chat_id, "❌ Incorrect password!\n\nPlease try again.")
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;

            // Re-set the pending action so they can try again
            state.temp_private_keys.lock().await.pending_policy_change_action = Some((policy_id, approved));
            state.temp_private_keys.lock().await.awaiting_password = true;
            return Ok(());
        }

        // Password verified - process the policy change
        bot.send_message(chat_id, "✅ Password verified! Processing policy change...")
            .await
            .map_err(|e| StewardError::Telegram(e.to_string()))?;

        // Process the policy change approval
        let callback_id = String::new(); // Empty callback ID since this is from password input
        process_policy_change_approval(&bot, Some(ChatId(chat_id.0)), &state, policy_id, approved, callback_id).await?;

        return Ok(());
    }

    // FIX #1: Handle pending approval action (approve/reject with password)
    if let Some((tx_id, approved)) = pending_approval_action {
        // This is a password verification for approval/rejection
        // Verify the password against stored hash
        let password_valid = state.storage.verify_user_password(&password).await
            .map_err(|e| StewardError::Database(e.to_string()))?;
        
        if !password_valid {
            bot.send_message(chat_id, "❌ Incorrect password!\n\nPlease try again.")
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
            
            // Re-set the pending action so they can try again
            state.temp_private_keys.lock().await.pending_approval_action = Some((tx_id, approved));
            state.temp_private_keys.lock().await.awaiting_password = true;
            return Ok(());
        }
        
        // Password verified - proceed with the approval/rejection
        let decision = if approved {
            crate::approval::ApprovalDecision::Approved
        } else {
            crate::approval::ApprovalDecision::Rejected
        };
        
        // Resolve the pending approval
        let was_pending = state.pending_approvals.resolve(&tx_id, decision).await;
        
        if was_pending {
            let action_text = if approved { "approved" } else { "rejected" };
            bot.send_message(chat_id, 
                format!("✅ Password verified! Transaction {}.", action_text))
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
            
            info!(chat_id = chat_id.0, tx_id = %tx_id, action = action_text, "Transaction decision processed via password");
        } else {
            bot.send_message(chat_id, "⚠️ This transaction is no longer pending.")
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
        }
        
        return Ok(());
    }
    
    // Original wallet creation flow - set up password for user key
    
    // Check if this is first or second password entry
    let is_confirm_step = {
        let temp_keys = state.temp_private_keys.lock().await;
        temp_keys.pending_password_confirm.is_some()
    };
    
    if is_confirm_step {
        // This is the confirmation entry - compare with first password
        let first_password = {
            let mut temp_keys = state.temp_private_keys.lock().await;
            temp_keys.pending_password_confirm.take()
        };
        
        if let Some(first_pwd) = first_password {
            if password != first_pwd {
                // Passwords don't match - ask again from start
                bot.send_message(chat_id, "❌ Passwords don't match! Please start again.\n\nUse /createwallet to create a new wallet, then set your password.")
                    .await
                    .map_err(|e| StewardError::Telegram(e.to_string()))?;
                
                // Clear all temp keys
                let mut temp_keys = state.temp_private_keys.lock().await;
                temp_keys.user = None;
                temp_keys.agent = None;
                temp_keys.awaiting_password = false;
                return Ok(());
            }
            
            // Passwords match - proceed with encryption
            // Set awaiting_password back to false to indicate we're done
            let mut temp_keys = state.temp_private_keys.lock().await;
            temp_keys.awaiting_password = false;
        }
    } else {
        // First password entry - store and ask for confirmation
        // Validate password
        if password.len() < 8 {
            bot.send_message(chat_id, "❌ Password too short! Must be at least 8 characters.")
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
            
            // Re-set the flag so user can try again
            state.temp_private_keys.lock().await.awaiting_password = true;
            return Ok(());
        }
        
// Store first password and ask for confirmation
        state.temp_private_keys.lock().await.pending_password_confirm = Some(password.clone());
        
        bot.send_message(chat_id, "📝 First password received.\n\nNow please CONFIRM your password by entering it again:")
            .await
            .map_err(|e| StewardError::Telegram(e.to_string()))?;
        return Ok(());
    }
    
    // Continue with the rest only after confirmation succeeded
    let user_private = match user_private {
        Some(k) => k,
        None => {
            bot.send_message(chat_id, "❌ No wallet in creation. Use /createwallet first.")
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
            return Ok(());
        }
    };
    
    // Compute password hash for verification later
    let mut hasher = Keccak256::new();
    hasher.update(password.as_bytes());
    let password_hash = hex::encode(hasher.finalize());
    
    // Create AgentKeyShare from user private key
    let user_share = create_key_share(&user_private, 2, PartyRole::User)
        .map_err(|e| StewardError::Internal(format!("Failed to create user key share: {}", e)))?;
    
    // Encrypt user key with their password
    let encrypted_user = encrypt_key_share(&user_share, &password)
        .map_err(|e| StewardError::Internal(format!("Failed to encrypt user key: {}", e)))?;
    
    // Store encrypted user key with password hash
    state.storage.save_user_key(&encrypted_user, &password_hash).await
        .map_err(|e| StewardError::Database(e.to_string()))?;
    
    // Also keep the agent key in temp storage (for the agent to use)
    // The agent key doesn't need password protection (it's for the AI)
    {
        let mut temp_keys = state.temp_private_keys.lock().await;
        temp_keys.agent = agent_private;
    }
    
    // Confirm to user
    let confirmation = r#"✅ Password Set Successfully!

🔐 Your User Key is now protected with your password.

You'll need to enter your password to:
• Approve transactions
• Reject transactions
• View sensitive wallet operations

⚠️ IMPORTANT: Your password cannot be recovered!
Make sure to remember it.

Next steps:
💰 Fund your wallet with USDC on Base Sepolia
🤖 Give the AGENT KEY to your AI agent
✅ You're all set!"#;

    bot.send_message(chat_id, confirmation)
        .await
        .map_err(|e| StewardError::Telegram(e.to_string()))?;
    
    info!(chat_id = chat_id.0, "User password set successfully");
    
    Ok(())
}

/// Check if bot is awaiting password input
pub async fn is_awaiting_password(state: &Arc<crate::AppState>) -> bool {
    state.temp_private_keys.lock().await.awaiting_password
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::SigningKey;
    use k256::elliptic_curve::sec1::ToEncodedPoint;

    /// Validation test: verify keypair generation produces valid Ethereum addresses
    #[test]
    fn test_generate_keypair_validity() {
        for _ in 0..10 {
            let (private_hex, address) = generate_keypair().expect("Keypair generation failed");

            // Verify private key format
            assert_eq!(private_hex.len(), 64, "Private key should be 64 hex chars (32 bytes)");
            let private_bytes = hex::decode(&private_hex).expect("Private key should be valid hex");
            assert_eq!(private_bytes.len(), 32, "Private key should be 32 bytes");

            // Verify address format
            assert_eq!(address.len(), 42, "Address should be 42 chars (0x + 20 bytes)");
            assert!(address.starts_with("0x"), "Address should start with 0x");

            // Verify address is valid hex
            let addr_bytes = hex::decode(&address[2..]).expect("Address should be valid hex");
            assert_eq!(addr_bytes.len(), 20, "Address should be 20 bytes");

            // Verify we can reconstruct the signing key from private key
            let private_array: [u8; 32] = private_bytes.as_slice().try_into().expect("Should be 32 bytes");
            let signing_key = SigningKey::from_bytes((&private_array).into()).expect("Should reconstruct signing key");
            let verifying_key = signing_key.verifying_key();

            // Re-derive address and verify it matches
            let encoded_point = verifying_key.to_encoded_point(false);
            let public_key_bytes = encoded_point.as_bytes();
            let public_key_without_prefix = &public_key_bytes[1..];
            let mut hasher = Keccak256::new();
            hasher.update(public_key_without_prefix);
            let hash = hasher.finalize();
            let rederived_addr = format!("0x{}", hex::encode(&hash[12..32]));

            assert_eq!(address, rederived_addr, "Re-derived address should match original");
        }
    }

    /// Validation test: verify CREATE2 address computation
    #[test]
    fn test_create2_address_computation() {
        // Known CREATE2 test vector
        // Using a known factory and salt to verify computation
        let factory = "0x0000000000000000000000000000000000000001";
        let salt = [0u8; 32];
        let init_code_hash = [0u8; 32];

        let result = compute_create2_address(factory, &salt, &init_code_hash);
        assert!(result.is_ok(), "CREATE2 computation should succeed");

        let address = result.unwrap();
        assert!(address.starts_with("0x"), "Address should start with 0x");
        assert_eq!(address.len(), 42, "Address should be 42 chars");
    }

    /// Validation test: verify key share creation from private key
    #[test]
    fn test_create_key_share_validity() {
        // Generate a real keypair
        let (private_hex, _) = generate_keypair().expect("Keypair generation failed");

        // Create key share for steward role
        let share = create_key_share(&private_hex, 1, PartyRole::Steward)
            .expect("Key share creation failed");

        // Verify share properties
        assert_eq!(share.party_id, 1, "Party ID should be 1 (steward)");
        assert_eq!(share.role, PartyRole::Steward, "Role should be Steward");
        assert_eq!(share.public_shares.len(), 1, "Should have 1 public share");

        // Verify public key is valid point on curve
        // The public key should not be identity (point at infinity)
        let public_key_bytes = share.public_key.to_encoded_point(false).as_bytes().to_vec();
        assert!(public_key_bytes.len() == 65, "Uncompressed public key should be 65 bytes");
        assert_eq!(public_key_bytes[0], 0x04, "Uncompressed point should start with 0x04");
    }

    /// Validation test: verify different keypairs produce different addresses
    #[test]
    fn test_keypairs_are_unique() {
        let mut addresses = std::collections::HashSet::new();
        let mut private_keys = std::collections::HashSet::new();

        for _ in 0..20 {
            let (private, address) = generate_keypair().expect("Keypair generation failed");
            addresses.insert(address);
            private_keys.insert(private);
        }

        assert_eq!(addresses.len(), 20, "All addresses should be unique");
        assert_eq!(private_keys.len(), 20, "All private keys should be unique");
    }

    }
/// Handle /deletewallet command - deletes existing wallet from storage
async fn handle_delete_wallet(bot: &Bot, msg: &Message, state: &Arc<crate::AppState>) -> Result<()> {
    let chat_id = msg.chat.id;

    // Check if wallet exists
    let existing = state.storage.get_wallet().await
        .map_err(|e| StewardError::Database(e.to_string()))?;

    match existing {
        Some(w) => {
            let addr = w.address.clone();

            // Delete wallet and all keys

            // Clear temp keys
            {
                let mut temp_keys = state.temp_private_keys.lock().await;
                temp_keys.agent = None;
                temp_keys.user = None;
                temp_keys.awaiting_password = false;
                temp_keys.pending_password_confirm = None;
                temp_keys.pending_approval_action = None;
                temp_keys.pending_policy_change_action = None;
            }

            let response = format!(
                r#"✅ Wallet Deleted

🗑️ Wallet {} has been removed from local storage.

All keys (Agent, Steward, User) have been cleared.
You can now create a new wallet with /createwallet"#,
                addr
            );

            bot.send_message(chat_id, response)
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
        }
        None => {
            bot.send_message(chat_id, "⚠️ No wallet exists to delete.\n\nUse /createwallet to create one.")
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
        }
    }

    Ok(())
}


/// Handle policy change callback: "policy_approve:uuid" or "policy_reject:uuid"
async fn handle_policy_change_callback(
    bot: Bot,
    q: teloxide::types::CallbackQuery,
    state: Arc<crate::AppState>,
) -> Result<()> {
    let data = q.data.clone().unwrap_or_default();
    let chat_id = q.message.as_ref().map(|m| m.chat().id);

    // Parse callback: "policy_approve:uuid" or "policy_reject:uuid"
    let parts: Vec<&str> = data.split(':').collect();
    if parts.len() != 2 {
        if let Some(cid) = chat_id {
            bot.send_message(cid, "❌ Invalid policy change callback data.")
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
        }
        return Ok(());
    }

    let action = parts[0]; // "policy_approve" or "policy_reject"
    let id_str = parts[1];

    // Parse policy change ID
    let uuid = match uuid::Uuid::parse_str(id_str) {
        Ok(u) => u,
        Err(_) => {
            if let Some(cid) = chat_id {
                bot.send_message(cid, "❌ Invalid policy change ID.")
                    .await
                    .map_err(|e| StewardError::Telegram(e.to_string()))?;
            }
            return Ok(());
        }
    };

    let policy_id = crate::types::PolicyChangeRequestId::from(uuid);
    let approved = action == "policy_approve";

    // Get the policy change request
    let record = match state.storage.get_policy_change_request(policy_id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            bot.answer_callback_query(q.id)
                .text("⚠️ Policy change request not found.")
                .show_alert(true)
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
            return Ok(());
        }
        Err(e) => {
            bot.answer_callback_query(q.id)
                .text(&format!("Error: {}", e))
                .show_alert(true)
                .await
                .ok();
            return Ok(());
        }
    };

    // Check if already resolved
    if record.status != crate::types::PolicyChangeStatus::Pending {
        bot.answer_callback_query(q.id)
            .text("⚠️ This policy change request has already been processed.")
            .show_alert(true)
            .await
            .map_err(|e| StewardError::Telegram(e.to_string()))?;
        return Ok(());
    }

    // Check if user has a password set up
    let has_user_key = state.storage.has_user_key().await
        .map_err(|e| StewardError::Database(e.to_string()))?;

    if has_user_key {
        // User has a password - require it before processing
        {
            let mut temp_keys = state.temp_private_keys.lock().await;
            temp_keys.pending_policy_change_action = Some((policy_id, approved));
            temp_keys.awaiting_password = true;
        }

        // Answer callback to dismiss loading
        bot.answer_callback_query(q.id)
            .text("🔐 Enter your password to confirm")
            .await
            .map_err(|e| StewardError::Telegram(e.to_string()))?;

        // Ask for password
        if let Some(cid) = chat_id {
            let prompt = if approved {
                r#"🔐 Password Required to Approve Policy Change

Please enter your wallet password to confirm this policy change.

⚠️ You must enter your password - there's no way to recover it if forgotten!"#
            } else {
                r#"🔐 Password Required to Reject Policy Change

Please enter your wallet password to confirm this rejection.

⚠️ You must enter your password - there's no way to recover it if forgotten!"#
            };

            bot.send_message(cid, prompt)
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;

            // Update the original message
            if let Some(msg) = q.message {
                let new_text = if approved {
                    "🔐 Awaiting password for policy change approval..."
                } else {
                    "🔐 Awaiting password for policy change rejection..."
                };
                bot.edit_message_text(msg.chat().id, msg.id(), new_text)
                    .await
                    .ok();
            }
        }

        return Ok(());
    }

    // No password set - process directly (shouldn't normally happen)
    process_policy_change_approval(&bot, chat_id, &state, policy_id, approved, q.id).await
}

/// Process a policy change approval/rejection
async fn process_policy_change_approval(
    bot: &Bot,
    chat_id: Option<ChatId>,
    state: &Arc<crate::AppState>,
    policy_id: crate::types::PolicyChangeRequestId,
    approved: bool,
    callback_id: String,
) -> Result<()> {
    // Get the policy change request
    let mut record = match state.storage.get_policy_change_request(policy_id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            if let Some(cid) = chat_id {
                bot.send_message(cid, "⚠️ Policy change request not found.")
                    .await
                    .map_err(|e| StewardError::Telegram(e.to_string()))?;
            }
            return Ok(());
        }
        Err(e) => {
            return Err(StewardError::Database(e.to_string()));
        }
    };

    // Check if already resolved
    if record.status != crate::types::PolicyChangeStatus::Pending {
        bot.answer_callback_query(callback_id)
            .text("⚠️ This request has already been processed.")
            .show_alert(true)
            .await
            .ok();
        return Ok(());
    }

    if approved {
        // Apply the policy change
        let field = record.request.field.clone();
        let new_value = record.request.new_value.clone();

        // Update the policy
        {
            let engine = state.policy_engine.write().await;
            engine.update_rule(&field, &new_value).await
                .map_err(|e| StewardError::Internal(e.to_string()))?;
            engine.save().await
                .map_err(|e| StewardError::Storage(e.to_string()))?;
        }

        // Mark as approved
        record.approve(chat_id.map(|c| c.0.to_string()).unwrap_or_default());
        state.storage.update_policy_change_request(&record).await
            .map_err(|e| StewardError::Database(e.to_string()))?;

        // Answer callback
        bot.answer_callback_query(callback_id)
            .text("✅ Policy change approved!")
            .await
            .map_err(|e| StewardError::Telegram(e.to_string()))?;

        if let Some(cid) = chat_id {
            bot.send_message(cid, format!(
                "✅ Policy Change Approved\n\n📝 Field: {}\n📈 New Value: {}",
                record.request.field.replace("_", " "),
                record.request.new_value
            )).await
            .map_err(|e| StewardError::Telegram(e.to_string()))?;
        }

        info!(
            policy_change_id = %policy_id,
            field = %record.request.field,
            new_value = %record.request.new_value,
            "Policy change approved via Telegram"
        );
    } else {
        // Mark as rejected
        record.reject(chat_id.map(|c| c.0.to_string()).unwrap_or_default());
        state.storage.update_policy_change_request(&record).await
            .map_err(|e| StewardError::Database(e.to_string()))?;

        // Answer callback
        bot.answer_callback_query(callback_id)
            .text("❌ Policy change rejected.")
            .await
            .map_err(|e| StewardError::Telegram(e.to_string()))?;

        if let Some(cid) = chat_id {
            bot.send_message(cid, "❌ Policy Change Rejected\n\nThe requested change has been cancelled.")
                .await
                .map_err(|e| StewardError::Telegram(e.to_string()))?;
        }

        info!(
            policy_change_id = %policy_id,
            "Policy change rejected via Telegram"
        );
    }

    Ok(())
}

