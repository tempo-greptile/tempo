# Token Bridge Application Specification

This document specifies the Token Bridge - an application built on top of the base messaging layer.

## Overview

The Token Bridge:
- Uses the base `MessageBridge` for cross-chain message verification
- Implements lock/mint and burn/unlock token transfer logic
- Uses a **nonce** to ensure every transfer is uniquely identifiable
- Uses **canonical asset IDs** to handle token address differences across chains
- **Deployed at the same address on all chains** (via CREATE2) for simple sender verification

## Key Design Decisions

### 1. Same Address on All Chains

The TokenBridge is deployed at the **same address** on all chains using CREATE2. This means:
- No need for a "trusted remote bridge" registry
- Claims just verify the message came from `address(this)` on the origin chain
- Simpler code, fewer admin functions

### 2. Every Transfer is Unique (Nonce)

Each transfer includes a monotonically increasing nonce to prevent:
- Hash collisions (two users bridging same token/amount/recipient)
- Griefing attacks (burning a hash tuple forever with dust)

### 3. Canonical Asset Identity

Tokens are identified by `(homeChainId, homeTokenAddress)`, not local addresses. This ensures:
- The same asset has the same identity across all chains
- Message hashes are consistent regardless of which chain you're on

## Data Structures

### Asset

```solidity
struct Asset {
    uint64 homeChainId;      // Chain where canonical token lives
    address homeToken;       // Token address on home chain
    address localToken;      // Token address on THIS chain (may be wrapped)
    bool active;
}
```

### Message Hash

```solidity
messageHash = keccak256(abi.encode(
    "TOKEN_BRIDGE_V1",       // Domain separator
    originChainId,           // Source chain
    destinationChainId,      // Destination chain
    homeChainId,             // Asset's home chain (canonical identity)
    homeToken,               // Asset's home token address (canonical identity)
    recipient,               // Recipient on destination
    amount,                  // Transfer amount
    nonce                    // Unique per-bridge nonce
))
```

## Interface

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface ITokenBridge {
    //=============================================================
    //                          TYPES
    //=============================================================
    
    struct Asset {
        uint64 homeChainId;
        address homeToken;
        address localToken;
        bool active;
    }
    
    //=============================================================
    //                          ERRORS
    //=============================================================
    
    error Unauthorized();
    error ContractPaused();
    error MessageNotReceived();
    error AlreadyClaimed();
    error InvalidAmount();
    error InvalidRecipient();
    error AssetNotRegistered(bytes32 assetId);
    error AssetNotActive(bytes32 assetId);
    
    //=============================================================
    //                          EVENTS
    //=============================================================
    
    event TokensBridged(
        bytes32 indexed messageHash,
        bytes32 indexed assetId,
        uint256 indexed nonce,
        address sender,
        address recipient,
        uint256 amount,
        uint64 destinationChainId
    );
    
    event TokensClaimed(
        bytes32 indexed messageHash,
        bytes32 indexed assetId,
        address indexed recipient,
        uint256 amount,
        uint64 originChainId
    );
    
    event AssetRegistered(bytes32 indexed assetId, uint64 homeChainId, address homeToken, address localToken);
    
    //=============================================================
    //                      BRIDGE FUNCTIONS
    //=============================================================
    
    /// @notice Bridge tokens to another chain
    function bridgeTokens(
        bytes32 assetId,
        address recipient,
        uint256 amount,
        uint64 destinationChainId
    ) external returns (bytes32 messageHash, uint256 nonce);
    
    /// @notice Claim bridged tokens
    function claimTokens(
        bytes32 assetId,
        address recipient,
        uint256 amount,
        uint256 nonce,
        uint64 originChainId
    ) external;
    
    //=============================================================
    //                      VIEW FUNCTIONS
    //=============================================================
    
    function computeMessageHash(
        uint64 originChainId,
        uint64 destinationChainId,
        uint64 homeChainId,
        address homeToken,
        address recipient,
        uint256 amount,
        uint256 nonce
    ) external pure returns (bytes32);
    
    function getAsset(bytes32 assetId) external view returns (Asset memory);
    function isClaimed(uint64 originChainId, bytes32 messageHash) external view returns (bool);
    function nonce() external view returns (uint256);
    
    //=============================================================
    //                      ADMIN FUNCTIONS
    //=============================================================
    
    function registerAsset(bytes32 assetId, uint64 homeChainId, address homeToken, address localToken) external;
    function setAssetActive(bytes32 assetId, bool active) external;
    function pause() external;
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
/// @notice Token bridge deployed at same address on all chains via CREATE2
contract TokenBridge is ITokenBridge {
    using SafeERC20 for IERC20;
    
    //=============================================================
    //                          STORAGE
    //=============================================================
    
    address public owner;
    bool public paused;
    
    IMessageBridge public immutable messageBridge;
    uint64 public immutable chainId;
    
    mapping(bytes32 => Asset) public assets;
    mapping(uint64 => mapping(bytes32 => bool)) public claimed; // originChainId => hash => claimed
    uint256 public nonce;
    
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
    
    constructor(address _owner, address _messageBridge) {
        owner = _owner;
        messageBridge = IMessageBridge(_messageBridge);
        chainId = uint64(block.chainid);
    }
    
    //=============================================================
    //                      BRIDGE FUNCTIONS
    //=============================================================
    
    function bridgeTokens(
        bytes32 assetId,
        address recipient,
        uint256 amount,
        uint64 destinationChainId
    ) external whenNotPaused returns (bytes32 messageHash, uint256 transferNonce) {
        if (amount == 0) revert InvalidAmount();
        if (recipient == address(0)) revert InvalidRecipient();
        
        Asset memory asset = assets[assetId];
        if (asset.localToken == address(0)) revert AssetNotRegistered(assetId);
        if (!asset.active) revert AssetNotActive(assetId);
        
        bool isHomeChain = asset.homeChainId == chainId;
        
        if (isHomeChain) {
            // Home chain: lock tokens
            uint256 balanceBefore = IERC20(asset.localToken).balanceOf(address(this));
            IERC20(asset.localToken).safeTransferFrom(msg.sender, address(this), amount);
            amount = IERC20(asset.localToken).balanceOf(address(this)) - balanceBefore;
        } else {
            // Remote chain: burn wrapped tokens
            IBurnable(asset.localToken).burnFrom(msg.sender, amount);
        }
        
        transferNonce = nonce++;
        
        messageHash = _computeMessageHash(
            chainId,
            destinationChainId,
            asset.homeChainId,
            asset.homeToken,
            recipient,
            amount,
            transferNonce
        );
        
        messageBridge.send(messageHash, destinationChainId);
        
        emit TokensBridged(messageHash, assetId, transferNonce, msg.sender, recipient, amount, destinationChainId);
    }
    
    function claimTokens(
        bytes32 assetId,
        address recipient,
        uint256 amount,
        uint256 transferNonce,
        uint64 originChainId
    ) external whenNotPaused {
        Asset memory asset = assets[assetId];
        if (asset.localToken == address(0)) revert AssetNotRegistered(assetId);
        
        bytes32 messageHash = _computeMessageHash(
            originChainId,
            chainId,
            asset.homeChainId,
            asset.homeToken,
            recipient,
            amount,
            transferNonce
        );
        
        // Verify message received from TokenBridge on origin chain
        // Since we're deployed at the same address on all chains, sender = address(this)
        if (messageBridge.receivedAt(originChainId, address(this), messageHash) == 0) {
            revert MessageNotReceived();
        }
        
        if (claimed[originChainId][messageHash]) revert AlreadyClaimed();
        claimed[originChainId][messageHash] = true;
        
        bool isHomeChain = asset.homeChainId == chainId;
        
        if (isHomeChain) {
            // Home chain: unlock from escrow
            IERC20(asset.localToken).safeTransfer(recipient, amount);
        } else {
            // Remote chain: mint wrapped tokens
            IMintable(asset.localToken).mint(recipient, amount);
        }
        
        emit TokensClaimed(messageHash, assetId, recipient, amount, originChainId);
    }
    
    //=============================================================
    //                      VIEW FUNCTIONS
    //=============================================================
    
    function computeMessageHash(
        uint64 originChainId,
        uint64 destinationChainId,
        uint64 homeChainId,
        address homeToken,
        address recipient,
        uint256 amount,
        uint256 transferNonce
    ) external pure returns (bytes32) {
        return _computeMessageHash(originChainId, destinationChainId, homeChainId, homeToken, recipient, amount, transferNonce);
    }
    
    function getAsset(bytes32 assetId) external view returns (Asset memory) {
        return assets[assetId];
    }
    
    function isClaimed(uint64 originChainId, bytes32 messageHash) external view returns (bool) {
        return claimed[originChainId][messageHash];
    }
    
    //=============================================================
    //                      ADMIN FUNCTIONS
    //=============================================================
    
    function registerAsset(
        bytes32 assetId,
        uint64 homeChainId,
        address homeToken,
        address localToken
    ) external onlyOwner {
        assets[assetId] = Asset({
            homeChainId: homeChainId,
            homeToken: homeToken,
            localToken: localToken,
            active: true
        });
        emit AssetRegistered(assetId, homeChainId, homeToken, localToken);
    }
    
    function setAssetActive(bytes32 assetId, bool active) external onlyOwner {
        assets[assetId].active = active;
    }
    
    function pause() external onlyOwner { paused = true; }
    function unpause() external onlyOwner { paused = false; }
    
    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0));
        owner = newOwner;
    }
    
    //=============================================================
    //                      INTERNAL
    //=============================================================
    
    function _computeMessageHash(
        uint64 originChainId,
        uint64 destinationChainId,
        uint64 homeChainId,
        address homeToken,
        address recipient,
        uint256 amount,
        uint256 transferNonce
    ) internal pure returns (bytes32) {
        return keccak256(abi.encode(
            "TOKEN_BRIDGE_V1",
            originChainId,
            destinationChainId,
            homeChainId,
            homeToken,
            recipient,
            amount,
            transferNonce
        ));
    }
}

interface IMintable {
    function mint(address to, uint256 amount) external;
}

interface IBurnable {
    function burnFrom(address from, uint256 amount) external;
}
```

## Transfer Flow

### Ethereum → Tempo

```
ETHEREUM                                       TEMPO
────────                                       ─────

1. User: bridgeTokens(WETH_ID, recipient, 1e18, TEMPO_CHAIN_ID)
   ├─► WETH locked in TokenBridge
   ├─► nonce = 0, hash = keccak256(...)
   ├─► messageBridge.send(hash, TEMPO_CHAIN_ID)
   └─► MessageSent(TokenBridge, hash, TEMPO_CHAIN_ID)

2. Validators sign & relay
   └─► messageBridge.write(TokenBridge, hash, ETH_CHAIN_ID, sig)
       └─► received[1][TokenBridge][hash] = timestamp

3. Anyone: claimTokens(WETH_ID, recipient, 1e18, 0, ETH_CHAIN_ID)
   ├─► hash = keccak256(...)
   ├─► messageBridge.receivedAt(1, address(this), hash) > 0 ✓
   │   └─► Works because TokenBridge has same address on both chains!
   ├─► claimed[1][hash] = true
   └─► Mint wrapped WETH to recipient
```

## Sender Verification

The key insight is that `address(this)` is the same on all chains:

```solidity
// On Tempo, checking a message from Ethereum:
messageBridge.receivedAt(
    ETH_CHAIN_ID,      // originChainId
    address(this),     // sender = TokenBridge address (same on all chains!)
    messageHash
)
```

This works because:
1. On Ethereum, TokenBridge at `0xABCD` sends the message
2. Base layer records: `received[1][0xABCD][hash] = timestamp`
3. On Tempo, TokenBridge is also at `0xABCD`
4. Checking `receivedAt(1, address(this), hash)` = `receivedAt(1, 0xABCD, hash)` ✓

## Deployment

Use CREATE2 with identical init code and salt on all chains:

```solidity
// Deployer contract (same on all chains)
bytes32 salt = keccak256("TokenBridge_v1");
bytes memory initCode = abi.encodePacked(
    type(TokenBridge).creationCode,
    abi.encode(owner, messageBridgeAddress)
);

address bridge = CREATE2(salt, initCode);
// bridge will be the same address on all chains
```

**Important**: `messageBridgeAddress` must also be the same on all chains for this to work.

## Gas Estimates

| Function | Gas |
|----------|-----|
| `bridgeTokens` (lock) | ~85,000 |
| `bridgeTokens` (burn) | ~65,000 |
| `claimTokens` (mint) | ~75,000 |
| `claimTokens` (unlock) | ~55,000 |

## Invariants

1. **Unique Transfers**: Every `(originChainId, nonce)` maps to exactly one transfer
2. **Single Claim**: Each `(originChainId, messageHash)` can only be claimed once
3. **Same Address**: TokenBridge has identical address on all chains
4. **Token Conservation**: Locked on home = minted on remote

## File Locations

| Component | Path |
|-----------|------|
| TokenBridge | `contracts/bridge/src/apps/TokenBridge.sol` |
| Interface | `contracts/bridge/src/interfaces/ITokenBridge.sol` |
| WrappedToken | `contracts/bridge/src/tokens/WrappedToken.sol` |
