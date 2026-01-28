---
id: TIP-YYYY
title: Cached Policy Type for TIP-20 Transfer Policy
description: Cache the TIP-403 policy type in TIP-20 storage to eliminate one SLOAD per transfer authorization check.
author: Mallesh Pai
status: Draft
related: TIP-403
---

# TIP-YYYY: Cached Policy Type for TIP-20 Transfer Policy

## Abstract

This TIP proposes caching the TIP-403 policy type alongside the `transferPolicyId` in TIP-20 token storage. Since TIP-403 policy types are immutable after creation, caching the type eliminates the `_policyData[policyId]` SLOAD during transfer authorization. With two `isAuthorized` calls per transfer (for `from` and `to`) sharing the same policy, this saves ~2,200 gas per transfer (one cold SLOAD eliminated, one warm SLOAD eliminated).

## Motivation

Currently, the TIP-403 `isAuthorized(policyId, user)` function reads `_policyData[policyId]` to determine the policy type (whitelist vs blacklist) before checking membership. This SLOAD is redundant because:

1. TIP-403 policy types are **immutable** — set at creation and never changed
2. The TIP-20 already stores the `transferPolicyId` and reads it on every transfer
3. The policy type can be cached when `changeTransferPolicyId` is called

By packing the policy type into the same storage slot as `transferPolicyId` and providing a new `isAuthorizedWithType` function, we eliminate one SLOAD per authorization check.

---

# Specification

## Storage Layout Change

The current TIP-20 storage:

```solidity
uint64 public transferPolicyId = 1;
```

Becomes a packed 72-bit value:

```solidity
// Bits 0-63: transferPolicyId (uint64)
// Bits 64-71: transferPolicyType (uint8, 0 = whitelist, 1 = blacklist)
uint256 internal _transferPolicyPacked;
```

For backward compatibility, an explicit `transferPolicyId()` view function is added to return just the policy ID:

```solidity
/// @notice Returns the current transfer policy ID
/// @return The policy ID (without the cached type)
function transferPolicyId() external view returns (uint64) {
    return uint64(_transferPolicyPacked);
}
```

## Interface Changes

### TIP-403 Registry Addition

```solidity
/// @notice Checks authorization using a provided policy type (avoids policyData SLOAD)
/// @param policyId The policy to check
/// @param policyType The cached policy type (must match the actual policy type)
/// @param user The address to check
/// @return True if the user is authorized under the policy
/// @dev Caller is responsible for ensuring policyType matches the policy's actual type.
///      If policyType is incorrect, authorization result will be wrong.
function isAuthorizedWithType(
    uint64 policyId, 
    PolicyType policyType, 
    address user
) external view returns (bool);
```

Implementation:

```solidity
function isAuthorizedWithType(
    uint64 policyId, 
    PolicyType policyType, 
    address user
) public view returns (bool) {
    // Special case for the "always-allow" and "always-reject" policies.
    if (policyId < 2) {
        return policyId == 1;
    }

    // Skip policyData read — caller provides the type
    return policyType == PolicyType.WHITELIST
        ? policySet[policyId][user]
        : !policySet[policyId][user];
}
```

### TIP-20 Changes

The `changeTransferPolicyId` function caches the policy type:

```solidity
function changeTransferPolicyId(uint64 newPolicyId) external onlyRole(DEFAULT_ADMIN_ROLE) {
    if (!TIP403_REGISTRY.policyExists(newPolicyId)) {
        revert InvalidTransferPolicyId();
    }
    
    // Cache the policy type (immutable, so always valid)
    PolicyType policyType;
    if (newPolicyId >= 2) {
        (policyType, ) = TIP403_REGISTRY.policyData(newPolicyId);
    }
    
    _transferPolicyPacked = uint256(newPolicyId) | (uint256(uint8(policyType)) << 64);
    
    emit TransferPolicyUpdate(msg.sender, newPolicyId);
}
```

The `transferAuthorized` modifier uses the cached type:

```solidity
modifier transferAuthorized(address from, address to) {
    uint64 policyId = uint64(_transferPolicyPacked);
    PolicyType policyType = PolicyType(uint8(_transferPolicyPacked >> 64));
    
    if (
        !TIP403_REGISTRY.isAuthorizedWithType(policyId, policyType, from)
            || !TIP403_REGISTRY.isAuthorizedWithType(policyId, policyType, to)
    ) revert PolicyForbids();
    _;
}
```

## Gas Savings

Since both `from` and `to` are checked against the **same** `transferPolicyId`, the `_policyData[policyId]` storage slot is only cold on the first access. Subsequent accesses to the same slot within a transaction cost only ~100 gas (warm) instead of ~2,100 gas (cold).

**Before TIP-YYYY (current):**
| Call | `_policyData` | `policySet` | Total |
|------|---------------|-------------|-------|
| `isAuthorized(policyId, from)` | ~2,100 (cold) | ~2,100 (cold) | ~4,200 |
| `isAuthorized(policyId, to)` | ~100 (warm) | ~2,100 (cold) | ~2,200 |
| **Transfer total** | | | **~6,400** |

**After TIP-YYYY:**
| Call | `_policyData` | `policySet` | Total |
|------|---------------|-------------|-------|
| `isAuthorizedWithType(policyId, type, from)` | — | ~2,100 (cold) | ~2,100 |
| `isAuthorizedWithType(policyId, type, to)` | — | ~2,100 (cold) | ~2,100 |
| **Transfer total** | | | **~4,200** |

**Savings: ~2,200 gas per transfer**

The savings come from eliminating the `_policyData[policyId]` SLOAD entirely — one cold read (~2,100 gas) on the first call, and one warm read (~100 gas) on the second call.

---

# Invariants

1. **Cached Type Validity**: The cached `transferPolicyType` MUST always match `policyData[transferPolicyId].policyType`. This is guaranteed because TIP-403 policy types are immutable.

2. **Backward Compatibility**: The `transferPolicyId()` view function MUST continue to return the policy ID without the cached type.

3. **Special Policy Handling**: Policy IDs 0 and 1 (always-reject and always-allow) do not require type caching and MUST continue to work correctly.
