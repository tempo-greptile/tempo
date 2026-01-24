# Token Bridge Application Specification

This document specifies the Token Bridge - an application built on top of the base messaging layer that enables cross-chain token transfers.

## Overview

The Token Bridge is an **application layer** contract that:
- Uses the base `MessageBridge` for cross-chain message passing
- Implements lock/mint and burn/unlock token transfer logic
- Encodes token transfer data into 32-byte message hashes
- Manages token mappings between chains

The Token Bridge demonstrates the separation of concerns: **it has no knowledge of BLS signatures, validators, or attestations**. It simply sends message hashes through the base layer and reacts to received messages.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                           Token Bridge Architecture                              │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│                        ┌───────────────────────────┐                            │
│                        │       TokenBridge         │                            │
│                        │                           │                            │
│                        │  • bridgeTokens()         │                            │
│                        │  • claimTokens()          │                            │
│                        │  • registerAsset()        │                            │
│                        └─────────────┬─────────────┘                            │
│                                      │                                          │
│                                      │ sendMessage() / getMessage()             │
│                                      ▼                                          │
│                        ┌───────────────────────────┐                            │
│                        │      MessageBridge        │                            │
│                        │    (Base Layer)           │                            │
│                        │                           │                            │
│                        │  • 32-byte hash passing   │                            │
│                        │  • BLS verification       │                            │
│                        │  • Replay protection      │                            │
│                        └───────────────────────────┘                            │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Data Structures

### Asset Configuration

```solidity
struct AssetConfig {
    uint64 homeChainId;       // Chain where canonical token lives
    address homeToken;        // Token address on home chain
    address remoteToken;      // Wrapped token on remote chain
    bool active;              // Whether bridging is enabled
}
```

### Transfer Payload

The Token Bridge encodes transfer data into a payload, then hashes it to 32 bytes:

```solidity
struct TransferPayload {
    bytes32 assetId;          // Asset identifier (e.g., keccak256("WETH"))
    address recipient;        // Recipient on destination chain
    uint256 amount;           // Amount being transferred
    uint64 srcNonce;          // Nonce from source TokenBridge
}
```

### Message Hash Computation

```solidity
messageHash = keccak256(abi.encodePacked(
    "TOKEN_BRIDGE_V1",        // Application domain
    assetId,                  // bytes32
    recipient,                // address
    amount,                   // uint256
    srcNonce                  // uint64
))
```

## Interface Definition

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IMessageBridge} from "./IMessageBridge.sol";

/// @title ITokenBridge
/// @notice Token bridge application built on base messaging layer
interface ITokenBridge {
    //=============================================================
    //                          TYPES
    //=============================================================
    
    struct AssetConfig {
        uint64 homeChainId;
        address homeToken;
        address remoteToken;
        bool active;
    }
    
    //=============================================================
    //                          ERRORS
    //=============================================================
    
    error Unauthorized();
    error ContractPaused();
    error AssetNotRegistered(bytes32 assetId);
    error AssetNotActive(bytes32 assetId);
    error InvalidRecipient();
    error ZeroAmount();
    error MessageNotReceived(bytes32 messageHash);
    error TransferAlreadyClaimed(bytes32 messageHash);
    error InvalidTransferData();
    
    //=============================================================
    //                          EVENTS
    //=============================================================
    
    /// @notice Emitted when tokens are locked/burned for bridging
    event TokensBridged(
        bytes32 indexed assetId,
        bytes32 indexed messageHash,
        address indexed sender,
        address recipient,
        uint256 amount,
        uint64 nonce
    );
    
    /// @notice Emitted when tokens are minted/unlocked from bridge
    event TokensClaimed(
        bytes32 indexed assetId,
        bytes32 indexed messageHash,
        address indexed recipient,
        uint256 amount
    );
    
    /// @notice Emitted when an asset is registered
    event AssetRegistered(
        bytes32 indexed assetId,
        uint64 homeChainId,
        address homeToken,
        address remoteToken
    );
    
    //=============================================================
    //                      BRIDGE FUNCTIONS
    //=============================================================
    
    /// @notice Bridge tokens to the remote chain
    /// @param assetId The asset identifier
    /// @param recipient The recipient address on the remote chain
    /// @param amount The amount to bridge
    /// @return messageHash The message hash sent to base layer
    function bridgeTokens(
        bytes32 assetId,
        address recipient,
        uint256 amount
    ) external returns (bytes32 messageHash);
    
    /// @notice Claim tokens that were bridged from the remote chain
    /// @param assetId The asset identifier
    /// @param recipient The recipient address (must match what was bridged)
    /// @param amount The amount to claim
    /// @param srcNonce The nonce from the source chain
    function claimTokens(
        bytes32 assetId,
        address recipient,
        uint256 amount,
        uint64 srcNonce
    ) external;
    
    //=============================================================
    //                      VIEW FUNCTIONS
    //=============================================================
    
    /// @notice Get asset configuration
    function getAsset(bytes32 assetId) external view returns (AssetConfig memory);
    
    /// @notice Check if a transfer has been claimed
    function isClaimed(bytes32 messageHash) external view returns (bool);
    
    /// @notice Compute the message hash for a transfer
    function computeMessageHash(
        bytes32 assetId,
        address recipient,
        uint256 amount,
        uint64 srcNonce
    ) external pure returns (bytes32);
    
    /// @notice Get the base message bridge
    function messageBridge() external view returns (IMessageBridge);
    
    /// @notice Get current nonce
    function nonce() external view returns (uint64);
    
    //=============================================================
    //                      ADMIN FUNCTIONS
    //=============================================================
    
    /// @notice Register an asset for bridging
    function registerAsset(
        bytes32 assetId,
        uint64 homeChainId,
        address homeToken,
        address remoteToken
    ) external;
    
    /// @notice Set asset active status
    function setAssetActive(bytes32 assetId, bool active) external;
    
    /// @notice Pause the contract
    function pause() external;
    
    /// @notice Unpause the contract
    function unpause() external;
}
```

## Implementation

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ITokenBridge} from "./interfaces/ITokenBridge.sol";
import {IMessageBridge} from "./interfaces/IMessageBridge.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

/// @title TokenBridge
/// @notice Token bridge application using base messaging layer
contract TokenBridge is ITokenBridge {
    using SafeERC20 for IERC20;
    
    //=============================================================
    //                          CONSTANTS
    //=============================================================
    
    bytes public constant DOMAIN = "TOKEN_BRIDGE_V1";
    
    //=============================================================
    //                          STORAGE
    //=============================================================
    
    address public owner;
    bool public paused;
    
    /// @notice The base message bridge
    IMessageBridge public immutable messageBridge;
    
    /// @notice Remote token bridge address
    address public remoteTokenBridge;
    
    /// @notice Remote chain ID
    uint64 public remoteChainId;
    
    /// @notice Asset configurations
    mapping(bytes32 => AssetConfig) public assets;
    
    /// @notice Claimed transfers
    mapping(bytes32 => bool) public claimed;
    
    /// @notice Outbound nonce
    uint64 public nonce;
    
    //=============================================================
    //                        MODIFIERS
    //=============================================================
    
    modifier onlyOwner() {
        if (msg.sender != owner) revert Unauthorized();
        _;
    }
    
    modifier whenNotPaused() {
        if (paused) revert ContractPaused();
        _;
    }
    
    //=============================================================
    //                       CONSTRUCTOR
    //=============================================================
    
    constructor(
        address _owner,
        address _messageBridge,
        address _remoteTokenBridge,
        uint64 _remoteChainId
    ) {
        owner = _owner;
        messageBridge = IMessageBridge(_messageBridge);
        remoteTokenBridge = _remoteTokenBridge;
        remoteChainId = _remoteChainId;
    }
    
    //=============================================================
    //                      BRIDGE FUNCTIONS
    //=============================================================
    
    /// @inheritdoc ITokenBridge
    function bridgeTokens(
        bytes32 assetId,
        address recipient,
        uint256 amount
    ) external whenNotPaused returns (bytes32 messageHash) {
        if (recipient == address(0)) revert InvalidRecipient();
        if (amount == 0) revert ZeroAmount();
        
        AssetConfig memory asset = assets[assetId];
        if (asset.homeToken == address(0)) revert AssetNotRegistered(assetId);
        if (!asset.active) revert AssetNotActive(assetId);
        
        // Determine if we lock or burn based on home chain
        address localToken = _getLocalToken(asset);
        bool isHomeChain = asset.homeChainId == uint64(block.chainid);
        
        if (isHomeChain) {
            // Home chain: lock tokens in this contract
            IERC20(localToken).safeTransferFrom(msg.sender, address(this), amount);
        } else {
            // Remote chain: burn wrapped tokens
            IBurnable(localToken).burnFrom(msg.sender, amount);
        }
        
        // Compute message hash
        uint64 currentNonce = nonce++;
        messageHash = _computeMessageHash(assetId, recipient, amount, currentNonce);
        
        // Send through base messaging layer
        messageBridge.sendMessage(messageHash, remoteTokenBridge);
        
        emit TokensBridged(
            assetId,
            messageHash,
            msg.sender,
            recipient,
            amount,
            currentNonce
        );
    }
    
    /// @inheritdoc ITokenBridge
    function claimTokens(
        bytes32 assetId,
        address recipient,
        uint256 amount,
        uint64 srcNonce
    ) external whenNotPaused {
        // Compute expected message hash
        bytes32 messageHash = _computeMessageHash(assetId, recipient, amount, srcNonce);
        
        // Verify message was received by base layer
        if (!messageBridge.hasMessage(messageHash)) {
            revert MessageNotReceived(messageHash);
        }
        
        // Verify not already claimed
        if (claimed[messageHash]) {
            revert TransferAlreadyClaimed(messageHash);
        }
        
        // Verify message came from remote token bridge
        IMessageBridge.Message memory msg_ = messageBridge.getMessage(messageHash);
        if (msg_.sender != remoteTokenBridge) {
            revert InvalidTransferData();
        }
        
        // Mark as claimed
        claimed[messageHash] = true;
        
        // Get asset config
        AssetConfig memory asset = assets[assetId];
        if (asset.homeToken == address(0)) revert AssetNotRegistered(assetId);
        
        // Determine if we unlock or mint based on home chain
        address localToken = _getLocalToken(asset);
        bool isHomeChain = asset.homeChainId == uint64(block.chainid);
        
        if (isHomeChain) {
            // Home chain: unlock from escrow
            IERC20(localToken).safeTransfer(recipient, amount);
        } else {
            // Remote chain: mint wrapped tokens
            IMintable(localToken).mint(recipient, amount);
        }
        
        emit TokensClaimed(assetId, messageHash, recipient, amount);
    }
    
    //=============================================================
    //                      VIEW FUNCTIONS
    //=============================================================
    
    /// @inheritdoc ITokenBridge
    function getAsset(bytes32 assetId) external view returns (AssetConfig memory) {
        return assets[assetId];
    }
    
    /// @inheritdoc ITokenBridge
    function isClaimed(bytes32 messageHash) external view returns (bool) {
        return claimed[messageHash];
    }
    
    /// @inheritdoc ITokenBridge
    function computeMessageHash(
        bytes32 assetId,
        address recipient,
        uint256 amount,
        uint64 srcNonce
    ) external pure returns (bytes32) {
        return _computeMessageHash(assetId, recipient, amount, srcNonce);
    }
    
    //=============================================================
    //                      ADMIN FUNCTIONS
    //=============================================================
    
    /// @inheritdoc ITokenBridge
    function registerAsset(
        bytes32 assetId,
        uint64 homeChainId,
        address homeToken,
        address remoteToken
    ) external onlyOwner {
        assets[assetId] = AssetConfig({
            homeChainId: homeChainId,
            homeToken: homeToken,
            remoteToken: remoteToken,
            active: true
        });
        
        emit AssetRegistered(assetId, homeChainId, homeToken, remoteToken);
    }
    
    /// @inheritdoc ITokenBridge
    function setAssetActive(bytes32 assetId, bool active) external onlyOwner {
        assets[assetId].active = active;
    }
    
    /// @inheritdoc ITokenBridge
    function pause() external onlyOwner {
        paused = true;
    }
    
    /// @inheritdoc ITokenBridge
    function unpause() external onlyOwner {
        paused = false;
    }
    
    //=============================================================
    //                      INTERNAL FUNCTIONS
    //=============================================================
    
    function _computeMessageHash(
        bytes32 assetId,
        address recipient,
        uint256 amount,
        uint64 srcNonce
    ) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(
            "TOKEN_BRIDGE_V1",
            assetId,
            recipient,
            amount,
            srcNonce
        ));
    }
    
    function _getLocalToken(AssetConfig memory asset) internal view returns (address) {
        if (asset.homeChainId == uint64(block.chainid)) {
            return asset.homeToken;
        }
        return asset.remoteToken;
    }
}

interface IMintable {
    function mint(address to, uint256 amount) external;
}

interface IBurnable {
    function burnFrom(address from, uint256 amount) external;
}
```

## Token Transfer Flow

### Ethereum → Tempo (ETH-native token)

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                        ETH → Tempo Transfer Flow                              │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  ETHEREUM                                        TEMPO                        │
│                                                                               │
│  1. User calls tokenBridge.bridgeTokens()                                    │
│     └─► WETH locked in TokenBridge escrow                                    │
│     └─► messageHash = hash(assetId, recipient, amount, nonce)                │
│     └─► messageBridge.sendMessage(messageHash, remoteTokenBridge)            │
│     └─► Emits MessageSent + TokensBridged                                    │
│                                                                               │
│  2. Validators observe MessageSent event                                     │
│     └─► Wait for finality (~15 min)                                          │
│     └─► Sign attestation with BLS key shares                                 │
│     └─► Aggregate threshold signature                                        │
│                                                                               │
│  3. Aggregator calls messageBridge.receiveMessage()     ──────────────────►  │
│     └─► BLS signature verified                                               │
│     └─► Message stored in MessageBridge                                      │
│     └─► Emits MessageReceived                                                │
│                                                                               │
│  4. Anyone calls tokenBridge.claimTokens()              on Tempo             │
│     └─► Verifies message exists in MessageBridge                             │
│     └─► Verifies sender was Ethereum TokenBridge                             │
│     └─► Mints wrapped WETH to recipient                                      │
│     └─► Emits TokensClaimed                                                  │
│                                                                               │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Tempo → Ethereum (Tempo-native token)

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                        Tempo → ETH Transfer Flow                              │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  TEMPO                                           ETHEREUM                     │
│                                                                               │
│  1. User calls tokenBridge.bridgeTokens()                                    │
│     └─► PATH locked in TokenBridge escrow (Tempo is home)                    │
│     └─► messageHash = hash(assetId, recipient, amount, nonce)                │
│     └─► messageBridge.sendMessage(messageHash, remoteTokenBridge)            │
│     └─► Emits MessageSent + TokensBridged                                    │
│                                                                               │
│  2. Validators observe MessageSent event                                     │
│     └─► Tempo has instant finality                                           │
│     └─► Sign attestation with BLS key shares                                 │
│     └─► Aggregate threshold signature                                        │
│                                                                               │
│  3. Aggregator calls messageBridge.receiveMessage()     ──────────────────►  │
│     └─► BLS signature verified                                               │
│     └─► Message stored in MessageBridge                                      │
│     └─► Emits MessageReceived                                                │
│                                                                               │
│  4. Anyone calls tokenBridge.claimTokens()              on Ethereum          │
│     └─► Verifies message exists in MessageBridge                             │
│     └─► Verifies sender was Tempo TokenBridge                                │
│     └─► Mints wrapped PATH to recipient                                      │
│     └─► Emits TokensClaimed                                                  │
│                                                                               │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Asset Registration

### Example: Register WETH (Home: Ethereum)

**On Ethereum:**
```solidity
tokenBridge.registerAsset(
    keccak256("WETH"),                              // assetId
    1,                                              // homeChainId (Ethereum)
    0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2,    // homeToken (WETH)
    address(0)                                      // remoteToken (set later)
);
```

**On Tempo:**
```solidity
// First create wrapped token
address wrappedWETH = TIP20Factory.createToken("Wrapped Ether", "wWETH", 18, 0, tokenBridge);

// Register with bridge
tokenBridge.registerAsset(
    keccak256("WETH"),                              // assetId
    1,                                              // homeChainId (Ethereum)
    0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2,    // homeToken (WETH)
    wrappedWETH                                     // remoteToken
);
```

### Example: Register PATH (Home: Tempo)

**On Tempo:**
```solidity
tokenBridge.registerAsset(
    keccak256("PATH"),
    TEMPO_CHAIN_ID,                                 // homeChainId (Tempo)
    PATH_ADDRESS,                                   // homeToken
    address(0)                                      // remoteToken (set later)
);
```

**On Ethereum:**
```solidity
// Deploy wrapped token
WrappedToken wrappedPATH = new WrappedToken("Wrapped PATH", "wPATH", 18, tokenBridge);

// Register with bridge
tokenBridge.registerAsset(
    keccak256("PATH"),
    TEMPO_CHAIN_ID,                                 // homeChainId (Tempo)
    PATH_ADDRESS,                                   // homeToken
    address(wrappedPATH)                           // remoteToken
);
```

## Wrapped Token Template

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC20Burnable} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Burnable.sol";

/// @title WrappedToken
/// @notice Wrapped token for cross-chain bridging
contract WrappedToken is ERC20, ERC20Burnable {
    address public immutable bridge;
    uint8 private immutable _decimals;
    
    modifier onlyBridge() {
        require(msg.sender == bridge, "Only bridge");
        _;
    }
    
    constructor(
        string memory name,
        string memory symbol,
        uint8 decimals_,
        address bridge_
    ) ERC20(name, symbol) {
        _decimals = decimals_;
        bridge = bridge_;
    }
    
    function decimals() public view override returns (uint8) {
        return _decimals;
    }
    
    function mint(address to, uint256 amount) external onlyBridge {
        _mint(to, amount);
    }
}
```

## Gas Estimates

| Function | Estimated Gas |
|----------|---------------|
| `bridgeTokens` (lock) | ~80,000 |
| `bridgeTokens` (burn) | ~60,000 |
| `claimTokens` (mint) | ~70,000 |
| `claimTokens` (unlock) | ~50,000 |
| `registerAsset` | ~45,000 |

## Invariants

1. **Token Conservation**: Locked amount on home chain == minted amount on remote chain
2. **Single Claim**: Each `messageHash` can only be claimed once
3. **Sender Verification**: Claims only valid if original sender was remote TokenBridge
4. **Message Dependency**: Claims require message to exist in base MessageBridge
5. **Nonce Uniqueness**: Each outbound transfer has unique nonce

## Composability

The Token Bridge demonstrates that **any application** can be built on the base messaging layer:

| Application | Message Hash Contains |
|-------------|----------------------|
| Token Bridge | assetId, recipient, amount, nonce |
| NFT Bridge | collectionId, tokenId, recipient, nonce |
| Governance | proposalId, voteData, voter, nonce |
| Oracle | queryId, answer, timestamp |

Each application:
1. Encodes its data into bytes
2. Hashes to 32 bytes
3. Sends through `messageBridge.sendMessage()`
4. Decodes and executes on destination after receiving

## File Locations

| Component | Path |
|-----------|------|
| Token Bridge | `contracts/bridge/src/apps/TokenBridge.sol` |
| Interface | `contracts/bridge/src/interfaces/ITokenBridge.sol` |
| Wrapped Token | `contracts/bridge/src/tokens/WrappedToken.sol` |
| Deploy Script | `contracts/bridge/script/DeployTokenBridge.s.sol` |
