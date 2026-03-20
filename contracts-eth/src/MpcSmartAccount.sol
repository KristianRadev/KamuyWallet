// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import {IEntryPoint, UserOperation} from "./interfaces/IEntryPoint.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/**
 * @title MpcSmartAccount
 * @notice ERC-4337 smart account with MPC 2-of-3 threshold signature validation
 * @dev Supports three key holders:
 *      - Agent (Key #1): AI agent that initiates transactions
 *      - Steward (Key #2): Policy engine that co-signs compliant transactions
 *      - User (Key #3): Ultimate owner for recovery and overrides
 *      Any 2 of 3 parties can authorize transactions.
 */
contract MpcSmartAccount is ReentrancyGuard {
    // ============ Constants ============

    /// @notice Number of parties in the MPC scheme
    uint8 public constant N_PARTIES = 3;

    /// @notice Threshold of signatures required (2-of-3)
    uint8 public constant THRESHOLD = 2;

    /// @notice Size of ECDSA signature (r, s, v)
    uint8 public constant SIGNATURE_SIZE = 65;

    /// @notice Multi-signature size (2 signatures + party indices)
    uint8 public constant MULTI_SIG_SIZE = 131; // 65 * 2 + 1 (party indices byte)

    /// @notice Maximum batch size for executeBatch
    uint256 public constant MAX_BATCH_SIZE = 32;

    // ============ Storage ============

    /// @notice The three public keys for MPC parties
    /// @dev Index 0 = Agent, 1 = Steward, 2 = User
    address[3] public signers;

    /// @notice The EntryPoint contract address
    IEntryPoint public immutable ENTRY_POINT;

    /// @notice Nonce for replay protection
    uint256 public nonce;

    /// @notice Flag to indicate initialization status
    bool private initialized;

    // ============ Events ============

    event MpcAccountInitialized(address indexed agent, address indexed steward, address indexed user);
    event TransactionExecuted(address indexed destination, uint256 value, bytes data);
    event SignerUpdated(uint8 indexed partyIndex, address indexed oldSigner, address indexed newSigner);

    // ============ Errors ============

    error AlreadyInitialized();
    error NotInitialized();
    error InvalidSignerCount();
    error InvalidArrayLength();
    error InvalidSignature();
    error InvalidSignatureLength();
    error InvalidPartyIndices();
    error DuplicateSigners();
    error NotFromEntryPoint();
    error ExecutionFailed(bytes reason);
    error UnauthorizedSignerUpdate();

    // ============ Modifiers ============

    modifier onlyEntryPoint() {
        if (msg.sender != address(ENTRY_POINT)) {
            revert NotFromEntryPoint();
        }
        _;
    }

    modifier requiresInitialized() {
        if (!initialized) revert NotInitialized();
        _;
    }

    // ============ Constructor ============

    /**
     * @notice Constructor - sets immutable EntryPoint and initializes signers
     * @dev Called by factory with all signer addresses
     * @param _entryPoint EntryPoint contract address
     * @param _agent Agent's public key address
     * @param _steward Steward's public key address
     * @param _user User's public key address
     */
    constructor(address _entryPoint, address _agent, address _steward, address _user) {
        // [FIX H-3] Validate EntryPoint is a contract
        require(_entryPoint.code.length > 0, "Invalid EntryPoint");

        ENTRY_POINT = IEntryPoint(_entryPoint);

        // Validate signers
        if (_agent == address(0) || _steward == address(0) || _user == address(0)) {
            revert InvalidSignerCount();
        }

        // Check for duplicate signers
        if (_agent == _steward || _agent == _user || _steward == _user) {
            revert DuplicateSigners();
        }

        signers[0] = _agent;
        signers[1] = _steward;
        signers[2] = _user;
        initialized = true;

        emit MpcAccountInitialized(_agent, _steward, _user);
    }

    // ============ ERC-4337 Entry Point ============

    /**
     * @notice Validate user operation signature (ERC-4337)
     * @dev Signature format: [partyIndices: 1 byte] [sig1: 65 bytes] [sig2: 65 bytes]
     *      partyIndices encodes which 2 of 3 parties signed (bits 0-3 = first party, bits 4-7 = second party)
     * @return validationData 0 = valid, 1 = invalid, SIG_VALIDATION_FAILED = invalid
     */
    function validateUserOp(UserOperation calldata userOp, bytes32, uint256)
        external
        view
        onlyEntryPoint
        returns (uint256 validationData)
    {
        // Verify signature from 2 of 3 MPC parties
        if (!_validateMpcSignature(userOp)) {
            return 1; // SIG_VALIDATION_FAILED
        }
        return 0; // SIG_VALIDATION_SUCCESS
    }

    /**
     * @notice Execute a single transaction
     * @param dest Destination address
     * @param value ETH value to send
     * @param func Function call data
     */
    function execute(address dest, uint256 value, bytes calldata func)
        external
        onlyEntryPoint
        requiresInitialized
        nonReentrant
    {
        // Increment nonce BEFORE external call to prevent replay on revert
        nonce++;

        emit TransactionExecuted(dest, value, func);

        (bool success, bytes memory result) = dest.call{value: value}(func);

        if (!success) {
            revert ExecutionFailed(result);
        }
    }

    /**
     * @notice Execute multiple transactions in batch
     * @param dest Destination addresses
     * @param value ETH values to send
     * @param func Function call data
     */
    function executeBatch(address[] calldata dest, uint256[] calldata value, bytes[] calldata func)
        external
        onlyEntryPoint
        requiresInitialized
        nonReentrant
    {
        uint256 length = dest.length;

        require(length <= MAX_BATCH_SIZE, "Batch too large");

        if (length != value.length || length != func.length) {
            revert InvalidArrayLength();
        }

        // Increment nonce once for the batch
        nonce++;

        for (uint256 i = 0; i < length; i++) {
            emit TransactionExecuted(dest[i], value[i], func[i]);

            (bool success, bytes memory result) = dest[i].call{value: value[i]}(func[i]);

            if (!success) {
                revert ExecutionFailed(result);
            }
        }
    }

    // ============ Signature Validation ============

    /**
     * @notice Validate MPC 2-of-3 signature
     * @dev Signature format:
     *      [0:1] - party indices (packed: lower nibble = first party, upper nibble = second party)
     *      [1:66] - first signature (r, s, v)
     *      [66:131] - second signature (r, s, v)
     */

    /// @notice ECDSA signature s-value upper bound to prevent malleability
    bytes32 private constant S_VALUE_BOUND = 0x7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF5D576E7357A4501DDFE92F46681B20A0;

    /**
     * @notice Validate that s-value is within bounds to prevent signature malleability
     * @param s The s-value from the signature
     */
    function _isValidSValue(bytes32 s) internal pure returns (bool) {
        return uint256(s) <= uint256(S_VALUE_BOUND);
    }

    function _validateMpcSignature(UserOperation calldata userOp) internal view returns (bool) {
        bytes calldata signature = userOp.signature;

        if (signature.length != MULTI_SIG_SIZE) {
            revert InvalidSignatureLength();
        }

        // Extract party indices
        uint8 partyIndices = uint8(signature[0]);
        uint8 party1 = partyIndices & 0x0F;
        uint8 party2 = (partyIndices >> 4) & 0x0F;

        // Validate party indices are distinct and valid
        if (party1 >= N_PARTIES || party2 >= N_PARTIES || party1 == party2) {
            revert InvalidPartyIndices();
        }

        // Get the hash that was signed (EntryPoint-computed)
        bytes32 hash = ENTRY_POINT.getUserOpHash(userOp);

        // Extract and verify first signature
        bytes32 r1 = bytes32(signature[1:33]);
        bytes32 s1 = bytes32(signature[33:65]);
        uint8 v1 = uint8(signature[65]);

        // Check s-value to prevent signature malleability
        if (!_isValidSValue(s1)) {
            return false;
        }

        address signer1 = ecrecover(hash, v1, r1, s1);
        require(signer1 != address(0), "Invalid signature1");
        if (signer1 != signers[party1]) {
            return false;
        }

        // Extract and verify second signature
        bytes32 r2 = bytes32(signature[66:98]);
        bytes32 s2 = bytes32(signature[98:130]);
        uint8 v2 = uint8(signature[130]);

        // Check s-value to prevent signature malleability
        if (!_isValidSValue(s2)) {
            return false;
        }

        address signer2 = ecrecover(hash, v2, r2, s2);
        require(signer2 != address(0), "Invalid signature2");
        if (signer2 != signers[party2]) {
            return false;
        }

        return true;
    }

    // ============ Admin Functions ============

    /**
     * @notice Update a signer (requires MPC signature from 2 parties)
     * @dev Signature must include the current nonce to prevent replay attacks.
     *      Nonce is incremented at the start of the function.
     * @param partyIndex Index of signer to update (0=Agent, 1=Steward, 2=User)
     * @param newSigner New signer address
     * @param mpcSignature MPC 2-of-3 signature authorizing the change
     *        Format: [partyIndices: 1 byte] [sig1: 65 bytes] [sig2: 65 bytes]
     */
    function updateSigner(uint8 partyIndex, address newSigner, bytes calldata mpcSignature)
        external
        requiresInitialized
        nonReentrant
    {
        if (partyIndex >= N_PARTIES) revert InvalidPartyIndices();
        if (newSigner == address(0)) revert InvalidSignerCount();

        // [FIX C-1] Increment nonce first to prevent replay attacks
        // This ensures each signature can only be used once
        nonce++;

        // Check new signer is not duplicate
        for (uint8 i = 0; i < N_PARTIES; i++) {
            if (i != partyIndex && signers[i] == newSigner) {
                revert DuplicateSigners();
            }
        }

        // Construct the message hash that was signed
        // Include chain ID and current nonce to prevent cross-chain and replay attacks
        bytes32 messageHash = keccak256(
            abi.encodePacked(
                "\x19Ethereum Signed Message:\n32",
                keccak256(abi.encodePacked("updateSigner", partyIndex, newSigner, nonce, block.chainid))
            )
        );

        // Verify MPC signature
        if (mpcSignature.length != MULTI_SIG_SIZE) {
            revert InvalidSignatureLength();
        }

        uint8 partyIndices = uint8(mpcSignature[0]);
        uint8 party1 = partyIndices & 0x0F;
        uint8 party2 = (partyIndices >> 4) & 0x0F;

        if (party1 >= N_PARTIES || party2 >= N_PARTIES || party1 == party2) {
            revert InvalidPartyIndices();
        }

        // Verify first signature
        bytes32 r1 = bytes32(mpcSignature[1:33]);
        bytes32 s1 = bytes32(mpcSignature[33:65]);
        uint8 v1 = uint8(mpcSignature[65]);

        // Check s-value to prevent signature malleability
        if (!_isValidSValue(s1)) {
            revert InvalidSignature();
        }

        address signer1 = ecrecover(messageHash, v1, r1, s1);
        require(signer1 != address(0), "Invalid signature1");

        // Verify second signature
        bytes32 r2 = bytes32(mpcSignature[66:98]);
        bytes32 s2 = bytes32(mpcSignature[98:130]);
        uint8 v2 = uint8(mpcSignature[130]);

        // Check s-value to prevent signature malleability
        if (!_isValidSValue(s2)) {
            revert InvalidSignature();
        }

        address signer2 = ecrecover(messageHash, v2, r2, s2);
        require(signer2 != address(0), "Invalid signature2");

        // Both signatures must be from valid signers
        bool sig1Valid = signer1 == signers[party1];
        bool sig2Valid = signer2 == signers[party2];

        if (!sig1Valid || !sig2Valid) {
            revert UnauthorizedSignerUpdate();
        }

        // Perform the update
        address oldSigner = signers[partyIndex];
        signers[partyIndex] = newSigner;

        emit SignerUpdated(partyIndex, oldSigner, newSigner);
    }

    // ============ View Functions ============

    /**
     * @notice Get all signers
     */
    function getSigners() external view returns (address[3] memory) {
        return signers;
    }

    /**
     * @notice Check if account is initialized
     */
    function isInitialized() external view returns (bool) {
        return initialized;
    }

    // ============ Receive ============

    receive() external payable {}
    fallback() external payable {}
}

/**
 * @title MpcSmartAccountFactory
 * @notice Factory for creating MPC Smart Accounts with CREATE2
 */
contract MpcSmartAccountFactory {
    // ============ Events ============

    event AccountCreated(address indexed account, address indexed agent, address indexed steward, address user);

    // ============ State ============

    IEntryPoint public immutable ENTRY_POINT;
    mapping(address => address) public accountOwners;
    address[] public accounts;

    // ============ Constructor ============

    constructor(address _entryPoint) {
        // Validate EntryPoint is a contract
        require(_entryPoint.code.length > 0, "Invalid EntryPoint");

        ENTRY_POINT = IEntryPoint(_entryPoint);
    }

    // ============ Account Creation ============

    /**
     * @notice Create a new MPC Smart Account
     * @param agent Agent's public key address
     * @param steward Steward's public key address
     * @param user User's public key address
     * @param salt CREATE2 salt for deterministic address
     * @return account The deployed account address
     */
    function createAccount(address agent, address steward, address user, uint256 salt)
        external
        returns (address account)
    {
        // Validate no duplicates
        require(agent != steward && agent != user && steward != user, "Duplicate signers");
        require(agent != address(0) && steward != address(0) && user != address(0), "Zero address");

        bytes memory bytecode = _getBytecode(agent, steward, user);

        assembly {
            account := create2(0, add(bytecode, 0x20), mload(bytecode), salt)

            if iszero(extcodesize(account)) {
                revert(0, 0)
            }
        }

        accounts.push(account);
        accountOwners[account] = user; // User is the primary owner

        emit AccountCreated(account, agent, steward, user);
    }

    /**
     * @notice Get the deterministic address for an account
     */
    function getAddress(address agent, address steward, address user, uint256 salt) external view returns (address) {
        require(agent != address(0) && steward != address(0) && user != address(0), "Zero address");
        bytes memory bytecode = _getBytecode(agent, steward, user);
        bytes32 hash = keccak256(abi.encodePacked(bytes1(0xff), address(this), salt, keccak256(bytecode)));
        return address(uint160(uint256(hash)));
    }

    /**
     * @notice Get bytecode for account deployment
     */
    function _getBytecode(address agent, address steward, address user) internal view returns (bytes memory) {
        return
            abi.encodePacked(type(MpcSmartAccount).creationCode, abi.encode(address(ENTRY_POINT), agent, steward, user));
    }

    // ============ View Functions ============

    function getAccounts() external view returns (address[] memory) {
        return accounts;
    }

    function getAccountCount() external view returns (uint256) {
        return accounts.length;
    }
}