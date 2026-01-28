---
id: TIP-XXXX
title: Account-Level Transfer Policies
description: Extends TIP-403 to allow individual accounts to set their own send, receive, and token receive policies, enabling regulated entities to enforce account-level compliance controls.
authors: Mallesh Pai @malleshpai
status: Draft
related: TIP-403
---

# TIP-XXXX: Account-Level Transfer Policies

## Abstract

This TIP extends the TIP-403 transfer policy system to support account-level policies. Currently, TIP-403 policies are set at the token level—each TIP-20 token has a single `transferPolicyId` that governs all transfers. This proposal adds the ability for individual accounts to set their own policies across three dimensions:

1. **Send policy** (counterparty filter): Control who this account can send to
2. **Receive policy** (counterparty filter): Control who can send to this account
3. **Token receive policy** (token filter): Control which tokens this account can receive

This enables regulated entities (banks, exchanges, etc.) to enforce compliance controls at the account level, independent of and in addition to token-level policies.

## Motivation

Regulated entities operating on Tempo need the ability to control their transactions, regardless of what policies the token issuer has set. For example:

- A bank may want to only receive funds from KYC'd addresses (whitelist on receives)
- An exchange may need to block sends to sanctioned addresses (blacklist on sends)
- A custodian may require that all incoming and outgoing transfers pass through approved counterparties
- **A regulated entity may only want to hold approved stablecoins** (whitelist on tokens)
- **An institution may need to block specific tokens** known to be associated with illicit activity (blacklist on tokens)

The current TIP-403 system only supports token-level policies: the token issuer sets a policy, and all transfers of that token must satisfy it. This does not allow individual account holders to impose their own restrictions on counterparties or tokens.

### Design Goals

1. **Minimal changes**: Reuse the existing TIP-403 policy infrastructure (policy creation, membership management, isAuthorized logic)
2. **Composable**: Account-level policies AND token-level policies must both pass (neither can override the other)
3. **Opt-in**: Accounts without policies have zero overhead beyond two storage reads
4. **Gas-efficient**: Minimize hot-path gas cost for transfers
5. **State-efficient**: Minimize storage overhead per opted-in account

### Alternatives Considered

1. **Per-token account policies**: Store account policies per token. Rejected due to state bloat (N accounts × M tokens).
2. **Merkle tree policies**: Store policy membership as merkle roots, require proofs at transfer time. Rejected due to UX burden (users must obtain and submit proofs).
3. **Bloom filter policies**: Use probabilistic data structures. Rejected due to false positive risk (security hole for whitelists).

---

# Specification

## Overview

The TIP-403 Registry is extended with a new mapping that allows any account to set its own policies:

- **Send policy**: Controls which addresses this account can send tokens to (counterparty filter)
- **Receive policy**: Controls which addresses can send tokens to this account (counterparty filter)  
- **Token receive policy**: Controls which tokens this account can receive (token filter)

These policies are checked on every TIP-20 transfer and mint in addition to the existing token-level policy check. Since TIP-20 is implemented as a precompile, a hardfork that adds this functionality will apply to all tokens automatically.

## Storage Layout

### New Storage

```solidity
mapping(address => uint256) public accountPolicies;
```

The `accountPolicies` mapping packs policy IDs and their types into a single 256-bit storage slot:

| Bits | Field | Description |
|------|-------|-------------|
| 0–63 | `sendPolicyId` | Policy checked when this account is the sender (counterparty filter) |
| 64–71 | `sendPolicyType` | Type of send policy (0 = whitelist, 1 = blacklist) |
| 72–135 | `receivePolicyId` | Policy checked when this account is the receiver (counterparty filter) |
| 136–143 | `receivePolicyType` | Type of receive policy (0 = whitelist, 1 = blacklist) |
| 144–207 | `tokenReceivePolicyId` | Policy checked on tokens being received (token filter) |
| 208–215 | `tokenReceivePolicyType` | Type of token receive policy (0 = whitelist, 1 = blacklist) |
| 216–255 | Reserved | For future use (must be 0) |

A policy ID of `0` means "no policy" (always authorized). This differs from token-level TIP-403 semantics where policy ID 0 is "always-reject"; for account-level policies, ID 0 means "no account policy set" (defer to token policy only).

### Why Embed Policy Type?

TIP-403 policy types are **immutable** — set at creation and never changed. By embedding the policy type in the account storage, we avoid an SLOAD to `policyData[policyId]` on every authorization check, saving ~2,100 gas per check.

When `setAccountPolicies` is called, the policy type is read from the registry once and cached in the account's storage. Since policy types cannot change, this cached value remains valid forever.

### Encoding

```solidity
function encodeAccountPolicies(
    uint64 sendPolicyId, 
    PolicyType sendPolicyType,
    uint64 receivePolicyId, 
    PolicyType receivePolicyType,
    uint64 tokenReceivePolicyId,
    PolicyType tokenReceivePolicyType
) internal pure returns (uint256) {
    return uint256(sendPolicyId) 
        | (uint256(uint8(sendPolicyType)) << 64)
        | (uint256(receivePolicyId) << 72)
        | (uint256(uint8(receivePolicyType)) << 136)
        | (uint256(tokenReceivePolicyId) << 144)
        | (uint256(uint8(tokenReceivePolicyType)) << 208);
}

function decodeAccountPolicies(uint256 packed) 
    internal pure returns (
        uint64 sendPolicyId, 
        PolicyType sendPolicyType,
        uint64 receivePolicyId, 
        PolicyType receivePolicyType,
        uint64 tokenReceivePolicyId,
        PolicyType tokenReceivePolicyType
    ) 
{
    sendPolicyId = uint64(packed);
    sendPolicyType = PolicyType(uint8(packed >> 64));
    receivePolicyId = uint64(packed >> 72);
    receivePolicyType = PolicyType(uint8(packed >> 136));
    tokenReceivePolicyId = uint64(packed >> 144);
    tokenReceivePolicyType = PolicyType(uint8(packed >> 208));
}
```

## Interface Extensions

The following functions are added to the TIP-403 Registry:

```solidity
/// @notice Sets the send, receive, and token receive policies for the caller's account
/// @param sendPolicyId Policy to check when caller sends (0 = no policy)
/// @param receivePolicyId Policy to check when caller receives (0 = no policy)
/// @param tokenReceivePolicyId Policy to check on tokens being received (0 = no policy)
/// @dev All policies must exist (or be 0). Caller can only set their own policies.
/// @dev Policy types are cached from the registry at set time (immutable, so always valid).
function setAccountPolicies(
    uint64 sendPolicyId, 
    uint64 receivePolicyId, 
    uint64 tokenReceivePolicyId
) external;

/// @notice Returns all policies for an account
/// @param account The account to query
/// @return sendPolicyId The policy checked when account sends (0 = no policy)
/// @return sendPolicyType The type of the send policy
/// @return receivePolicyId The policy checked when account receives (0 = no policy)
/// @return receivePolicyType The type of the receive policy
/// @return tokenReceivePolicyId The policy checked on tokens being received (0 = no policy)
/// @return tokenReceivePolicyType The type of the token receive policy
function getAccountPolicies(address account) 
    external view returns (
        uint64 sendPolicyId, 
        PolicyType sendPolicyType,
        uint64 receivePolicyId, 
        PolicyType receivePolicyType,
        uint64 tokenReceivePolicyId,
        PolicyType tokenReceivePolicyType
    );

/// @notice Checks if a transfer is authorized under account-level policies
/// @param from The sender address
/// @param to The receiver address
/// @param token The token contract address being transferred
/// @return True if the transfer is authorized under all account policies
/// @dev Returns true if:
///      - from's sendPolicy is 0 OR to is authorized under from's sendPolicy
///      - to's receivePolicy is 0 OR from is authorized under to's receivePolicy
///      - to's tokenReceivePolicy is 0 OR token is authorized under to's tokenReceivePolicy
function isTransferAuthorized(address from, address to, address token) external view returns (bool);
```

### Events

```solidity
/// @notice Emitted when an account updates its policies
/// @param account The account that updated its policies
/// @param sendPolicyId The new send policy (0 = no policy)
/// @param receivePolicyId The new receive policy (0 = no policy)
/// @param tokenReceivePolicyId The new token receive policy (0 = no policy)
event AccountPoliciesUpdated(
    address indexed account, 
    uint64 sendPolicyId, 
    uint64 receivePolicyId,
    uint64 tokenReceivePolicyId
);
```

### Errors

```solidity
/// @notice Error when setting a policy that does not exist
error PolicyNotFound();
```

## Authorization Logic

### setAccountPolicies

```solidity
function setAccountPolicies(
    uint64 sendPolicyId, 
    uint64 receivePolicyId,
    uint64 tokenReceivePolicyId
) external {
    PolicyType sendPolicyType;
    PolicyType receivePolicyType;
    PolicyType tokenReceivePolicyType;
    
    // Validate and cache send policy type
    if (sendPolicyId != 0) {
        if (!policyExists(sendPolicyId)) {
            revert PolicyNotFound();
        }
        (sendPolicyType, ) = policyData(sendPolicyId);
    }
    
    // Validate and cache receive policy type
    if (receivePolicyId != 0) {
        if (!policyExists(receivePolicyId)) {
            revert PolicyNotFound();
        }
        (receivePolicyType, ) = policyData(receivePolicyId);
    }
    
    // Validate and cache token receive policy type
    if (tokenReceivePolicyId != 0) {
        if (!policyExists(tokenReceivePolicyId)) {
            revert PolicyNotFound();
        }
        (tokenReceivePolicyType, ) = policyData(tokenReceivePolicyId);
    }
    
    // Store packed policy data (IDs + types)
    accountPolicies[msg.sender] = encodeAccountPolicies(
        sendPolicyId, 
        sendPolicyType,
        receivePolicyId, 
        receivePolicyType,
        tokenReceivePolicyId,
        tokenReceivePolicyType
    );
    
    emit AccountPoliciesUpdated(msg.sender, sendPolicyId, receivePolicyId, tokenReceivePolicyId);
}
```

### getAccountPolicies

```solidity
function getAccountPolicies(address account) 
    external view returns (
        uint64 sendPolicyId, 
        PolicyType sendPolicyType,
        uint64 receivePolicyId, 
        PolicyType receivePolicyType,
        uint64 tokenReceivePolicyId,
        PolicyType tokenReceivePolicyType
    ) 
{
    return decodeAccountPolicies(accountPolicies[account]);
}
```

### isTransferAuthorized

This function checks account-level policies **without** reading `policyData` — the policy type is embedded in the account storage.

```solidity
function isTransferAuthorized(address from, address to, address token) external view returns (bool) {
    // Decode sender's policies (includes cached policy type)
    (
        uint64 fromSendPolicy, 
        PolicyType fromSendType,
        ,
        ,
        ,
    ) = decodeAccountPolicies(accountPolicies[from]);
    
    // Check sender's send policy: "who can I send to?"
    if (fromSendPolicy != 0) {
        bool inSet = policySet[fromSendPolicy][to];
        bool authorized = (fromSendType == PolicyType.WHITELIST) ? inSet : !inSet;
        if (!authorized) {
            return false;
        }
    }
    
    // Decode receiver's policies (includes cached policy type)
    (
        ,
        ,
        uint64 toReceivePolicy, 
        PolicyType toReceiveType,
        uint64 toTokenReceivePolicy,
        PolicyType toTokenReceiveType
    ) = decodeAccountPolicies(accountPolicies[to]);
    
    // Check receiver's receive policy: "who can send to me?"
    if (toReceivePolicy != 0) {
        bool inSet = policySet[toReceivePolicy][from];
        bool authorized = (toReceiveType == PolicyType.WHITELIST) ? inSet : !inSet;
        if (!authorized) {
            return false;
        }
    }
    
    // Check receiver's token receive policy: "which tokens can I receive?"
    if (toTokenReceivePolicy != 0) {
        bool inSet = policySet[toTokenReceivePolicy][token];
        bool authorized = (toTokenReceiveType == PolicyType.WHITELIST) ? inSet : !inSet;
        if (!authorized) {
            return false;
        }
    }
    
    return true;
}
```

## Integration with TIP-20

The `transferAuthorized` modifier in TIP-20 is updated to check both token-level and account-level policies:

```solidity
modifier transferAuthorized(address from, address to) {
    // Token-level policy check (existing behavior)
    if (
        !TIP403_REGISTRY.isAuthorized(transferPolicyId, from)
            || !TIP403_REGISTRY.isAuthorized(transferPolicyId, to)
    ) revert PolicyForbids();
    
    // Account-level policy check (new) - includes counterparty and token filtering
    if (!TIP403_REGISTRY.isTransferAuthorized(from, to, address(this))) {
        revert PolicyForbids();
    }
    _;
}
```

### Affected Functions

The following TIP-20 functions use the `transferAuthorized` modifier and will now also check account-level policies:

- `transfer(address to, uint256 amount)`
- `transferFrom(address from, address to, uint256 amount)`
- `transferWithMemo(address to, uint256 amount, bytes32 memo)`
- `transferFromWithMemo(address from, address to, uint256 amount, bytes32 memo)`
- `systemTransferFrom(address from, address to, uint256 amount)`

### Mint Behavior

Minting checks both the token-level policy AND the recipient's account-level policies (receive policy and token receive policy). This is important because TIP-20 creation is permissionless — anyone can create a token and attempt to mint to any address. A regulated entity can block:

1. **Unauthorized issuers** via receive policy (counterparty filter)
2. **Unwanted tokens** via token receive policy (token filter)

For account-level policy purposes, the **issuer** (msg.sender calling mint) is treated as the "from" address.

```solidity
function _mint(address to, uint256 amount) internal {
    // Token-level policy check (existing, unchanged)
    if (!TIP403_REGISTRY.isAuthorized(transferPolicyId, to)) {
        revert PolicyForbids();
    }
    
    // Account-level policy check (new)
    // Treat issuer (msg.sender) as "from", address(this) as the token
    if (!TIP403_REGISTRY.isTransferAuthorized(msg.sender, to, address(this))) {
        revert PolicyForbids();
    }
    
    // ... rest of mint logic
}
```

**Rationale:** A regulated entity (e.g., bank) may only want to:
- Receive tokens from approved issuers (receive policy)
- Hold approved tokens like USDC/EURC (token receive policy)

Without these checks, anyone could spam regulated accounts with unwanted tokens from malicious TIP-20 contracts.

### Burn Behavior

Burning does not involve a counterparty, so account-level policies are not applicable. The existing behavior is unchanged.

### Fee Transfers

Fee transfers via `transferFeePreTx` and `transferFeePostTx` do NOT check account-level policies. Rationale: fee transfers are system operations between the user and the FeeManager precompile, and should not be blocked by user-configured policies.

## Gas Cost Analysis

### Per-Transfer Overhead (Incremental)

| Scenario | Additional Gas |
|----------|----------------|
| Neither account has policies set | ~4,200 gas (2 SLOADs for accountPolicies) |
| Sender has send policy | ~6,300 gas (+1 policySet SLOAD) |
| Receiver has receive policy | ~6,300 gas (+1 policySet SLOAD) |
| Receiver has token receive policy | ~6,300 gas (+1 policySet SLOAD) |
| Receiver has receive + token policy | ~8,400 gas (+2 policySet SLOADs) |
| Both parties, all policies | ~12,600 gas (+4 policySet SLOADs) |

### Breakdown

1. `accountPolicies[from]` SLOAD: ~2,100 gas
2. `accountPolicies[to]` SLOAD: ~2,100 gas
3. If `fromSendPolicy != 0`: `policySet[fromSendPolicy][to]` SLOAD: ~2,100 gas
4. If `toReceivePolicy != 0`: `policySet[toReceivePolicy][from]` SLOAD: ~2,100 gas
5. If `toTokenReceivePolicy != 0`: `policySet[toTokenReceivePolicy][token]` SLOAD: ~2,100 gas

**Note on optimization**: Because we embed the policy type in `accountPolicies`, we do NOT need to read `policyData[policyId]` during authorization. This saves ~2,100 gas per policy check compared to a naive implementation.

The baseline ~4,200 gas is incurred on ALL transfers, even when no accounts have policies set. This is the cost of reading the two `accountPolicies` slots to check if policies exist. Token receive policy adds no baseline cost (packed in same slot).

### State Creation Costs

| Operation | Gas Cost |
|-----------|----------|
| First call to `setAccountPolicies` (creates slot) | 250,000 gas |
| Subsequent updates to account policies | 5,000 gas |
| Adding address to policy membership | 250,000 gas (first time) |
| Updating address in policy membership | 5,000 gas |

## Shared Policy Semantics

An account can reference **any existing TIP-403 policy**, including policies administered by other accounts. This enables shared compliance lists:

```solidity
// Consortium admin creates a shared "approved counterparties" policy
uint64 sharedPolicy = TIP403_REGISTRY.createPolicy(consortiumAdmin, PolicyType.WHITELIST);

// Multiple banks use the same policy
TIP403_REGISTRY.setAccountPolicies(0, sharedPolicy, 0);  // Bank A: receives only from approved
TIP403_REGISTRY.setAccountPolicies(0, sharedPolicy, 0);  // Bank B: uses same list
TIP403_REGISTRY.setAccountPolicies(sharedPolicy, 0, 0);  // Bank C: sends only to approved
```

**Important considerations:**

1. **Admin controls membership**: The policy admin can add/remove addresses from the policy. All accounts using that policy are affected.
2. **No notification on changes**: Accounts using a shared policy are not notified when membership changes.
3. **Policy type is cached**: The policy type (whitelist/blacklist) is cached when `setAccountPolicies` is called. Since policy types are immutable, this is always correct.
4. **Self-administered policies**: For full control, accounts can create their own policies and serve as the admin.

This design is intentional: shared policies reduce state duplication for entities with common compliance requirements (e.g., banks in the same regulatory jurisdiction).

## Example Usage

### Regulated Entity: Whitelist Receives

A bank wants to only receive funds from KYC'd addresses:

```solidity
// 1. Bank creates a whitelist policy (or reuses existing)
uint64 kycWhitelist = TIP403_REGISTRY.createPolicy(bankAdmin, PolicyType.WHITELIST);

// 2. Bank adds KYC'd addresses to the whitelist
TIP403_REGISTRY.modifyPolicyWhitelist(kycWhitelist, kycAddress1, true);
TIP403_REGISTRY.modifyPolicyWhitelist(kycWhitelist, kycAddress2, true);

// 3. Bank sets receive policy (send policy = 0, token policy = 0)
TIP403_REGISTRY.setAccountPolicies(0, kycWhitelist, 0);

// Result: Bank can only receive from addresses on kycWhitelist
// Bank can send to anyone (subject to token-level policy)
// Bank can receive any token
```

### Regulated Entity: Blacklist Sends

An exchange wants to block sends to sanctioned addresses:

```solidity
// 1. Exchange creates a blacklist policy
uint64 sanctionsBlacklist = TIP403_REGISTRY.createPolicy(exchangeAdmin, PolicyType.BLACKLIST);

// 2. Exchange adds sanctioned addresses
TIP403_REGISTRY.modifyPolicyBlacklist(sanctionsBlacklist, sanctionedAddr, true);

// 3. Exchange sets send policy (receive policy = 0, token policy = 0)
TIP403_REGISTRY.setAccountPolicies(sanctionsBlacklist, 0, 0);

// Result: Exchange cannot send to addresses on sanctionsBlacklist
// Exchange can receive from anyone (subject to token-level policy)
```

### Combined: Whitelist Both Directions

A custodian wants to only transact with approved counterparties:

```solidity
// 1. Custodian creates a whitelist for approved counterparties
uint64 approvedList = TIP403_REGISTRY.createPolicy(custodianAdmin, PolicyType.WHITELIST);

// 2. Add approved addresses
TIP403_REGISTRY.modifyPolicyWhitelist(approvedList, approvedAddr1, true);
TIP403_REGISTRY.modifyPolicyWhitelist(approvedList, approvedAddr2, true);

// 3. Set same policy for both send and receive (no token restriction)
TIP403_REGISTRY.setAccountPolicies(approvedList, approvedList, 0);

// Result: Custodian can only send to AND receive from approved addresses
```

### Token Whitelist: Only Approved Stablecoins

A regulated institution only wants to hold approved stablecoins:

```solidity
// 1. Institution creates a whitelist for approved tokens
uint64 approvedTokens = TIP403_REGISTRY.createPolicy(complianceAdmin, PolicyType.WHITELIST);

// 2. Add approved token contract addresses
TIP403_REGISTRY.modifyPolicyWhitelist(approvedTokens, USDC_ADDRESS, true);
TIP403_REGISTRY.modifyPolicyWhitelist(approvedTokens, EURC_ADDRESS, true);
TIP403_REGISTRY.modifyPolicyWhitelist(approvedTokens, PATH_USD_ADDRESS, true);

// 3. Set token receive policy (no counterparty restrictions in this example)
TIP403_REGISTRY.setAccountPolicies(0, 0, approvedTokens);

// Result: Institution can only receive USDC, EURC, and pathUSD
// Any attempt to mint or transfer other tokens to this account will revert
```

### Token Blacklist: Block Specific Tokens

An account wants to block tokens associated with known scams:

```solidity
// 1. Create a blacklist for blocked tokens
uint64 blockedTokens = TIP403_REGISTRY.createPolicy(accountOwner, PolicyType.BLACKLIST);

// 2. Add blocked token contract addresses
TIP403_REGISTRY.modifyPolicyBlacklist(blockedTokens, SCAM_TOKEN_ADDRESS, true);

// 3. Set token receive policy
TIP403_REGISTRY.setAccountPolicies(0, 0, blockedTokens);

// Result: Account blocks SCAM_TOKEN but can receive all other tokens
```

### Full Compliance Setup

A fully regulated entity with all controls:

```solidity
// Create policies
uint64 kycCounterparties = TIP403_REGISTRY.createPolicy(admin, PolicyType.WHITELIST);
uint64 approvedTokens = TIP403_REGISTRY.createPolicy(admin, PolicyType.WHITELIST);

// Populate policies
TIP403_REGISTRY.modifyPolicyWhitelist(kycCounterparties, approvedAddr1, true);
TIP403_REGISTRY.modifyPolicyWhitelist(approvedTokens, USDC_ADDRESS, true);

// Set all three policies
TIP403_REGISTRY.setAccountPolicies(kycCounterparties, kycCounterparties, approvedTokens);

// Result:
// - Can only send to KYC'd addresses
// - Can only receive from KYC'd addresses  
// - Can only receive approved tokens (USDC in this case)
```

---

# Invariants

The following invariants must always hold:

1. **Account Sovereignty**: Only an account itself can set its own account-level policies via `setAccountPolicies`. No other account can modify another account's policies.

2. **Policy Existence**: `setAccountPolicies` MUST revert with `PolicyNotFound()` if any of `sendPolicyId`, `receivePolicyId`, or `tokenReceivePolicyId` is non-zero and does not correspond to an existing policy.

3. **Zero Policy Semantics**: A policy ID of `0` in `accountPolicies` MUST be interpreted as "no account-level policy" (always authorized at the account level). This differs from the token-level semantics where policy ID 0 is "always-reject".

4. **Composable Authorization**: A transfer is authorized if and only if ALL of the following are true:
   - Token-level policy authorizes `from`: `isAuthorized(token.transferPolicyId, from)`
   - Token-level policy authorizes `to`: `isAuthorized(token.transferPolicyId, to)`
   - Account-level send policy authorizes `to`: `from.sendPolicyId == 0 OR isAuthorized(from.sendPolicyId, to)`
   - Account-level receive policy authorizes `from`: `to.receivePolicyId == 0 OR isAuthorized(to.receivePolicyId, from)`
   - Account-level token receive policy authorizes `token`: `to.tokenReceivePolicyId == 0 OR isAuthorized(to.tokenReceivePolicyId, token)`

5. **Mint Policy Check**: Minting operations MUST check the recipient's account-level receive policy (with issuer as "from") AND token receive policy (with the minted token). This protects regulated accounts from spam tokens and unauthorized issuers.

6. **Burn Exemption**: Burn operations MUST NOT check account-level policies.

7. **Fee Transfer Exemption**: Fee transfers (`transferFeePreTx`, `transferFeePostTx`) MUST NOT check account-level policies.

8. **Storage Efficiency**: Account policies MUST be stored in a single storage slot per account (packed policy IDs and types into 256 bits).

9. **Gas Consistency**: Reading `accountPolicies[address]` for a non-existent entry MUST return 0 (interpreted as no policies set), incurring only the cold SLOAD cost (~2,100 gas).

10. **Cached Policy Type Validity**: The policy type cached in `accountPolicies` MUST always match the policy type in `policyData[policyId]`. This is guaranteed because TIP-403 policy types are immutable after creation.

## Critical Test Cases

### Counterparty Policies (Send/Receive)

1. **Basic send policy**: Account with send policy can only send to addresses authorized under that policy
2. **Basic receive policy**: Account with receive policy can only receive from addresses authorized under that policy
3. **Combined counterparty policies**: Account with both send and receive policies enforces both on all transfers
4. **No policy (default)**: Account without policies set can send/receive freely (subject to token-level policy)
5. **Policy ID 0**: Setting any policy to 0 removes that restriction
6. **Token-level + account-level policies**: Both token-level AND account-level policies must pass
7. **Self-transfer**: Account with policies can transfer to itself if authorized under its own policies
8. **Whitelist send policy**: Account with whitelist send policy can only send to whitelisted addresses
9. **Blacklist send policy**: Account with blacklist send policy cannot send to blacklisted addresses
10. **Whitelist receive policy**: Account with whitelist receive policy can only receive from whitelisted addresses
11. **Blacklist receive policy**: Account with blacklist receive policy cannot receive from blacklisted addresses
12. **Cross-account policies**: A sends to B where A has send policy and B has receive policy; both must authorize
13. **Bidirectional transfer**: A and B both have policies; transfer A→B checks A's send + B's receive; transfer B→A checks B's send + A's receive

### Token Receive Policy

14. **Basic token whitelist**: Account with token whitelist can only receive tokens on the list
15. **Basic token blacklist**: Account with token blacklist cannot receive tokens on the list
16. **Token policy on transfer**: Transfer of non-whitelisted token to account with token whitelist reverts
17. **Token policy on mint**: Mint of non-whitelisted token to account with token whitelist reverts
18. **Token policy + counterparty policy**: All three policies (send, receive, token) must pass for transfer to succeed
19. **No token policy**: Account without token receive policy can receive any token

### Mint/Burn/Fee Behavior

20. **Mint receive policy**: Minting checks issuer against recipient's receive policy
21. **Mint token policy**: Minting checks token against recipient's token receive policy
22. **Burn exemption**: Burning from an account with send policy does NOT check the send policy
23. **Fee transfer exemption**: Fee transfers bypass all account-level policies

### Policy Management

24. **Policy update**: Account can update its policies; new policies take effect immediately
25. **Invalid policy**: Setting non-existent policy ID reverts with `PolicyNotFound()`
26. **Shared policy**: Multiple accounts using same policy all see membership changes
27. **Self-administered policy**: Account creates own policy and uses it (full control)

### Storage and Gas

28. **Storage packing**: Verify all six fields (3 IDs + 3 types) are correctly packed/unpacked in 256 bits
29. **Gas measurement**: Verify gas costs match expected values for each scenario
30. **Cached policy type**: Verify cached policy type matches registry policy type
31. **Baseline gas**: Accounts without policies still incur ~4,200 gas overhead for accountPolicies reads
