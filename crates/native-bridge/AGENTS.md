# Native Bridge

## BLS Variant

- Bridge uses **MinSig variant** (G2 public keys, G1 signatures) - same as consensus - enabling validators to reuse DKG shares for both consensus and bridge signing
- DST suffix indicates hash-to-curve target: `G1_XMD` for MinSig, `G2_XMD` for MinPk

### FinalizationBridge Signature Format

The FinalizationBridge contract verifies consensus finalization signatures directly. The signature format matches what commonware-consensus produces:

- **Namespace**: `TEMPO_FINALIZE` (consensus appends `_FINALIZE` to base namespace `TEMPO`)
- **Message**: The signed message is: `varint(len(namespace)) || namespace || proposal.encode()`
  - For namespace "TEMPO_FINALIZE" (14 bytes), varint is `0x0E`
  - Proposal contains: epoch (u64) + view (u64) + parent (u64) + payload (32 bytes = block hash)
- **DST**: `BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_POP_` (commonware standard for MinSig)

### MessageBridge Signature Format (separate signing)

MessageBridge uses bridge-specific signing (not consensus signatures):

- **DST**: `TEMPO_BRIDGE_BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_`
- **Message**: `keccak256(domain || sender || messageHash || originChainId || destinationChainId)`

## Tempo Block Header Format

- **Tempo uses a non-standard block header format**: `rlp([generalGasLimit, sharedGasLimit, timestampMillisPart, inner])` where `inner` is the standard Ethereum header
- The block hash is `keccak256(rlp(tempoHeader))` - not the inner Ethereum header
- `BlockHeaderDecoder.sol` parses this nested structure: `receiptsRoot` is at index 5 of the inner list (index 3 of outer)
- Tempo RPC returns extra fields: `mainBlockGeneralGasLimit`, `sharedGasLimit`, `timestampMillisPart` - must use raw RPC requests to fetch these (alloy types don't include them)

## EIP-2537 BLS Precompiles

- EIP-2537 (BLS12-381 precompiles at 0x0b-0x12) is live on Ethereum mainnet since the Pectra hardfork
- Anvil requires `--hardfork prague` flag to enable EIP-2537 precompiles for testing BLS contracts
- EIP-2537 uses padded format: G1=128 bytes (2×64), G2=256 bytes (4×64)
- bls-solidity library uses compact format: G1=96 bytes (2×48), G2=192 bytes (4×48)
- BLS12381.sol wrapper handles conversion between formats
- G1 generator point for test deployments (128 bytes uncompressed):
  ```
  0000000000000000000000000000000017f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb0000000000000000000000000000000008b3f481e3aaa0f1a09e30ed741d8ae4fcf5e095d5d00af600db18cb2c04b3edd03cc744a2888ae40caa232946c5e7e1
  ```

## Dependencies

- On-chain BLS verification uses [randa-mu/bls-solidity](https://github.com/randa-mu/bls-solidity) - implements MinSig with EIP-2537 precompiles
- Install Foundry deps with `forge install` from `contracts/` directory (not manual git clone)

## Testing

- E2E tests should use real contracts, not mocks - the MessageBridge bytecode is at `contracts/out/MessageBridge.sol/MessageBridge.bytecode.hex`
- Run `forge build` in `contracts/` to regenerate bytecode after Solidity changes
- Calls from contracts to TIP-20 precompiles require high gas limits (~5M) - 500k will cause silent reverts with no error data
- **TIP-1000 increases contract deployment gas costs on Tempo**: base deployment=500k, code deposit=1k/byte, account creation=250k. Use 2M+ gas for contract deploys, 500k+ for calls that create accounts
- **Anvil needs higher gas limit for large contracts**: Use `--gas-limit 100000000` when starting Anvil to deploy FinalizationBridge with libraries
- **Foundry EVM version must be `cancun`**: bls-solidity uses `mcopy` opcode which is not available in Shanghai
- **Bytecode extraction from Forge**: Forge outputs JSON, extract with `jq -r '.bytecode.object' <contract>.json | sed 's/^0x//' > <contract>.bytecode.hex`

## Devnet Setup

- **Use the `devnet-generate` ClusterWorkflowTemplate** to generate genesis for new devnets - it handles genesis generation, validator config initialization, and R2 upload automatically
- **Genesis URL convention**: `https://devnet-assets.tempoxyz.dev/{devnet-name}.json` - the workflow uploads to this path
- **Trigger genesis generation workflow**:
  ```bash
  kubectl create -f - <<EOF
  apiVersion: argoproj.io/v1alpha1
  kind: Workflow
  metadata:
    generateName: {devnet-name}-generate-
    namespace: argo-workflows
  spec:
    workflowTemplateRef:
      name: devnet-generate
      clusterScope: true
    arguments:
      parameters:
        - name: accounts
          value: "100"
        - name: branch
          value: "main"
        - name: chain-id
          value: "{chain-id}"
        - name: epoch-length
          value: "1000"
        - name: gas-limit
          value: "500000000"
        - name: name
          value: "{devnet-name}"
        - name: validators
          value: "{ip1}:30302,{ip2}:30302,..."
  EOF
  ```
- **After changing genesis**: Must delete PVCs and restart pods - genesis hash mismatch causes `CrashLoopBackOff`
- **Use `--seed 0`**: Ensures deterministic validator keys matching `values.yaml` defaults

## Common Pitfalls

- **ValidatorConfig precompile missing**: If genesis is generated incorrectly (e.g., manually without proper xtask), `0xCCCCCCCC00000000000000000000000000000000` will have no storage - consensus won't progress
- **Consensus P2P port**: Helm chart uses port `30302` for consensus P2P - genesis must use matching port in `--validators` argument
- **Validator IPs must match**: The `validatorIps` in ArgoCD app must match the IPs used in `--validators` during genesis generation
- **Bridge config in integrated mode**: `signer` and `threshold.sharing_file` are optional - the share comes from `--consensus.signing-share` and sharing polynomial is extracted from genesis `extraData`

## Debugging Steps

- Check consensus logs for `nullify` spam → usually means validators can't reach each other or ValidatorConfig is misconfigured
- Verify ValidatorConfig precompile: `cast call 0xCCCCCCCC00000000000000000000000000000000 "validatorCount()(uint256)" --rpc-url <rpc>` - should return validator count
- Verify genesis has ValidatorConfig: `curl -s <genesis-url> | jq '.alloc | keys | map(select(. | test("cccc"; "i")))'`
- Check service IPs match genesis: `kubectl get svc -n {namespace} -l app.tempo.xyz/nodetype=validator,app.tempo.xyz/service-type=p2p`
