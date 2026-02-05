// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity >=0.8.28 <0.9.0;

/// @title CrossChainAccount
/// @notice A passkey-authenticated smart wallet with cross-chain deterministic addresses.
/// @dev Deployed via CREATE2 by CrossChainAccountFactory. No constructor args to ensure
///      identical initCode (and thus identical address) across all chains.
///
/// Key design decisions:
/// 1. No constructor args - keeps creationCode identical across chains
/// 2. One-time initialization guarded by initialized flag
/// 3. Supports P-256 passkey signatures (WebAuthn compatible)
/// 4. AccountKeychain stored in storage (set by factory per chain)
/// 5. Supports adding/removing keys for rotation without changing address
contract CrossChainAccount {

    // ============ Storage ============

    /// @notice The AccountKeychain precompile for this chain
    address public accountKeychain;

    /// @notice Primary owner passkey coordinates
    bytes32 public ownerX;
    bytes32 public ownerY;

    /// @notice Additional authorized keys (for rotation/recovery)
    mapping(bytes32 => mapping(bytes32 => bool)) public authorizedKeys;

    /// @notice Nonce for replay protection
    uint256 public nonce;

    /// @notice Initialization flag
    bool private _initialized;

    // ============ Events ============

    event Initialized(bytes32 indexed ownerX, bytes32 indexed ownerY, address accountKeychain);
    event KeyAdded(bytes32 indexed x, bytes32 indexed y);
    event KeyRemoved(bytes32 indexed x, bytes32 indexed y);
    event Executed(address indexed target, uint256 value, bytes data);

    // ============ Errors ============

    error AlreadyInitialized();
    error NotAuthorized();
    error InvalidSignature();
    error ExecutionFailed();
    error InvalidKey();
    error KeyAlreadyExists();
    error CannotRemovePrimaryKey();

    // ============ Constructor ============

    /// @dev No constructor args to ensure identical creationCode across chains
    constructor() { }

    // ============ Initialization ============

    /// @notice Initialize the account with owner passkey and accountKeychain
    /// @dev Called atomically by factory after CREATE2 deployment
    /// @param _ownerX The x-coordinate of the owner's passkey public key
    /// @param _ownerY The y-coordinate of the owner's passkey public key
    /// @param _accountKeychain The AccountKeychain precompile address for this chain
    function initialize(bytes32 _ownerX, bytes32 _ownerY, address _accountKeychain) external {
        if (_initialized) {
            revert AlreadyInitialized();
        }
        if (_ownerX == bytes32(0) || _ownerY == bytes32(0)) {
            revert InvalidKey();
        }

        _initialized = true;
        ownerX = _ownerX;
        ownerY = _ownerY;
        accountKeychain = _accountKeychain;
        authorizedKeys[_ownerX][_ownerY] = true;

        emit Initialized(_ownerX, _ownerY, _accountKeychain);
    }

    // ============ Execution Functions ============

    /// @notice Execute a call from this account
    /// @param target The target address
    /// @param value The ETH value to send
    /// @param data The calldata
    function execute(
        address target,
        uint256 value,
        bytes calldata data
    )
        external
        onlyAuthorized
        returns (bytes memory)
    {
        (bool success, bytes memory result) = target.call{ value: value }(data);
        if (!success) {
            revert ExecutionFailed();
        }
        emit Executed(target, value, data);
        return result;
    }

    /// @notice Execute a batch of calls
    /// @param targets The target addresses
    /// @param values The ETH values to send
    /// @param datas The calldatas
    function executeBatch(
        address[] calldata targets,
        uint256[] calldata values,
        bytes[] calldata datas
    )
        external
        onlyAuthorized
        returns (bytes[] memory results)
    {
        require(targets.length == values.length && values.length == datas.length, "Length mismatch");
        results = new bytes[](targets.length);
        for (uint256 i = 0; i < targets.length; i++) {
            (bool success, bytes memory result) = targets[i].call{ value: values[i] }(datas[i]);
            if (!success) {
                revert ExecutionFailed();
            }
            results[i] = result;
            emit Executed(targets[i], values[i], datas[i]);
        }
    }

    // ============ Key Management ============

    /// @notice Add an additional authorized key (for recovery/rotation)
    /// @param x The x-coordinate of the new key
    /// @param y The y-coordinate of the new key
    function addKey(bytes32 x, bytes32 y) external onlyAuthorized {
        if (x == bytes32(0) || y == bytes32(0)) {
            revert InvalidKey();
        }
        if (authorizedKeys[x][y]) {
            revert KeyAlreadyExists();
        }
        authorizedKeys[x][y] = true;
        emit KeyAdded(x, y);
    }

    /// @notice Remove an authorized key
    /// @dev Cannot remove the primary owner key
    /// @param x The x-coordinate of the key to remove
    /// @param y The y-coordinate of the key to remove
    function removeKey(bytes32 x, bytes32 y) external onlyAuthorized {
        if (x == ownerX && y == ownerY) {
            revert CannotRemovePrimaryKey();
        }
        authorizedKeys[x][y] = false;
        emit KeyRemoved(x, y);
    }

    // ============ View Functions ============

    /// @notice Check if a key is authorized
    function isAuthorizedKey(bytes32 x, bytes32 y) external view returns (bool) {
        return authorizedKeys[x][y];
    }

    /// @notice Check if the account is initialized
    function initialized() external view returns (bool) {
        return _initialized;
    }

    // ============ Receive Functions ============

    /// @notice Receive ETH
    receive() external payable { }

    /// @notice Fallback for receiving ETH with data
    fallback() external payable { }

    // ============ Modifiers ============

    /// @dev Modifier to restrict to authorized callers
    modifier onlyAuthorized() {
        // Allow calls from accountKeychain (for validated ops)
        // or direct calls that will be validated via signature
        if (msg.sender != accountKeychain && msg.sender != address(this)) {
            revert NotAuthorized();
        }
        _;
    }

}
