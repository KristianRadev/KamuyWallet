//! # Telegram Bot
//!
//! Main bot setup and event handling.

use super::commands::{handle_callback, handle_command, handle_password_input, is_awaiting_password, Command};
use crate::error::{Result, StewardError};
use std::sync::Arc;
use teloxide::dispatching::{Dispatcher, UpdateFilterExt};
use teloxide::dptree;
use teloxide::prelude::*;
use teloxide::types::{BotCommand, Update};
use teloxide::utils::command::BotCommands;
use tracing::info;

/// Start the Telegram bot
pub async fn start(state: Arc<crate::AppState>) -> Result<()> {
    let config = &state.config.telegram;

    // Validate configuration
    super::validate_config(config)?;

    if !config.enabled {
        info!("Telegram bot is disabled");
        return Ok(());
    }

    let token = config.token.as_ref().unwrap().clone();
    info!("Starting Telegram bot...");

    // Create bot
    let bot = Bot::new(token);

    // Register commands so they appear as suggestions when typing "/"
    // FIX: Added /deletewallet command that was missing from registration
    let commands = vec![
        BotCommand::new("start", "Start the bot and show welcome message"),
        BotCommand::new("help", "Show help message"),
        BotCommand::new("createwallet", "Create a new smart wallet"),
        BotCommand::new("deletewallet", "Delete the existing wallet"),
        BotCommand::new("status", "Show wallet status and pending transactions"),
        BotCommand::new("policy", "Show current policy rules"),
        BotCommand::new("pending", "List transactions awaiting approval"),
        BotCommand::new("history", "Show last 5 transactions"),
        BotCommand::new("wallet", "Show wallet address and balance"),
        BotCommand::new("send", "Send USDC to an address"),
    ];
    bot.set_my_commands(commands)
        .await
        .map_err(|e| StewardError::Telegram(format!("Failed to set commands: {}", e)))?;
    info!("Registered bot commands with Telegram");

    // Set up message handler
    let message_handler = Update::filter_message().endpoint(
        |bot: Bot, msg: Message, state: Arc<crate::AppState>| async move {
            if let Some(text) = msg.text() {
                // Check if we're awaiting password input
                if is_awaiting_password(&state).await {
                    if let Err(e) = handle_password_input(bot.clone(), msg.clone(), state.clone(), text.to_string()).await {
                        let _ = bot.send_message(msg.chat.id, format!("Error: {}", e)).await;
                    }
                    return Ok::<_, StewardError>(());
                }
                
                // Handle commands
                // FIX: Added logging for command parsing failures to help debug issues
                match Command::parse(text, "kamuy_steward_bot") {
                    Ok(cmd) => {
                        tracing::info!(chat_id = msg.chat.id.0, command = ?cmd, "Processing command");
                        if let Err(e) = handle_command(bot.clone(), msg.clone(), cmd, state.clone()).await {
                            tracing::error!(chat_id = msg.chat.id.0, error = %e, "Command handler error");
                            let _ = bot.send_message(msg.chat.id, format!("❌ Error: {}", e)).await;
                        }
                        return Ok::<_, StewardError>(());
                    }
                    Err(e) => {
                        // Log the parsing error for debugging
                        tracing::debug!(chat_id = msg.chat.id.0, text = text, error = %e, "Command parse failed");
                        
                        // Check if it looks like a command attempt (starts with /)
                        if text.starts_with('/') {
                            bot.send_message(
                                msg.chat.id,
                                "❓ I didn't recognize that command.\n\nUse /help to see available commands."
                            ).await
                             .map_err(|e| StewardError::Telegram(e.to_string()))?;
                            return Ok::<_, StewardError>(());
                        }
                    }
                }

                // Handle non-command text (only if not awaiting password and not a command)
                bot.send_message(
                    msg.chat.id,
                    "💬 I only understand commands.\n\nUse /help to see what I can do!"
                ).await
                 .map_err(|e| StewardError::Telegram(e.to_string()))?;
            }
            Ok(())
        },
    );

    // Set up callback handler
    let callback_handler = Update::filter_callback_query().endpoint(
        |bot: Bot, q: teloxide::types::CallbackQuery, state: Arc<crate::AppState>| async move {
            if let Err(e) = handle_callback(bot, q, state).await {
                tracing::error!("Callback error: {}", e);
            }
            Ok::<_, StewardError>(())
        },
    );

    // Combine handlers
    let handler = dptree::entry()
        .branch(message_handler)
        .branch(callback_handler);

    // Start dispatcher
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}