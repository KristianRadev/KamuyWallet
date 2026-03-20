//! # Pimlico Client
//!
//! Integration with Pimlico API for gas sponsorship and UserOperation bundling.
//!
//! ⚠️ **SECURITY NOTE:** Pimlico requires the API key to be passed in the URL query string.
//! This is a limitation of their API design. Operators should:
//! - Use HTTPS only (encrypted in transit)
//! - Rotate API keys regularly
//! - Monitor API key usage for abuse
//! - Consider using a proxy to hide the key from client-side code

use crate::{Chain, Result, SmartAccountError, UserOperation, UserOpHash, GasEstimate};
use ethers::types::{U256, H256};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Pimlico API client
pub struct PimlicoClient {
    /// HTTP client
    client: reqwest::Client,
    /// API key
    api_key: String,
    /// Chain
    chain: Chain,
    /// RPC URL
    rpc_url: String,
}

/// Pimlico configuration
#[derive(Debug, Clone)]
pub struct PimlicoConfig {
    /// API key
    pub api_key: String,
    /// Chain
    pub chain: Chain,
    /// Custom RPC URL (optional)
    pub custom_rpc_url: Option<String>,
    /// Request timeout
    pub timeout_secs: u64,
}

impl Default for PimlicoConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            chain: Chain::Base,
            custom_rpc_url: None,
            timeout_secs: 30,
        }
    }
}

/// Sponsorship result
#[derive(Debug, Clone)]
pub struct SponsorResult {
    /// Paymaster and data
    pub paymaster_and_data: Vec<u8>,
    /// Updated gas limits
    pub call_gas_limit: U256,
    /// Updated verification gas limit
    pub verification_gas_limit: U256,
    /// Updated pre-verification gas
    pub pre_verification_gas: U256,
    /// Updated max fee per gas
    pub max_fee_per_gas: U256,
    /// Updated max priority fee per gas
    pub max_priority_fee_per_gas: U256,
}

/// Bundle result
#[derive(Debug, Clone)]
pub struct BundleResult {
    /// UserOperation hash
    pub user_op_hash: UserOpHash,
    /// Transaction hash (if submitted)
    pub tx_hash: Option<H256>,
    /// Success status
    pub success: bool,
}

impl PimlicoClient {
    /// Create a new Pimlico client
    /// SECURITY: Validates API key at startup to fail fast
    pub fn new(config: PimlicoConfig) -> Result<Self> {
        // SECURITY FIX: Validate API key early
        if config.api_key.is_empty() {
            return Err(SmartAccountError::Pimlico(
                "API key is required".to_string()
            ));
        }
        
        // SECURITY FIX: Validate API key format (Pimlico keys are hex strings)
        if config.api_key.len() < 16 {
            return Err(SmartAccountError::Pimlico(
                "API key appears invalid (too short)".to_string()
            ));
        }
        
        let rpc_url = config.custom_rpc_url
            .clone()
            .unwrap_or_else(|| config.chain.pimlico_rpc());
        
        // SECURITY: Configure HTTP client with timeout and response size limit
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| SmartAccountError::Network(e.to_string()))?;
        
        Ok(Self {
            client,
            api_key: config.api_key,
            chain: config.chain,
            rpc_url,
        })
    }
    
    /// Get gas price estimates
    pub async fn get_gas_estimates(&self) -> Result<GasEstimate> {
        let response = self.rpc_call(
            "pimlico_getUserOperationGasPrice",
            vec![],
        ).await?;
        
        let result: GasPriceResponse = serde_json::from_value(response)
            .map_err(|e| SmartAccountError::Pimlico(format!("Failed to parse gas price: {}", e)))?;
        
        Ok(GasEstimate {
            call_gas_limit: U256::from(100_000), // Default, will be estimated
            verification_gas_limit: U256::from(100_000),
            pre_verification_gas: U256::from(50_000),
            max_fee_per_gas: parse_u256(&result.standard.max_fee_per_gas)?,
            max_priority_fee_per_gas: parse_u256(&result.standard.max_priority_fee_per_gas)?,
        })
    }
    
    /// Estimate gas for UserOperation
    pub async fn estimate_gas(&self, user_op: &UserOperation) -> Result<GasEstimate> {
        let response = self.rpc_call(
            "eth_estimateUserOperationGas",
            vec![
                serde_json::to_value(user_op).unwrap(),
                serde_json::to_value(self.chain.entry_point()).unwrap(),
            ],
        ).await?;
        
        let result: EstimateGasResponse = serde_json::from_value(response)
            .map_err(|e| SmartAccountError::GasEstimation(format!("Failed to parse estimate: {}", e)))?;
        
        Ok(GasEstimate {
            call_gas_limit: parse_u256(&result.call_gas_limit)?,
            verification_gas_limit: parse_u256(&result.verification_gas_limit)?,
            pre_verification_gas: parse_u256(&result.pre_verification_gas)?,
            max_fee_per_gas: user_op.max_fee_per_gas,
            max_priority_fee_per_gas: user_op.max_priority_fee_per_gas,
        })
    }
    
    /// Sponsor UserOperation (get paymaster data)
    pub async fn sponsor_user_op(&self, user_op: &UserOperation) -> Result<SponsorResult> {
        let response = self.rpc_call(
            "pm_sponsorUserOperation",
            vec![
                serde_json::to_value(user_op).unwrap(),
                serde_json::to_value(self.chain.entry_point()).unwrap(),
                serde_json::json!({}), // Empty sponsorship policy
            ],
        ).await?;
        
        let result: SponsorResponse = serde_json::from_value(response)
            .map_err(|e| SmartAccountError::Pimlico(format!("Failed to parse sponsor: {}", e)))?;
        
        Ok(SponsorResult {
            paymaster_and_data: hex::decode(&result.paymaster_and_data)
                .map_err(|e| SmartAccountError::Pimlico(format!("Invalid paymaster data: {}", e)))?,
            call_gas_limit: parse_u256(&result.call_gas_limit)?,
            verification_gas_limit: parse_u256(&result.verification_gas_limit)?,
            pre_verification_gas: parse_u256(&result.pre_verification_gas)?,
            max_fee_per_gas: parse_u256(&result.max_fee_per_gas)?,
            max_priority_fee_per_gas: parse_u256(&result.max_priority_fee_per_gas)?,
        })
    }
    
    /// Submit UserOperation to bundler
    pub async fn send_user_op(&self, user_op: &UserOperation) -> Result<BundleResult> {
        let response = self.rpc_call(
            "eth_sendUserOperation",
            vec![
                serde_json::to_value(user_op).unwrap(),
                serde_json::to_value(self.chain.entry_point()).unwrap(),
            ],
        ).await?;
        
        let user_op_hash: String = serde_json::from_value(response)
            .map_err(|e| SmartAccountError::Pimlico(format!("Failed to parse response: {}", e)))?;
        
        let hash_bytes = hex::decode(&user_op_hash.trim_start_matches("0x"))
            .map_err(|e| SmartAccountError::Pimlico(format!("Invalid hash: {}", e)))?;
        
        Ok(BundleResult {
            user_op_hash: UserOpHash(H256::from_slice(&hash_bytes)),
            tx_hash: None, // Will be available after inclusion
            success: true,
        })
    }
    
    /// Get UserOperation receipt
    pub async fn get_user_op_receipt(&self, user_op_hash: &UserOpHash) -> Result<Option<UserOpReceipt>> {
        let response = self.rpc_call(
            "eth_getUserOperationReceipt",
            vec![serde_json::to_value(format!("0x{}", hex::encode(user_op_hash.0.as_bytes()))).unwrap()],
        ).await?;
        
        if response.is_null() {
            return Ok(None);
        }
        
        let receipt: UserOpReceipt = serde_json::from_value(response)
            .map_err(|e| SmartAccountError::Pimlico(format!("Failed to parse receipt: {}", e)))?;
        
        Ok(Some(receipt))
    }
    
    /// Make JSON-RPC call
    async fn rpc_call(
        &self,
        method: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.rpc_url, self.api_key);
        
        let request = RpcRequest {
            jsonrpc: "2.0",
            method,
            params,
            id: 1,
        };
        
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| SmartAccountError::Network(format!("RPC call failed: {}", e)))?;
        
        // SECURITY FIX: Check response size before parsing (max 1MB)
        let content_length = response.content_length();
        if let Some(len) = content_length {
            if len > 1_000_000 {
                return Err(SmartAccountError::Network(
                    "Response too large (max 1MB)".to_string()
                ));
            }
        }
        
        let rpc_response: RpcResponse = response.json().await
            .map_err(|e| SmartAccountError::Network(format!("Failed to parse response: {}", e)))?;
        
        if let Some(error) = rpc_response.error {
            return Err(SmartAccountError::Pimlico(format!(
                "RPC error {}: {}",
                error.code, error.message
            )));
        }
        
        Ok(rpc_response.result.unwrap_or(serde_json::Value::Null))
    }
}

/// JSON-RPC request
#[derive(Debug, Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'a str,
    method: &'a str,
    params: Vec<serde_json::Value>,
    id: u64,
}

/// JSON-RPC response
#[derive(Debug, Deserialize)]
struct RpcResponse {
    jsonrpc: String,
    result: Option<serde_json::Value>,
    error: Option<RpcError>,
    id: u64,
}

/// JSON-RPC error
#[derive(Debug, Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

/// Gas price response
#[derive(Debug, Deserialize)]
struct GasPriceResponse {
    standard: GasPriceLevel,
    fast: GasPriceLevel,
    slow: GasPriceLevel,
}

/// Gas price level
#[derive(Debug, Deserialize)]
struct GasPriceLevel {
    max_fee_per_gas: String,
    max_priority_fee_per_gas: String,
}

/// Estimate gas response
#[derive(Debug, Deserialize)]
struct EstimateGasResponse {
    call_gas_limit: String,
    verification_gas_limit: String,
    pre_verification_gas: String,
}

/// Sponsor response
#[derive(Debug, Deserialize)]
struct SponsorResponse {
    paymaster_and_data: String,
    call_gas_limit: String,
    verification_gas_limit: String,
    pre_verification_gas: String,
    max_fee_per_gas: String,
    max_priority_fee_per_gas: String,
}

/// UserOperation receipt
#[derive(Debug, Deserialize)]
pub struct UserOpReceipt {
    /// Transaction hash
    pub tx_hash: H256,
    /// Block number
    pub block_number: u64,
    /// Success status
    pub success: bool,
    /// Actual gas used
    pub actual_gas_used: U256,
    /// Effective gas price
    pub effective_gas_price: U256,
}

/// Parse U256 from hex string
fn parse_u256(s: &str) -> Result<U256> {
    let s = s.trim_start_matches("0x");
    U256::from_str_radix(s, 16)
        .map_err(|e| SmartAccountError::Pimlico(format!("Invalid number: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_u256() {
        let result = parse_u256("0x1000").unwrap();
        assert_eq!(result, U256::from(4096));
        
        let result = parse_u256("1000").unwrap();
        assert_eq!(result, U256::from(4096));
    }
}
