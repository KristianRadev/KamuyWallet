//! # Kamuy Smart Account
//!
//! ERC-4337 Smart Account integration with Pimlico for gas sponsorship and bundling.
//!
//! ## Architecture
//!
//! ```
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Smart Account Module                        │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                  │
//! │  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐     │
//! │  │  UserOp      │    │   Pimlico    │    │   Kernel     │     │
//! │  │  Builder     │───▶│   Client     │───▶│   Wallet     │     │
//! │  │              │    │              │    │              │     │
//! │  │ • Build      │    │ • Sponsor    │    │ • Validate   │     │
//! │  │ • Hash       │    │ • Bundle     │    │ • Execute    │     │
//! │  │ • Sign       │    │ • Submit     │    │ • Paymaster  │     │
//! │  └──────────────┘    └──────────────┘    └──────────────┘     │
//! │                                                                  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Features
//!
//! - UserOperation building and validation
//! - Pimlico API integration (sponsorship, bundling, gas estimation)
//! - Kernel wallet smart account support
//! - Fee estimation and collection
//! - Multi-chain support (Base, Polygon, Arbitrum, Optimism, Ethereum)

#![warn(missing_docs)]

pub mod gas;
pub mod kernel;
pub mod mpc;
pub mod pimlico;
pub mod steward;
pub mod userop;

// Re-export main types
pub use gas::{FeeCalculator, GasEstimator};
pub use kernel::{KernelConfig, KernelWallet};
pub use mpc::{MpcSignature, Party, TestMpcKeys};
pub use pimlico::{PimlicoClient, PimlicoConfig, SponsorResult};
pub use steward::{MpcAccountClient, MpcAccountConfig};
pub use userop::{entry_point_address, GasEstimate, UserOpBuilder, UserOperation, ENTRY_POINT_V07};

use ethers::types::{Address, U256, H256};
use thiserror::Error;

/// Smart account result type
pub type Result<T> = std::result::Result<T, SmartAccountError>;

/// Smart account errors
#[derive(Error, Debug)]
pub enum SmartAccountError {
    /// Invalid UserOperation
    #[error("Invalid UserOperation: {0}")]
    InvalidUserOp(String),
    
    /// Pimlico API error
    #[error("Pimlico error: {0}")]
    Pimlico(String),
    
    /// Kernel wallet error
    #[error("Kernel wallet error: {0}")]
    Kernel(String),
    
    /// Network error
    #[error("Network error: {0}")]
    Network(String),
    
    /// Signature error
    #[error("Signature error: {0}")]
    Signature(String),
    
    /// Gas estimation error
    #[error("Gas estimation error: {0}")]
    GasEstimation(String),
    
    /// Transaction failed
    #[error("Transaction failed: {0}")]
    TransactionFailed(String),
    
    /// Unsupported chain
    #[error("Unsupported chain: {0}")]
    UnsupportedChain(u64),
    
    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Supported chains
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chain {
    /// Ethereum mainnet
    Ethereum,
    /// Base L2
    Base,
    /// Polygon
    Polygon,
    /// Arbitrum
    Arbitrum,
    /// Optimism
    Optimism,
    /// Sepolia testnet
    Sepolia,
    /// Base Sepolia testnet
    BaseSepolia,
}

impl Chain {
    /// Get chain ID
    pub fn chain_id(&self) -> u64 {
        match self {
            Chain::Ethereum => 1,
            Chain::Base => 8453,
            Chain::Polygon => 137,
            Chain::Arbitrum => 42161,
            Chain::Optimism => 10,
            Chain::Sepolia => 11155111,
            Chain::BaseSepolia => 84532,
        }
    }
    
    /// Get chain from ID
    pub fn from_chain_id(id: u64) -> Option<Self> {
        match id {
            1 => Some(Chain::Ethereum),
            8453 => Some(Chain::Base),
            137 => Some(Chain::Polygon),
            42161 => Some(Chain::Arbitrum),
            10 => Some(Chain::Optimism),
            11155111 => Some(Chain::Sepolia),
            84532 => Some(Chain::BaseSepolia),
            _ => None,
        }
    }
    
    /// Get default Pimlico RPC URL
    pub fn pimlico_rpc(&self) -> String {
        format!("https://api.pimlico.io/v1/{}/rpc?apikey=", self.pimlico_name())
    }
    
    /// Get chain name for Pimlico
    fn pimlico_name(&self) -> &'static str {
        match self {
            Chain::Ethereum => "ethereum",
            Chain::Base => "base",
            Chain::Polygon => "polygon",
            Chain::Arbitrum => "arbitrum",
            Chain::Optimism => "optimism",
            Chain::Sepolia => "sepolia",
            Chain::BaseSepolia => "base-sepolia",
        }
    }
    
    /// Get entry point address for this chain
    pub fn entry_point(&self) -> Address {
        // EntryPoint v0.7 address is the same across chains
        "0x0000000071727De22E5E9d8BAf0edAc6f37da032"
            .parse()
            .expect("Valid entry point address")
    }
}

/// Smart account configuration
#[derive(Debug, Clone)]
pub struct SmartAccountConfig {
    /// Chain
    pub chain: Chain,
    /// Pimlico API key
    pub pimlico_api_key: String,
    /// Kernel wallet address
    pub wallet_address: Address,
    /// Owner address (MPC wallet)
    pub owner_address: Address,
    /// Fee configuration
    pub fee_config: FeeConfig,
}

/// Fee configuration
#[derive(Debug, Clone)]
pub struct FeeConfig {
    /// Project fee percentage (in basis points, 100 = 1%)
    pub fee_percent_bps: u32,
    /// Minimum gas to cover (in wei)
    pub min_gas: U256,
}

impl Default for FeeConfig {
    fn default() -> Self {
        Self {
            fee_percent_bps: 50, // 0.5%
            min_gas: U256::from(10_000_000_000_000u64), // 0.01 ETH
        }
    }
}

/// Transaction request for smart account
#[derive(Debug, Clone)]
pub struct TransactionRequest {
    /// Target address
    pub to: Address,
    /// Value to send
    pub value: U256,
    /// Call data
    pub data: Vec<u8>,
    /// Gas limit (optional)
    pub gas_limit: Option<U256>,
}

/// Transaction result
#[derive(Debug, Clone)]
pub struct TransactionResult {
    /// UserOperation hash
    pub user_op_hash: UserOpHash,
    /// Transaction hash (if submitted)
    pub tx_hash: Option<H256>,
    /// Success status
    pub success: bool,
    /// Gas used
    pub gas_used: Option<U256>,
    /// Fee collected (in wei)
    pub fee_collected: U256,
}

/// UserOperation hash type
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserOpHash(pub H256);

impl std::fmt::Display for UserOpHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0.as_bytes()))
    }
}

/// Validate Ethereum address checksum (EIP-55)
/// SECURITY: Ensures address is properly checksummed
pub fn is_checksum_valid(addr: &str) -> bool {
    // Check basic format
    if !addr.starts_with("0x") || addr.len() != 42 {
        return false;
    }
    
    // Check all characters are valid hex
    if !addr[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    
    // For mixed-case addresses, verify EIP-55 checksum
    let has_upper = addr[2..].chars().any(|c| c.is_ascii_uppercase());
    let has_lower = addr[2..].chars().any(|c| c.is_ascii_lowercase());
    
    if has_upper && has_lower {
        // Mixed case - verify checksum
        // Compute keccak256 of lowercase address (without 0x)
        let lowercase = addr.to_lowercase();
        let hash = ethers::utils::keccak256(lowercase.as_bytes());
        
        // Check each character
        for (i, c) in addr[2..].chars().enumerate() {
            let hash_byte = hash[i / 2];
            let hash_nibble = if i % 2 == 0 { hash_byte >> 4 } else { hash_byte & 0x0f };
            
            if c.is_ascii_alphabetic() {
                // Should be uppercase if hash nibble >= 8
                let should_be_upper = hash_nibble >= 8;
                let is_upper = c.is_ascii_uppercase();
                
                if should_be_upper != is_upper {
                    return false;
                }
            }
        }
    }
    
    true
}

/// Validate Ethereum address format and checksum
/// SECURITY: Comprehensive address validation
pub fn validate_address(addr: &str) -> Result<Address> {
    // Check format
    if !addr.starts_with("0x") || addr.len() != 42 {
        return Err(SmartAccountError::InvalidUserOp(
            format!("Invalid address format: {}", addr)
        ));
    }
    
    // Check checksum
    if !is_checksum_valid(addr) {
        return Err(SmartAccountError::InvalidUserOp(
            format!("Invalid address checksum: {}", addr)
        ));
    }
    
    // Parse address
    addr.parse()
        .map_err(|e| SmartAccountError::InvalidUserOp(
            format!("Failed to parse address: {}", e)
        ))
}
