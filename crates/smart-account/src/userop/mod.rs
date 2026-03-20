//! # ERC-4337 EntryPoint v0.7 Types
//!
//! This module provides types compatible with EntryPoint v0.7.
//! Key differences from v0.6:
//! - `initCode` replaces `init_code` (renamed)
//! - Account creation is handled differently
//! - Hash computation is different

use ethers::types::{Address, U256, H256, Bytes};
use ethers::utils::keccak256;
use serde::{Deserialize, Serialize};

/// EntryPoint v0.7 address (same across all chains)
pub const ENTRY_POINT_V07: &str = "0x0000000071727De22E5E9d8BAf0edAc6f37da032";

/// Get EntryPoint address
pub fn entry_point_address() -> Address {
    ENTRY_POINT_V07.parse().expect("Valid EntryPoint address")
}

/// ERC-4337 UserOperation v0.7
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserOperation {
    /// Sender address (smart account)
    pub sender: Address,
    /// Nonce
    pub nonce: U256,
    /// Init code (for account creation)
    pub init_code: Bytes,
    /// Call data
    pub call_data: Bytes,
    /// Call gas limit
    pub call_gas_limit: U256,
    /// Verification gas limit
    pub verification_gas_limit: U256,
    /// Pre-verification gas
    pub pre_verification_gas: U256,
    /// Max fee per gas
    pub max_fee_per_gas: U256,
    /// Max priority fee per gas
    pub max_priority_fee_per_gas: U256,
    /// Paymaster and data
    pub paymaster_and_data: Bytes,
    /// Signature
    pub signature: Bytes,
}

impl UserOperation {
    /// Create a new UserOperation
    pub fn new(sender: Address) -> Self {
        Self {
            sender,
            nonce: U256::zero(),
            init_code: Bytes::default(),
            call_data: Bytes::default(),
            call_gas_limit: U256::from(200_000),
            verification_gas_limit: U256::from(500_000),
            pre_verification_gas: U256::from(100_000),
            max_fee_per_gas: U256::zero(),
            max_priority_fee_per_gas: U256::zero(),
            paymaster_and_data: Bytes::default(),
            signature: Bytes::default(),
        }
    }

    /// Compute the hash of the UserOperation (v0.7 format)
    /// hash = keccak256(abi.encode(userOp)) - for the userOp itself
    /// userOpHash = keccak256(abi.encode(hash(sender), nonce, hash(initCode), hash(callData),
    ///                 callGasLimit, verificationGasLimit, preVerificationGas,
    ///                 maxFeePerGas, maxPriorityFeePerGas, hash(paymasterAndData)))
    pub fn pack(&self) -> Vec<u8> {
        ethers::abi::encode(&[
            ethers::abi::Token::Address(self.sender),
            ethers::abi::Token::Uint(self.nonce),
            ethers::abi::Token::Uint(U256::from(keccak256(&self.init_code))),
            ethers::abi::Token::Uint(U256::from(keccak256(&self.call_data))),
            ethers::abi::Token::Uint(self.call_gas_limit),
            ethers::abi::Token::Uint(self.verification_gas_limit),
            ethers::abi::Token::Uint(self.pre_verification_gas),
            ethers::abi::Token::Uint(self.max_fee_per_gas),
            ethers::abi::Token::Uint(self.max_priority_fee_per_gas),
            ethers::abi::Token::Uint(U256::from(keccak256(&self.paymaster_and_data))),
        ])
    }

    /// Compute the UserOperation hash
    /// This is the hash that needs to be signed by the MPC parties
    pub fn hash(&self, entry_point: Address, chain_id: u64) -> H256 {
        let packed = keccak256(self.pack());
        let with_context = ethers::abi::encode(&[
            ethers::abi::Token::FixedBytes(packed.to_vec()),
            ethers::abi::Token::Address(entry_point),
            ethers::abi::Token::Uint(chain_id.into()),
        ]);
        H256::from_slice(&keccak256(&with_context))
    }

    /// Get the total gas cost estimate
    pub fn total_gas(&self) -> U256 {
        self.call_gas_limit
            + self.verification_gas_limit
            + self.pre_verification_gas
    }

    /// Get the max total cost
    pub fn max_cost(&self) -> U256 {
        self.total_gas() * self.max_fee_per_gas
    }

    /// Set the signature (MPC 2-of-3 format)
    pub fn with_mpc_signature(mut self, party_indices: u8, sig1: &[u8; 65], sig2: &[u8; 65]) -> Self {
        let mut signature = Vec::with_capacity(131);
        signature.push(party_indices);
        signature.extend_from_slice(sig1);
        signature.extend_from_slice(sig2);
        self.signature = Bytes::from(signature);
        self
    }

    /// Set the signature (standard format)
    pub fn with_signature(mut self, signature: Bytes) -> Self {
        self.signature = signature;
        self
    }

    /// Validate the UserOperation
    pub fn validate(&self) -> crate::Result<()> {
        if self.sender == Address::zero() {
            return Err(crate::SmartAccountError::InvalidUserOp(
                "Sender cannot be zero address".to_string()
            ));
        }

        if self.call_gas_limit > U256::from(30_000_000) {
            return Err(crate::SmartAccountError::InvalidUserOp(
                "Call gas limit too high".to_string()
            ));
        }

        if self.max_fee_per_gas.is_zero() {
            return Err(crate::SmartAccountError::InvalidUserOp(
                "Max fee per gas cannot be zero".to_string()
            ));
        }

        Ok(())
    }
}

/// Gas estimates for UserOperation
#[derive(Debug, Clone)]
pub struct GasEstimate {
    /// Call gas limit
    pub call_gas_limit: U256,
    /// Verification gas limit
    pub verification_gas_limit: U256,
    /// Pre-verification gas
    pub pre_verification_gas: U256,
    /// Max fee per gas
    pub max_fee_per_gas: U256,
    /// Max priority fee per gas
    pub max_priority_fee_per_gas: U256,
}

impl GasEstimate {
    /// Get total gas
    pub fn total(&self) -> U256 {
        self.call_gas_limit + self.verification_gas_limit + self.pre_verification_gas
    }

    /// Get max cost
    pub fn max_cost(&self) -> U256 {
        self.total() * self.max_fee_per_gas
    }
}

/// UserOperation builder for EntryPoint v0.7
pub struct UserOpBuilder {
    /// Chain ID
    chain_id: u64,
    /// Entry point address
    entry_point: Address,
    /// Sender (smart account address)
    sender: Address,
    /// Current UserOperation being built
    user_op: UserOperation,
}

impl UserOpBuilder {
    /// Create a new builder
    pub fn new(chain_id: u64, sender: Address) -> Self {
        Self {
            chain_id,
            entry_point: entry_point_address(),
            sender,
            user_op: UserOperation::new(sender),
        }
    }

    /// Set the nonce
    pub fn nonce(mut self, nonce: U256) -> Self {
        self.user_op.nonce = nonce;
        self
    }

    /// Set init code (for account creation)
    pub fn init_code(mut self, init_code: impl Into<Bytes>) -> Self {
        self.user_op.init_code = init_code.into();
        self
    }

    /// Set the call data
    pub fn call_data(mut self, data: impl Into<Bytes>) -> Self {
        self.user_op.call_data = data.into();
        self
    }

    /// Set gas limits
    pub fn gas_limits(
        mut self,
        call_gas: U256,
        verification_gas: U256,
        pre_verification_gas: U256,
    ) -> Self {
        self.user_op.call_gas_limit = call_gas;
        self.user_op.verification_gas_limit = verification_gas;
        self.user_op.pre_verification_gas = pre_verification_gas;
        self
    }

    /// Set fee values
    pub fn fees(mut self, max_fee: U256, max_priority_fee: U256) -> Self {
        self.user_op.max_fee_per_gas = max_fee;
        self.user_op.max_priority_fee_per_gas = max_priority_fee;
        self
    }

    /// Set paymaster data
    pub fn paymaster(mut self, paymaster_and_data: impl Into<Bytes>) -> Self {
        self.user_op.paymaster_and_data = paymaster_and_data.into();
        self
    }

    /// Build the UserOperation
    pub fn build(self) -> crate::Result<UserOperation> {
        self.user_op.validate()?;
        Ok(self.user_op)
    }

    /// Get the hash for signing
    pub fn hash(&self) -> H256 {
        self.user_op.hash(self.entry_point, self.chain_id)
    }

    /// Get the entry point address
    pub fn entry_point(&self) -> Address {
        self.entry_point
    }

    /// Get the chain ID
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_op_hash() {
        let sender: Address = "0x1234567890123456789012345678901234567890".parse().unwrap();
        let user_op = UserOperation::new(sender);

        let hash = user_op.hash(entry_point_address(), 84532);
        assert_ne!(hash, H256::zero());
    }

    #[test]
    fn test_mpc_signature_format() {
        let sender: Address = "0x1234567890123456789012345678901234567890".parse().unwrap();
        let sig1 = [0x01u8; 65];
        let sig2 = [0x02u8; 65];
        let party_indices = 0x12; // Party 0 (lower nibble) and Party 1 (upper nibble)

        let user_op = UserOperation::new(sender)
            .with_mpc_signature(party_indices, &sig1, &sig2);

        assert_eq!(user_op.signature.len(), 131);
        assert_eq!(user_op.signature[0], party_indices);
        assert_eq!(&user_op.signature[1..66], &[0x01u8; 65]);
        assert_eq!(&user_op.signature[66..131], &[0x02u8; 65]);
    }
}