//! # Kamuy CLI
//!
//! Command-line interface for Kamuy Wallet - MPC-based threshold wallet.
//!
//! ## Commands
//!
//! - `create-wallet` - Generate MPC keys and create smart account
//! - `sign` - Sign transaction via MPC
//! - `policy` - View and update policy
//! - `status` - Check wallet status and balances
//! - `unlock` - Load Steward key with password
//! - `lock` - Unload Steward key
//! - `rotate` - Rotate agent key
//! - `recover` - Recover wallet with user key

use anyhow::Result;
use clap::{Parser, Subcommand, crate_version, crate_name};
use colored::Colorize;
use std::sync::Arc;

mod commands;
mod config;
mod context;
mod progress;

use commands::*;
use config::CliConfig;
use context::CliContext;
use progress::ProgressTracker;

/// Kamuy Wallet CLI - MPC-based threshold wallet for AI agents
#[derive(Parser)]
#[command(
    name = crate_name!(),
    version = crate_version!(),
    about = "Kamuy Wallet - MPC-based threshold wallet for AI agents",
    long_about = None,
)]
struct Cli {
    /// Configuration file path
    #[arg(short, long, value_name = "FILE")]
    config: Option<String>,

    /// Steward service URL
    #[arg(short, long, env = "STEWARD_URL")]
    steward_url: Option<String>,

    /// API key for Steward service
    #[arg(long, env = "STEWARD_API_KEY")]
    api_key: Option<String>,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Disable colored output
    #[arg(long)]
    no_color: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new wallet (recommended for new users)
    Init {
        /// Chain to create wallet on
        #[arg(short, long, default_value = "base")]
        chain: String,

        /// Output file for keys (JSON format)
        #[arg(short, long)]
        output: Option<String>,

        /// Reset wallet (delete existing and create new)
        #[arg(long)]
        reset: bool,
    },

    /// Create a new wallet (generates MPC keys)
    #[command(name = "create-wallet")]
    CreateWallet {
        /// Chain to create wallet on
        #[arg(short, long, default_value = "base")]
        chain: String,

        /// Output file for keys
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Sign a transaction
    Sign {
        /// Transaction data (hex encoded)
        #[arg(short, long)]
        data: Option<String>,
        
        /// Transaction file
        #[arg(short, long)]
        file: Option<String>,
        
        /// To address
        #[arg(short, long)]
        to: Option<String>,
        
        /// Amount to send
        #[arg(short, long)]
        amount: Option<String>,
        
        /// Token symbol
        #[arg(short, long, default_value = "ETH")]
        token: String,
        
        /// Chain ID
        #[arg(long)]
        chain_id: Option<u64>,
        
        /// Submit transaction after signing
        #[arg(long)]
        submit: bool,
    },

    /// View or update policy
    Policy {
        #[command(subcommand)]
        action: Option<PolicyAction>,
    },

    /// Check wallet status and balances
    Status {
        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
    },

    /// Unlock the wallet (load Steward key)
    Unlock,

    /// Lock the wallet (unload Steward key)
    Lock,

    /// Start the steward daemon
    Start {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },

    /// Stop the steward daemon
    Stop,

    /// Rotate the agent key
    Rotate {
        /// Force rotation without confirmation
        #[arg(long)]
        force: bool,
    },

    /// Recover wallet with user key
    Recover {
        /// User key file
        #[arg(short, long)]
        key_file: String,
    },

    /// Show recovery key (requires password)
    ShowRecoveryKey,

    /// Export agent configuration for AI agent setup
    ExportAgentConfig,

    /// List pending transactions
    Pending {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: PendingFormat,
    },

    /// Approve pending items (transactions, policy changes, addresses)
    Approve {
        #[command(subcommand)]
        action: ApproveAction,
    },

    /// Reject a pending transaction
    Reject {
        /// Transaction ID
        tx_id: String,
    },

    /// Show transaction history
    History {
        /// Number of transactions to show
        #[arg(short, long, default_value = "10")]
        limit: u32,
    },

    /// Configure CLI settings
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
enum PolicyAction {
    /// Show current policy
    Show,
    
    /// Update a policy value
    Set {
        /// Policy key to update
        key: String,
        /// New value
        value: String,
    },
    
    /// Edit policy in default editor
    Edit,
    
    /// Reset to default policy
    Reset,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current configuration
    Show,

    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,
        /// Configuration value
        value: String,
    },

    /// Get a configuration value
    Get {
        /// Configuration key (api_key, steward_url, steward_log, steward_pid_file)
        key: String,
    },

    /// Initialize configuration file
    Init,
}

/// Output format for pending command
#[derive(Clone, Debug, Default, clap::ValueEnum)]
enum PendingFormat {
    /// Table format (human readable)
    #[default]
    Table,
    /// Telegram format (for agent to forward)
    Telegram,
    /// JSON format (machine readable)
    Json,
}

/// Approval subcommands for v2.0
#[derive(Subcommand)]
enum ApproveAction {
    /// Approve a policy change request (requires TerminalPassword)
    Policy {
        /// Policy change request ID
        id: String,
    },

    /// Approve a new address over threshold (requires TerminalPassword)
    Address {
        /// Address approval ID
        id: String,
    },

    /// Approve a pending transaction (TelegramButton or optional override)
    Tx {
        /// Transaction ID
        id: String,

        /// Force terminal password approval (override TelegramButton)
        #[arg(short, long)]
        with_password: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize colored output
    if cli.no_color {
        colored::control::set_override(false);
    }

    // Initialize logging
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            if cli.verbose {
                "kamuy_cli=debug,info"
            } else {
                "kamuy_cli=info,warn"
            }
        )
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)?;

    // AUTO-RECOVERY: Check for saved progress
    let mut progress_tracker = ProgressTracker::load_or_create()?;

    if progress_tracker.should_resume() {
        println!("{}", progress_tracker.resume_message().yellow());
        println!();
    }

    // Load configuration - prefer SimpleConfig (v2.0) over CliConfig (v1.0)
    let (steward_url, api_key) = if let Some(simple_config) = config::SimpleConfig::load()? {
        // v2.0 config exists - use auto-generated values
        (
            cli.steward_url.unwrap_or(simple_config.steward_url),
            cli.api_key.or(Some(simple_config.api_key)),
        )
    } else {
        // Fall back to v1.0 config
        let config = CliConfig::load(cli.config.as_deref())?;
        (
            cli.steward_url.unwrap_or(config.steward_url),
            cli.api_key.or(config.api_key),
        )
    };

    // Create config for context
    let config = CliConfig::default()
        .with_steward_url(Some(steward_url))
        .with_api_key(api_key);

    // Create CLI context
    let ctx = Arc::new(CliContext::new(config).await?);

    // Execute command and track progress
    match cli.command {
        Commands::Init { chain, output, reset } => {
            progress_tracker.set_current_command("init")?;
            init::execute(ctx, chain, output, reset).await?;
            progress_tracker.command_completed("init")?;
        }
        Commands::CreateWallet { chain, output } => {
            progress_tracker.set_current_command("create_wallet")?;
            create_wallet::execute(ctx, chain, output).await?;
            progress_tracker.command_completed("create_wallet")?;
        }
        Commands::Sign { data, file, to, amount, token, chain_id, submit } => {
            progress_tracker.set_current_command("sign")?;
            sign::execute(ctx, data, file, to, amount, token, chain_id, submit).await?;
            progress_tracker.command_completed("sign")?;
        }
        Commands::Policy { action } => {
            progress_tracker.set_current_command("policy")?;
            match action {
                Some(PolicyAction::Show) | None => {
                    policy::show(ctx).await?;
                }
                Some(PolicyAction::Set { key, value }) => {
                    policy::set(ctx, key, value).await?;
                }
                Some(PolicyAction::Edit) => {
                    policy::edit(ctx).await?;
                }
                Some(PolicyAction::Reset) => {
                    policy::reset(ctx).await?;
                }
            }
            progress_tracker.command_completed("policy")?;
        }
        Commands::Status { detailed } => {
            progress_tracker.set_current_command("status")?;
            status::execute(ctx, detailed).await?;
            progress_tracker.command_completed("status")?;
        }
        Commands::Unlock => {
            progress_tracker.set_current_command("unlock")?;
            unlock::execute(ctx).await?;
            progress_tracker.command_completed("unlock")?;
        }
        Commands::Lock => {
            progress_tracker.set_current_command("lock")?;
            lock::execute(ctx).await?;
            progress_tracker.command_completed("lock")?;
        }
        Commands::Start { port } => {
            progress_tracker.set_current_command("start")?;
            // SimpleConfig::load() already respects KAMUY_CONFIG env var
            start::execute(Some(port)).await?;
            progress_tracker.command_completed("start")?;
        }
        Commands::Stop => {
            progress_tracker.set_current_command("stop")?;
            stop::execute().await?;
            progress_tracker.command_completed("stop")?;
        }
        Commands::Rotate { force } => {
            progress_tracker.set_current_command("rotate")?;
            rotate::execute(ctx, force).await?;
            progress_tracker.command_completed("rotate")?;
        }
        Commands::Recover { key_file } => {
            progress_tracker.set_current_command("recover")?;
            recover::execute(ctx, key_file).await?;
            progress_tracker.command_completed("recover")?;
        }
        Commands::ShowRecoveryKey => {
            progress_tracker.set_current_command("show_recovery_key")?;
            show_recovery_key::execute(ctx).await?;
            progress_tracker.command_completed("show_recovery_key")?;
        }
        Commands::ExportAgentConfig => {
            progress_tracker.set_current_command("export_agent_config")?;
            export_agent_config::execute(ctx).await?;
            progress_tracker.command_completed("export_agent_config")?;
        }
        Commands::Pending { format } => {
            progress_tracker.set_current_command("pending")?;
            pending::execute(ctx, format).await?;
            progress_tracker.command_completed("pending")?;
        }
        Commands::Approve { action } => {
            progress_tracker.set_current_command("approve")?;
            match action {
                ApproveAction::Policy { id } => {
                    approve::approve_policy(ctx, id).await?;
                }
                ApproveAction::Address { id } => {
                    approve::approve_address(ctx, id).await?;
                }
                ApproveAction::Tx { id, with_password } => {
                    approve::approve_tx(ctx, id, with_password).await?;
                }
            }
            progress_tracker.command_completed("approve")?;
        }
        Commands::Reject { tx_id } => {
            progress_tracker.set_current_command("reject")?;
            reject::execute(ctx, tx_id).await?;
            progress_tracker.command_completed("reject")?;
        }
        Commands::History { limit } => {
            progress_tracker.set_current_command("history")?;
            history::execute(ctx, limit).await?;
            progress_tracker.command_completed("history")?;
        }
        Commands::Config { action } => {
            progress_tracker.set_current_command("config")?;
            match action {
                ConfigAction::Show => {
                    config_cmd::show(ctx).await?;
                }
                ConfigAction::Set { key, value } => {
                    config_cmd::set(ctx, key, value).await?;
                }
                ConfigAction::Get { key } => {
                    config_cmd::get(ctx, key).await?;
                }
                ConfigAction::Init => {
                    config_cmd::init(ctx).await?;
                }
            }
            progress_tracker.command_completed("config")?;
        }
        Commands::Completions { shell } => {
            completions::generate(shell)?;
        }
    }

    Ok(())
}

/// Print success message
pub fn print_success(msg: &str) {
    println!("{} {}", "✓".green().bold(), msg);
}

/// Print error message
pub fn print_error(msg: &str) {
    eprintln!("{} {}", "✗".red().bold(), msg);
}

/// Print warning message
pub fn print_warning(msg: &str) {
    println!("{} {}", "⚠".yellow().bold(), msg);
}

/// Print info message
pub fn print_info(msg: &str) {
    println!("{} {}", "ℹ".blue(), msg);
}
