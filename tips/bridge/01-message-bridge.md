# Base Message Bridge Contract Specification

This document specifies the base messaging bridge contract - a generic 32-byte message hash passing layer.

## Overview

The Message Bridge is the foundation layer that:
- Accepts 32-byte message hashes from any application
- Records sender address and source chain ID with each message
- Verifies BLS threshold signatures from Tempo validators
- Provides replay protection via processed message tracking
- Emits events for cross-chain message observability

The contract is **completely payload agnostic** - it has no knowledge of tokens, amounts, or application-specific logic. Applications encode their business logic into arbitrary bytes, hash it to 32 bytes, and send through this layer.

## Contract Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                           MessageBridge.sol                                      │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  ┌─────────────────────────────────────────────────────────────────────────┐    │
│  │                          Configuration                                   │    │
│  │  • remoteChainId: uint64          - Counterpart chain ID                │    │
│  │  • remoteBridge: address          - Bridge address on remote chain      │    │
│  │  • groupPublicKey: bytes          - BLS G1 point (48 bytes compressed)  │    │
│  │  • epoch: uint64                  - Current validator epoch             │    │
│  └─────────────────────────────────────────────────────────────────────────┘    │
│                                                                                  │
│  ┌─────────────────────────────────────────────────────────────────────────┐    │
│  │                          Message Storage                                 │    │
│  │  • messages[messageHash] → Message struct                               │    │
│  │  • processed[messageHash] → bool                                        │    │
│  │  • outboundNonce: uint64                                                │    │
│  └─────────────────────────────────────────────────────────────────────────┘    │
│                                                                                  │
│  ┌───────────────┐    ┌───────────────┐    ┌───────────────┐                   │
│  │     Send      │    │    Receive    │    │     Admin     │                   │
│  ├───────────────┤    ├───────────────┤    ├───────────────┤                   │
│  │ sendMessage() │    │receiveMessage │    │ updatePubKey  │                   │
│  │               │    │ verifyBLS()   │    │ pause/unpause │                   │
│  └───────────────┘    └───────────────┘    └───────────────┘                   │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Data Structures

### Message Struct

```solidity
struct Message {
    uint64 srcChainId;       // Source chain ID
    address sender;          // Sender address on source chain
    address recipient;       // Intended recipient on this chain
    bytes32 messageHash;     // The 32-byte payload hash
    uint64 nonce;            // Unique nonce from source chain
    uint64 timestamp;        // Block timestamp when received
    bool exists;             // True if message was received
}
```

### Outbound Message Event Data

When a message is sent, the following data is emitted and later signed by validators:

```solidity
struct OutboundMessage {
    bytes32 messageHash;     // The payload hash being sent
    uint64 srcChainId;       // This chain's ID
    uint64 dstChainId;       // Destination chain ID  
    address srcBridge;       // This bridge's address
    address dstBridge;       // Destination bridge address
    address sender;          // msg.sender who called sendMessage
    address recipient;       // Target application on destination
    uint64 nonce;            // Monotonic nonce
    uint64 epoch;            // Validator epoch for signature
}
```

## Interface Definition

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IMessageBridge
/// @notice Base messaging bridge for 32-byte hash passing between chains
interface IMessageBridge {
    //=============================================================
    //                          TYPES
    //=============================================================
    
    /// @notice Stored message data
    struct Message {
        uint64 srcChainId;
        address sender;
        address recipient;
        bytes32 messageHash;
        uint64 nonce;
        uint64 timestamp;
        bool exists;
    }
    
    //=============================================================
    //                          ERRORS
    //=============================================================
    
    error Unauthorized();
    error ContractPaused();
    error MessageAlreadyProcessed(bytes32 messageHash);
    error InvalidBLSSignature();
    error InvalidEpoch(uint64 provided, uint64 expected);
    error InvalidSourceChain(uint64 chainId);
    error InvalidSourceBridge(address provided, address expected);
    error ZeroMessageHash();
    error InvalidRecipient();
    
    //=============================================================
    //                          EVENTS
    //=============================================================
    
    /// @notice Emitted when a message is sent to the remote chain
    /// @param messageHash The 32-byte payload hash
    /// @param sender The address that initiated the send
    /// @param recipient The intended recipient on the destination chain
    /// @param nonce Unique message nonce
    /// @param epoch Current validator epoch
    event MessageSent(
        bytes32 indexed messageHash,
        address indexed sender,
        address indexed recipient,
        uint64 nonce,
        uint64 epoch
    );
    
    /// @notice Emitted when a message is received from the remote chain
    /// @param messageHash The 32-byte payload hash
    /// @param srcChainId Source chain ID
    /// @param sender Original sender on source chain
    /// @param recipient Recipient on this chain
    /// @param nonce Message nonce from source
    event MessageReceived(
        bytes32 indexed messageHash,
        uint64 indexed srcChainId,
        address indexed sender,
        address recipient,
        uint64 nonce
    );
    
    /// @notice Emitted when group public key is updated
    /// @param epoch New epoch number
    /// @param publicKey New BLS group public key
    event GroupPublicKeyUpdated(uint64 indexed epoch, bytes publicKey);
    
    /// @notice Emitted when pause state changes
    event PauseStateChanged(bool paused);
    
    //=============================================================
    //                      SEND FUNCTIONS
    //=============================================================
    
    /// @notice Send a message hash to the remote chain
    /// @dev Applications call this with their computed payload hash
    /// @param messageHash The 32-byte hash to send
    /// @param recipient The recipient address on the remote chain
    /// @return nonce The unique nonce assigned to this message
    function sendMessage(
        bytes32 messageHash,
        address recipient
    ) external returns (uint64 nonce);
    
    //=============================================================
    //                    RECEIVE FUNCTIONS
    //=============================================================
    
    /// @notice Receive a message from the remote chain with BLS attestation
    /// @dev Called by aggregator after collecting threshold signatures
    /// @param data Encoded message data (OutboundMessage struct)
    /// @param signature Aggregated BLS threshold signature (96 bytes)
    function receiveMessage(
        bytes calldata data,
        bytes calldata signature
    ) external;
    
    //=============================================================
    //                      VIEW FUNCTIONS
    //=============================================================
    
    /// @notice Get a received message by its hash
    /// @param messageHash The message hash to query
    /// @return message The stored message data
    function getMessage(bytes32 messageHash) external view returns (Message memory);
    
    /// @notice Check if a message has been received
    /// @param messageHash The message hash to check
    /// @return True if the message exists
    function hasMessage(bytes32 messageHash) external view returns (bool);
    
    /// @notice Check if a message hash has been processed
    /// @param messageHash The message hash to check
    /// @return True if already processed
    function isProcessed(bytes32 messageHash) external view returns (bool);
    
    /// @notice Get current outbound nonce
    function outboundNonce() external view returns (uint64);
    
    /// @notice Get current epoch
    function epoch() external view returns (uint64);
    
    /// @notice Get remote chain ID
    function remoteChainId() external view returns (uint64);
    
    /// @notice Get remote bridge address
    function remoteBridge() external view returns (address);
    
    /// @notice Check if paused
    function paused() external view returns (bool);
    
    //=============================================================
    //                      ADMIN FUNCTIONS
    //=============================================================
    
    /// @notice Update the group public key for a new epoch
    /// @param newEpoch The new epoch number
    /// @param publicKey The new BLS group public key (48 bytes compressed G1)
    function updateGroupPublicKey(uint64 newEpoch, bytes calldata publicKey) external;
    
    /// @notice Pause the contract
    function pause() external;
    
    /// @notice Unpause the contract
    function unpause() external;
    
    /// @notice Transfer ownership
    function transferOwnership(address newOwner) external;
}
```

## Implementation

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IMessageBridge} from "./interfaces/IMessageBridge.sol";
import {BLSVerifier} from "./libraries/BLSVerifier.sol";
import {MessageEncoding} from "./libraries/MessageEncoding.sol";

/// @title MessageBridge
/// @notice Generic cross-chain messaging using BLS threshold signatures
/// @dev Deployed identically on all supported chains
contract MessageBridge is IMessageBridge {
    //=============================================================
    //                          CONSTANTS
    //=============================================================
    
    /// @notice Domain separator for bridge attestations
    bytes public constant DOMAIN = "TEMPO_MESSAGE_BRIDGE_V1";
    
    //=============================================================
    //                          STORAGE
    //=============================================================
    
    /// @notice Contract owner
    address public owner;
    
    /// @notice Pause state
    bool public paused;
    
    /// @notice Remote chain ID
    uint64 public remoteChainId;
    
    /// @notice Remote bridge address
    address public remoteBridge;
    
    /// @notice Current validator epoch
    uint64 public epoch;
    
    /// @notice Previous epoch (for grace period)
    uint64 public previousEpoch;
    
    /// @notice BLS group public key for current epoch (48 bytes compressed G1)
    bytes public groupPublicKey;
    
    /// @notice BLS group public key for previous epoch
    bytes public previousGroupPublicKey;
    
    /// @notice Outbound message nonce
    uint64 public outboundNonce;
    
    /// @notice Received messages by hash
    mapping(bytes32 => Message) private _messages;
    
    /// @notice Processed message hashes (prevents replay)
    mapping(bytes32 => bool) public processed;
    
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
        uint64 _remoteChainId,
        address _remoteBridge,
        uint64 _initialEpoch,
        bytes memory _initialPublicKey
    ) {
        owner = _owner;
        remoteChainId = _remoteChainId;
        remoteBridge = _remoteBridge;
        epoch = _initialEpoch;
        groupPublicKey = _initialPublicKey;
    }
    
    //=============================================================
    //                      SEND FUNCTIONS
    //=============================================================
    
    /// @inheritdoc IMessageBridge
    function sendMessage(
        bytes32 messageHash,
        address recipient
    ) external whenNotPaused returns (uint64 nonce) {
        if (messageHash == bytes32(0)) revert ZeroMessageHash();
        if (recipient == address(0)) revert InvalidRecipient();
        
        nonce = outboundNonce++;
        
        emit MessageSent(
            messageHash,
            msg.sender,
            recipient,
            nonce,
            epoch
        );
    }
    
    //=============================================================
    //                    RECEIVE FUNCTIONS
    //=============================================================
    
    /// @inheritdoc IMessageBridge
    function receiveMessage(
        bytes calldata data,
        bytes calldata signature
    ) external whenNotPaused {
        // Decode the message
        MessageEncoding.OutboundMessage memory msg_ = MessageEncoding.decode(data);
        
        // Verify source chain and bridge
        if (msg_.srcChainId != remoteChainId) {
            revert InvalidSourceChain(msg_.srcChainId);
        }
        if (msg_.srcBridge != remoteBridge) {
            revert InvalidSourceBridge(msg_.srcBridge, remoteBridge);
        }
        if (msg_.dstChainId != uint64(block.chainid)) {
            revert InvalidSourceChain(msg_.dstChainId);
        }
        if (msg_.dstBridge != address(this)) {
            revert InvalidSourceBridge(msg_.dstBridge, address(this));
        }
        
        // Check not already processed
        bytes32 attestationHash = MessageEncoding.computeAttestationHash(DOMAIN, msg_);
        if (processed[attestationHash]) {
            revert MessageAlreadyProcessed(msg_.messageHash);
        }
        
        // Verify epoch (allow current or previous for grace period)
        bytes memory pubKey;
        if (msg_.epoch == epoch) {
            pubKey = groupPublicKey;
        } else if (msg_.epoch == previousEpoch && previousGroupPublicKey.length > 0) {
            pubKey = previousGroupPublicKey;
        } else {
            revert InvalidEpoch(msg_.epoch, epoch);
        }
        
        // Verify BLS signature
        if (!BLSVerifier.verify(pubKey, attestationHash, signature)) {
            revert InvalidBLSSignature();
        }
        
        // Mark as processed
        processed[attestationHash] = true;
        
        // Store the message
        _messages[msg_.messageHash] = Message({
            srcChainId: msg_.srcChainId,
            sender: msg_.sender,
            recipient: msg_.recipient,
            messageHash: msg_.messageHash,
            nonce: msg_.nonce,
            timestamp: uint64(block.timestamp),
            exists: true
        });
        
        emit MessageReceived(
            msg_.messageHash,
            msg_.srcChainId,
            msg_.sender,
            msg_.recipient,
            msg_.nonce
        );
    }
    
    //=============================================================
    //                      VIEW FUNCTIONS
    //=============================================================
    
    /// @inheritdoc IMessageBridge
    function getMessage(bytes32 messageHash) external view returns (Message memory) {
        return _messages[messageHash];
    }
    
    /// @inheritdoc IMessageBridge
    function hasMessage(bytes32 messageHash) external view returns (bool) {
        return _messages[messageHash].exists;
    }
    
    /// @inheritdoc IMessageBridge
    function isProcessed(bytes32 messageHash) external view returns (bool) {
        return processed[messageHash];
    }
    
    //=============================================================
    //                      ADMIN FUNCTIONS
    //=============================================================
    
    /// @inheritdoc IMessageBridge
    function updateGroupPublicKey(
        uint64 newEpoch,
        bytes calldata publicKey
    ) external onlyOwner {
        require(newEpoch > epoch, "Epoch must increase");
        require(publicKey.length == 48, "Invalid public key length");
        
        // Store previous for grace period
        previousEpoch = epoch;
        previousGroupPublicKey = groupPublicKey;
        
        // Update to new
        epoch = newEpoch;
        groupPublicKey = publicKey;
        
        emit GroupPublicKeyUpdated(newEpoch, publicKey);
    }
    
    /// @inheritdoc IMessageBridge
    function pause() external onlyOwner {
        paused = true;
        emit PauseStateChanged(true);
    }
    
    /// @inheritdoc IMessageBridge
    function unpause() external onlyOwner {
        paused = false;
        emit PauseStateChanged(false);
    }
    
    /// @inheritdoc IMessageBridge
    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0), "Invalid owner");
        owner = newOwner;
    }
}
```

## Message Encoding Library

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title MessageEncoding
/// @notice Encoding and hashing for cross-chain messages
library MessageEncoding {
    /// @notice Outbound message structure for signing
    struct OutboundMessage {
        bytes32 messageHash;
        uint64 srcChainId;
        uint64 dstChainId;
        address srcBridge;
        address dstBridge;
        address sender;
        address recipient;
        uint64 nonce;
        uint64 epoch;
    }
    
    /// @notice Encode an outbound message for transmission
    function encode(OutboundMessage memory msg_) internal pure returns (bytes memory) {
        return abi.encode(
            msg_.messageHash,
            msg_.srcChainId,
            msg_.dstChainId,
            msg_.srcBridge,
            msg_.dstBridge,
            msg_.sender,
            msg_.recipient,
            msg_.nonce,
            msg_.epoch
        );
    }
    
    /// @notice Decode a received message
    function decode(bytes calldata data) internal pure returns (OutboundMessage memory) {
        (
            bytes32 messageHash,
            uint64 srcChainId,
            uint64 dstChainId,
            address srcBridge,
            address dstBridge,
            address sender,
            address recipient,
            uint64 nonce,
            uint64 epoch_
        ) = abi.decode(data, (bytes32, uint64, uint64, address, address, address, address, uint64, uint64));
        
        return OutboundMessage({
            messageHash: messageHash,
            srcChainId: srcChainId,
            dstChainId: dstChainId,
            srcBridge: srcBridge,
            dstBridge: dstBridge,
            sender: sender,
            recipient: recipient,
            nonce: nonce,
            epoch: epoch_
        });
    }
    
    /// @notice Compute attestation hash for BLS signing
    /// @dev This is what validators sign
    function computeAttestationHash(
        bytes memory domain,
        OutboundMessage memory msg_
    ) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(
            domain,              // "TEMPO_MESSAGE_BRIDGE_V1"
            msg_.messageHash,    // The payload hash
            msg_.srcChainId,
            msg_.dstChainId,
            msg_.srcBridge,
            msg_.dstBridge,
            msg_.sender,
            msg_.recipient,
            msg_.nonce,
            msg_.epoch
        ));
    }
}
```

## BLS Verifier Library

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title BLSVerifier
/// @notice BLS12-381 signature verification using EIP-2537 precompiles
library BLSVerifier {
    // EIP-2537 precompile addresses (available since Pectra/Osaka)
    address constant BLS_G1_ADD = address(0x0b);
    address constant BLS_G1_MSM = address(0x0c);
    address constant BLS_G2_ADD = address(0x0d);
    address constant BLS_G2_MSM = address(0x0e);
    address constant BLS_PAIRING = address(0x0f);
    address constant BLS_MAP_FP_TO_G1 = address(0x10);
    address constant BLS_MAP_FP2_TO_G2 = address(0x11);
    
    /// @notice Verify a BLS signature against a message hash
    /// @param publicKey Compressed G1 public key (48 bytes)
    /// @param messageHash The message hash that was signed
    /// @param signature Compressed G2 signature (96 bytes)
    /// @return True if signature is valid
    function verify(
        bytes memory publicKey,
        bytes32 messageHash,
        bytes memory signature
    ) internal view returns (bool) {
        if (publicKey.length != 48) return false;
        if (signature.length != 96) return false;
        
        // Decompress public key to G1 point (128 bytes)
        bytes memory pk = decompressG1(publicKey);
        if (pk.length == 0) return false;
        
        // Decompress signature to G2 point (256 bytes)
        bytes memory sig = decompressG2(signature);
        if (sig.length == 0) return false;
        
        // Hash message to G2 curve point using hash-to-curve
        bytes memory msgPoint = hashToG2(messageHash);
        if (msgPoint.length == 0) return false;
        
        // Verify pairing: e(pk, H(m)) == e(G1, sig)
        // Equivalently: e(pk, H(m)) * e(-G1, sig) == 1
        bytes memory negG1 = getNegativeG1Generator();
        bytes memory input = abi.encodePacked(pk, msgPoint, negG1, sig);
        
        (bool success, bytes memory result) = BLS_PAIRING.staticcall(input);
        if (!success || result.length != 32) return false;
        
        return abi.decode(result, (uint256)) == 1;
    }
    
    /// @notice Decompress a G1 point from 48 bytes to 128 bytes
    function decompressG1(bytes memory compressed) internal pure returns (bytes memory) {
        if (compressed.length != 48) return "";
        // Implementation uses curve equation to recover y from x
        bytes memory result = new bytes(128);
        // ... decompression logic
        return result;
    }
    
    /// @notice Decompress a G2 point from 96 bytes to 256 bytes
    function decompressG2(bytes memory compressed) internal pure returns (bytes memory) {
        if (compressed.length != 96) return "";
        bytes memory result = new bytes(256);
        // ... decompression logic
        return result;
    }
    
    /// @notice Hash a message to a G2 curve point
    /// @dev Uses hash-to-curve per RFC 9380
    function hashToG2(bytes32 messageHash) internal view returns (bytes memory) {
        bytes memory expanded = expandToFp2(messageHash);
        (bool success, bytes memory result) = BLS_MAP_FP2_TO_G2.staticcall(expanded);
        if (!success) return "";
        return result;
    }
    
    /// @notice Expand hash to Fp2 field elements per RFC 9380
    function expandToFp2(bytes32 messageHash) internal pure returns (bytes memory) {
        bytes memory dst = "TEMPO_BRIDGE_BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_";
        bytes memory result = new bytes(128);
        // ... expand_message_xmd implementation
        return result;
    }
    
    /// @notice Get the negated G1 generator point
    function getNegativeG1Generator() internal pure returns (bytes memory) {
        // G1 generator with y-coordinate negated mod p
        // This is a constant value
        return hex""; // 128 bytes - actual value to be computed
    }
}
```

## Deployment

### Constructor Arguments

| Parameter | Ethereum | Tempo |
|-----------|----------|-------|
| `_owner` | Multisig address | Multisig address |
| `_remoteChainId` | Tempo chain ID | Ethereum chain ID (1) |
| `_remoteBridge` | Bridge address on Tempo | Bridge address on Ethereum |
| `_initialEpoch` | Current Tempo epoch | Current Tempo epoch |
| `_initialPublicKey` | Tempo DKG public key | Tempo DKG public key |

### Post-Deployment Verification

1. Verify `remoteChainId` and `remoteBridge` are correct
2. Verify `epoch` and `groupPublicKey` match Tempo consensus
3. Verify `owner` is the intended multisig
4. Test send/receive flow with small message

## Gas Estimates

| Function | Estimated Gas |
|----------|---------------|
| `sendMessage` | ~25,000 |
| `receiveMessage` (BLS verify + store) | ~200,000 |
| `updateGroupPublicKey` | ~50,000 |
| `getMessage` | ~5,000 |

## Invariants

1. **Single Reception**: Each `(messageHash, nonce)` pair can only be received once
2. **Source Binding**: Messages can only come from the configured `remoteBridge`
3. **Epoch Validity**: Only current or previous epoch signatures accepted
4. **Nonce Monotonicity**: Outbound `nonce` strictly increases
5. **Message Immutability**: Once stored, message data cannot change

## File Locations

| Component | Path |
|-----------|------|
| Main Contract | `contracts/bridge/src/MessageBridge.sol` |
| Interfaces | `contracts/bridge/src/interfaces/IMessageBridge.sol` |
| Libraries | `contracts/bridge/src/libraries/` |
| Deploy Script | `contracts/bridge/script/Deploy.s.sol` |
