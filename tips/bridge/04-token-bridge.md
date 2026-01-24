# Token Bridge Application Specification

This document specifies the Token Bridge - an application built on top of the base messaging layer.

## Overview

The Token Bridge:
- Uses the base `MessageBridge` for cross-chain message verification
- Implements lock/mint and burn/unlock token transfer logic
- Encodes transfer data into message hashes
- Checks `receivedAt > 0` to verify messages before releasing tokens

The Token Bridge has **no knowledge of BLS signatures or validators** - it simply checks if the base layer has received a message.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                           Token Bridge Architecture                              │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│   USER                                                                           │
│     │                                                                            │
│     │ bridgeTokens(token, amount, recipient)                                    │
│     ▼                                                                            │
│   ┌───────────────────────────────────────────────────────────────┐             │
│   │                      TokenBridge                              │             │
│   │                                                               │             │
│   │  1. Lock/burn tokens                                          │             │
│   │  2. Compute: hash = keccak256(token, amount, recipient)       │             │
│   │  3. Call: messageBridge.send(hash, destChainId)               │             │
│   └───────────────────────────────────────────────────────────────┘             │
│                              │                                                   │
│                              ▼                                                   │
│   ┌───────────────────────────────────────────────────────────────┐             │
│   │                      MessageBridge                            │             │
│   │               (Base Messaging Layer)                          │             │
│   └───────────────────────────────────────────────────────────────┘             │
│                                                                                  │
│   ═══════════════════════════════════════════════════════════════════           │
│                         Validators sign & relay                                  │
│   ═══════════════════════════════════════════════════════════════════           │
│                                                                                  │
│   ┌───────────────────────────────────────────────────────────────┐             │
│   │                      MessageBridge                            │             │
│   │           received[origin][sender][hash] = timestamp          │             │
│   └───────────────────────────────────────────────────────────────┘             │
│                              │                                                   │
│                              ▼                                                   │
│   ┌───────────────────────────────────────────────────────────────┐             │
│   │                      TokenBridge                              │             │
│   │                                                               │             │
│   │  claimTokens(token, amount, recipient, originalSender):       │             │
│   │  1. Compute: hash = keccak256(token, amount, recipient)       │             │
│   │  2. Check: messageBridge.receivedAt(origin, sender, hash) > 0 │             │
│   │  3. Mint/unlock tokens to recipient                           │             │
│   └───────────────────────────────────────────────────────────────┘             │
│                              │                                                   │
│     ▲                        ▼                                                   │
│   USER                  TOKENS RECEIVED                                          │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Interface

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface ITokenBridge {
    //=============================================================
    //                          ERRORS
    //=============================================================
    
    error MessageNotReceived();
    error AlreadyClaimed();
    error InvalidAmount();
    error AssetNotRegistered();
    
    //=============================================================
    //                          EVENTS
    //=============================================================
    
    event TokensBridged(
        bytes32 indexed messageHash,
        address indexed token,
        address indexed sender,
        address recipient,
        uint256 amount,
        uint64 destinationChainId
    );
    
    event TokensClaimed(
        bytes32 indexed messageHash,
        address indexed token,
        address indexed recipient,
        uint256 amount
    );
    
    //=============================================================
    //                      BRIDGE FUNCTIONS
    //=============================================================
    
    /// @notice Bridge tokens to another chain
    /// @param token The token to bridge
    /// @param amount The amount to bridge
    /// @param recipient The recipient on the destination chain
    /// @param destinationChainId The destination chain
    function bridgeTokens(
        address token,
        uint256 amount,
        address recipient,
        uint64 destinationChainId
    ) external;
    
    /// @notice Claim bridged tokens
    /// @param token The token to claim
    /// @param amount The amount to claim
    /// @param recipient The recipient (must match what was bridged)
    /// @param originalSender The sender on the origin chain
    /// @param originChainId The origin chain
    function claimTokens(
        address token,
        uint256 amount,
        address recipient,
        address originalSender,
        uint64 originChainId
    ) external;
    
    //=============================================================
    //                      VIEW FUNCTIONS
    //=============================================================
    
    /// @notice Compute the message hash for a transfer
    function computeMessageHash(
        address token,
        uint256 amount,
        address recipient
    ) external pure returns (bytes32);
    
    /// @notice Check if a transfer has been claimed
    function isClaimed(bytes32 messageHash) external view returns (bool);
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

contract TokenBridge is ITokenBridge {
    using SafeERC20 for IERC20;
    
    //=============================================================
    //                          STORAGE
    //=============================================================
    
    /// @notice The base message bridge
    IMessageBridge public immutable messageBridge;
    
    /// @notice This chain's ID
    uint64 public immutable chainId;
    
    /// @notice Token mappings: originToken => localToken (for wrapped tokens)
    mapping(address => address) public tokenMappings;
    
    /// @notice Home chain for each token
    mapping(address => uint64) public homeChain;
    
    /// @notice Claimed message hashes
    mapping(bytes32 => bool) public claimed;
    
    //=============================================================
    //                       CONSTRUCTOR
    //=============================================================
    
    constructor(address _messageBridge) {
        messageBridge = IMessageBridge(_messageBridge);
        chainId = uint64(block.chainid);
    }
    
    //=============================================================
    //                      BRIDGE FUNCTIONS
    //=============================================================
    
    /// @inheritdoc ITokenBridge
    function bridgeTokens(
        address token,
        uint256 amount,
        address recipient,
        uint64 destinationChainId
    ) external {
        if (amount == 0) revert InvalidAmount();
        
        bool isHomeChain = homeChain[token] == chainId;
        
        if (isHomeChain) {
            // Home chain: lock tokens
            IERC20(token).safeTransferFrom(msg.sender, address(this), amount);
        } else {
            // Remote chain: burn wrapped tokens
            IBurnable(token).burnFrom(msg.sender, amount);
        }
        
        // Compute message hash
        bytes32 messageHash = computeMessageHash(token, amount, recipient);
        
        // Send through base layer
        messageBridge.send(messageHash, destinationChainId);
        
        emit TokensBridged(
            messageHash,
            token,
            msg.sender,
            recipient,
            amount,
            destinationChainId
        );
    }
    
    /// @inheritdoc ITokenBridge
    function claimTokens(
        address token,
        uint256 amount,
        address recipient,
        address originalSender,
        uint64 originChainId
    ) external {
        // Compute expected message hash
        bytes32 messageHash = computeMessageHash(token, amount, recipient);
        
        // Check message was received by base layer
        uint256 receivedAt = messageBridge.receivedAt(originChainId, originalSender, messageHash);
        if (receivedAt == 0) revert MessageNotReceived();
        
        // Check not already claimed
        if (claimed[messageHash]) revert AlreadyClaimed();
        claimed[messageHash] = true;
        
        // Get local token (may be wrapped version)
        address localToken = tokenMappings[token];
        if (localToken == address(0)) localToken = token;
        
        bool isHomeChain = homeChain[localToken] == chainId;
        
        if (isHomeChain) {
            // Home chain: unlock tokens
            IERC20(localToken).safeTransfer(recipient, amount);
        } else {
            // Remote chain: mint wrapped tokens
            IMintable(localToken).mint(recipient, amount);
        }
        
        emit TokensClaimed(messageHash, localToken, recipient, amount);
    }
    
    //=============================================================
    //                      VIEW FUNCTIONS
    //=============================================================
    
    /// @inheritdoc ITokenBridge
    function computeMessageHash(
        address token,
        uint256 amount,
        address recipient
    ) public pure returns (bytes32) {
        return keccak256(abi.encodePacked(token, amount, recipient));
    }
    
    /// @inheritdoc ITokenBridge
    function isClaimed(bytes32 messageHash) external view returns (bool) {
        return claimed[messageHash];
    }
    
    //=============================================================
    //                      ADMIN FUNCTIONS
    //=============================================================
    
    /// @notice Register a token mapping
    function registerToken(
        address originToken,
        address localToken,
        uint64 tokenHomeChain
    ) external {
        tokenMappings[originToken] = localToken;
        homeChain[localToken] = tokenHomeChain;
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
ETHEREUM                                    TEMPO
────────                                    ─────

1. User calls tokenBridge.bridgeTokens(WETH, 1e18, recipient, TEMPO_CHAIN_ID)
   └─► WETH locked in TokenBridge
   └─► hash = keccak256(WETH, 1e18, recipient)
   └─► messageBridge.send(hash, TEMPO_CHAIN_ID)
   └─► Emits MessageSent(tokenBridge, hash, TEMPO_CHAIN_ID)

2. Validators observe MessageSent
   └─► Wait for Ethereum finality (~15 min)
   └─► Sign attestation: (tokenBridge, hash, ETH_CHAIN_ID, TEMPO_CHAIN_ID)
   └─► Aggregate threshold signature

3. Aggregator calls messageBridge.write(tokenBridge, hash, ETH_CHAIN_ID, sig)
   └─► BLS signature verified                       ◄──────────────────
   └─► received[ETH_CHAIN_ID][tokenBridge][hash] = timestamp
   └─► Emits MessageReceived

4. Anyone calls tokenBridge.claimTokens(WETH, 1e18, recipient, tokenBridge, ETH_CHAIN_ID)
   └─► hash = keccak256(WETH, 1e18, recipient)
   └─► Check: messageBridge.receivedAt(...) > 0 ✓
   └─► Mint wrapped WETH to recipient
   └─► Emits TokensClaimed
```

### Tempo → Ethereum

```
TEMPO                                       ETHEREUM
─────                                       ────────

1. User calls tokenBridge.bridgeTokens(PATH, 1e18, recipient, ETH_CHAIN_ID)
   └─► PATH locked in TokenBridge (Tempo is home)
   └─► hash = keccak256(PATH, 1e18, recipient)
   └─► messageBridge.send(hash, ETH_CHAIN_ID)
   └─► Emits MessageSent(tokenBridge, hash, ETH_CHAIN_ID)

2. Validators observe MessageSent
   └─► Tempo has instant finality
   └─► Sign attestation: (tokenBridge, hash, TEMPO_CHAIN_ID, ETH_CHAIN_ID)
   └─► Aggregate threshold signature

3. Aggregator calls messageBridge.write(tokenBridge, hash, TEMPO_CHAIN_ID, sig)
   └─► BLS signature verified                       ◄──────────────────
   └─► received[TEMPO_CHAIN_ID][tokenBridge][hash] = timestamp
   └─► Emits MessageReceived

4. Anyone calls tokenBridge.claimTokens(PATH, 1e18, recipient, tokenBridge, TEMPO_CHAIN_ID)
   └─► hash = keccak256(PATH, 1e18, recipient)
   └─► Check: messageBridge.receivedAt(...) > 0 ✓
   └─► Mint wrapped PATH to recipient
   └─► Emits TokensClaimed
```

## Message Hash Design

The message hash encodes the transfer intent:

```solidity
messageHash = keccak256(abi.encodePacked(
    token,      // 20 bytes - which token
    amount,     // 32 bytes - how much
    recipient   // 20 bytes - to whom
))
```

This is simple but sufficient because:
- **Uniqueness**: Ensured by MessageBridge's `sent[sender][hash]` check
- **Sender binding**: MessageBridge records sender, TokenBridge verifies it matches
- **Chain binding**: MessageBridge records origin chain

## Wrapped Token Template

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20, ERC20Burnable} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Burnable.sol";

contract WrappedToken is ERC20, ERC20Burnable {
    address public immutable bridge;
    
    modifier onlyBridge() {
        require(msg.sender == bridge, "Only bridge");
        _;
    }
    
    constructor(string memory name, string memory symbol, address _bridge) ERC20(name, symbol) {
        bridge = _bridge;
    }
    
    function mint(address to, uint256 amount) external onlyBridge {
        _mint(to, amount);
    }
}
```

## Gas Estimates

| Function | Gas |
|----------|-----|
| `bridgeTokens` (lock) | ~75,000 |
| `bridgeTokens` (burn) | ~55,000 |
| `claimTokens` (mint) | ~65,000 |
| `claimTokens` (unlock) | ~45,000 |

## Invariants

1. **Token Conservation**: Locked on home chain = minted on remote chain
2. **Single Claim**: Each message hash can only be claimed once
3. **Message Dependency**: Claims require `receivedAt > 0` in base layer
4. **Sender Matching**: Claims verify the original sender was the TokenBridge

## Extensibility

Other applications follow the same pattern:

| Application | Message Hash Contains |
|-------------|----------------------|
| Token Bridge | `keccak256(token, amount, recipient)` |
| NFT Bridge | `keccak256(collection, tokenId, recipient)` |
| Governance | `keccak256(proposalId, vote, voter)` |

Each application:
1. Computes a message hash from its data
2. Sends via `messageBridge.send(hash, destChain)`
3. Checks `receivedAt > 0` before executing on destination

## File Locations

| Component | Path |
|-----------|------|
| TokenBridge | `contracts/bridge/src/apps/TokenBridge.sol` |
| Interface | `contracts/bridge/src/interfaces/ITokenBridge.sol` |
| WrappedToken | `contracts/bridge/src/tokens/WrappedToken.sol` |
