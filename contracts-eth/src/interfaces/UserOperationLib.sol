// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import {UserOperation} from "./IEntryPoint.sol";

/**
 * @title UserOperationLib
 * @notice Library for UserOperation utilities
 */
library UserOperationLib {
    /**
     * @notice Get the packed data from a UserOperation
     */
    function getPackData(UserOperation calldata userOp) internal pure returns (bytes memory) {
        return abi.encode(
            userOp.sender,
            userOp.nonce,
            keccak256(userOp.initCode),
            keccak256(userOp.callData),
            userOp.callGasLimit,
            userOp.verificationGasLimit,
            userOp.preVerificationGas,
            userOp.maxFeePerGas,
            userOp.maxPriorityFeePerGas,
            keccak256(userOp.paymasterAndData)
        );
    }

    /**
     * @notice Hash a UserOperation
     */
    function hash(UserOperation calldata userOp) internal pure returns (bytes32) {
        return keccak256(getPackData(userOp));
    }
}
