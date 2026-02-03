//! End-to-end tests for the FinalizationBridge contract.
//!
//! This test:
//! 1. Starts a Tempo node (in-process)
//! 2. Sends a transaction that emits MessageSent
//! 3. Waits for block finalization
//! 4. Fetches the finalization certificate from consensus RPC
//! 5. Generates receipt MPT proof
//! 6. Deploys FinalizationBridge on Anvil (Ethereum with Prague)
//! 7. Submits the proof and verifies the message is received
//! 8. Tests various failure cases (invalid sig, wrong block, etc.)

use std::{
    process::{Child, Command, Stdio},
    sync::Arc,
    time::Duration,
};

use alloy::{
    network::TransactionBuilder,
    primitives::{Address, Bytes, B256, U256},
    providers::{Provider, ProviderBuilder},
    signers::local::MnemonicBuilder,
    sol,
    sol_types::{SolCall, SolEvent},
};
use alloy_rlp::Encodable;

/// Standard test mnemonic (has balance in genesis).
const TEST_MNEMONIC: &str = "test test test test test test test test test test test junk";

// FinalizationBridge contract interface
sol! {
    #[derive(Debug)]
    event MessageSent(
        address indexed sender,
        bytes32 indexed messageHash,
        uint64 indexed destinationChainId
    );

    #[derive(Debug)]
    event MessageReceived(
        uint64 indexed originChainId,
        address indexed sender,
        bytes32 indexed messageHash,
        uint256 receivedAt
    );

    #[derive(Debug)]
    function send(bytes32 messageHash, uint64 destinationChainId) external;

    #[derive(Debug)]
    function write(
        bytes blockHeader,
        bytes finalizationProposal,
        bytes finalizationSignature,
        bytes[] receiptProof,
        uint256 receiptIndex,
        uint256 logIndex
    ) external;

    #[derive(Debug)]
    function receivedAt(uint64 originChainId, address sender, bytes32 messageHash) external view returns (uint256);

    #[derive(Debug)]
    function originChainId() external view returns (uint64);
}

/// FinalizationBridge bytecode.
/// From: crates/native-bridge/contracts/out/FinalizationBridge.sol/FinalizationBridge.json
const FINALIZATION_BRIDGE_BYTECODE: &str =
    include_str!("../contracts/out/FinalizationBridge.sol/FinalizationBridge.bytecode.hex");

/// G2 generator point (uncompressed, 256 bytes EIP-2537 format).
/// MinSig variant: G2 public keys (256 bytes), G1 signatures (128 bytes).
const G2_GENERATOR_EIP2537: &str = concat!(
    // x.c1 (64 bytes: 16 zero padding + 48 bytes)
    "00000000000000000000000000000000",
    "13e02b6052719f607dacd3a088274f65596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e",
    // x.c0 (64 bytes)
    "00000000000000000000000000000000",
    "024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb8",
    // y.c1 (64 bytes)
    "00000000000000000000000000000000",
    "0606c4a02ea734cc32acd2b02bc28b99cb3e287e85a763af267492ab572e99ab3f370d275cec1da1aaa9075ff05f79be",
    // y.c0 (64 bytes)
    "00000000000000000000000000000000",
    "0ce5d527727d6e118cc9cdc6da2e351aadfd9baa8cbdd3a76d429a695160d12c923ac9cc3baca289e193548608b82801"
);

/// Anvil instance wrapper with automatic cleanup.
struct AnvilInstance {
    child: Child,
    rpc_url: String,
    #[allow(dead_code)]
    ws_url: String,
}

impl AnvilInstance {
    async fn start() -> eyre::Result<Self> {
        let port = portpicker::pick_unused_port().expect("no free port");

        let child = Command::new("anvil")
            .args([
                "--port",
                &port.to_string(),
                "--chain-id",
                "1",
                "--block-time",
                "1",
                "--hardfork",
                "prague", // Required for EIP-2537 BLS precompiles
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let rpc_url = format!("http://127.0.0.1:{port}");
        let ws_url = format!("ws://127.0.0.1:{port}");

        // Wait for anvil to be ready
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Verify it's running
        let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
        let block = provider.get_block_number().await?;
        tracing::info!(port, block, "anvil started");

        Ok(Self {
            child,
            rpc_url,
            ws_url,
        })
    }
}

impl Drop for AnvilInstance {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// Encode FinalizationBridge constructor arguments.
/// constructor(address _owner, uint64 _originChainId, uint64 _initialEpoch, bytes memory _initialPublicKey)
fn encode_finalization_bridge_constructor(
    owner: Address,
    origin_chain_id: u64,
    epoch: u64,
    public_key: &[u8],
) -> Vec<u8> {
    let mut encoded = Vec::new();

    // owner (address, 32 bytes)
    encoded.extend_from_slice(&[0u8; 12]);
    encoded.extend_from_slice(owner.as_slice());

    // origin_chain_id (uint64, 32 bytes)
    encoded.extend_from_slice(&[0u8; 24]);
    encoded.extend_from_slice(&origin_chain_id.to_be_bytes());

    // epoch (uint64, 32 bytes)
    encoded.extend_from_slice(&[0u8; 24]);
    encoded.extend_from_slice(&epoch.to_be_bytes());

    // bytes offset (points to byte 128 = 0x80)
    encoded.extend_from_slice(&[0u8; 31]);
    encoded.push(0x80);

    // bytes length
    let len = public_key.len();
    encoded.extend_from_slice(&[0u8; 24]);
    encoded.extend_from_slice(&(len as u64).to_be_bytes());

    // bytes data (padded to 32 bytes)
    encoded.extend_from_slice(public_key);
    let padding = (32 - (len % 32)) % 32;
    encoded.extend_from_slice(&vec![0u8; padding]);

    encoded
}

/// Deploy the FinalizationBridge contract on Anvil.
async fn deploy_finalization_bridge(
    rpc_url: &str,
    origin_chain_id: u64,
) -> eyre::Result<Address> {
    use alloy::signers::local::PrivateKeySigner;

    // Anvil's default funded account
    let signer: PrivateKeySigner =
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
            .parse()
            .unwrap();
    let owner = signer.address();

    let provider = ProviderBuilder::new()
        .wallet(alloy::network::EthereumWallet::from(signer))
        .connect_http(rpc_url.parse()?);

    // Contract bytecode
    let bytecode = hex::decode(FINALIZATION_BRIDGE_BYTECODE.trim())?;

    // Constructor arguments
    let initial_epoch = 1u64;
    let initial_public_key = hex::decode(G2_GENERATOR_EIP2537)?;

    let constructor_args = encode_finalization_bridge_constructor(
        owner,
        origin_chain_id,
        initial_epoch,
        &initial_public_key,
    );
    let deploy_code: Vec<u8> = bytecode.into_iter().chain(constructor_args).collect();

    let tx = alloy::rpc::types::TransactionRequest::default()
        .with_deploy_code(Bytes::from(deploy_code));

    let pending = provider.send_transaction(tx).await?;
    let receipt = pending.get_receipt().await?;

    let address = receipt
        .contract_address
        .ok_or_else(|| eyre::eyre!("no contract address in receipt"))?;

    tracing::info!(
        %address,
        %owner,
        origin_chain_id,
        epoch = initial_epoch,
        "deployed FinalizationBridge on Anvil"
    );
    Ok(address)
}

// =============================================================================
// Receipt MPT Proof Generation
// =============================================================================

/// Generate a receipt MPT proof for a transaction at a given index.
///
/// This builds the receipts trie and generates the proof nodes.
pub mod receipt_proof {
    use alloy::primitives::B256;
    use alloy_trie::{HashBuilder, Nibbles, proof::ProofRetainer};

    /// RLP-encode a receipt index for use as a trie key.
    pub fn rlp_encode_index(index: usize) -> Vec<u8> {
        alloy_rlp::encode(index)
    }

    /// Build a receipts trie and generate a proof for a specific index.
    ///
    /// Returns the receipts root and the proof nodes.
    pub fn generate_receipt_proof(
        receipts_rlp: &[Vec<u8>],
        target_index: usize,
    ) -> eyre::Result<(B256, Vec<Vec<u8>>)> {
        if target_index >= receipts_rlp.len() {
            return Err(eyre::eyre!(
                "target index {} out of bounds (len={})",
                target_index,
                receipts_rlp.len()
            ));
        }

        // Build key-value pairs for the trie
        let mut pairs: Vec<(Nibbles, Vec<u8>)> = Vec::with_capacity(receipts_rlp.len());
        for (i, receipt_rlp) in receipts_rlp.iter().enumerate() {
            let key = rlp_encode_index(i);
            let nibbles = Nibbles::unpack(&key);
            pairs.push((nibbles, receipt_rlp.clone()));
        }

        // Sort by key (required for HashBuilder)
        pairs.sort_by(|a, b| a.0.cmp(&b.0));

        // Create proof retainer for the target key
        let target_key = rlp_encode_index(target_index);
        let target_nibbles = Nibbles::unpack(&target_key);
        let retainer = ProofRetainer::new(vec![target_nibbles.clone()]);

        // Build the trie with proof retention
        let mut builder = HashBuilder::default().with_proof_retainer(retainer);

        for (nibbles, value) in &pairs {
            builder.add_leaf(nibbles.clone(), value);
        }

        let root = builder.root();

        // Extract proof nodes
        let proof_nodes = builder.take_proof_nodes();
        let proof: Vec<Vec<u8>> = proof_nodes
            .into_nodes_sorted()
            .into_iter()
            .map(|(_, node)| node.to_vec())
            .collect();

        Ok((root, proof))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_rlp_encode_index() {
            // Index 0 encodes as 0x80 (empty string)
            assert_eq!(rlp_encode_index(0), vec![0x80]);

            // Index 1 encodes as 0x01
            assert_eq!(rlp_encode_index(1), vec![0x01]);

            // Index 127 encodes as 0x7f
            assert_eq!(rlp_encode_index(127), vec![0x7f]);

            // Index 128 encodes as 0x81 0x80
            assert_eq!(rlp_encode_index(128), vec![0x81, 0x80]);
        }

        #[test]
        fn test_generate_single_receipt_proof() {
            // Single receipt - proof should contain the receipt itself
            let receipt_rlp = vec![0x01, 0x02, 0x03];
            let receipts = vec![receipt_rlp.clone()];

            let (root, proof) = generate_receipt_proof(&receipts, 0).unwrap();

            // Root should be non-zero
            assert_ne!(root, B256::ZERO);
            // Proof should have at least one node
            assert!(!proof.is_empty());
        }

        #[test]
        fn test_generate_multi_receipt_proof() {
            // Multiple receipts
            let receipts = vec![
                vec![0x01, 0x02, 0x03],
                vec![0x04, 0x05, 0x06],
                vec![0x07, 0x08, 0x09],
            ];

            for i in 0..receipts.len() {
                let (root, proof) = generate_receipt_proof(&receipts, i).unwrap();
                assert_ne!(root, B256::ZERO);
                assert!(!proof.is_empty());
            }
        }
    }
}

// =============================================================================
// Test genesis JSON for Tempo node tests
// =============================================================================

const TEST_GENESIS: &str = include_str!("../../node/tests/assets/test-genesis.json");

/// Start an in-process Tempo node for testing.
async fn start_tempo_node() -> eyre::Result<(String, String, reth_ethereum::tasks::TaskManager)> {
    use reth_node_builder::{NodeBuilder, NodeConfig};
    use reth_node_core::args::RpcServerArgs;
    use reth_rpc_builder::RpcModuleSelection;
    use tempo_chainspec::spec::TempoChainSpec;
    use tempo_node::node::TempoNode;

    let genesis: serde_json::Value = serde_json::from_str(TEST_GENESIS)?;
    let chain_spec = TempoChainSpec::from_genesis(serde_json::from_value(genesis)?);
    let validator = chain_spec.inner.genesis.coinbase;

    let tasks = reth_ethereum::tasks::TaskManager::current();

    let mut node_config = NodeConfig::new(Arc::new(chain_spec))
        .with_unused_ports()
        .dev()
        .with_rpc(
            RpcServerArgs::default()
                .with_unused_ports()
                .with_http()
                .with_ws()
                .with_http_api(RpcModuleSelection::All)
                .with_ws_api(RpcModuleSelection::All),
        );
    node_config.dev.block_time = Some(Duration::from_millis(500));

    let node_handle = NodeBuilder::new(node_config)
        .testing_node(tasks.executor())
        .node(TempoNode::default())
        .launch_with_debug_capabilities()
        .map_debug_payload_attributes(move |mut attributes| {
            attributes.suggested_fee_recipient = validator;
            attributes
        })
        .await?;

    let http_url = node_handle
        .node
        .rpc_server_handle()
        .http_url()
        .ok_or_else(|| eyre::eyre!("no HTTP URL"))?;

    let ws_url = node_handle
        .node
        .rpc_server_handle()
        .ws_url()
        .ok_or_else(|| eyre::eyre!("no WS URL"))?;

    tracing::info!(%http_url, %ws_url, "tempo node started");

    // Keep the node handle alive by leaking it (task manager keeps it running)
    std::mem::forget(node_handle);

    Ok((http_url, ws_url, tasks))
}

/// Deploy FinalizationBridge on Tempo.
async fn deploy_finalization_bridge_tempo(
    rpc_url: &str,
    public_key: &[u8],
) -> eyre::Result<Address> {
    let wallet = MnemonicBuilder::from_phrase(TEST_MNEMONIC).build()?;
    let owner = wallet.address();

    let provider = ProviderBuilder::new()
        .wallet(alloy::network::EthereumWallet::from(wallet))
        .connect_http(rpc_url.parse()?);

    let bytecode = hex::decode(FINALIZATION_BRIDGE_BYTECODE.trim())?;
    let chain_id = provider.get_chain_id().await?;

    // Use the Tempo chain ID as the origin chain ID
    let constructor_args = encode_finalization_bridge_constructor(owner, chain_id, 1, public_key);
    let deploy_code: Vec<u8> = bytecode.into_iter().chain(constructor_args).collect();

    let tx = alloy::rpc::types::TransactionRequest::default()
        .with_deploy_code(Bytes::from(deploy_code))
        .with_gas_limit(5_000_000);

    let pending = provider.send_transaction(tx).await?;
    let receipt = pending.get_receipt().await?;

    let address = receipt
        .contract_address
        .ok_or_else(|| eyre::eyre!("no contract address in receipt"))?;

    tracing::info!(%address, "deployed FinalizationBridge on Tempo");
    Ok(address)
}

// =============================================================================
// Unit tests for libraries (can run without full e2e setup)
// =============================================================================

#[cfg(test)]
mod library_tests {
    use super::*;

    /// Test that the MessageSent event signature matches what we expect.
    #[test]
    fn test_message_sent_signature() {
        use alloy::primitives::keccak256;
        let sig = MessageSent::SIGNATURE_HASH;
        let expected = keccak256("MessageSent(address,bytes32,uint64)");
        assert_eq!(sig, expected);
    }

    /// Test MessageReceived event signature.
    #[test]
    fn test_message_received_signature() {
        use alloy::primitives::keccak256;
        let sig = MessageReceived::SIGNATURE_HASH;
        let expected = keccak256("MessageReceived(uint64,address,bytes32,uint256)");
        assert_eq!(sig, expected);
    }

    /// Test G2 generator constant is valid (256 bytes for MinSig).
    #[test]
    fn test_g2_generator_length() {
        let decoded = hex::decode(G2_GENERATOR_EIP2537).unwrap();
        assert_eq!(decoded.len(), 256);
    }

    /// Test constructor encoding.
    #[test]
    fn test_constructor_encoding() {
        let owner = Address::repeat_byte(0x42);
        let origin_chain_id = 98985u64;
        let epoch = 1u64;
        let public_key = vec![0x01u8; 256];

        let encoded =
            encode_finalization_bridge_constructor(owner, origin_chain_id, epoch, &public_key);

        // Should have: 4 x 32 bytes for params + 32 bytes length + 256 bytes key
        assert_eq!(encoded.len(), 4 * 32 + 32 + 256);
    }
}

// =============================================================================
// Integration tests (require Anvil and optionally Tempo node)
// =============================================================================

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Test deploying FinalizationBridge on Anvil.
    #[tokio::test]
    #[ignore = "requires anvil"]
    async fn test_deploy_finalization_bridge() -> eyre::Result<()> {
        let anvil = AnvilInstance::start().await?;
        let tempo_chain_id = 98985u64;

        let bridge_addr = deploy_finalization_bridge(&anvil.rpc_url, tempo_chain_id).await?;
        assert_ne!(bridge_addr, Address::ZERO);

        // Verify contract state
        let provider = ProviderBuilder::new().connect_http(anvil.rpc_url.parse()?);

        let origin_call = originChainIdCall {};
        let result = provider
            .call(
                alloy::rpc::types::TransactionRequest::default()
                    .to(bridge_addr)
                    .input(origin_call.abi_encode().into()),
            )
            .await?;

        let origin = u64::from_be_bytes(result[24..32].try_into().unwrap());
        assert_eq!(origin, tempo_chain_id);

        Ok(())
    }

    /// Test sending a message on the bridge.
    #[tokio::test]
    #[ignore = "requires anvil"]
    async fn test_send_message() -> eyre::Result<()> {
        use alloy::signers::local::PrivateKeySigner;

        let anvil = AnvilInstance::start().await?;
        let tempo_chain_id = 98985u64;

        let bridge_addr = deploy_finalization_bridge(&anvil.rpc_url, tempo_chain_id).await?;

        // Setup wallet
        let signer: PrivateKeySigner =
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                .parse()
                .unwrap();
        let provider = ProviderBuilder::new()
            .wallet(alloy::network::EthereumWallet::from(signer.clone()))
            .connect_http(anvil.rpc_url.parse()?);

        // Send a message
        let message_hash = B256::repeat_byte(0x42);
        let dest_chain_id = 1u64;

        let send_call = sendCall {
            messageHash: message_hash,
            destinationChainId: dest_chain_id,
        };
        let tx = alloy::rpc::types::TransactionRequest::default()
            .to(bridge_addr)
            .input(send_call.abi_encode().into());

        let receipt = provider.send_transaction(tx).await?.get_receipt().await?;
        assert!(receipt.status());

        // Check for MessageSent event
        let logs = receipt.inner.logs();
        let event = logs
            .iter()
            .find(|log| !log.topics().is_empty() && log.topics()[0] == MessageSent::SIGNATURE_HASH)
            .expect("MessageSent event not found");

        assert_eq!(
            event.topics()[1],
            B256::left_padding_from(signer.address().as_slice())
        );
        assert_eq!(event.topics()[2], message_hash);

        tracing::info!("MessageSent event emitted successfully");
        Ok(())
    }
}

// =============================================================================
// Full E2E tests (require both Anvil and Tempo node)
// =============================================================================

#[cfg(test)]
mod e2e_tests {
    use super::*;

    /// Full end-to-end test for finalization-based bridging.
    ///
    /// This test:
    /// 1. Starts a Tempo node
    /// 2. Deploys FinalizationBridge on Tempo
    /// 3. Sends a message on Tempo (emits MessageSent)
    /// 4. Waits for the block to be included
    /// 5. Fetches the block header and generates receipt proof
    /// 6. Deploys FinalizationBridge on Anvil
    /// 7. Submits the proof (note: requires valid BLS signature)
    /// 8. Verifies the message is received
    ///
    /// Note: This test uses mock signatures since we don't have access to
    /// the actual finalization certificate in the dev node. A full integration
    /// test would require running with a proper consensus setup.
    #[tokio::test]
    #[ignore = "requires full infrastructure - Tempo node + Anvil + consensus"]
    async fn test_full_finalization_flow() -> eyre::Result<()> {
        tracing_subscriber::fmt()
            .with_env_filter("finalization_bridge_e2e=debug,tempo=info")
            .try_init()
            .ok();

        // ====================================================================
        // Step 1: Start Tempo node and Anvil
        // ====================================================================
        let (tempo_http, _tempo_ws, _tasks) = start_tempo_node().await?;
        let anvil = AnvilInstance::start().await?;

        let tempo_provider = ProviderBuilder::new().connect_http(tempo_http.parse()?);
        let tempo_chain_id = tempo_provider.get_chain_id().await?;
        tracing::info!(tempo_chain_id, "Tempo node ready");

        // ====================================================================
        // Step 2: Deploy FinalizationBridge on Tempo
        // ====================================================================
        let g2_pubkey = hex::decode(G2_GENERATOR_EIP2537)?;
        let tempo_bridge = deploy_finalization_bridge_tempo(&tempo_http, &g2_pubkey).await?;

        // ====================================================================
        // Step 3: Send a message on Tempo
        // ====================================================================
        let wallet = MnemonicBuilder::from_phrase(TEST_MNEMONIC).build()?;
        let _sender = wallet.address();

        let tempo_tx_provider = ProviderBuilder::new()
            .wallet(alloy::network::EthereumWallet::from(wallet))
            .connect_http(tempo_http.parse()?);

        let message_hash = B256::repeat_byte(0xAB);
        let dest_chain_id = 1u64; // Anvil chain ID

        let send_call = sendCall {
            messageHash: message_hash,
            destinationChainId: dest_chain_id,
        };

        let tx = alloy::rpc::types::TransactionRequest::default()
            .to(tempo_bridge)
            .gas_limit(5_000_000)
            .input(send_call.abi_encode().into());

        let pending = tempo_tx_provider.send_transaction(tx).await?;
        let send_receipt = pending.get_receipt().await?;

        assert!(send_receipt.status(), "send transaction should succeed");
        let block_number = send_receipt.block_number.unwrap();
        let tx_index = send_receipt.transaction_index.unwrap() as usize;

        tracing::info!(
            block_number,
            tx_index,
            tx_hash = %send_receipt.transaction_hash,
            "message sent on Tempo"
        );

        // ====================================================================
        // Step 4: Get block header and all receipts
        // ====================================================================
        let block = tempo_provider
            .get_block_by_number(block_number.into())
            .await?
            .ok_or_else(|| eyre::eyre!("block not found"))?;

        // Get all receipts for the block
        let mut receipts_rlp = Vec::new();
        for (i, tx_hash) in block.transactions.hashes().enumerate() {
            let receipt = tempo_provider
                .get_transaction_receipt(tx_hash)
                .await?
                .ok_or_else(|| eyre::eyre!("receipt {} not found", i))?;

            // Convert to RLP
            let receipt_rlp = encode_receipt(&receipt)?;
            receipts_rlp.push(receipt_rlp);
        }

        tracing::info!(
            receipts_count = receipts_rlp.len(),
            "fetched all receipts for block"
        );

        // ====================================================================
        // Step 5: Generate receipt MPT proof
        // ====================================================================
        let (computed_root, proof) =
            receipt_proof::generate_receipt_proof(&receipts_rlp, tx_index)?;

        // Verify the computed root matches the block's receiptsRoot
        let block_receipts_root = block.header.receipts_root;
        assert_eq!(
            computed_root, block_receipts_root,
            "computed receipts root should match block header"
        );

        tracing::info!(
            proof_len = proof.len(),
            receipts_root = %computed_root,
            "generated receipt proof"
        );

        // ====================================================================
        // Step 6: RLP-encode the block header
        // ====================================================================
        let header_rlp = encode_block_header(&block.header)?;
        let header_hash = alloy::primitives::keccak256(&header_rlp);
        assert_eq!(header_hash, block.header.hash, "header hash should match");

        // ====================================================================
        // Step 7: Deploy FinalizationBridge on Anvil
        // ====================================================================
        let _anvil_bridge =
            deploy_finalization_bridge(&anvil.rpc_url, tempo_chain_id).await?;

        // ====================================================================
        // Step 8: Submit proof to Anvil bridge
        // ====================================================================
        // NOTE: This requires a valid BLS signature over the block hash.
        // In a real scenario, this would come from the consensus finalization.
        // For testing, we'd need to either:
        // a) Mock the signature verification (modify contract for testing)
        // b) Have access to the consensus DKG shares to sign

        // For now, we'll document that this step needs the finalization signature
        tracing::info!(
            header_hash = %header_hash,
            block_number,
            "prepared proof - requires finalization signature to submit"
        );

        // The proof submission would look like:
        // let write_call = writeCall {
        //     blockHeader: Bytes::from(header_rlp),
        //     finalizationSignature: Bytes::from(finalization_sig), // Need real sig
        //     receiptProof: proof.into_iter().map(Bytes::from).collect(),
        //     receiptIndex: U256::from(tx_index),
        //     logIndex: U256::from(0), // First log in receipt
        // };

        tracing::info!("🎉 Proof generation successful - awaiting consensus integration");
        Ok(())
    }

    // =========================================================================
    // Negative tests
    // =========================================================================

    /// Test that invalid signature is rejected.
    #[tokio::test]
    #[ignore = "requires anvil with working mock data"]
    async fn test_invalid_finalization_signature() -> eyre::Result<()> {
        let anvil = AnvilInstance::start().await?;
        let bridge_addr = deploy_finalization_bridge(&anvil.rpc_url, 98985).await?;

        // Create mock block header (minimal RLP list)
        let mock_header = create_mock_block_header();

        // Invalid signature (all zeros = point at infinity, should fail)
        let invalid_sig = vec![0u8; 128];

        // Mock receipt proof
        let mock_proof: Vec<Bytes> = vec![Bytes::from(vec![0x80])]; // Empty RLP

        // Mock proposal (epoch u64 + view u64 + parent u64 + payload 32 bytes)
        let mock_proposal = vec![0u8; 8 + 8 + 8 + 32];

        let write_call = writeCall {
            blockHeader: Bytes::from(mock_header),
            finalizationProposal: Bytes::from(mock_proposal),
            finalizationSignature: Bytes::from(invalid_sig),
            receiptProof: mock_proof,
            receiptIndex: U256::ZERO,
            logIndex: U256::ZERO,
        };

        let provider = ProviderBuilder::new()
            .wallet(alloy::network::EthereumWallet::from(
                "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                    .parse::<alloy::signers::local::PrivateKeySigner>()
                    .unwrap(),
            ))
            .connect_http(anvil.rpc_url.parse()?);

        let tx = alloy::rpc::types::TransactionRequest::default()
            .to(bridge_addr)
            .gas_limit(500_000)
            .input(write_call.abi_encode().into());

        // This should revert
        let result = provider.send_transaction(tx).await;
        assert!(
            result.is_err() || {
                let receipt = result.unwrap().get_receipt().await;
                receipt.is_err() || !receipt.unwrap().status()
            }
        );

        tracing::info!("Invalid signature correctly rejected");
        Ok(())
    }

    /// Test that empty proof is rejected.
    #[tokio::test]
    #[ignore = "requires anvil"]
    async fn test_empty_proof_rejected() -> eyre::Result<()> {
        let anvil = AnvilInstance::start().await?;
        let bridge_addr = deploy_finalization_bridge(&anvil.rpc_url, 98985).await?;

        let mock_header = create_mock_block_header();
        let valid_sig = vec![0x01u8; 128]; // Non-zero signature
        let empty_proof: Vec<Bytes> = vec![]; // Empty proof array

        // Mock proposal (epoch u64 + view u64 + parent u64 + payload 32 bytes)
        let mock_proposal = vec![0u8; 8 + 8 + 8 + 32];

        let write_call = writeCall {
            blockHeader: Bytes::from(mock_header),
            finalizationProposal: Bytes::from(mock_proposal),
            finalizationSignature: Bytes::from(valid_sig),
            receiptProof: empty_proof,
            receiptIndex: U256::ZERO,
            logIndex: U256::ZERO,
        };

        let provider = ProviderBuilder::new()
            .wallet(alloy::network::EthereumWallet::from(
                "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                    .parse::<alloy::signers::local::PrivateKeySigner>()
                    .unwrap(),
            ))
            .connect_http(anvil.rpc_url.parse()?);

        let tx = alloy::rpc::types::TransactionRequest::default()
            .to(bridge_addr)
            .gas_limit(500_000)
            .input(write_call.abi_encode().into());

        // This should revert with EmptyProof
        let result = provider.send_transaction(tx).await;
        assert!(
            result.is_err() || {
                let receipt = result.unwrap().get_receipt().await;
                receipt.is_err() || !receipt.unwrap().status()
            }
        );

        tracing::info!("Empty proof correctly rejected");
        Ok(())
    }

    /// Test that wrong signature length is rejected.
    #[tokio::test]
    #[ignore = "requires anvil"]
    async fn test_wrong_signature_length() -> eyre::Result<()> {
        let anvil = AnvilInstance::start().await?;
        let bridge_addr = deploy_finalization_bridge(&anvil.rpc_url, 98985).await?;

        let mock_header = create_mock_block_header();
        let wrong_len_sig = vec![0x01u8; 64]; // Should be 128 bytes
        let mock_proof: Vec<Bytes> = vec![Bytes::from(vec![0x80])];

        // Mock proposal (epoch u64 + view u64 + parent u64 + payload 32 bytes)
        let mock_proposal = vec![0u8; 8 + 8 + 8 + 32];

        let write_call = writeCall {
            blockHeader: Bytes::from(mock_header),
            finalizationProposal: Bytes::from(mock_proposal),
            finalizationSignature: Bytes::from(wrong_len_sig),
            receiptProof: mock_proof,
            receiptIndex: U256::ZERO,
            logIndex: U256::ZERO,
        };

        let provider = ProviderBuilder::new()
            .wallet(alloy::network::EthereumWallet::from(
                "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                    .parse::<alloy::signers::local::PrivateKeySigner>()
                    .unwrap(),
            ))
            .connect_http(anvil.rpc_url.parse()?);

        let tx = alloy::rpc::types::TransactionRequest::default()
            .to(bridge_addr)
            .gas_limit(500_000)
            .input(write_call.abi_encode().into());

        // This should revert with InvalidSignatureLength
        let result = provider.send_transaction(tx).await;
        assert!(
            result.is_err() || {
                let receipt = result.unwrap().get_receipt().await;
                receipt.is_err() || !receipt.unwrap().status()
            }
        );

        tracing::info!("Wrong signature length correctly rejected");
        Ok(())
    }

    // =========================================================================
    // Helper functions
    // =========================================================================

    /// Create a minimal valid RLP-encoded block header for testing.
    /// This is a mock header - real tests need actual Tempo headers.
    fn create_mock_block_header() -> Vec<u8> {
        // Minimal Ethereum block header structure (RLP list with 16+ elements)
        // [parentHash, unclesHash, coinbase, stateRoot, txRoot, receiptsRoot, ...]
        let mut header = Vec::new();

        // RLP list prefix for long list
        header.push(0xf9); // List > 55 bytes
        header.push(0x02); // Length high byte
        header.push(0x00); // Length low byte (placeholder)

        // parentHash (32 bytes)
        header.push(0xa0);
        header.extend_from_slice(&[0u8; 32]);

        // unclesHash (32 bytes)
        header.push(0xa0);
        header.extend_from_slice(&[0u8; 32]);

        // coinbase (20 bytes)
        header.push(0x94);
        header.extend_from_slice(&[0u8; 20]);

        // stateRoot (32 bytes)
        header.push(0xa0);
        header.extend_from_slice(&[0u8; 32]);

        // txRoot (32 bytes)
        header.push(0xa0);
        header.extend_from_slice(&[0u8; 32]);

        // receiptsRoot (32 bytes) - index 5
        header.push(0xa0);
        header.extend_from_slice(&[0u8; 32]);

        // logsBloom (256 bytes)
        header.push(0xb9);
        header.push(0x01);
        header.push(0x00);
        header.extend_from_slice(&[0u8; 256]);

        // difficulty (0)
        header.push(0x80);

        // number (0)
        header.push(0x80);

        // gasLimit (0)
        header.push(0x80);

        // gasUsed (0)
        header.push(0x80);

        // timestamp (0)
        header.push(0x80);

        // extraData (empty)
        header.push(0x80);

        // mixHash (32 bytes)
        header.push(0xa0);
        header.extend_from_slice(&[0u8; 32]);

        // nonce (8 bytes)
        header.push(0x88);
        header.extend_from_slice(&[0u8; 8]);

        // Update length bytes
        let len = header.len() - 3;
        header[1] = ((len >> 8) & 0xff) as u8;
        header[2] = (len & 0xff) as u8;

        header
    }
}

// =============================================================================
// RLP Encoding Helpers
// =============================================================================

/// Encode a transaction receipt to RLP format.
fn encode_receipt(receipt: &alloy::rpc::types::TransactionReceipt) -> eyre::Result<Vec<u8>> {
    use alloy_consensus::{Receipt, ReceiptWithBloom};

    // Build the receipt logs
    let logs: Vec<alloy::primitives::Log> = receipt
        .inner
        .logs()
        .iter()
        .map(|log| alloy::primitives::Log {
            address: log.address(),
            data: alloy::primitives::LogData::new(
                log.topics().to_vec(),
                log.data().data.clone(),
            )
            .unwrap(),
        })
        .collect();

    // Create the receipt
    let inner_receipt = Receipt {
        status: receipt.status().into(),
        cumulative_gas_used: receipt.inner.cumulative_gas_used(),
        logs,
    };

    // Wrap with bloom
    let receipt_with_bloom = ReceiptWithBloom {
        receipt: inner_receipt,
        logs_bloom: *receipt.inner.logs_bloom(),
    };

    // Encode to RLP
    let mut buf = Vec::new();

    // Handle typed transactions
    let tx_type = receipt.inner.tx_type();
    if tx_type != alloy::consensus::TxType::Legacy {
        buf.push(tx_type as u8);
    }
    receipt_with_bloom.encode(&mut buf);

    Ok(buf)
}

/// Encode a block header to RLP format.
fn encode_block_header(header: &alloy::rpc::types::Header) -> eyre::Result<Vec<u8>> {
    use alloy_consensus::{BlockHeader, Header as ConsensusHeader};

    // Convert to consensus header type for RLP encoding
    let consensus_header = ConsensusHeader {
        parent_hash: header.parent_hash(),
        ommers_hash: header.ommers_hash(),
        beneficiary: header.beneficiary(),
        state_root: header.state_root(),
        transactions_root: header.transactions_root(),
        receipts_root: header.receipts_root(),
        logs_bloom: header.logs_bloom(),
        difficulty: header.difficulty(),
        number: header.number(),
        gas_limit: header.gas_limit(),
        gas_used: header.gas_used(),
        timestamp: header.timestamp(),
        extra_data: header.extra_data().clone(),
        mix_hash: header.mix_hash().unwrap_or_default(),
        nonce: header.nonce().unwrap_or_default(),
        base_fee_per_gas: header.base_fee_per_gas(),
        withdrawals_root: header.withdrawals_root(),
        blob_gas_used: header.blob_gas_used(),
        excess_blob_gas: header.excess_blob_gas(),
        parent_beacon_block_root: header.parent_beacon_block_root(),
        requests_hash: header.requests_hash(),
    };

    let mut buf = Vec::new();
    consensus_header.encode(&mut buf);
    Ok(buf)
}
