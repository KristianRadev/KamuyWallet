//! # Test MPC Keys Generation
//!
//! Run this binary to generate test keys for Base Sepolia testing.

use kamuy_smart_account::mpc::TestMpcKeys;
use hex;

fn main() {
    // Generate deterministic test keys
    let seed = b"kamuy_base_sepolia_test_key_seed_2024";
    let keys = TestMpcKeys::from_seed(seed);

    println!("=== MPC Test Keys for Base Sepolia ===\n");

    println!("Agent Key (Key #1):");
    println!("  Private: 0x{}", hex::encode(keys.agent_key));
    println!("  Address: {:?}", keys.agent_address);
    println!();

    println!("Steward Key (Key #2):");
    println!("  Private: 0x{}", hex::encode(keys.steward_key));
    println!("  Address: {:?}", keys.steward_address);
    println!();

    println!("User Key (Key #3):");
    println!("  Private: 0x{}", hex::encode(keys.user_key));
    println!("  Address: {:?}", keys.user_address);
    println!();

    println!("=== For MpcSmartAccount.sol ===");
    println!("Initialize with:");
    println!("  _agent: {:?}", keys.agent_address);
    println!("  _steward: {:?}", keys.steward_address);
    println!("  _user: {:?}", keys.user_address);
    println!();

    println!("=== Environment Variables ===");
    println!("export MPC_AGENT_KEY=0x{}", hex::encode(keys.agent_key));
    println!("export MPC_STEWARD_KEY=0x{}", hex::encode(keys.steward_key));
    println!("export MPC_USER_KEY=0x{}", hex::encode(keys.user_key));
}