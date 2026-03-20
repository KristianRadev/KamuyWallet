// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import {Script, console} from "forge-std/Script.sol";
import {MpcSmartAccountFactory} from "../src/MpcSmartAccount.sol";

/**
 * @title DeployMpcContracts
 * @notice Foundry script to deploy MPC wallet contracts
 * @dev Usage: forge script script/DeployMpcContracts.s.sol --rpc-url $RPC_URL --broadcast
 */
contract DeployMpcContracts is Script {
    // EntryPoint v0.7 addresses (from official deployments)
    address constant ENTRY_POINT = 0x0000000071727De22E5E9d8BAf0edAc6f37da032;

    // USDC addresses
    address constant USDC_BASE = 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913;
    address constant USDC_ETHEREUM = 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48;
    address constant USDC_ARBITRUM = 0xaf88d065e77c8cC2239327C5EDb3A432268e5831;
    address constant USDC_OPTIMISM = 0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85;
    address constant USDC_POLYGON = 0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359;
    address constant USDC_BASE_SEPOLIA = 0x036CbD53842c5426634e7929541eC2318f3dCF7e;

    // Chainlink Price Feeds (USDC/ETH)
    // Note: These are ETH/USD feeds. For USDC/ETH, we need to compute the ratio
    // or use a direct USDC/ETH feed if available
    address constant ETH_USD_FEED_BASE = 0x71041dddad3595F9CEd3DcCFBe3D1F4b0a16Bb70;
    address constant ETH_USD_FEED_ETHEREUM = 0x5f4eC3Df9cbd43714FE2740f5E3616155c5b8419;
    address constant ETH_USD_FEED_BASE_SEPOLIA = 0x4AdC67696Ba383F43dd60a9E78f2c97fbbFC88CB;

    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        uint256 chainId = block.chainid;

        (address usdc, address priceFeed) = _getChainConfig(chainId);

        vm.startBroadcast(deployerPrivateKey);

        // Deploy MpcSmartAccountFactory
        MpcSmartAccountFactory factory = new MpcSmartAccountFactory(ENTRY_POINT);
        console.log("MpcSmartAccountFactory deployed at:", address(factory));

        vm.stopBroadcast();

        // Log deployment info
        console.log("--- Deployment Complete ---");
        console.log("Chain ID:", chainId);
        console.log("EntryPoint:", ENTRY_POINT);
        console.log("USDC:", usdc);
        console.log("Price Feed:", priceFeed);
        console.log("Factory:", address(factory));

        // Log test keys for Base Sepolia
        if (chainId == 84532) {
            console.log("--- Test MPC Keys ---");
            console.log("Agent: 0x5f66e52ede8a002e97b5bf729410b7d11b0b87e2");
            console.log("Steward: 0x343aadc413b9a933d9b27d3fc9cd5d78cb92332d");
            console.log("User: 0x0656656a9be64c62222551baac005471bc0ec145");
        }
    }

    function _getChainConfig(uint256 chainId) internal pure returns (address usdc, address priceFeed) {
        if (chainId == 8453) {
            // Base mainnet
            usdc = USDC_BASE;
            priceFeed = ETH_USD_FEED_BASE;
        } else if (chainId == 1) {
            // Ethereum mainnet
            usdc = USDC_ETHEREUM;
            priceFeed = ETH_USD_FEED_ETHEREUM;
        } else if (chainId == 84532) {
            // Base Sepolia testnet
            usdc = USDC_BASE_SEPOLIA;
            priceFeed = ETH_USD_FEED_BASE_SEPOLIA;
        } else if (chainId == 42161) {
            // Arbitrum
            usdc = USDC_ARBITRUM;
            priceFeed = ETH_USD_FEED_ETHEREUM; // Use mainnet feed as proxy
        } else if (chainId == 10) {
            // Optimism
            usdc = USDC_OPTIMISM;
            priceFeed = ETH_USD_FEED_ETHEREUM; // Use mainnet feed as proxy
        } else if (chainId == 137) {
            // Polygon
            usdc = USDC_POLYGON;
            priceFeed = ETH_USD_FEED_ETHEREUM; // Use mainnet feed as proxy
        } else {
            // Default to Base Sepolia for unknown chains (testing)
            usdc = USDC_BASE_SEPOLIA;
            priceFeed = ETH_USD_FEED_BASE_SEPOLIA;
        }
    }
}
