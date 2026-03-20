//! # Gas Estimation
//!
//! Gas estimation and fee calculation for UserOperations.

use crate::{Result, SmartAccountError, UserOperation, GasEstimate};
use ethers::types::U256;

/// Gas estimator for UserOperations
pub struct GasEstimator;

impl GasEstimator {
    /// Estimate gas for a simple transfer
    pub fn estimate_transfer() -> GasEstimate {
        GasEstimate {
            call_gas_limit: U256::from(21_000),
            verification_gas_limit: U256::from(100_000),
            pre_verification_gas: U256::from(50_000),
            max_fee_per_gas: U256::from(1_000_000_000), // 1 gwei
            max_priority_fee_per_gas: U256::from(100_000_000), // 0.1 gwei
        }
    }
    
    /// Estimate gas for ERC-20 transfer
    pub fn estimate_erc20_transfer() -> GasEstimate {
        GasEstimate {
            call_gas_limit: U256::from(65_000),
            verification_gas_limit: U256::from(100_000),
            pre_verification_gas: U256::from(50_000),
            max_fee_per_gas: U256::from(1_000_000_000),
            max_priority_fee_per_gas: U256::from(100_000_000),
        }
    }
    
    /// Estimate gas for contract interaction
    pub fn estimate_contract_call(data_size: usize) -> GasEstimate {
        // Base cost + cost per byte of calldata
        let base_gas = 100_000u64;
        let data_gas = (data_size as u64) * 16; // 16 gas per non-zero byte
        
        GasEstimate {
            call_gas_limit: U256::from(base_gas + data_gas),
            verification_gas_limit: U256::from(100_000),
            pre_verification_gas: U256::from(50_000),
            max_fee_per_gas: U256::from(1_000_000_000),
            max_priority_fee_per_gas: U256::from(100_000_000),
        }
    }
    
    /// Calculate total gas cost
    pub fn calculate_cost(gas: &GasEstimate) -> U256 {
        gas.total() * gas.max_fee_per_gas
    }
    
    /// Calculate project fee
    pub fn calculate_project_fee(
        gas_cost: U256,
        fee_percent_bps: u32,
    ) -> U256 {
        // fee = gas_cost * fee_percent_bps / 10000
        gas_cost * U256::from(fee_percent_bps) / U256::from(10_000)
    }
    
    /// Validate gas limits are within reasonable bounds
    pub fn validate_gas_limits(user_op: &UserOperation) -> Result<()> {
        const MAX_CALL_GAS: u64 = 10_000_000;
        const MAX_VERIFICATION_GAS: u64 = 1_000_000;
        const MAX_PRE_VERIFICATION_GAS: u64 = 500_000;
        
        if user_op.call_gas_limit > U256::from(MAX_CALL_GAS) {
            return Err(SmartAccountError::GasEstimation(
                format!("Call gas limit {} exceeds maximum {}", 
                    user_op.call_gas_limit, MAX_CALL_GAS)
            ));
        }
        
        if user_op.verification_gas_limit > U256::from(MAX_VERIFICATION_GAS) {
            return Err(SmartAccountError::GasEstimation(
                format!("Verification gas limit {} exceeds maximum {}",
                    user_op.verification_gas_limit, MAX_VERIFICATION_GAS)
            ));
        }
        
        if user_op.pre_verification_gas > U256::from(MAX_PRE_VERIFICATION_GAS) {
            return Err(SmartAccountError::GasEstimation(
                format!("Pre-verification gas {} exceeds maximum {}",
                    user_op.pre_verification_gas, MAX_PRE_VERIFICATION_GAS)
            ));
        }
        
        Ok(())
    }
    
    /// Apply gas buffer for safety
    pub fn apply_buffer(gas: &mut GasEstimate, buffer_percent: u32) {
        let multiplier = U256::from(100 + buffer_percent);
        let divisor = U256::from(100);
        
        gas.call_gas_limit = gas.call_gas_limit * multiplier / divisor;
        gas.verification_gas_limit = gas.verification_gas_limit * multiplier / divisor;
    }
}

/// Fee calculator for project fees
pub struct FeeCalculator {
    /// Fee percentage in basis points (100 = 1%)
    fee_percent_bps: u32,
    /// Minimum gas cost to charge fee on
    min_gas: U256,
}

impl FeeCalculator {
    /// Create new fee calculator
    pub fn new(fee_percent_bps: u32, min_gas: U256) -> Self {
        Self {
            fee_percent_bps,
            min_gas,
        }
    }
    
    /// Calculate fee for a transaction
    pub fn calculate_fee(&self, gas_cost: U256) -> U256 {
        if gas_cost < self.min_gas {
            return U256::zero();
        }
        
        // fee = gas_cost * fee_percent_bps / 10000
        gas_cost * U256::from(self.fee_percent_bps) / U256::from(10_000)
    }
    
    /// Calculate total cost including fee
    pub fn calculate_total(&self, gas_cost: U256) -> U256 {
        let fee = self.calculate_fee(gas_cost);
        gas_cost + fee
    }
    
    /// Get fee percentage
    pub fn fee_percent(&self) -> f64 {
        self.fee_percent_bps as f64 / 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_calculate_project_fee() {
        let gas_cost = U256::from(1_000_000_000_000_000u64); // 0.001 ETH
        let fee = GasEstimator::calculate_project_fee(gas_cost, 50); // 0.5%
        
        // 0.001 ETH * 0.005 = 0.000005 ETH = 5_000_000_000_000 wei
        assert_eq!(fee, U256::from(5_000_000_000_000u64));
    }
    
    #[test]
    fn test_fee_calculator() {
        let calc = FeeCalculator::new(50, U256::from(10_000_000_000_000u64));
        
        // Below minimum - no fee
        let small_cost = U256::from(1_000_000_000u64);
        assert_eq!(calc.calculate_fee(small_cost), U256::zero());
        
        // Above minimum - fee applied
        let large_cost = U256::from(100_000_000_000_000u64);
        let fee = calc.calculate_fee(large_cost);
        assert!(fee > U256::zero());
    }
    
    #[test]
    fn test_apply_buffer() {
        let mut gas = GasEstimator::estimate_transfer();
        let original_call_gas = gas.call_gas_limit;
        
        GasEstimator::apply_buffer(&mut gas, 20); // 20% buffer
        
        assert_eq!(gas.call_gas_limit, original_call_gas * 120 / 100);
    }
}
