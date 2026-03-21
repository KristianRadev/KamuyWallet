//! # UserOperation Tests
//!
//! Comprehensive tests for UserOperation building and validation.

use kamuy_smart_account::{Chain, UserOpBuilder, UserOperation, GasEstimator};
use ethers::types::{Address, U256, Bytes};

const BASE_CHAIN_ID: u64 = 8453;

#[test]
fn test_user_op_builder_basic() {
    let sender = "0x1234567890123456789012345678901234567890"
        .parse()
        .unwrap();

    let user_op = UserOpBuilder::new(BASE_CHAIN_ID, sender)
        .nonce(U256::from(1))
        .call_data(vec![0x01, 0x02, 0x03])
        .fees(U256::from(1000000000), U256::from(1000000))
        .build()
        .unwrap();

    assert_eq!(user_op.sender, sender);
    assert_eq!(user_op.nonce, U256::from(1));
    assert_eq!(user_op.call_data.to_vec(), vec![0x01, 0x02, 0x03]);
    assert_eq!(user_op.max_fee_per_gas, U256::from(1000000000));
    assert_eq!(user_op.max_priority_fee_per_gas, U256::from(1000000));
}

#[test]
fn test_user_op_builder_with_gas_limits() {
    let sender = "0x1234567890123456789012345678901234567890"
        .parse()
        .unwrap();

    let user_op = UserOpBuilder::new(BASE_CHAIN_ID, sender)
        .gas_limits(
            U256::from(100_000),
            U256::from(50_000),
            U256::from(25_000),
        )
        .fees(U256::from(1000000000), U256::from(1000000))
        .build()
        .unwrap();

    assert_eq!(user_op.call_gas_limit, U256::from(100_000));
    assert_eq!(user_op.verification_gas_limit, U256::from(50_000));
    assert_eq!(user_op.pre_verification_gas, U256::from(25_000));
}

#[test]
fn test_user_op_builder_with_paymaster() {
    let sender = "0x1234567890123456789012345678901234567890"
        .parse()
        .unwrap();

    let paymaster_data = vec![0xab, 0xcd, 0xef];

    let user_op = UserOpBuilder::new(BASE_CHAIN_ID, sender)
        .paymaster(paymaster_data.clone())
        .fees(U256::from(1000000000), U256::from(1000000))
        .build()
        .unwrap();

    assert_eq!(user_op.paymaster_and_data.to_vec(), paymaster_data);
}

#[test]
fn test_user_op_validation_empty_sender() {
    let user_op = UserOperation::new(Address::zero());
    
    let result = user_op.validate();
    assert!(result.is_err());
}

#[test]
fn test_user_op_validation_zero_max_fee() {
    let sender = "0x1234567890123456789012345678901234567890"
        .parse()
        .unwrap();
    
    let user_op = UserOperation::new(sender);
    
    let result = user_op.validate();
    assert!(result.is_err());
}

#[test]
fn test_user_op_hash() {
    let sender = "0x1234567890123456789012345678901234567890"
        .parse()
        .unwrap();

    let user_op = UserOpBuilder::new(BASE_CHAIN_ID, sender)
        .nonce(U256::from(1))
        .call_data(vec![0x01])
        .fees(U256::from(1000000000), U256::from(1000000))
        .build()
        .unwrap();

    let entry_point = Chain::Base.entry_point();
    let hash = user_op.hash(entry_point, BASE_CHAIN_ID);

    // Hash should be non-zero
    assert_ne!(hash.0, [0u8; 32]);
}

#[test]
fn test_user_op_hash_deterministic() {
    let sender = "0x1234567890123456789012345678901234567890"
        .parse()
        .unwrap();

    let user_op1 = UserOpBuilder::new(BASE_CHAIN_ID, sender)
        .nonce(U256::from(1))
        .call_data(vec![0x01])
        .fees(U256::from(1000000000), U256::from(1000000))
        .build()
        .unwrap();

    let user_op2 = UserOpBuilder::new(BASE_CHAIN_ID, sender)
        .nonce(U256::from(1))
        .call_data(vec![0x01])
        .fees(U256::from(1000000000), U256::from(1000000))
        .build()
        .unwrap();

    let entry_point = Chain::Base.entry_point();
    let hash1 = user_op1.hash(entry_point, BASE_CHAIN_ID);
    let hash2 = user_op2.hash(entry_point, BASE_CHAIN_ID);

    // Same inputs should produce same hash
    assert_eq!(hash1.0, hash2.0);
}

#[test]
fn test_user_op_total_gas() {
    let sender = "0x1234567890123456789012345678901234567890"
        .parse()
        .unwrap();

    let user_op = UserOpBuilder::new(BASE_CHAIN_ID, sender)
        .gas_limits(
            U256::from(100_000),
            U256::from(50_000),
            U256::from(25_000),
        )
        .fees(U256::from(1000000000), U256::from(1000000))
        .build()
        .unwrap();

    let total = user_op.total_gas();
    assert_eq!(total, U256::from(175_000));
}

#[test]
fn test_user_op_max_cost() {
    let sender = "0x1234567890123456789012345678901234567890"
        .parse()
        .unwrap();

    let user_op = UserOpBuilder::new(BASE_CHAIN_ID, sender)
        .gas_limits(
            U256::from(100_000),
            U256::from(50_000),
            U256::from(25_000),
        )
        .fees(U256::from(10), U256::from(1))
        .build()
        .unwrap();

    let max_cost = user_op.max_cost();
    assert_eq!(max_cost, U256::from(1_750_000)); // 175_000 * 10
}

#[test]
fn test_gas_estimator_transfer() {
    let gas = GasEstimator::estimate_transfer();
    
    assert_eq!(gas.call_gas_limit, U256::from(21_000));
    assert_eq!(gas.verification_gas_limit, U256::from(100_000));
    assert_eq!(gas.pre_verification_gas, U256::from(50_000));
}

#[test]
fn test_gas_estimator_erc20() {
    let gas = GasEstimator::estimate_erc20_transfer();
    
    assert_eq!(gas.call_gas_limit, U256::from(65_000));
    assert!(gas.total() > U256::zero());
}

#[test]
fn test_gas_estimator_contract_call() {
    let gas = GasEstimator::estimate_contract_call(100);
    
    // Base 100_000 + 100 * 16 = 101_600
    assert_eq!(gas.call_gas_limit, U256::from(101_600));
}

#[test]
fn test_chain_from_id() {
    assert_eq!(Chain::from_chain_id(1), Some(Chain::Ethereum));
    assert_eq!(Chain::from_chain_id(8453), Some(Chain::Base));
    assert_eq!(Chain::from_chain_id(137), Some(Chain::Polygon));
    assert_eq!(Chain::from_chain_id(99999), None);
}

#[test]
fn test_chain_entry_point() {
    let entry_base = Chain::Base.entry_point();
    let entry_eth = Chain::Ethereum.entry_point();
    
    // Entry point should be same across chains
    assert_eq!(entry_base, entry_eth);
}
