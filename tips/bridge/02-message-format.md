# Message Format Specification

This document defines the canonical message format used for BLS threshold attestations in the Tempo Message Bridge.

## Overview

The base messaging layer passes 32-byte message hashes between chains. Validators sign an **attestation** that includes:
- The message hash itself
- Metadata about the source/destination (chain IDs, bridge addresses)
- Sender/recipient information
- Nonce for uniqueness
- Epoch for signature validation

This document specifies:
- The attestation structure validators sign
- Domain separation for replay protection
- BLS signing conventions
- Implementation in Rust and Solidity

## Attestation Structure

### What Validators Sign

Validators do NOT sign the raw message hash. They sign an **attestation hash** that binds the message to its context:

| Field | Type | Size | Description |
|-------|------|------|-------------|
| `domain` | bytes | 24 | Fixed: `"TEMPO_MESSAGE_BRIDGE_V1"` |
| `messageHash` | bytes32 | 32 | The application payload hash |
| `srcChainId` | uint64 | 8 | Source chain ID |
| `dstChainId` | uint64 | 8 | Destination chain ID |
| `srcBridge` | address | 20 | Bridge contract on source |
| `dstBridge` | address | 20 | Bridge contract on destination |
| `sender` | address | 20 | Original sender address |
| `recipient` | address | 20 | Target recipient address |
| `nonce` | uint64 | 8 | Per-bridge monotonic nonce |
| `epoch` | uint64 | 8 | Validator epoch |

**Total encoded length**: 168 bytes (before hashing)

### Attestation Hash Computation

```
attestationHash = keccak256(abi.encodePacked(
    "TEMPO_MESSAGE_BRIDGE_V1",  // 24 bytes - domain separator
    messageHash,                // 32 bytes
    srcChainId,                 // 8 bytes (uint64)
    dstChainId,                 // 8 bytes (uint64)
    srcBridge,                  // 20 bytes (address)
    dstBridge,                  // 20 bytes (address)
    sender,                     // 20 bytes (address)
    recipient,                  // 20 bytes (address)
    nonce,                      // 8 bytes (uint64)
    epoch                       // 8 bytes (uint64)
))
```

## Domain Separation

### Protocol Domain

The fixed domain prefix `"TEMPO_MESSAGE_BRIDGE_V1"` provides:

1. **Protocol Isolation**: Prevents signatures from being replayed in other protocols
2. **Version Control**: Allows upgrading message format in future versions
3. **Namespace Separation**: Distinct from consensus signing namespace

### BLS Signing Domain (DST)

For BLS hash-to-curve, we use a Domain Separation Tag following RFC 9380:

```
DST = "TEMPO_BRIDGE_BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_"
```

This ensures:
- Signatures cannot be replayed across protocols using BLS12-381
- The hash-to-curve output is unique to this application

## Replay Protection

The attestation structure provides multiple layers of replay protection:

### 1. Cross-Chain Replay Prevention

```
srcChainId, dstChainId, srcBridge, dstBridge
```

A signature for Ethereum → Tempo cannot be replayed:
- As Arbitrum → Tempo (different `srcChainId`)
- As Tempo → Ethereum (reversed direction)
- On a different bridge deployment (different addresses)

### 2. Message Uniqueness

```
messageHash, nonce
```

Each message from a source chain has a unique nonce. Even if two applications send identical `messageHash` values, they will have different nonces.

### 3. Recipient Binding

```
sender, recipient
```

The sender and recipient are signed, preventing:
- Front-running attacks that redirect messages
- Spoofing the sender address

### 4. Temporal Binding

```
epoch
```

Signatures are bound to a validator epoch, providing:
- Bounded validity window
- Protection against using old validator sets

## BLS Threshold Signatures

### Threshold Scheme

Tempo validators use BLS12-381 threshold signatures from the consensus DKG:

1. **Key Generation**: Validators run DKG to generate key shares
2. **Partial Signing**: Each validator signs with their share
3. **Aggregation**: Aggregator collects t-of-n partial signatures
4. **Recovery**: Lagrange interpolation recovers threshold signature Σ
5. **Verification**: On-chain verification using EIP-2537 pairing

### Signature Format

| Format | Size | Use |
|--------|------|-----|
| Compressed G2 | 96 bytes | Transmission, storage |
| Uncompressed G2 | 192 bytes | Internal pairing operations |

The **MinSig** variant is used:
- Signatures in G2 (96 bytes compressed)
- Public keys in G1 (48 bytes compressed)

### Signing Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           BLS Threshold Signing                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   attestationHash ──► hash_to_curve(DST) ──► H(m) ∈ G2                      │
│                                                                              │
│   Validator i:  σᵢ = skᵢ · H(m)     (partial signature)                     │
│                                                                              │
│   Aggregator:   σ = Σ λᵢ · σᵢ      (Lagrange interpolation)                 │
│                     i∈S                                                      │
│                 where |S| ≥ threshold                                        │
│                                                                              │
│   Verification: e(PK, H(m)) == e(G1, σ)                                     │
│                 where PK is group public key                                 │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Rust Implementation

```rust
use alloy_primitives::{keccak256, Address, B256};

pub const BRIDGE_DOMAIN: &[u8] = b"TEMPO_MESSAGE_BRIDGE_V1";
pub const BLS_DST: &[u8] = b"TEMPO_BRIDGE_BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_";

/// Outbound message that validators attest to
#[derive(Debug, Clone)]
pub struct OutboundMessage {
    pub message_hash: B256,
    pub src_chain_id: u64,
    pub dst_chain_id: u64,
    pub src_bridge: Address,
    pub dst_bridge: Address,
    pub sender: Address,
    pub recipient: Address,
    pub nonce: u64,
    pub epoch: u64,
}

impl OutboundMessage {
    /// Compute the attestation hash that validators sign
    pub fn attestation_hash(&self) -> B256 {
        let mut data = Vec::with_capacity(168);
        
        // Domain separator (24 bytes)
        data.extend_from_slice(BRIDGE_DOMAIN);
        
        // Message hash (32 bytes)
        data.extend_from_slice(self.message_hash.as_slice());
        
        // Chain IDs (8 bytes each)
        data.extend_from_slice(&self.src_chain_id.to_be_bytes());
        data.extend_from_slice(&self.dst_chain_id.to_be_bytes());
        
        // Bridge addresses (20 bytes each)
        data.extend_from_slice(self.src_bridge.as_slice());
        data.extend_from_slice(self.dst_bridge.as_slice());
        
        // Sender and recipient (20 bytes each)
        data.extend_from_slice(self.sender.as_slice());
        data.extend_from_slice(self.recipient.as_slice());
        
        // Nonce and epoch (8 bytes each)
        data.extend_from_slice(&self.nonce.to_be_bytes());
        data.extend_from_slice(&self.epoch.to_be_bytes());
        
        keccak256(&data)
    }
    
    /// Encode for transmission to destination chain
    pub fn encode(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(168);
        
        data.extend_from_slice(self.message_hash.as_slice());
        data.extend_from_slice(&self.src_chain_id.to_be_bytes());
        data.extend_from_slice(&self.dst_chain_id.to_be_bytes());
        data.extend_from_slice(self.src_bridge.as_slice());
        data.extend_from_slice(self.dst_bridge.as_slice());
        data.extend_from_slice(self.sender.as_slice());
        data.extend_from_slice(self.recipient.as_slice());
        data.extend_from_slice(&self.nonce.to_be_bytes());
        data.extend_from_slice(&self.epoch.to_be_bytes());
        
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_attestation_hash_deterministic() {
        let msg = OutboundMessage {
            message_hash: B256::repeat_byte(0x11),
            src_chain_id: 1,
            dst_chain_id: 12345,
            src_bridge: Address::repeat_byte(0xAA),
            dst_bridge: Address::repeat_byte(0xBB),
            sender: Address::repeat_byte(0xCC),
            recipient: Address::repeat_byte(0xDD),
            nonce: 42,
            epoch: 100,
        };
        
        let hash1 = msg.attestation_hash();
        let hash2 = msg.attestation_hash();
        assert_eq!(hash1, hash2);
    }
    
    #[test]
    fn test_different_message_hash_different_attestation() {
        let msg1 = OutboundMessage {
            message_hash: B256::repeat_byte(0x11),
            ..Default::default()
        };
        let msg2 = OutboundMessage {
            message_hash: B256::repeat_byte(0x22),
            ..Default::default()
        };
        
        assert_ne!(msg1.attestation_hash(), msg2.attestation_hash());
    }
}
```

## Solidity Implementation

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

library MessageFormat {
    bytes constant DOMAIN = "TEMPO_MESSAGE_BRIDGE_V1";
    
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
    
    /// @notice Compute the attestation hash that validators sign
    function computeAttestationHash(
        OutboundMessage memory msg_
    ) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(
            DOMAIN,
            msg_.messageHash,
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
    
    /// @notice Encode message for transmission
    function encode(
        OutboundMessage memory msg_
    ) internal pure returns (bytes memory) {
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
    
    /// @notice Decode received message
    function decode(
        bytes calldata data
    ) internal pure returns (OutboundMessage memory) {
        (
            bytes32 messageHash,
            uint64 srcChainId,
            uint64 dstChainId,
            address srcBridge,
            address dstBridge,
            address sender,
            address recipient,
            uint64 nonce,
            uint64 epoch
        ) = abi.decode(data, (
            bytes32, uint64, uint64, address, address, address, address, uint64, uint64
        ));
        
        return OutboundMessage({
            messageHash: messageHash,
            srcChainId: srcChainId,
            dstChainId: dstChainId,
            srcBridge: srcBridge,
            dstBridge: dstBridge,
            sender: sender,
            recipient: recipient,
            nonce: nonce,
            epoch: epoch
        });
    }
}
```

## Test Vectors

### Test Vector 1: Basic Message

**Input:**
```
messageHash:  0x1111111111111111111111111111111111111111111111111111111111111111
srcChainId:   1
dstChainId:   12345
srcBridge:    0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
dstBridge:    0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB
sender:       0xCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC
recipient:    0xDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD
nonce:        42
epoch:        100
```

**Expected attestationHash:**
```
0x... (to be computed during implementation - must match Rust and Solidity)
```

### Test Vector 2: Different Direction

**Input:**
```
messageHash:  0x2222222222222222222222222222222222222222222222222222222222222222
srcChainId:   12345
dstChainId:   1
srcBridge:    0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB
dstBridge:    0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
sender:       0xEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE
recipient:    0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF
nonce:        1
epoch:        100
```

**Expected attestationHash:**
```
0x... (to be computed during implementation)
```

## Invariants

1. **Determinism**: Given the same inputs, `attestation_hash()` MUST always produce the same output
2. **Uniqueness**: Different inputs MUST produce different outputs (collision resistance)
3. **Parity**: Rust and Solidity implementations MUST produce identical hashes for same inputs
4. **Domain Isolation**: Attestation hashes MUST differ from any other protocol's hashes

## References

- [EIP-2537: BLS12-381 Precompiles](https://eips.ethereum.org/EIPS/eip-2537)
- [RFC 9380: Hashing to Elliptic Curves](https://www.rfc-editor.org/rfc/rfc9380)
- [BLS Signatures - IETF Draft](https://datatracker.ietf.org/doc/draft-irtf-cfrg-bls-signature/)
