//! # Steward Integration with MpcSmartAccount
//!
//! This module provides the integration layer between the Steward service
//! and the MpcSmartAccount contract.
//!
//! ## Responsibilities
//!
//! - Wallet creation via factory
//! - Transaction signing with MPC keys
//! - UserOperation building and submission
//! - Policy enforcement before signing

use crate::{Result, SmartAccountError, UserOperation, UserOpBuilder, entry_point_address};
use crate::mpc::{MpcSignature, Party, sign_message};
use ethers::types::{Address, U256, H256, Bytes};

/// MpcSmartAccount configuration
#[derive(Debug, Clone)]
pub struct MpcAccountConfig {
    /// Factory contract address
    pub factory_address: Address,
    /// EntryPoint address (v0.7)
    pub entry_point_address: Address,
    /// Chain ID
    pub chain_id: u64,
    /// Agent signer address
    pub agent_address: Address,
    /// Steward signer address
    pub steward_address: Address,
    /// User signer address
    pub user_address: Address,
}

impl MpcAccountConfig {
    /// Create config for Base Sepolia testnet
    pub fn base_sepolia(
        factory_address: Address,
        agent_address: Address,
        steward_address: Address,
        user_address: Address,
    ) -> Self {
        Self {
            factory_address,
            entry_point_address: entry_point_address(),
            chain_id: 84532, // Base Sepolia
            agent_address,
            steward_address,
            user_address,
        }
    }

    /// Create config for Base mainnet
    pub fn base_mainnet(
        factory_address: Address,
        agent_address: Address,
        steward_address: Address,
        user_address: Address,
    ) -> Self {
        Self {
            factory_address,
            entry_point_address: entry_point_address(),
            chain_id: 8453,
            agent_address,
            steward_address,
            user_address,
        }
    }
}

/// MpcSmartAccount client
pub struct MpcAccountClient {
    /// Configuration
    config: MpcAccountConfig,
    /// Deployed account address (None if not yet created)
    account_address: Option<Address>,
    /// Steward private key (for signing)
    steward_key: Option<[u8; 32]>,
}

impl MpcAccountClient {
    /// Create a new client
    pub fn new(config: MpcAccountConfig) -> Self {
        Self {
            config,
            account_address: None,
            steward_key: None,
        }
    }

    /// Load the Steward key
    pub fn load_steward_key(&mut self, key: [u8; 32]) {
        self.steward_key = Some(key);
    }

    /// Set the account address (if already deployed)
    pub fn set_account_address(&mut self, address: Address) {
        self.account_address = Some(address);
    }

    /// Get the account address
    pub fn account_address(&self) -> Option<Address> {
        self.account_address
    }

    /// Get the signers
    pub fn signers(&self) -> [Address; 3] {
        [
            self.config.agent_address,
            self.config.steward_address,
            self.config.user_address,
        ]
    }

    /// Build init_code for account creation
    /// This is the bytecode to deploy a new MpcSmartAccount
    pub fn build_init_code(&self, salt: U256) -> Bytes {
        // init_code = factory_address + salt + init_call_data
        // init_call_data = initialize(agent, steward, user)

        let init_call_data = ethers::abi::encode(&[
            ethers::abi::Token::Address(self.config.agent_address),
            ethers::abi::Token::Address(self.config.steward_address),
            ethers::abi::Token::Address(self.config.user_address),
        ]);

        // Encode factory call
        let factory_call = ethers::abi::encode(&[
            ethers::abi::Token::Address(self.config.agent_address),
            ethers::abi::Token::Address(self.config.steward_address),
            ethers::abi::Token::Address(self.config.user_address),
            ethers::abi::Token::Uint(salt),
        ]);

        // init_code = factory_address + function_selector + args
        let mut init_code = Vec::new();
        init_code.extend_from_slice(self.config.factory_address.as_bytes());
        // function selector for createAccount(address,address,address,uint256)
        init_code.extend_from_slice(&keccak256_hash("createAccount(address,address,address,uint256)")[..4]);
        init_code.extend_from_slice(&factory_call);

        Bytes::from(init_code)
    }

    /// Build call_data for execute function
    pub fn build_execute_calldata(&self, to: Address, value: U256, data: Bytes) -> Bytes {
        // execute(address,uint256,bytes)
        let encoded = ethers::abi::encode(&[
            ethers::abi::Token::Address(to),
            ethers::abi::Token::Uint(value),
            ethers::abi::Token::Bytes(data.to_vec()),
        ]);

        let mut call_data = Vec::new();
        // function selector for execute(address,uint256,bytes)
        call_data.extend_from_slice(&keccak256_hash("execute(address,uint256,bytes)")[..4]);
        call_data.extend_from_slice(&encoded);

        Bytes::from(call_data)
    }

    /// Build a UserOperation for a transaction
    pub fn build_user_op(
        &self,
        to: Address,
        value: U256,
        data: Bytes,
        nonce: U256,
        gas_estimate: crate::GasEstimate,
        init_code: Option<Bytes>,
    ) -> Result<UserOperation> {
        let sender = self.account_address.ok_or_else(|| {
            SmartAccountError::InvalidUserOp("Account not deployed".to_string())
        })?;

        let call_data = self.build_execute_calldata(to, value, data);

        let mut builder = UserOpBuilder::new(self.config.chain_id, sender)
            .nonce(nonce)
            .call_data(call_data)
            .gas_limits(
                gas_estimate.call_gas_limit,
                gas_estimate.verification_gas_limit,
                gas_estimate.pre_verification_gas,
            )
            .fees(gas_estimate.max_fee_per_gas, gas_estimate.max_priority_fee_per_gas);

        if let Some(init) = init_code {
            builder = builder.init_code(init);
        }

        builder.build()
    }

    /// Sign a UserOperation hash with Agent + Steward keys
    /// This is for auto-approved transactions
    pub fn sign_with_agent_steward(
        &self,
        user_op_hash: &H256,
        agent_key: &[u8; 32],
    ) -> Result<MpcSignature> {
        let steward_key = self.steward_key.ok_or_else(|| {
            SmartAccountError::Signature("Steward key not loaded".to_string())
        })?;

        let sig1 = sign_message(agent_key, user_op_hash)?;
        let sig2 = sign_message(&steward_key, user_op_hash)?;

        Ok(MpcSignature::new(Party::Agent, Party::Steward, sig1, sig2))
    }

    /// Sign a UserOperation hash with Agent + User keys
    /// This is for user override scenarios
    pub fn sign_with_agent_user(
        &self,
        user_op_hash: &H256,
        agent_key: &[u8; 32],
        user_key: &[u8; 32],
    ) -> Result<MpcSignature> {
        let sig1 = sign_message(agent_key, user_op_hash)?;
        let sig2 = sign_message(user_key, user_op_hash)?;

        Ok(MpcSignature::new(Party::Agent, Party::User, sig1, sig2))
    }

    /// Sign a UserOperation hash with Steward + User keys
    /// This is for recovery scenarios
    pub fn sign_with_steward_user(
        &self,
        user_op_hash: &H256,
        user_key: &[u8; 32],
    ) -> Result<MpcSignature> {
        let steward_key = self.steward_key.ok_or_else(|| {
            SmartAccountError::Signature("Steward key not loaded".to_string())
        })?;

        let sig1 = sign_message(&steward_key, user_op_hash)?;
        let sig2 = sign_message(user_key, user_op_hash)?;

        Ok(MpcSignature::new(Party::Steward, Party::User, sig1, sig2))
    }

    /// Apply MPC signature to UserOperation
    pub fn apply_signature(&self, user_op: UserOperation, mpc_sig: &MpcSignature) -> UserOperation {
        user_op.with_signature(Bytes::from(mpc_sig.encode()))
    }
}

/// Compute keccak256 hash
fn keccak256_hash(input: &str) -> [u8; 32] {
    use sha3::{Digest, Keccak256};
    let mut hasher = Keccak256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Compute the counterfactual address for an MpcSmartAccount
/// This is the address the account will have before it's deployed
pub fn compute_account_address(
    factory_address: Address,
    agent_address: Address,
    steward_address: Address,
    user_address: Address,
    salt: U256,
) -> Address {
    // This would require the bytecode of MpcSmartAccount
    // For now, we'll compute it on-chain via the factory
    // In production, this would be computed locally using CREATE2 logic

    // Placeholder - actual implementation would need:
    // 1. The creation code of MpcSmartAccount
    // 2. The constructor arguments
    // 3. Apply CREATE2 formula: keccak256(0xff + factory + salt + keccak256(bytecode))

    Address::zero()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_execute_calldata() {
        let config = MpcAccountConfig {
            factory_address: Address::zero(),
            entry_point_address: entry_point_address(),
            chain_id: 84532,
            agent_address: Address::zero(),
            steward_address: Address::zero(),
            user_address: Address::zero(),
        };

        let client = MpcAccountClient::new(config);

        let to: Address = "0x1234567890123456789012345678901234567890".parse().unwrap();
        let calldata = client.build_execute_calldata(to, U256::from(100), Bytes::default());

        // Should start with function selector
        assert_eq!(calldata.len(), 4 + 128); // selector + 3 args (padded)
    }
}