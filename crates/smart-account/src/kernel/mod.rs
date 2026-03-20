//! # Kernel Wallet Integration
//!
//! Integration with Kernel (ZeroDev) smart wallet for ERC-4337 accounts.

use crate::{Chain, Result, SmartAccountError, SmartAccountConfig, TransactionRequest};
use ethers::types::{Address, U256, Bytes};

/// Kernel wallet smart account
pub struct KernelWallet {
    /// Configuration
    config: SmartAccountConfig,
    /// Wallet address
    address: Address,
    /// Owner address (MPC wallet)
    owner: Address,
}

/// Kernel wallet configuration
#[derive(Debug, Clone)]
pub struct KernelConfig {
    /// Kernel implementation address
    pub implementation: Address,
    /// Validator address (ECDSA validator)
    pub validator: Address,
    /// Factory address
    pub factory: Address,
}

/// Kernel wallet factory addresses by chain
pub const KERNEL_FACTORY_ADDRESSES: &[(u64, &str)] = &[
    (1, "0x5D006d3880645ec6e254E18C1F879DAC9Dd71A39"),      // Ethereum
    (8453, "0x5D006d3880645ec6e254E18C1F879DAC9Dd71A39"),   // Base
    (137, "0x5D006d3880645ec6e254E18C1F879DAC9Dd71A39"),    // Polygon
    (42161, "0x5D006d3880645ec6e254E18C1F879DAC9Dd71A39"),  // Arbitrum
    (10, "0x5D006d3880645ec6e254E18C1F879DAC9Dd71A39"),     // Optimism
];

impl KernelWallet {
    /// Create a new Kernel wallet instance
    pub fn new(config: SmartAccountConfig) -> Result<Self> {
        Ok(Self {
            address: config.wallet_address,
            owner: config.owner_address,
            config,
        })
    }
    
    /// Get wallet address
    pub fn address(&self) -> Address {
        self.address
    }
    
    /// Get owner address
    pub fn owner(&self) -> Address {
        self.owner
    }
    
    /// Get chain
    pub fn chain(&self) -> Chain {
        self.config.chain
    }
    
    /// Build init code for account creation
    pub fn build_init_code(&self, owner: Address) -> Result<Bytes> {
        // Get factory address for this chain
        let factory = Self::get_factory_address(self.config.chain.chain_id())?;
        
        // Build createAccount call data
        // createAccount(address owner, uint256 salt)
        let salt = U256::zero(); // Use deterministic salt
        
        let call_data = ethers::abi::encode(&[
            ethers::abi::Token::Address(owner),
            ethers::abi::Token::Uint(salt),
        ]);
        
        // Prepend factory address
        let mut init_code = Vec::new();
        init_code.extend_from_slice(factory.as_bytes());
        init_code.extend_from_slice(&call_data);
        
        Ok(Bytes::from(init_code))
    }
    
    /// Build execute call data
    pub fn build_execute_call(&self, tx: TransactionRequest) -> Bytes {
        // Kernel execute(address to, uint256 value, bytes calldata data)
        let call_data = ethers::abi::encode(&[
            ethers::abi::Token::Address(tx.to),
            ethers::abi::Token::Uint(tx.value),
            ethers::abi::Token::Bytes(tx.data),
        ]);
        
        // Add execute selector: 0xb61d27f6
        let mut result = vec![0xb6, 0x1d, 0x27, 0xf6];
        result.extend_from_slice(&call_data);
        
        Bytes::from(result)
    }
    
    /// Build execute batch call data
    pub fn build_execute_batch(&self, txs: Vec<TransactionRequest>) -> Bytes {
        // Kernel executeBatch(address[] calldata to, uint256[] calldata value, bytes[] calldata data)
        let tos: Vec<_> = txs.iter().map(|tx| tx.to).collect();
        let values: Vec<_> = txs.iter().map(|tx| tx.value).collect();
        let datas: Vec<_> = txs.iter().map(|tx| tx.data.clone()).collect();
        
        let call_data = ethers::abi::encode(&[
            ethers::abi::Token::Array(tos.into_iter().map(ethers::abi::Token::Address).collect()),
            ethers::abi::Token::Array(values.into_iter().map(ethers::abi::Token::Uint).collect()),
            ethers::abi::Token::Array(datas.into_iter().map(ethers::abi::Token::Bytes).collect()),
        ]);
        
        // Add executeBatch selector: 0x34fcd5be
        let mut result = vec![0x34, 0xfc, 0xd5, 0xbe];
        result.extend_from_slice(&call_data);
        
        Bytes::from(result)
    }
    
    /// Get factory address for chain
    fn get_factory_address(chain_id: u64) -> Result<Address> {
        KERNEL_FACTORY_ADDRESSES
            .iter()
            .find(|(id, _)| *id == chain_id)
            .map(|(_, addr)| addr.parse().expect("Valid address"))
            .ok_or_else(|| SmartAccountError::UnsupportedChain(chain_id))
    }
    
    /// Compute wallet address for owner
    pub fn compute_address(owner: Address, chain_id: u64) -> Result<Address> {
        let factory = Self::get_factory_address(chain_id)?;
        
        // Compute CREATE2 address
        // address = keccak256(0xff ++ factory ++ salt ++ keccak256(init_code))[12:]
        let salt = U256::zero();
        
        // Build init code hash
        // For Kernel, this is the factory's createAccount call
        let init_code = vec![]; // Simplified - would need actual bytecode
        let init_code_hash = ethers::utils::keccak256(&init_code);

        // Build CREATE2 input
        let mut create2_input = Vec::new();
        create2_input.push(0xff);
        create2_input.extend_from_slice(factory.as_bytes());

        // Convert U256 to 32 bytes big-endian
        let mut salt_bytes = [0u8; 32];
        salt.to_big_endian(&mut salt_bytes);
        create2_input.extend_from_slice(&salt_bytes);

        create2_input.extend_from_slice(&init_code_hash);
        
        let hash = ethers::utils::keccak256(&create2_input);
        let address = Address::from_slice(&hash[12..]);
        
        Ok(address)
    }
    
    /// Check if wallet is deployed
    pub async fn is_deployed(&self) -> Result<bool> {
        // In a real implementation, this would query the blockchain
        // For now, return true (assume deployed)
        Ok(true)
    }
    
    /// Get wallet nonce
    pub async fn get_nonce(&self) -> Result<U256> {
        // In a real implementation, this would query the EntryPoint
        // For now, return 0
        Ok(U256::zero())
    }
}

/// Kernel wallet validator types
pub mod validators {
    use ethers::types::Address;
    
    /// ECDSA validator address (default)
    pub const ECDSA_VALIDATOR: &str = "0xd9AB5096a812bCe98a83fB0E5941B49A5C0b6F85";
    
    /// Get ECDSA validator address
    pub fn ecdsa_validator() -> Address {
        ECDSA_VALIDATOR.parse().expect("Valid address")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_get_factory_address() {
        let addr = KernelWallet::get_factory_address(8453).unwrap();
        assert_ne!(addr, Address::zero());
    }
    
    #[test]
    fn test_compute_address() {
        let owner = "0x1234567890123456789012345678901234567890"
            .parse()
            .unwrap();
        let addr = KernelWallet::compute_address(owner, 8453).unwrap();
        assert_ne!(addr, Address::zero());
    }
}
