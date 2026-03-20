# Base Sepolia Deployment

This directory contains deployment artifacts and addresses for Base Sepolia testnet.

## Deployed Contracts

### EntryPoint v0.7
- Address: `0x0000000071727De22E5E9d8BAf0edAc6f37da032`
- Same address across all chains

### MpcSmartAccountFactory
- Address: TBD (deploy first)

### MpcPaymaster
- Address: TBD (deploy after factory)

## Deployment Steps

1. Set environment variables:
```bash
export PRIVATE_KEY=your_private_key
export RPC_URL=https://sepolia.base.org
```

2. Deploy contracts:
```bash
forge script script/DeployMpcContracts.s.sol --rpc-url $RPC_URL --broadcast
```

3. Record deployed addresses in this file.

## Test Keys

For testing, use deterministic keys derived from a seed:

```rust
use kamuy_smart_account::mpc::TestMpcKeys;

let keys = TestMpcKeys::from_seed(b"test_seed_for_base_sepolia");
println!("Agent: {:?}", keys.agent_address);
println!("Steward: {:?}", keys.steward_address);
println!("User: {:?}", keys.user_address);
```

## Pimlico API

Get an API key from [pimlico.io](https://pimlico.io) and set:
```bash
export PIMLICO_API_KEY=your_api_key
```

RPC URL: `https://api.pimlico.io/v1/base-sepolia/rpc?apikey=$PIMLICO_API_KEY`