# TLS Email Ownership Precompile

Proves email ownership on-chain using TLSNotary attestations.

**Address:** `0x714E000000000000000000000000000000000000`

## How It Works

1. **Off-chain:** User runs a TLSNotary session against Google's userinfo API (`https://www.googleapis.com/oauth2/v3/userinfo`), which returns their email
2. **Notary attestation:** A trusted Notary verifies the TLS proof and signs a compact attestation binding the email to the user's Ethereum address
3. **On-chain:** User submits the attestation + response body to this precompile, which verifies the Notary's secp256k1 signature and stores the email claim

## Attestation Digest

The Notary signs `keccak256(abi.encodePacked(...))` over:

```
"TempoEmailAttestationV1"     // domain separator
subject                       // user's Ethereum address (20 bytes)
keccak256(serverName)          // "www.googleapis.com"
keccak256(endpoint)            // "/oauth2/v3/userinfo"
responseBodyHash               // keccak256(responseBody)
emailHash                      // keccak256(email)
notaryKeyId                    // 32 bytes
```

## E2E Flow (localnet)

### 1. Start localnet

```bash
just localnet
export ETH_RPC_URL="http://localhost:8545"
```

### 2. Fund a wallet

```bash
WALLET=$(cast wallet new --json)
PK=$(echo "$WALLET" | jq -r '.[0].private_key')
ADDR=$(echo "$WALLET" | jq -r '.[0].address')
cast rpc tempo_fundAddress $ADDR
sleep 2
```

### 3. Initialize the precompile (genesis does this, but for manual testing)

The precompile is initialized during genesis with the chain owner. The owner can then register trusted Notary keys.

### 4. Register a Notary key (owner only)

```bash
# Generate a Notary key (for testing only)
NOTARY_WALLET=$(cast wallet new --json)
NOTARY_PK=$(echo "$NOTARY_WALLET" | jq -r '.[0].private_key')
NOTARY_ADDR=$(echo "$NOTARY_WALLET" | jq -r '.[0].address')
NOTARY_KEY_ID="0x0000000000000000000000000000000000000000000000000000000000000001"

# Owner registers the Notary (replace OWNER_PK with actual owner private key)
PRECOMPILE="0x714E000000000000000000000000000000000000"
cast send $PRECOMPILE "setNotaryKey(bytes32,address)" \
    $NOTARY_KEY_ID $NOTARY_ADDR \
    --private-key $OWNER_PK
```

### 5. Create and sign the attestation (off-chain)

The Notary creates the attestation after verifying the TLSNotary proof:

```bash
# The Google userinfo response body (from TLSNotary session)
RESPONSE_BODY='{"sub":"1234567890","email":"zygimantas@tempo.xyz","email_verified":true}'
RESPONSE_HASH=$(cast keccak "$RESPONSE_BODY")
EMAIL_HASH=$(cast keccak "zygimantas@tempo.xyz")
SERVER_NAME_HASH=$(cast keccak "www.googleapis.com")
ENDPOINT_HASH=$(cast keccak "/oauth2/v3/userinfo")

# Compute attestation digest
DIGEST=$(cast keccak $(cast abi-encode-packed \
    "string,address,bytes32,bytes32,bytes32,bytes32,bytes32" \
    "TempoEmailAttestationV1" \
    $ADDR \
    $SERVER_NAME_HASH \
    $ENDPOINT_HASH \
    $RESPONSE_HASH \
    $EMAIL_HASH \
    $NOTARY_KEY_ID))

# Notary signs the digest
SIG=$(cast wallet sign --private-key $NOTARY_PK $DIGEST)
```

### 6. Submit the attestation on-chain

```bash
# Parse signature components
R=$(echo $SIG | cut -c1-66)
S="0x$(echo $SIG | cut -c67-130)"
V=$(echo $SIG | cut -c131-132)

# Call verifyEmail
cast send $PRECOMPILE \
    "verifyEmail(bytes32,address,string,string,bytes,uint8,bytes32,bytes32)" \
    $NOTARY_KEY_ID \
    $ADDR \
    "www.googleapis.com" \
    "/oauth2/v3/userinfo" \
    $(cast --from-utf8 "$RESPONSE_BODY") \
    $V $R $S \
    --private-key $PK
```

### 7. Query the verified email

```bash
# Check if verified
cast call $PRECOMPILE "isVerified(address)(bool)" $ADDR

# Get the full claim
cast call $PRECOMPILE "getVerifiedEmail(address)((string,bytes32,uint64,bytes32))" $ADDR
```

## Interface

```solidity
interface ITLSEmailOwnership {
    // Verify email ownership
    function verifyEmail(bytes32 notaryKeyId, address subject, string serverName,
        string endpoint, bytes responseBody, uint8 v, bytes32 r, bytes32 s)
        external returns (string email);

    // Query verified emails
    function getVerifiedEmail(address user) external view returns (EmailClaim);
    function isVerified(address user) external view returns (bool);

    // Admin: Notary key management
    function setNotaryKey(bytes32 notaryKeyId, address notaryAddress) external;
    function removeNotaryKey(bytes32 notaryKeyId) external;
    function getNotaryKey(bytes32 notaryKeyId) external view returns (address);

    // User: revoke
    function revokeMyEmail() external;
}
```
