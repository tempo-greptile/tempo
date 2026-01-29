---
id: TIP-XXXX
title: Complex Transfer Policies
description: Extends TIP-403 with complex policies that specify different authorization rules for senders and recipients.
authors: Dan Robinson @danrobinson
status: Draft
related: TIP-403, TIP-20
protocolVersion: T2
---

# TIP-XXXX: Complex Transfer Policies

## Abstract

This TIP extends the TIP-403 policy registry to support **complex policies** that allow token issuers to specify different authorization rules for senders and recipients. A complex policy references three simple policies: one for sender authorization, one for recipient authorization, and one default policy for legacy `isAuthorized` calls. Complex policies are immutable once created.

## Motivation

The current TIP-403 system applies the same policy to both senders and recipients of a token transfer. However, real-world compliance requirements often differ between sending and receiving:

- **Sender restrictions**: An issuer may want to block sanctioned addresses from sending tokens, while allowing anyone to receive tokens (e.g., for refunds or seizure).
- **Recipient restrictions**: An issuer may require recipients to be KYC-verified, while allowing any holder to send tokens out.
- **Asymmetric compliance**: Different jurisdictions may have different requirements for inflows vs outflows.

Complex policies enable these use cases while maintaining backward compatibility with existing simple policies.

---

# Specification

## Policy Types

TIP-403 currently supports two policy types: `WHITELIST` and `BLACKLIST`. This TIP adds a third type:

```solidity
enum PolicyType {
    WHITELIST,
    BLACKLIST,
    COMPLEX
}
```

## Complex Policy Structure

A complex policy references three existing simple policies by their policy IDs:

```solidity
struct ComplexPolicyData {
    uint64 senderPolicyId;    // Policy checked for senders
    uint64 recipientPolicyId; // Policy checked for recipients
    uint64 defaultPolicyId;   // Policy used for legacy isAuthorized() calls
}
```

All three referenced policies MUST be simple policies (WHITELIST or BLACKLIST), not complex policies. This prevents circular references and unbounded recursion.

## Interface Additions

The TIP403Registry interface is extended with the following:

```solidity
interface ITIP403Registry {
    // ... existing interface ...

    // =========================================================================
    //                      Complex Policy Creation
    // =========================================================================

    /// @notice Creates a new immutable complex policy
    /// @param senderPolicyId Policy ID to check for senders
    /// @param recipientPolicyId Policy ID to check for recipients
    /// @param defaultPolicyId Policy ID for legacy isAuthorized() calls
    /// @return newPolicyId ID of the newly created complex policy
    /// @dev All three policy IDs must reference existing simple policies (not complex).
    /// Complex policies are immutable - they cannot be modified after creation.
    /// Emits ComplexPolicyCreated event.
    function createComplexPolicy(
        uint64 senderPolicyId,
        uint64 recipientPolicyId,
        uint64 defaultPolicyId
    ) external returns (uint64 newPolicyId);

    // =========================================================================
    //                      Sender/Recipient Authorization
    // =========================================================================

    /// @notice Checks if a user is authorized as a sender under the given policy
    /// @param policyId Policy ID to check against
    /// @param user Address to check
    /// @return True if authorized to send, false otherwise
    /// @dev For simple policies: equivalent to isAuthorized()
    /// For complex policies: checks against the senderPolicyId
    function isAuthorizedSender(uint64 policyId, address user) external view returns (bool);

    /// @notice Checks if a user is authorized as a recipient under the given policy
    /// @param policyId Policy ID to check against
    /// @param user Address to check
    /// @return True if authorized to receive, false otherwise
    /// @dev For simple policies: equivalent to isAuthorized()
    /// For complex policies: checks against the recipientPolicyId
    function isAuthorizedRecipient(uint64 policyId, address user) external view returns (bool);

    // =========================================================================
    //                      Complex Policy Queries
    // =========================================================================

    /// @notice Returns the constituent policy IDs for a complex policy
    /// @param policyId ID of the complex policy to query
    /// @return senderPolicyId Policy ID for sender checks
    /// @return recipientPolicyId Policy ID for recipient checks
    /// @return defaultPolicyId Policy ID for legacy isAuthorized() calls
    /// @dev Reverts if policyId is not a complex policy
    function complexPolicyData(uint64 policyId) external view returns (
        uint64 senderPolicyId,
        uint64 recipientPolicyId,
        uint64 defaultPolicyId
    );

    // =========================================================================
    //                      Events
    // =========================================================================

    /// @notice Emitted when a new complex policy is created
    /// @param policyId ID of the newly created complex policy
    /// @param creator Address that created the policy
    /// @param senderPolicyId Policy ID for sender checks
    /// @param recipientPolicyId Policy ID for recipient checks
    /// @param defaultPolicyId Policy ID for legacy isAuthorized() calls
    event ComplexPolicyCreated(
        uint64 indexed policyId,
        address indexed creator,
        uint64 senderPolicyId,
        uint64 recipientPolicyId,
        uint64 defaultPolicyId
    );

    // =========================================================================
    //                      Errors
    // =========================================================================

    /// @notice The referenced policy is not a simple policy
    error PolicyNotSimple();

    /// @notice The referenced policy does not exist
    error PolicyDoesNotExist();
}
```

## Authorization Logic

### isAuthorizedSender

```solidity
function isAuthorizedSender(uint64 policyId, address user) external view returns (bool) {
    PolicyData memory data = policyData[policyId];
    
    if (data.policyType == PolicyType.COMPLEX) {
        ComplexPolicyData memory complex = complexPolicyData[policyId];
        return isAuthorized(complex.senderPolicyId, user);
    }
    
    // For simple policies, sender authorization equals general authorization
    return isAuthorized(policyId, user);
}
```

### isAuthorizedRecipient

```solidity
function isAuthorizedRecipient(uint64 policyId, address user) external view returns (bool) {
    PolicyData memory data = policyData[policyId];
    
    if (data.policyType == PolicyType.COMPLEX) {
        ComplexPolicyData memory complex = complexPolicyData[policyId];
        return isAuthorized(complex.recipientPolicyId, user);
    }
    
    // For simple policies, recipient authorization equals general authorization
    return isAuthorized(policyId, user);
}
```

### isAuthorized (updated)

The existing `isAuthorized` function is updated to handle complex policies:

```solidity
function isAuthorized(uint64 policyId, address user) external view returns (bool) {
    if (policyId < 2) {
        return policyId == 1; // 0 = reject, 1 = allow
    }
    
    PolicyData memory data = policyData[policyId];
    
    if (data.policyType == PolicyType.COMPLEX) {
        ComplexPolicyData memory complex = complexPolicyData[policyId];
        return isAuthorized(complex.defaultPolicyId, user);
    }
    
    return data.policyType == PolicyType.WHITELIST
        ? policySet[policyId][user]
        : !policySet[policyId][user];
}
```

## TIP-20 Integration

TIP-20 tokens MUST be updated to use the new sender/recipient authorization functions:

### Transfer Authorization

The `transferAuthorized` modifier is updated:

```solidity
modifier transferAuthorized(address from, address to) {
    uint64 policyId = transferPolicyId;
    
    if (!TIP403_REGISTRY.isAuthorizedSender(policyId, from)) {
        revert PolicyForbids();
    }
    if (!TIP403_REGISTRY.isAuthorizedRecipient(policyId, to)) {
        revert PolicyForbids();
    }
    _;
}
```

### Mint Operations

Mint operations check only the recipient:

```solidity
function _mint(address to, uint256 amount) internal {
    if (!TIP403_REGISTRY.isAuthorizedRecipient(transferPolicyId, to)) {
        revert PolicyForbids();
    }
    // ... mint logic
}
```

### Burn Blocked Operations

The `burnBlocked` function checks sender authorization to verify the address is blocked:

```solidity
function burnBlocked(address from, uint256 amount) external {
    require(hasRole(BURN_BLOCKED_ROLE, msg.sender));
    
    // Only allow burning from addresses blocked from sending
    if (TIP403_REGISTRY.isAuthorizedSender(transferPolicyId, from)) {
        revert PolicyForbids();
    }
    // ... burn logic
}
```

### System Transfer From

The `systemTransferFrom` function uses sender/recipient checks:

```solidity
function systemTransferFrom(address from, address to, uint256 amount) external returns (bool) {
    require(isSystemCaller(msg.sender));
    
    if (!TIP403_REGISTRY.isAuthorizedSender(transferPolicyId, from)) {
        revert PolicyForbids();
    }
    if (!TIP403_REGISTRY.isAuthorizedRecipient(transferPolicyId, to)) {
        revert PolicyForbids();
    }
    
    _transfer(from, to, amount);
    return true;
}
```

## Immutability

Complex policies are immutable once created. There is no `modifyComplexPolicy` function. To change policy behavior, token issuers must:

1. Create a new complex policy with the desired configuration
2. Update the token's `transferPolicyId` to the new policy

This design:
- Simplifies implementation and reduces attack surface
- Allows future extensions without breaking existing policies
- Enables clear auditability of policy changes via on-chain events

## Backward Compatibility

This TIP is fully backward compatible:

- Existing simple policies continue to work unchanged
- Tokens using simple policies will see identical behavior (since `isAuthorizedSender` and `isAuthorizedRecipient` delegate to `isAuthorized` for simple policies)
- The existing `isAuthorized` function continues to work for both simple and complex policies

---

# Invariants

1. **Simple Policy Constraint**: All three policy IDs in a complex policy MUST reference simple policies (WHITELIST or BLACKLIST). Complex policies cannot reference other complex policies.

2. **Immutability**: Once created, a complex policy's constituent policy IDs cannot be changed. The complex policy itself has no admin.

3. **Existence Check**: `createComplexPolicy` MUST revert if any of the three referenced policy IDs do not exist.

4. **Delegation Correctness**: For simple policies, `isAuthorizedSender(p, u)` MUST equal `isAuthorizedRecipient(p, u)` MUST equal `isAuthorized(p, u)`.

5. **Built-in Policy Compatibility**: Complex policies MAY reference built-in policies (0 = always-reject, 1 = always-allow) as any of their constituent policies.

## Test Cases

1. **Simple policy equivalence**: Verify that for simple policies, all three authorization functions return the same result.

2. **Complex policy creation**: Verify that complex policies can be created with valid simple policy references.

3. **Invalid creation**: Verify that `createComplexPolicy` reverts when referencing non-existent policies or complex policies.

4. **Sender/recipient differentiation**: Verify that a complex policy with different sender/recipient policies correctly authorizes asymmetric transfers.

5. **Default policy**: Verify that `isAuthorized` on a complex policy delegates to the defaultPolicyId.

6. **TIP-20 mint**: Verify that mints only check recipient authorization.

7. **TIP-20 burnBlocked**: Verify that burnBlocked checks sender authorization (and allows burning from blocked senders).

8. **Immutability**: Verify that there is no way to modify a complex policy after creation.
