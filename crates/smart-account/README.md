# Kamuy Smart Account

ERC-4337 Smart Account integration with Pimlico for gas sponsorship and bundling.

## Features

- **UserOperation Building**: Construct and validate ERC-4337 UserOperations
- **Pimlico Integration**: Gas sponsorship, bundling, and submission via Pimlico API
- **Kernel Wallet Support**: Integration with ZeroDev Kernel smart wallet
- **Multi-Chain**: Base, Polygon, Arbitrum, Optimism, Ethereum
- **Fee Collection**: Configurable project fees via paymaster

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Smart Account Module                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐     │
│  │  UserOp      │    │   Pimlico    │    │   Kernel     │     │
│  │  Builder     │───▶│   Client     │───▶│   Wallet     │     │
│  │              │    │              │    │              │     │
│  │ • Build      │    │ • Sponsor    │    │ • Validate   │     │
│  │ • Hash       │    │ • Bundle     │    │ • Execute    │     │
│  │ • Sign       │    │ • Submit     │    │ • Paymaster  │     │
│  └──────────────┘    └──────────────┘    └──────────────┘     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Quick Start

```rust
use kamuy_smart_account::{PimlicoClient, PimlicoConfig, Chain, UserOpBuilder};

#[tokio::main]
async fn main() -> Result<()> {
    // Configure Pimlico client
    let config = PimlicoConfig {
        api_key: "your-api-key".to_string(),
        chain: Chain::Base,
        ..Default::default()
    };
    
    let pimlico = PimlicoClient::new(config)?;
    
    // Get gas estimates
    let gas = pimlico.get_gas_estimates().await?;
    
    // Build UserOperation
    let user_op = UserOpBuilder::new(Chain::Base, wallet_address)
        .nonce(nonce)
        .call_data(calldata)
        .fees(gas.max_fee_per_gas, gas.max_priority_fee_per_gas)
        .build()?;
    
    // Get sponsorship
    let sponsor = pimlico.sponsor_user_op(&user_op).await?;
    
    // Submit
    let result = pimlico.send_user_op(&user_op).await?;
    
    Ok(())
}
```

## Supported Chains

| Chain | Chain ID | EntryPoint |
|-------|----------|------------|
| Ethereum | 1 | 0x5FF137D4b0FDCD49DcA30c7CF57E578a026d2789 |
| Base | 8453 | 0x5FF137D4b0FDCD49DcA30c7CF57E578a026d2789 |
| Polygon | 137 | 0x5FF137D4b0FDCD49DcA30c7CF57E578a026d2789 |
| Arbitrum | 42161 | 0x5FF137D4b0FDCD49DcA30c7CF57E578a026d2789 |
| Optimism | 10 | 0x5FF137D4b0FDCD49DcA30c7CF57E578a026d2789 |

## Fee Collection

Project fees are collected as a percentage of gas costs (not transaction value):

```rust
let fee_config = FeeConfig {
    fee_percent_bps: 50, // 0.5%
    min_gas: U256::from(10_000_000_000_000u64), // 0.01 ETH
};
```

## Security

- HTTP client timeout: 30 seconds
- Gas limits validated before submission
- Paymaster data verified
- Chain ID validated for all operations

## License

MIT OR Apache-2.0
