//! End-to-end tests for the FinalizationBridge contract using real consensus.
//!
//! This test:
//! 1. Starts Tempo validators with full consensus
//! 2. Deploys FinalizationBridge on Tempo
//! 3. Sends a message that emits MessageSent event
//! 4. Waits for block finalization
//! 5. Fetches the finalization certificate from consensus RPC
//! 6. Starts Anvil (Ethereum with Prague for EIP-2537)
//! 7. Deploys FinalizationBridge on Anvil
//! 8. Generates receipt MPT proof
//! 9. Submits the proof to Anvil bridge
//! 10. Verifies the message is received

use std::{
    net::SocketAddr,
    process::{Child, Command, Stdio},
    time::Duration,
};

use alloy::{
    network::TransactionBuilder,
    primitives::{Address, Bytes, B256, U256},
    providers::{Provider, ProviderBuilder},
    sol,
    sol_types::{SolCall, SolEvent},
};
use alloy_primitives::hex;
use alloy_rlp::Encodable;
use blst::{
    BLST_ERROR, blst_p1_affine, blst_p1_affine_serialize, blst_p1_uncompress, blst_p2_affine,
    blst_p2_affine_serialize, blst_p2_uncompress,
};
use commonware_codec::{Encode, ReadExt as _};
use commonware_consensus::simplex::{scheme::bls12381_threshold::vrf::Scheme, types::Finalization};
use commonware_cryptography::{bls12381::primitives::variant::MinSig, ed25519::PublicKey};
use commonware_macros::test_traced;
use commonware_runtime::{
    Clock, Metrics as _, Runner as _,
    deterministic::{self, Context, Runner},
};
use futures::channel::oneshot;
use jsonrpsee::{http_client::HttpClientBuilder, ws_client::WsClientBuilder};
use reth_ethereum::provider::BlockReader as _;
use tempo_commonware_node::consensus::Digest;
use tempo_node::rpc::consensus::{Event, Query, TempoConsensusApiClient};

use crate::{CONSENSUS_NODE_PREFIX, Setup, setup_validators};

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
}

/// FinalizationBridge bytecode (unlinked - contains library placeholder).
const FINALIZATION_BRIDGE_BYTECODE: &str =
    include_str!("../../../native-bridge/contracts/out/FinalizationBridge.sol/FinalizationBridge.bytecode.hex");

/// MerklePatricia library bytecode (unlinked - contains EthereumTrieDB placeholder).
const MERKLE_PATRICIA_BYTECODE: &str =
    include_str!("../../../native-bridge/contracts/out/MerklePatricia.sol/MerklePatricia.bytecode.hex");

/// EthereumTrieDB library bytecode (no dependencies).
const ETHEREUM_TRIE_DB_BYTECODE: &str =
    include_str!("../../../native-bridge/contracts/out/EthereumTrieDB.sol/EthereumTrieDB.bytecode.hex");

/// SimpleMessageSender bytecode (no dependencies, no BLS).
const SIMPLE_MESSAGE_SENDER_BYTECODE: &str =
    include_str!("../../../native-bridge/contracts/out/SimpleMessageSender.sol/SimpleMessageSender.bytecode.hex");

/// Library placeholder for MerklePatricia in FinalizationBridge.
const MERKLE_PATRICIA_PLACEHOLDER: &str = "__$3557184f57f01f44bdc1609b323054fd0c$__";

/// Library placeholder for EthereumTrieDB in MerklePatricia.
const ETHEREUM_TRIE_DB_PLACEHOLDER: &str = "__$0b959fb394b44b9234314687ba4fc71e61$__";

// =============================================================================
// BLS Format Conversion (compressed -> EIP-2537)
// =============================================================================

/// G1 compressed size (48 bytes).
const G1_COMPRESSED_LEN: usize = 48;
/// G1 EIP-2537 size (128 bytes: 2 × 64-byte Fp).
const G1_UNCOMPRESSED_LEN: usize = 128;
/// G2 compressed size (96 bytes).
const G2_COMPRESSED_LEN: usize = 96;
/// G2 EIP-2537 size (256 bytes: 4 × 64-byte Fp).
const G2_UNCOMPRESSED_LEN: usize = 256;

/// Uncompressed G1 size from blst (96 bytes: 2 × 48-byte Fp elements).
const BLST_G1_SERIALIZE_LEN: usize = 96;
/// Uncompressed G2 size from blst (192 bytes: 4 × 48-byte Fp elements).
const BLST_G2_SERIALIZE_LEN: usize = 192;

/// Convert a compressed G1 signature (48 bytes) to EIP-2537 format (128 bytes).
fn g1_to_eip2537(compressed: &[u8]) -> eyre::Result<[u8; G1_UNCOMPRESSED_LEN]> {
    if compressed.len() != G1_COMPRESSED_LEN {
        return Err(eyre::eyre!(
            "invalid G1 compressed length: {} (expected {})",
            compressed.len(),
            G1_COMPRESSED_LEN
        ));
    }

    let mut affine = blst_p1_affine::default();
    let result = unsafe { blst_p1_uncompress(&mut affine, compressed.as_ptr()) };

    if result != BLST_ERROR::BLST_SUCCESS {
        return Err(eyre::eyre!("failed to decompress G1 point: {result:?}"));
    }

    let mut serialized = [0u8; BLST_G1_SERIALIZE_LEN];
    unsafe { blst_p1_affine_serialize(serialized.as_mut_ptr(), &affine) };

    let mut output = [0u8; G1_UNCOMPRESSED_LEN];
    for i in 0..2 {
        output[i * 64 + 16..(i + 1) * 64].copy_from_slice(&serialized[i * 48..(i + 1) * 48]);
    }

    Ok(output)
}

/// Convert a compressed G2 public key (96 bytes) to EIP-2537 format (256 bytes).
fn g2_to_eip2537(compressed: &[u8]) -> eyre::Result<[u8; G2_UNCOMPRESSED_LEN]> {
    if compressed.len() != G2_COMPRESSED_LEN {
        return Err(eyre::eyre!(
            "invalid G2 compressed length: {} (expected {})",
            compressed.len(),
            G2_COMPRESSED_LEN
        ));
    }

    let mut affine = blst_p2_affine::default();
    let result = unsafe { blst_p2_uncompress(&mut affine, compressed.as_ptr()) };

    if result != BLST_ERROR::BLST_SUCCESS {
        return Err(eyre::eyre!("failed to decompress G2 point: {result:?}"));
    }

    let mut serialized = [0u8; BLST_G2_SERIALIZE_LEN];
    unsafe { blst_p2_affine_serialize(serialized.as_mut_ptr(), &affine) };

    let mut output = [0u8; G2_UNCOMPRESSED_LEN];
    for i in 0..4 {
        output[i * 64 + 16..(i + 1) * 64].copy_from_slice(&serialized[i * 48..(i + 1) * 48]);
    }

    Ok(output)
}

// =============================================================================
// Receipt MPT Proof Generation
// =============================================================================

/// RLP-encode a receipt index for use as a trie key.
fn rlp_encode_index(index: usize) -> Vec<u8> {
    alloy_rlp::encode(index)
}

/// Build a receipts trie and generate a proof for a specific index.
fn generate_receipt_proof(
    receipts_rlp: &[Vec<u8>],
    target_index: usize,
) -> eyre::Result<(B256, Vec<Vec<u8>>)> {
    use alloy_trie::{HashBuilder, Nibbles, proof::ProofRetainer};

    if target_index >= receipts_rlp.len() {
        return Err(eyre::eyre!(
            "target index {} out of bounds (len={})",
            target_index,
            receipts_rlp.len()
        ));
    }

    let mut pairs: Vec<(Nibbles, Vec<u8>)> = Vec::with_capacity(receipts_rlp.len());
    for (i, receipt_rlp) in receipts_rlp.iter().enumerate() {
        let key = rlp_encode_index(i);
        let nibbles = Nibbles::unpack(&key);
        pairs.push((nibbles, receipt_rlp.clone()));
    }

    pairs.sort_by(|a, b| a.0.cmp(&b.0));

    let target_key = rlp_encode_index(target_index);
    let target_nibbles = Nibbles::unpack(&target_key);
    let retainer = ProofRetainer::new(vec![target_nibbles.clone()]);

    let mut builder = HashBuilder::default().with_proof_retainer(retainer);

    for (nibbles, value) in &pairs {
        builder.add_leaf(nibbles.clone(), value);
    }

    let root = builder.root();

    let proof_nodes = builder.take_proof_nodes();
    let proof: Vec<Vec<u8>> = proof_nodes
        .into_nodes_sorted()
        .into_iter()
        .map(|(_, node)| node.to_vec())
        .collect();

    Ok((root, proof))
}

/// Encode a transaction receipt to RLP format.
fn encode_receipt(receipt: &alloy::rpc::types::TransactionReceipt) -> eyre::Result<Vec<u8>> {
    use alloy_consensus::{Receipt, ReceiptWithBloom};

    let logs: Vec<alloy::primitives::Log> = receipt
        .inner
        .logs()
        .iter()
        .map(|log| alloy::primitives::Log {
            address: log.address(),
            data: alloy::primitives::LogData::new(log.topics().to_vec(), log.data().data.clone())
                .unwrap(),
        })
        .collect();

    let inner_receipt = Receipt {
        status: receipt.status().into(),
        cumulative_gas_used: receipt.inner.cumulative_gas_used(),
        logs,
    };

    let receipt_with_bloom = ReceiptWithBloom {
        receipt: inner_receipt,
        logs_bloom: *receipt.inner.logs_bloom(),
    };

    let mut buf = Vec::new();

    let tx_type = receipt.inner.tx_type();
    if tx_type != alloy::consensus::TxType::Legacy {
        buf.push(tx_type as u8);
    }
    receipt_with_bloom.encode(&mut buf);

    Ok(buf)
}

/// Encode a Tempo block header to RLP format.
/// Tempo headers are encoded as: rlp([general_gas_limit, shared_gas_limit, timestamp_millis_part, inner])
fn encode_tempo_block_header(
    header: &alloy::rpc::types::Header,
    general_gas_limit: u64,
    shared_gas_limit: u64,
    timestamp_millis_part: u64,
) -> eyre::Result<Vec<u8>> {
    use alloy_consensus::{BlockHeader, Header as ConsensusHeader};
    use alloy_rlp::Encodable;

    // Build the inner Ethereum header
    let inner = ConsensusHeader {
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

    // Encode the inner header first to get its length
    let mut inner_buf = Vec::new();
    inner.encode(&mut inner_buf);

    // Calculate total payload length
    let payload_length = general_gas_limit.length()
        + shared_gas_limit.length()
        + timestamp_millis_part.length()
        + inner_buf.len();

    // Build the final RLP: list header + [general_gas_limit, shared_gas_limit, timestamp_millis_part, inner]
    let mut buf = Vec::new();
    alloy_rlp::Header {
        list: true,
        payload_length,
    }
    .encode(&mut buf);

    Encodable::encode(&general_gas_limit, &mut buf);
    Encodable::encode(&shared_gas_limit, &mut buf);
    Encodable::encode(&timestamp_millis_part, &mut buf);
    buf.extend_from_slice(&inner_buf);

    Ok(buf)
}

// =============================================================================
// Contract Deployment
// =============================================================================

/// Anvil instance wrapper with automatic cleanup.
struct AnvilInstance {
    child: Child,
    rpc_url: String,
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
                "prague",
                "--gas-limit",
                "100000000", // 100M gas limit to allow deploying large contracts
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let rpc_url = format!("http://127.0.0.1:{port}");

        tokio::time::sleep(Duration::from_secs(2)).await;

        let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
        let block = provider.get_block_number().await?;
        tracing::info!(port, block, "anvil started");

        Ok(Self { child, rpc_url })
    }
}

impl Drop for AnvilInstance {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

// FinalizationBridge constructor signature
sol! {
    #[derive(Debug)]
    function constructor(address _owner, uint64 _originChainId, uint64 _initialEpoch, bytes memory _initialPublicKey);
}

/// Encode FinalizationBridge constructor arguments using proper ABI encoding.
fn encode_finalization_bridge_constructor(
    owner: Address,
    origin_chain_id: u64,
    epoch: u64,
    public_key: &[u8],
) -> Vec<u8> {
    use alloy::sol_types::SolValue;
    
    (owner, origin_chain_id, epoch, Bytes::copy_from_slice(public_key)).abi_encode_params()
}

/// Deploy a library on Anvil and return its address.
async fn deploy_library(provider: &impl Provider, bytecode_hex: &str) -> eyre::Result<Address> {
    let bytecode = hex::decode(bytecode_hex.trim())?;
    let tx = alloy::rpc::types::TransactionRequest::default().with_deploy_code(Bytes::from(bytecode));

    let pending = provider.send_transaction(tx).await?;
    let receipt = pending.get_receipt().await?;

    receipt
        .contract_address
        .ok_or_else(|| eyre::eyre!("no contract address in receipt"))
}

/// Link bytecode by replacing library placeholder with actual address.
fn link_bytecode(bytecode_hex: &str, placeholder: &str, library_address: Address) -> String {
    let address_hex = hex::encode(library_address.as_slice());
    bytecode_hex.replace(placeholder, &address_hex)
}

/// Deploy all libraries and FinalizationBridge with proper linking.
/// Library chain: EthereumTrieDB -> MerklePatricia -> FinalizationBridge
async fn deploy_finalization_bridge_with_libraries(
    provider: &impl Provider,
    owner: Address,
    origin_chain_id: u64,
    public_key: &[u8],
    gas_limit: u64,
) -> eyre::Result<Address> {
    // Step 1: Deploy EthereumTrieDB (no dependencies)
    let trie_db_address = deploy_library(provider, ETHEREUM_TRIE_DB_BYTECODE).await?;
    tracing::info!(%trie_db_address, "deployed EthereumTrieDB library");

    // Step 2: Link EthereumTrieDB into MerklePatricia and deploy
    let linked_merkle_patricia = link_bytecode(
        MERKLE_PATRICIA_BYTECODE.trim(),
        ETHEREUM_TRIE_DB_PLACEHOLDER,
        trie_db_address,
    );
    
    // Verify the EthereumTrieDB library has code
    let trie_db_code = provider.get_code_at(trie_db_address).await?;
    tracing::info!(
        trie_db_address = %trie_db_address,
        trie_db_code_len = trie_db_code.len(),
        "EthereumTrieDB library code check"
    );
    
    let merkle_patricia_address = deploy_library(provider, &linked_merkle_patricia).await?;
    
    // Verify the MerklePatricia library has code
    let merkle_patricia_code = provider.get_code_at(merkle_patricia_address).await?;
    tracing::info!(
        merkle_patricia_address = %merkle_patricia_address,
        merkle_patricia_code_len = merkle_patricia_code.len(),
        "MerklePatricia library code check"
    );
    
    tracing::info!(%merkle_patricia_address, "deployed MerklePatricia library");

    // Step 3: Link MerklePatricia into FinalizationBridge and deploy
    let linked_bridge = link_bytecode(
        FINALIZATION_BRIDGE_BYTECODE.trim(),
        MERKLE_PATRICIA_PLACEHOLDER,
        merkle_patricia_address,
    );
    
    // Verify no unlinked placeholders remain
    if linked_bridge.contains("__$") {
        return Err(eyre::eyre!("bytecode still contains unlinked library placeholders"));
    }

    let bytecode = hex::decode(&linked_bridge)?;
    let constructor_args = encode_finalization_bridge_constructor(owner, origin_chain_id, 0, public_key);
    
    tracing::info!(
        bytecode_len = bytecode.len(),
        constructor_args_len = constructor_args.len(),
        constructor_args_hex = hex::encode(&constructor_args),
        "Preparing FinalizationBridge deployment"
    );
    
    let deploy_code: Vec<u8> = bytecode.into_iter().chain(constructor_args).collect();

    let tx = alloy::rpc::types::TransactionRequest::default()
        .with_deploy_code(Bytes::from(deploy_code.clone()))
        .with_gas_limit(gas_limit);

    let pending = provider.send_transaction(tx).await?;
    let receipt = pending.get_receipt().await?;
    
    tracing::info!(
        tx_hash = %receipt.transaction_hash,
        status = receipt.status(),
        gas_used = receipt.gas_used,
        contract_address = ?receipt.contract_address,
        logs_count = receipt.inner.logs().len(),
        "FinalizationBridge deployment receipt"
    );
    
    if !receipt.status() {
        // Try to get revert reason using eth_call simulation
        let deploy_tx = alloy::rpc::types::TransactionRequest::default()
            .with_deploy_code(Bytes::from(deploy_code.clone()))
            .with_gas_limit(gas_limit);
        
        match provider.call(deploy_tx).await {
            Ok(_) => tracing::info!("eth_call simulation succeeded (unexpected)"),
            Err(e) => tracing::error!(?e, "eth_call simulation failed - revert reason"),
        }
    }

    let address = receipt
        .contract_address
        .ok_or_else(|| eyre::eyre!("no contract address in receipt"))?;

    tracing::info!(%address, "deployed FinalizationBridge");
    Ok(address)
}

/// Deploy FinalizationBridge on Anvil (with library linking).
async fn deploy_on_anvil(
    rpc_url: &str,
    origin_chain_id: u64,
    public_key: &[u8],
) -> eyre::Result<Address> {
    use alloy::signers::local::PrivateKeySigner;

    let signer: PrivateKeySigner = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
        .parse()
        .unwrap();
    let owner = signer.address();

    let provider = ProviderBuilder::new()
        .wallet(alloy::network::EthereumWallet::from(signer))
        .connect_http(rpc_url.parse()?);

    deploy_finalization_bridge_with_libraries(&provider, owner, origin_chain_id, public_key, 50_000_000).await
}

/// Deploy FinalizationBridge on Tempo.
async fn deploy_on_tempo(
    rpc_url: &str,
    origin_chain_id: u64,
    public_key: &[u8],
) -> eyre::Result<Address> {
    use alloy::signers::local::PrivateKeySigner;

    let signer: PrivateKeySigner = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
        .parse()
        .unwrap();
    let owner = signer.address();

    let provider = ProviderBuilder::new()
        .wallet(alloy::network::EthereumWallet::from(signer))
        .connect_http(rpc_url.parse()?);

    deploy_finalization_bridge_with_libraries(&provider, owner, origin_chain_id, public_key, 50_000_000).await
}

/// Deploy SimpleMessageSender on Tempo (for testing without BLS).
async fn deploy_simple_sender_on_tempo(rpc_url: &str) -> eyre::Result<Address> {
    use alloy::signers::local::PrivateKeySigner;

    let signer: PrivateKeySigner = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
        .parse()
        .unwrap();

    let provider = ProviderBuilder::new()
        .wallet(alloy::network::EthereumWallet::from(signer))
        .connect_http(rpc_url.parse()?);

    let bytecode = hex::decode(SIMPLE_MESSAGE_SENDER_BYTECODE.trim())?;
    // TIP-1000: Base deployment=500k + code deposit=1k/byte + account creation=250k
    // For ~585 bytes: 500k + 585k + 250k + intrinsic = ~1.5M gas minimum
    let tx = alloy::rpc::types::TransactionRequest::default()
        .with_deploy_code(Bytes::from(bytecode))
        .with_gas_limit(2_000_000);

    let pending = provider.send_transaction(tx).await?;
    let receipt = pending.get_receipt().await?;
    
    tracing::info!(
        tx_hash = %receipt.transaction_hash,
        status = receipt.status(),
        gas_used = receipt.gas_used,
        contract_address = ?receipt.contract_address,
        "SimpleMessageSender deployment receipt"
    );

    receipt
        .contract_address
        .ok_or_else(|| eyre::eyre!("no contract address in receipt"))
}

// =============================================================================
// Test Helpers
// =============================================================================

/// Wait for a validator to reach a target height by checking metrics.
async fn wait_for_height(context: &Context, target_height: u64) {
    loop {
        let metrics = context.encode();
        for line in metrics.lines() {
            if !line.starts_with(CONSENSUS_NODE_PREFIX) {
                continue;
            }
            let mut parts = line.split_whitespace();
            let metric = parts.next().unwrap();
            let value = parts.next().unwrap();
            if metric.ends_with("_marshal_processed_height") {
                let height = value.parse::<u64>().unwrap();
                if height >= target_height {
                    return;
                }
            }
        }
        context.sleep(Duration::from_millis(100)).await;
    }
}

// =============================================================================
// Tests
// =============================================================================

/// Test the full finalization flow with real consensus.
///
/// This test:
/// 1. Starts a single validator with full consensus
/// 2. Deploys FinalizationBridge on Tempo
/// 3. Sends a message via the bridge
/// 4. Waits for the block to be finalized
/// 5. Fetches finalization certificate from consensus RPC
/// 6. Starts Anvil and deploys FinalizationBridge
/// 7. Generates receipt MPT proof
/// 8. Submits the proof to Anvil bridge
/// 9. Verifies the message is received
#[tokio::test]
#[test_traced]
async fn finalization_bridge_e2e_with_real_consensus() {
    let _ = tempo_eyre::install();

    let initial_height = 5;
    let setup = Setup::new().how_many_signers(1).epoch_length(100);
    let cfg = deterministic::Config::default().with_seed(setup.seed);

    let (addr_tx, addr_rx) = oneshot::channel::<(SocketAddr, SocketAddr, Vec<u8>)>();
    let (done_tx, done_rx) = oneshot::channel::<()>();

    let executor_handle = std::thread::spawn(move || {
        let executor = Runner::from(cfg);
        executor.start(|mut context| async move {
            let (mut validators, _execution_runtime) = setup_validators(&mut context, setup).await;
            validators[0].start(&context).await;
            wait_for_height(&context, initial_height).await;

            let execution = validators[0].execution();

            let provider = validators[0].execution_provider();
            let genesis = provider.block_by_number(0).unwrap().unwrap();
            let extra_data = &genesis.header.inner.extra_data;
            let dkg_outcome =
                tempo_dkg_onchain_artifacts::OnchainDkgOutcome::read(&mut extra_data.as_ref())
                    .expect("valid DKG outcome in genesis");

            let group_pubkey = dkg_outcome.sharing().public();
            let pubkey_bytes = group_pubkey.encode();

            addr_tx
                .send((
                    execution.rpc_server_handles.rpc.http_local_addr().unwrap(),
                    execution.rpc_server_handles.rpc.ws_local_addr().unwrap(),
                    pubkey_bytes.to_vec(),
                ))
                .unwrap();

            let _ = done_rx.await;
        });
    });

    let (http_addr, ws_addr, group_pubkey_bytes) = addr_rx.await.unwrap();
    let ws_url = format!("ws://{ws_addr}");
    let http_url = format!("http://{http_addr}");

    tracing::info!(%http_url, %ws_url, "Tempo node ready");

    // Convert compressed G2 public key to EIP-2537 format
    let pubkey_compressed: [u8; G2_COMPRESSED_LEN] = group_pubkey_bytes
        .as_slice()
        .try_into()
        .expect("group pubkey should be 96 bytes");
    let pubkey_eip2537 = g2_to_eip2537(&pubkey_compressed).expect("valid G2 point");

    tracing::info!(
        pubkey_compressed_len = group_pubkey_bytes.len(),
        pubkey_eip2537_len = pubkey_eip2537.len(),
        "Converted group public key to EIP-2537 format"
    );

    // Get Tempo chain ID
    let tempo_provider = ProviderBuilder::new().connect_http(http_url.parse().unwrap());
    let tempo_chain_id = tempo_provider.get_chain_id().await.unwrap();
    tracing::info!(tempo_chain_id, "Got Tempo chain ID");

    // Deploy SimpleMessageSender on Tempo (simpler contract without BLS dependencies)
    // This is used to generate the MessageSent event that we'll prove on Anvil
    let tempo_sender = deploy_simple_sender_on_tempo(&http_url)
        .await
        .unwrap();

    // Verify contract has code
    let tempo_code = tempo_provider.get_code_at(tempo_sender).await.unwrap();
    tracing::info!(
        tempo_sender = %tempo_sender,
        code_len = tempo_code.len(),
        "Deployed SimpleMessageSender contract on Tempo"
    );
    assert!(tempo_code.len() > 0, "SimpleMessageSender should have code deployed");

    // Send a message on Tempo
    let message_hash = B256::repeat_byte(0x42);
    let destination_chain_id = 1u64; // Anvil chain ID

    let send_call = sendCall {
        messageHash: message_hash,
        destinationChainId: destination_chain_id,
    };

    let signer: alloy::signers::local::PrivateKeySigner =
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
            .parse()
            .unwrap();
    let sender = signer.address();

    let tempo_signer_provider = ProviderBuilder::new()
        .wallet(alloy::network::EthereumWallet::from(signer.clone()))
        .connect_http(http_url.parse().unwrap());

    tracing::info!(
        tempo_sender = %tempo_sender,
        message_hash = %hex::encode(message_hash.as_slice()),
        destination_chain_id,
        "Calling send() on SimpleMessageSender"
    );

    // TIP-1000: Account nonce 0->1 transition costs 250k gas, plus normal execution
    let tx = alloy::rpc::types::TransactionRequest::default()
        .to(tempo_sender)
        .with_gas_limit(500_000)
        .input(send_call.abi_encode().into());

    // First try eth_call to see if it reverts
    match tempo_signer_provider.call(tx.clone()).await {
        Ok(result) => {
            tracing::info!(result_len = result.len(), "eth_call succeeded");
        }
        Err(e) => {
            tracing::error!(?e, "eth_call FAILED - send() would revert");
        }
    }

    let pending = tempo_signer_provider.send_transaction(tx).await.unwrap();
    let send_receipt = pending.get_receipt().await.unwrap();

    tracing::info!(
        tx_hash = %send_receipt.transaction_hash,
        block_number = ?send_receipt.block_number,
        status = send_receipt.status(),
        gas_used = send_receipt.gas_used,
        logs_count = send_receipt.inner.logs().len(),
        "Sent message on Tempo"
    );

    // Check if the transaction succeeded
    if !send_receipt.status() {
        tracing::error!(
            gas_used = send_receipt.gas_used,
            "send() transaction failed - likely gas issue with TIP-1000"
        );
        panic!("send() transaction failed!");
    }

    // Log all events from the send receipt
    for (i, log) in send_receipt.inner.logs().iter().enumerate() {
        tracing::info!(
            log_index = i,
            address = %log.address(),
            topics = ?log.topics(),
            "Log in send receipt"
        );
    }

    let send_block_number = send_receipt.block_number.unwrap();

    // Subscribe to consensus events and wait for finalization
    let ws_client = WsClientBuilder::default().build(&ws_url).await.unwrap();
    let mut subscription = ws_client.subscribe_events().await.unwrap();
    let http_client = HttpClientBuilder::default().build(&http_url).unwrap();

    let mut finalized_block = None;
    while finalized_block.is_none() {
        let event = tokio::time::timeout(Duration::from_secs(60), subscription.next())
            .await
            .expect("timeout waiting for finalization")
            .unwrap()
            .unwrap();

        if let Event::Finalized { block, .. } = event {
            if let Some(height) = block.height {
                if height >= send_block_number {
                    tracing::info!(
                        height = ?block.height,
                        digest = %block.digest,
                        "Got finalization event for our block"
                    );
                    finalized_block = Some(block);
                }
            }
        }
    }

    let finalized_block = finalized_block.unwrap();
    let finalized_height = finalized_block.height.expect("finalized block should have height");

    // The finalization certificate covers the finalized block (not necessarily the send block).
    // However, finalizing block N means blocks 0..N are all finalized.
    // We need the finalization certificate for the block containing our transaction.
    // Query the finalization for the send block specifically.
    let queried = http_client
        .get_finalization(Query::Height(send_block_number))
        .await
        .unwrap()
        .expect("finalization should exist for send block");

    tracing::info!(
        send_block_number,
        finalized_height,
        queried_digest = %queried.digest,
        "Queried finalization for send block"
    );

    // Decode the finalization certificate
    let cert_bytes = hex::decode(&queried.certificate).unwrap();
    let finalization =
        Finalization::<Scheme<PublicKey, MinSig>, Digest>::read(&mut cert_bytes.as_slice())
            .expect("valid finalization");

    let proposal_encoded = finalization.proposal.encode();
    let vote_signature_compressed = finalization.certificate.vote_signature.encode();

    // Convert vote signature to EIP-2537 format
    let sig_compressed: [u8; G1_COMPRESSED_LEN] = vote_signature_compressed
        .as_ref()
        .try_into()
        .expect("vote signature should be 48 bytes");
    let vote_signature_eip2537 = g1_to_eip2537(&sig_compressed).expect("valid G1 point");

    tracing::info!(
        proposal_len = proposal_encoded.len(),
        sig_compressed_len = vote_signature_compressed.len(),
        sig_eip2537_len = vote_signature_eip2537.len(),
        "Converted finalization data to EIP-2537 format"
    );

    // Get the block header for the send block and encode to RLP
    // We need to use the raw RPC to get Tempo-specific header fields
    let block_details = tempo_provider
        .get_block_by_number(send_block_number.into())
        .await
        .unwrap()
        .expect("block should exist");

    // Get the raw block response to extract Tempo-specific header fields
    let raw_block: serde_json::Value = tempo_provider
        .client()
        .request("eth_getBlockByNumber", (format!("0x{:x}", send_block_number), false))
        .await
        .unwrap();

    tracing::info!(
        raw_block_header_keys = ?raw_block.as_object().map(|o| o.keys().collect::<Vec<_>>()),
        "Raw block header keys"
    );

    // Extract Tempo-specific header fields
    let general_gas_limit = raw_block["mainBlockGeneralGasLimit"]
        .as_str()
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0);
    let shared_gas_limit = raw_block["sharedGasLimit"]
        .as_str()
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0);
    let timestamp_millis_part = raw_block["timestampMillisPart"]
        .as_str()
        .and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .unwrap_or(0);

    tracing::info!(
        general_gas_limit,
        shared_gas_limit,
        timestamp_millis_part,
        "Extracted Tempo header fields"
    );

    let block_header_rlp = encode_tempo_block_header(
        &block_details.header,
        general_gas_limit,
        shared_gas_limit,
        timestamp_millis_part,
    )
    .unwrap();

    // Verify the encoded header hashes to the expected block hash
    let computed_hash = alloy_primitives::keccak256(&block_header_rlp);
    let expected_hash = block_details.header.hash;

    tracing::info!(
        block_number = send_block_number,
        header_rlp_len = block_header_rlp.len(),
        receipts_root = %block_details.header.receipts_root,
        computed_hash = %computed_hash,
        expected_hash = %expected_hash,
        "Encoded block header to RLP"
    );

    if computed_hash != expected_hash {
        tracing::warn!(
            computed = %computed_hash,
            expected = %expected_hash,
            "Block hash mismatch - header encoding may be incorrect"
        );
    }

    // Get all receipts in the send block and generate MPT proof
    let block_receipts = tempo_provider
        .get_block_receipts(send_block_number.into())
        .await
        .unwrap()
        .expect("block receipts should exist");

    let receipts_rlp: Vec<Vec<u8>> = block_receipts
        .iter()
        .map(|r| encode_receipt(r).unwrap())
        .collect();

    // Find the receipt index for our transaction
    let receipt_index = block_receipts
        .iter()
        .position(|r| r.transaction_hash == send_receipt.transaction_hash)
        .expect("our receipt should be in the block");

    let (computed_root, proof_nodes) = generate_receipt_proof(&receipts_rlp, receipt_index).unwrap();

    tracing::info!(
        receipt_index,
        computed_root = %computed_root,
        expected_root = %block_details.header.receipts_root,
        proof_nodes_count = proof_nodes.len(),
        "Generated receipt MPT proof"
    );

    // Find the log index for MessageSent event
    let send_tx_receipt = &block_receipts[receipt_index];
    
    // Debug: print all log topics
    for (i, log) in send_tx_receipt.inner.logs().iter().enumerate() {
        tracing::info!(
            log_index = i,
            address = %log.address(),
            topics = ?log.topics(),
            "Log in receipt"
        );
    }
    
    let expected_topic = MessageSent::SIGNATURE_HASH;
    tracing::info!(expected_topic = %expected_topic, "Looking for MessageSent topic");
    
    // Find the MessageSent event - it should come from the sender contract
    let log_index = send_tx_receipt
        .inner
        .logs()
        .iter()
        .position(|log| {
            log.address() == tempo_sender && log.topics().first() == Some(&expected_topic)
        })
        .expect("MessageSent event should be present from SimpleMessageSender");
    tracing::info!(log_index, "Found MessageSent log index");

    // Start Anvil with Prague hardfork
    let anvil = AnvilInstance::start().await.unwrap();
    tracing::info!(rpc_url = %anvil.rpc_url, "Anvil ready with Prague hardfork");

    // Deploy FinalizationBridge on Anvil with the actual group public key
    let anvil_bridge = deploy_on_anvil(&anvil.rpc_url, tempo_chain_id, &pubkey_eip2537)
        .await
        .unwrap();
    tracing::info!(%anvil_bridge, "FinalizationBridge deployed on Anvil");

    // Prepare proof for submission
    let receipt_proof: Vec<Bytes> = proof_nodes.into_iter().map(Bytes::from).collect();

    let write_call = writeCall {
        blockHeader: Bytes::from(block_header_rlp),
        finalizationProposal: Bytes::from(proposal_encoded.to_vec()),
        finalizationSignature: Bytes::from(vote_signature_eip2537.to_vec()),
        receiptProof: receipt_proof,
        receiptIndex: U256::from(receipt_index),
        logIndex: U256::from(log_index),
    };

    let anvil_signer_provider = ProviderBuilder::new()
        .wallet(alloy::network::EthereumWallet::from(signer.clone()))
        .connect_http(anvil.rpc_url.parse().unwrap());

    let tx = alloy::rpc::types::TransactionRequest::default()
        .to(anvil_bridge)
        .with_gas_limit(5_000_000)
        .input(write_call.abi_encode().into());

    tracing::info!("Submitting proof to Anvil bridge...");

    // First test with eth_call to get revert reason if it would fail
    match anvil_signer_provider.call(tx.clone()).await {
        Ok(result) => {
            tracing::info!(result_len = result.len(), "eth_call succeeded on write()");
        }
        Err(e) => {
            tracing::error!(?e, "eth_call FAILED - write() would revert");
            panic!("write() would revert: {e}");
        }
    }

    let result = anvil_signer_provider.send_transaction(tx).await;

    match result {
        Ok(pending) => {
            let write_receipt = pending.get_receipt().await.unwrap();
            if write_receipt.status() {
                tracing::info!(
                    tx_hash = %write_receipt.transaction_hash,
                    "✅ Proof submitted successfully!"
                );

                // Verify the message was received
                let received_call = receivedAtCall {
                    originChainId: tempo_chain_id,
                    sender,
                    messageHash: message_hash,
                };

                let call_tx = alloy::rpc::types::TransactionRequest::default()
                    .to(anvil_bridge)
                    .input(received_call.abi_encode().into());

                let result = anvil_signer_provider.call(call_tx).await.unwrap();
                let timestamp = U256::from_be_slice(&result);

                assert!(timestamp > U256::ZERO, "message should be received");
                tracing::info!(%timestamp, "✅ Message verified as received!");
            } else {
                tracing::error!("❌ Write transaction reverted");
                panic!("Write transaction reverted");
            }
        }
        Err(e) => {
            tracing::error!(?e, "❌ Failed to send write transaction");
            panic!("Failed to send write transaction: {e}");
        }
    }

    tracing::info!("✅ Finalization bridge e2e test passed - full integration verified");

    drop(done_tx);
    executor_handle.join().unwrap();
}
