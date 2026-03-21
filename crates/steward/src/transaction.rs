//! # Transaction Executor
//!
//! Executes transactions on-chain using MPC signing and Pimlico gas sponsorship.

use crate::error::{Result, StewardError};
use crate::signing::{SigningCoordinator, MpcSignature};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

/// EntryPoint v0.7 address (same across all chains)
pub const ENTRY_POINT_V07: &str = "0x0000000071727De22E5E9d8BAf0edAc6f37da032";

/// Factory address on Base Sepolia
pub const FACTORY_BASE_SEPOLIA: &str = "0x8D9dd4062D0D68d4d8Dc439aE9762DEde9bcb821";

/// USDC address on Base Sepolia
pub const USDC_BASE_SEPOLIA: &str = "0x036CbD53842c5426634e7929541eC2318f3dCF7e";

/// Base Sepolia chain ID
pub const CHAIN_ID_BASE_SEPOLIA: u64 = 84532;

/// Transaction executor configuration
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Pimlico API key
    pub pimlico_api_key: String,
    /// Chain ID
    pub chain_id: u64,
    /// RPC URL
    pub rpc_url: String,
    /// EntryPoint address
    pub entry_point: String,
    /// Factory address for smart account deployment
    pub factory: String,
    /// USDC contract address
    pub usdc: String,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            pimlico_api_key: String::new(),
            chain_id: CHAIN_ID_BASE_SEPOLIA,
            rpc_url: "https://api.pimlico.io/v1/base-sepolia/rpc".to_string(),
            entry_point: ENTRY_POINT_V07.to_string(),
            factory: FACTORY_BASE_SEPOLIA.to_string(),
            usdc: USDC_BASE_SEPOLIA.to_string(),
        }
    }
}

impl ExecutorConfig {
    /// Create ExecutorConfig from Steward's PimlicoConfig
    pub fn from_pimlico_config(config: &crate::config::PimlicoConfig) -> Self {
        Self {
            pimlico_api_key: config.api_key.clone().unwrap_or_default(),
            chain_id: config.chain_id,
            rpc_url: config.get_rpc_url(),
            entry_point: config.entry_point.clone().unwrap_or_else(|| ENTRY_POINT_V07.to_string()),
            factory: config.factory.clone().unwrap_or_else(|| FACTORY_BASE_SEPOLIA.to_string()),
            usdc: config.usdc.clone().unwrap_or_else(|| USDC_BASE_SEPOLIA.to_string()),
        }
    }
}

/// UserOperation for ERC-4337
/// Field names match the ERC-4337 specification
#[allow(non_snake_case)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserOperation {
    /// Sender address (smart account)
    pub sender: String,
    /// Nonce
    pub nonce: String,
    /// Init code (for account creation)
    pub initCode: String,
    /// Call data
    pub callData: String,
    /// Call gas limit
    pub callGasLimit: String,
    /// Verification gas limit
    pub verificationGasLimit: String,
    /// Pre-verification gas
    pub preVerificationGas: String,
    /// Max fee per gas
    pub maxFeePerGas: String,
    /// Max priority fee per gas
    pub maxPriorityFeePerGas: String,
    /// Paymaster and data
    pub paymasterAndData: String,
    /// Signature
    pub signature: String,
}

impl UserOperation {
    /// Create a new UserOperation for a USDC transfer
    pub fn new_usdc_transfer(
        sender: &str,
        to: &str,
        amount: &str,  // In USDC units (6 decimals)
        nonce: u64,
    ) -> Self {
        // Build USDC transfer call data
        // transfer(address,uint256) = 0xa9059cbb
        let to_addr = to.trim_start_matches("0x");
        let amount_u256 = (amount.parse::<f64>().unwrap_or(0.0) * 1_000_000.0) as u128;
        
        let call_data = format!(
            "0xa9059cbb{}{}",
            format!("{:0>64}", to_addr),  // padded address
            format!("{:0>64x}", amount_u256)  // padded amount
        );
        
        Self {
            sender: sender.to_string(),
            nonce: format!("0x{:x}", nonce),
            initCode: "0x".to_string(),
            callData: call_data,
            callGasLimit: "0x30000".to_string(),  // ~200k gas
            verificationGasLimit: "0x100000".to_string(),  // ~1M gas
            preVerificationGas: "0x60000".to_string(),  // ~400k gas
            maxFeePerGas: "0x".to_string(),
            maxPriorityFeePerGas: "0x".to_string(),
            paymasterAndData: "0x".to_string(),
            signature: "0x".to_string(),
        }
    }
    
    /// Compute the hash to sign
    pub fn hash(&self, entry_point: &str, chain_id: u64) -> [u8; 32] {
        use sha3::{Keccak256, Digest};
        
        // Pack the UserOperation fields
        let mut packer = Vec::new();
        
        // Hash each field
        let sender_hash = hash_field(&self.sender);
        let init_code_hash = hash_field(&self.initCode);
        let call_data_hash = hash_field(&self.callData);
        let paymaster_hash = hash_field(&self.paymasterAndData);
        
        // Encode packed hash
        packer.extend_from_slice(&sender_hash);
        packer.extend_from_slice(&decode_hex_to_32(&self.nonce));
        packer.extend_from_slice(&init_code_hash);
        packer.extend_from_slice(&call_data_hash);
        packer.extend_from_slice(&decode_hex_to_32(&self.callGasLimit));
        packer.extend_from_slice(&decode_hex_to_32(&self.verificationGasLimit));
        packer.extend_from_slice(&decode_hex_to_32(&self.preVerificationGas));
        packer.extend_from_slice(&decode_hex_to_32(&self.maxFeePerGas));
        packer.extend_from_slice(&decode_hex_to_32(&self.maxPriorityFeePerGas));
        packer.extend_from_slice(&paymaster_hash);
        
        // Hash the packed data
        let packed_hash = Keccak256::digest(&packer);
        
        // Final hash with context
        let mut final_packer = Vec::new();
        final_packer.extend_from_slice(&packed_hash);
        final_packer.extend_from_slice(&decode_hex_to_32(entry_point));
        final_packer.extend_from_slice(&chain_id.to_be_bytes());
        
        let result = Keccak256::digest(&final_packer);
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }
    
    /// Set the MPC signature
    pub fn set_signature(&mut self, sig: &MpcSignature) {
        self.signature = format!("0x{}", hex::encode(sig.to_bytes()));
    }
}

/// Hash a field for UserOperation
fn hash_field(field: &str) -> [u8; 32] {
    use sha3::{Keccak256, Digest};
    let bytes = hex::decode(field.trim_start_matches("0x")).unwrap_or_default();
    let hash = Keccak256::digest(&bytes);
    let mut result = [0u8; 32];
    result.copy_from_slice(&hash);
    result
}

/// Decode hex string to 32 bytes
fn decode_hex_to_32(s: &str) -> [u8; 32] {
    let s = s.trim_start_matches("0x");
    let bytes = hex::decode(s).unwrap_or_default();
    let mut result = [0u8; 32];
    let start = 32 - bytes.len().min(32);
    result[start..].copy_from_slice(&bytes[..bytes.len().min(32)]);
    result
}

/// Transaction executor
pub struct TransactionExecutor {
    /// Configuration
    config: ExecutorConfig,
    /// HTTP client
    client: reqwest::Client,
    /// Signing coordinator
    signer: Arc<SigningCoordinator>,
}

impl TransactionExecutor {
    /// Create a new transaction executor
    pub fn new(config: ExecutorConfig, signer: Arc<SigningCoordinator>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| StewardError::Network(e.to_string()))?;
        
        Ok(Self {
            config,
            client,
            signer,
        })
    }
    
    /// Execute a USDC transfer
    pub async fn execute_usdc_transfer(
        &self,
        wallet_address: &str,
        to: &str,
        amount: &str,
    ) -> Result<TransactionResult> {
        info!(
            wallet = %wallet_address,
            to = %to,
            amount = %amount,
            "Executing USDC transfer"
        );
        
        // Check keys are loaded
        if !self.signer.is_keys_loaded().await {
            return Err(StewardError::KeyNotLoaded);
        }
        
        // Step 1: Get gas prices from Pimlico
        let gas_prices = self.get_gas_prices().await?;
        
        // Step 2: Get nonce from the network
        let nonce = self.get_nonce(wallet_address).await?;
        
        // Step 3: Build UserOperation
        let mut user_op = UserOperation::new_usdc_transfer(wallet_address, to, amount, nonce);
        user_op.maxFeePerGas = format!("0x{:x}", gas_prices.max_fee_per_gas);
        user_op.maxPriorityFeePerGas = format!("0x{:x}", gas_prices.max_priority_fee_per_gas);
        
        // Step 4: Request sponsorship from Pimlico
        let sponsorship = self.get_sponsorship(&user_op).await?;
        user_op.paymasterAndData = sponsorship.paymaster_and_data;
        user_op.callGasLimit = sponsorship.call_gas_limit;
        user_op.verificationGasLimit = sponsorship.verification_gas_limit;
        user_op.preVerificationGas = sponsorship.pre_verification_gas;
        
        // Step 5: Compute hash and sign
        let hash = user_op.hash(ENTRY_POINT_V07, self.config.chain_id);
        let signature = self.signer.sign_hash(&hash).await?;
        user_op.set_signature(&signature);
        
        // Step 6: Submit to Pimlico
        let user_op_hash = self.submit_user_op(&user_op).await?;
        
        info!(
            user_op_hash = %user_op_hash,
            "UserOperation submitted successfully"
        );
        
        Ok(TransactionResult {
            user_op_hash,
            tx_hash: None,
            status: TransactionStatus::Pending,
        })
    }
    
    /// Get gas prices from Pimlico
    async fn get_gas_prices(&self) -> Result<GasPrices> {
        let url = format!("{}{}", self.config.rpc_url, self.config.pimlico_api_key);
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "pimlico_getUserOperationGasPrice",
            "params": [],
            "id": 1
        });
        
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| StewardError::Network(e.to_string()))?;
        
        let json: serde_json::Value = response.json().await
            .map_err(|e| StewardError::Network(e.to_string()))?;
        
        let result = json.get("result")
            .ok_or_else(|| StewardError::Pimlico("No result in response".to_string()))?;
        
        let standard = result.get("standard")
            .ok_or_else(|| StewardError::Pimlico("No standard gas price".to_string()))?;
        
        Ok(GasPrices {
            max_fee_per_gas: parse_hex_u256(standard.get("maxFeePerGas")
                .and_then(|v| v.as_str())
                .unwrap_or("0x0"))?,
            max_priority_fee_per_gas: parse_hex_u256(standard.get("maxPriorityFeePerGas")
                .and_then(|v| v.as_str())
                .unwrap_or("0x0"))?,
        })
    }
    
    /// Get nonce for wallet
    async fn get_nonce(&self, wallet_address: &str) -> Result<u64> {
        let url = format!("{}{}", self.config.rpc_url, self.config.pimlico_api_key);
        
        // Call eth_getNonce through the EntryPoint
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getNonce",
            "params": [wallet_address, ENTRY_POINT_V07],
            "id": 1
        });
        
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| StewardError::Network(e.to_string()))?;
        
        let json: serde_json::Value = response.json().await
            .map_err(|e| StewardError::Network(e.to_string()))?;
        
        let result = json.get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("0x0");
        
        u64::from_str_radix(result.trim_start_matches("0x"), 16)
            .map_err(|e| StewardError::Pimlico(format!("Invalid nonce: {}", e)))
    }
    
    /// Get sponsorship from Pimlico paymaster
    async fn get_sponsorship(&self, user_op: &UserOperation) -> Result<SponsorshipResult> {
        let url = format!("{}{}", self.config.rpc_url, self.config.pimlico_api_key);
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "pm_sponsorUserOperation",
            "params": [user_op, ENTRY_POINT_V07],
            "id": 1
        });
        
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| StewardError::Network(e.to_string()))?;
        
        let json: serde_json::Value = response.json().await
            .map_err(|e| StewardError::Network(e.to_string()))?;
        
        if let Some(error) = json.get("error") {
            return Err(StewardError::Pimlico(format!(
                "Sponsorship error: {}",
                error
            )));
        }
        
        let result = json.get("result")
            .ok_or_else(|| StewardError::Pimlico("No sponsorship result".to_string()))?;
        
        Ok(SponsorshipResult {
            paymaster_and_data: result.get("paymasterAndData")
                .and_then(|v| v.as_str())
                .unwrap_or("0x")
                .to_string(),
            call_gas_limit: result.get("callGasLimit")
                .and_then(|v| v.as_str())
                .unwrap_or("0x30000")
                .to_string(),
            verification_gas_limit: result.get("verificationGasLimit")
                .and_then(|v| v.as_str())
                .unwrap_or("0x100000")
                .to_string(),
            pre_verification_gas: result.get("preVerificationGas")
                .and_then(|v| v.as_str())
                .unwrap_or("0x60000")
                .to_string(),
        })
    }
    
    /// Submit UserOperation to Pimlico
    async fn submit_user_op(&self, user_op: &UserOperation) -> Result<String> {
        let url = format!("{}{}", self.config.rpc_url, self.config.pimlico_api_key);
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_sendUserOperation",
            "params": [user_op, ENTRY_POINT_V07],
            "id": 1
        });
        
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| StewardError::Network(e.to_string()))?;
        
        let json: serde_json::Value = response.json().await
            .map_err(|e| StewardError::Network(e.to_string()))?;
        
        if let Some(error) = json.get("error") {
            return Err(StewardError::Pimlico(format!(
                "Submit error: {}",
                error
            )));
        }
        
        json.get("result")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| StewardError::Pimlico("No result in submit response".to_string()))
    }
}

/// Gas prices
#[derive(Debug, Clone)]
struct GasPrices {
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
}

/// Sponsorship result
#[derive(Debug, Clone)]
struct SponsorshipResult {
    paymaster_and_data: String,
    call_gas_limit: String,
    verification_gas_limit: String,
    pre_verification_gas: String,
}

/// Transaction result
#[derive(Debug, Clone)]
pub struct TransactionResult {
    /// UserOperation hash
    pub user_op_hash: String,
    /// Transaction hash (if included)
    pub tx_hash: Option<String>,
    /// Status
    pub status: TransactionStatus,
}

/// Transaction status
#[derive(Debug, Clone, PartialEq)]
pub enum TransactionStatus {
    Pending,
    Included,
    Failed,
}

impl TransactionExecutor {
    /// Check if a smart account is deployed (has code)
    pub async fn is_account_deployed(&self, address: &str) -> Result<bool> {
        let url = format!("{}{}", self.config.rpc_url, self.config.pimlico_api_key);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getCode",
            "params": [address, "latest"],
            "id": 1
        });

        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| StewardError::Network(e.to_string()))?;

        let json: serde_json::Value = response.json().await
            .map_err(|e| StewardError::Network(e.to_string()))?;

        let code = json.get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("0x");

        Ok(code != "0x" && code.len() > 2)
    }

    /// Get the predicted sender address for a smart account
    /// This calls the factory's getAddress function
    pub async fn get_sender_address(
        &self,
        agent_address: &str,
        steward_address: &str,
        user_address: &str,
        salt: &[u8; 32],
    ) -> Result<String> {
        let url = format!("{}{}", self.config.rpc_url, self.config.pimlico_api_key);

        // Encode the call to factory's getAddress(agent, steward, user, salt)
        // Function selector: keccak256("getAddress(address,address,address,bytes32)")[:4]
        // For now, use the factory address from config
        let factory = &self.config.factory;

        // Build call data for getAddress(address,address,address,bytes32)
        // Selector: we'll compute it or use known selector
        let get_address_selector = "0x25350f08"; // getAddress(address,address,address,bytes32)

        let agent_padded = format!("{:0>64}", agent_address.trim_start_matches("0x"));
        let steward_padded = format!("{:0>64}", steward_address.trim_start_matches("0x"));
        let user_padded = format!("{:0>64}", user_address.trim_start_matches("0x"));
        let salt_hex = hex::encode(salt);

        let call_data = format!(
            "{}{}{}{}{}",
            get_address_selector,
            agent_padded,
            steward_padded,
            user_padded,
            salt_hex
        );

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{
                "to": factory,
                "data": call_data
            }, "latest"],
            "id": 1
        });

        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| StewardError::Network(e.to_string()))?;

        let json: serde_json::Value = response.json().await
            .map_err(|e| StewardError::Network(e.to_string()))?;

        let result = json.get("result")
            .and_then(|v| v.as_str())
            .ok_or_else(|| StewardError::Pimlico("No result from getAddress".to_string()))?;

        // Extract the address (last 20 bytes of the 32-byte result)
        let addr_hex = &result[result.len().saturating_sub(40)..];
        Ok(format!("0x{}", addr_hex))
    }

    /// Build init_code for smart account deployment
    /// init_code = factory_address + encoded_createAccount(agent, steward, user, salt)
    pub fn build_init_code(
        &self,
        agent_address: &str,
        steward_address: &str,
        user_address: &str,
        salt: &[u8; 32],
    ) -> String {
        let factory = &self.config.factory;

        // createAccount(address,address,address,bytes32) selector
        let create_account_selector = "0x785ffb37";

        let agent_padded = format!("{:0>64}", agent_address.trim_start_matches("0x"));
        let steward_padded = format!("{:0>64}", steward_address.trim_start_matches("0x"));
        let user_padded = format!("{:0>64}", user_address.trim_start_matches("0x"));
        let salt_hex = hex::encode(salt);

        format!(
            "{}{}{}{}{}{}",
            factory,
            create_account_selector,
            agent_padded,
            steward_padded,
            user_padded,
            salt_hex
        )
    }
}

/// Parse hex string to u128
fn parse_hex_u256(s: &str) -> Result<u128> {
    let s = s.trim_start_matches("0x");
    u128::from_str_radix(s, 16)
        .map_err(|e| StewardError::Pimlico(format!("Invalid hex number: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_executor_config_from_pimlico_config() {
        let pimlico_config = crate::config::PimlicoConfig {
            api_key: Some("test_api_key".to_string()),
            chain_id: 84532,
            rpc_url: Some("https://custom.rpc.url".to_string()),
            enabled: true,
            entry_point: Some("0x1234567890123456789012345678901234567890".to_string()),
            factory: Some("0xabcdef0123456789abcdef0123456789abcdef01".to_string()),
            usdc: Some("0xusdc012345678901234567890123456789012345".to_string()),
        };

        let executor_config = ExecutorConfig::from_pimlico_config(&pimlico_config);

        assert_eq!(executor_config.pimlico_api_key, "test_api_key");
        assert_eq!(executor_config.chain_id, 84532);
        assert_eq!(executor_config.rpc_url, "https://custom.rpc.url");
        assert_eq!(executor_config.entry_point, "0x1234567890123456789012345678901234567890");
        assert_eq!(executor_config.factory, "0xabcdef0123456789abcdef0123456789abcdef01");
        assert_eq!(executor_config.usdc, "0xusdc012345678901234567890123456789012345");
    }

    #[test]
    fn test_executor_config_defaults() {
        let pimlico_config = crate::config::PimlicoConfig {
            api_key: None,
            chain_id: 1, // Ethereum mainnet
            rpc_url: None,
            enabled: false,
            entry_point: None,
            factory: None,
            usdc: None,
        };

        let executor_config = ExecutorConfig::from_pimlico_config(&pimlico_config);

        // Should use defaults
        assert!(executor_config.pimlico_api_key.is_empty());
        assert_eq!(executor_config.chain_id, 1);
        // RPC URL should be generated
        assert_eq!(executor_config.rpc_url, "https://api.pimlico.io/v1/ethereum/rpc");
        // Should use default entry point
        assert_eq!(executor_config.entry_point, ENTRY_POINT_V07);
    }

    #[test]
    fn test_user_operation_new_usdc_transfer() {
        let user_op = UserOperation::new_usdc_transfer(
            "0xsender1234567890123456789012345678901234",
            "0xrecipient123456789012345678901234567890",
            "1.5", // 1.5 USDC
            0,
        );

        assert_eq!(user_op.sender, "0xsender1234567890123456789012345678901234");
        assert_eq!(user_op.nonce, "0x0");
        // Call data should start with transfer(address,uint256) selector
        assert!(user_op.callData.starts_with("0xa9059cbb"));
        assert_eq!(user_op.signature, "0x");
    }

    #[test]
    fn test_user_operation_hash() {
        let user_op = UserOperation::new_usdc_transfer(
            "0xsender1234567890123456789012345678901234",
            "0xrecipient123456789012345678901234567890",
            "1.0",
            1,
        );

        let hash = user_op.hash(ENTRY_POINT_V07, 84532);

        // Hash should be 32 bytes
        assert_eq!(hash.len(), 32);

        // Same inputs should produce same hash
        let hash2 = user_op.hash(ENTRY_POINT_V07, 84532);
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_build_init_code() {
        let signing_coordinator = Arc::new(SigningCoordinator::new());
        let config = ExecutorConfig::default();
        let executor = TransactionExecutor::new(config, signing_coordinator).unwrap();

        let salt = [0u8; 32];
        let init_code = executor.build_init_code(
            "0xagent1234567890123456789012345678901234",
            "0xsteward1234567890123456789012345678901",
            "0xuser1234567890123456789012345678901234",
            &salt,
        );

        // Should start with factory address
        assert!(init_code.starts_with(&executor.config.factory));
        // Should contain the createAccount selector
        assert!(init_code.contains("785ffb37"));
    }

    #[test]
    fn test_mpc_signature_to_bytes() {
        let sig = MpcSignature {
            party_indices: 0x10, // Agent=0, Steward=1 => (1 << 4) | 0 = 0x10
            sig1: [1u8; 65],
            sig2: [2u8; 65],
        };

        let bytes = sig.to_bytes();

        // Total length should be 131 bytes
        assert_eq!(bytes.len(), 131);

        // First byte is party indices
        assert_eq!(bytes[0], 0x10);

        // Next 65 bytes are sig1
        assert_eq!(&bytes[1..66], &[1u8; 65]);

        // Last 65 bytes are sig2
        assert_eq!(&bytes[66..131], &[2u8; 65]);
    }

    #[test]
    fn test_parse_hex_u256() {
        assert_eq!(parse_hex_u256("0x0").unwrap(), 0);
        assert_eq!(parse_hex_u256("0x1").unwrap(), 1);
        assert_eq!(parse_hex_u256("0xff").unwrap(), 255);
        assert_eq!(parse_hex_u256("0x1000").unwrap(), 4096);

        // Without 0x prefix should still work
        assert_eq!(parse_hex_u256("ff").unwrap(), 255);
    }
}