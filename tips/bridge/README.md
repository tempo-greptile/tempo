# Tempo Native Bridge Specification

This directory contains the complete specification for the Tempo ↔ External Chain messaging bridge.

## Overview

The Tempo Native Bridge is a **generic cross-chain messaging layer** that enables arbitrary 32-byte message hash passing between Tempo and other EVM chains. It uses a **BLS threshold signature model** where validators collectively sign attestations using their BLS12-381 key shares from the consensus DKG.

The bridge follows a **layered architecture**:

1. **Base Messaging Layer** - Generic 32-byte message hash passing with sender/chain metadata
2. **Application Layer** - Token bridges, NFT bridges, and other apps built on top

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              APPLICATION LAYER                                   │
│  ┌───────────────┐   ┌───────────────┐   ┌───────────────┐                     │
│  │  Token Bridge │   │   NFT Bridge  │   │  Custom Apps  │                     │
│  │  (lock/mint)  │   │  (lock/mint)  │   │  (arbitrary)  │                     │
│  └───────┬───────┘   └───────┬───────┘   └───────┬───────┘                     │
│          │                   │                   │                              │
│          └───────────────────┼───────────────────┘                              │
│                              ▼                                                   │
├─────────────────────────────────────────────────────────────────────────────────┤
│                          BASE MESSAGING LAYER                                    │
│                                                                                  │
│   ┌─────────────────┐                           ┌─────────────────┐             │
│   │   Any Chain     │                           │      Tempo      │             │
│   │                 │                           │                 │             │
│   │  ┌───────────┐  │                           │  ┌───────────┐  │             │
│   │  │  Message  │◄─┼───── BLS Attestation ─────┼──►│  Message  │  │             │
│   │  │  Bridge   │  │                           │  │  Bridge   │  │             │
│   │  └───────────┘  │                           │  └───────────┘  │             │
│   │        ▲        │                           │        ▲        │             │
│   │        │        │                           │        │        │             │
│   │  sendMessage()  │                           │  sendMessage()  │             │
│   │  receiveMessage │                           │  receiveMessage │             │
│   │        │        │                           │        │        │             │
│   │   ┌────┴────┐   │                           │   ┌────┴────┐   │             │
│   │   │   App   │   │                           │   │   App   │   │             │
│   │   └─────────┘   │                           │   └─────────┘   │             │
│   │                 │                           │                 │             │
│   └─────────────────┘                           └─────────────────┘             │
│                                                                                  │
│                        ┌───────────────────────┐                                │
│                        │   Validator Sidecar   │                                │
│                        │  (watches both chains │                                │
│                        │   signs with BLS key) │                                │
│                        └───────────────────────┘                                │
│                                    │                                            │
│                        ┌───────────┴───────────┐                                │
│                        │      Aggregator       │                                │
│                        │  (collects partials,  │                                │
│                        │   submits threshold)  │                                │
│                        └───────────────────────┘                                │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Key Design Principles

### 1. Payload Agnostic Base Layer

The base messaging contract knows nothing about tokens, amounts, or application logic. It only:
- Stores received 32-byte message hashes
- Records the source chain ID and sender address
- Emits events for message sending/receiving
- Verifies BLS threshold signatures

### 2. Applications Encode Their Own Payloads

Token bridges, NFT bridges, and other applications:
- Encode their business logic into arbitrary bytes
- Compute a 32-byte hash of the payload
- Send the hash through the base layer
- Decode and execute on the destination

### 3. Single Universal Contract

The **same contract code** is deployed on all chains (Ethereum, Tempo, other L2s). Configuration differences:
- `remoteChainId` - The chain ID of the counterpart
- `remoteBridge` - Address of the bridge on the remote chain
- `groupPublicKey` - Current BLS group public key from Tempo's DKG

## Documents

| Document | Description |
|----------|-------------|
| [01-message-bridge.md](./01-message-bridge.md) | Base messaging bridge contract specification |
| [02-message-format.md](./02-message-format.md) | Message hash computation and BLS signing conventions |
| [03-sidecar.md](./03-sidecar.md) | Validator sidecar and aggregator specification |
| [04-token-bridge.md](./04-token-bridge.md) | Token bridge application built on base layer |

## Quick Start

### Base Layer: Send a Message

```solidity
// On Source Chain - any application can send a message hash
bytes32 messageHash = keccak256(abi.encode(myAppData));

bridge.sendMessage(
    messageHash,      // 32-byte payload hash
    recipientApp      // Application address on destination
);
// → Emits MessageSent event with (messageHash, sender, chainId, nonce)
```

### Base Layer: Receive a Message

```solidity
// After validator attestation on Destination Chain
bridge.receiveMessage(
    messageData,      // Encoded message metadata
    blsSignature      // Aggregated threshold signature (96 bytes)
);
// → Stores message, emits MessageReceived
// → Application can now query and process

// Application queries received messages
Message memory msg = bridge.getMessage(messageHash);
// Returns: (srcChainId, sender, recipient, nonce, timestamp)
```

### Application Layer: Token Bridge Example

```solidity
// Token Bridge built on top of base messaging

// Step 1: Lock tokens and send message
tokenBridge.bridgeTokens(
    tokenAddress,     // Token to bridge
    amount,           // Amount to lock
    destChainId,      // Destination chain
    recipient         // Recipient on destination
);
// → Locks tokens in escrow
// → Computes: messageHash = keccak256(token, amount, recipient, nonce)
// → Calls: bridge.sendMessage(messageHash, remoteTokenBridge)

// Step 2: On destination, after message is received
tokenBridge.claimTokens(
    token,            // Token address on this chain
    amount,           // Amount to mint/unlock
    originalSender,   // Sender from source chain
    nonce             // Original nonce
);
// → Verifies message exists in base bridge
// → Mints/unlocks tokens to recipient
```

## Implementation Status

| Component | Status | Location |
|-----------|--------|----------|
| Message Bridge Contract | 📋 Spec | `contracts/bridge/` |
| Message Format | 📋 Spec | This directory |
| Sidecar Binary | 📋 Spec | `crates/bridge-sidecar/` |
| Token Bridge (App) | 📋 Spec | `contracts/bridge/apps/` |

## Why This Architecture?

### Separation of Concerns

| Layer | Responsibility |
|-------|----------------|
| **Base Messaging** | Hash verification, BLS signatures, replay protection, sequencing |
| **Applications** | Token logic, NFT logic, custom business rules, payload encoding |

### Benefits

1. **Composability** - Any application can use the base layer
2. **Upgradability** - Applications can evolve without changing base layer
3. **Simplicity** - Base layer is minimal and auditable
4. **Extensibility** - New apps (governance, oracles, etc.) use same infrastructure
5. **Security** - Single point of verification for all cross-chain messages
