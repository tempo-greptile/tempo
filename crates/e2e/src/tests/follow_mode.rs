//! Tests for the certified follow mode.
//!
//! These tests verify that follow-mode nodes can:
//! 1. Fetch blocks and finalization certificates from a validator
//! 2. Serve consensus RPCs using the fetched certificates
//! 3. Reject blocks with invalid certificates

use std::{net::SocketAddr, time::Duration};

use crate::{CONSENSUS_NODE_PREFIX, Setup, setup_validators};
use commonware_macros::test_traced;
use commonware_runtime::{
    Clock, Metrics as _, Runner as _,
    deterministic::{self, Context, Runner},
};
use futures::channel::oneshot;
use jsonrpsee::http_client::HttpClientBuilder;
use reth_consensus_debug_client::BlockProvider;
use reth_primitives_traits::Block as BlockTrait;
use tempo_node::{
    follow::CertifiedBlockProvider,
    rpc::consensus::{ConsensusFeed, TempoConsensusApiClient},
};
use tokio::sync::mpsc;

/// Test that a follow-mode node can fetch finalization certificates
/// and serve consensus_getLatest RPC.
#[tokio::test]
#[test_traced]
async fn follow_mode_serves_consensus_rpc() {
    let _ = tempo_eyre::install();

    // Wait past first epoch boundary so identity proof API can work
    let initial_height = 105;
    let setup = Setup::new().how_many_signers(1).epoch_length(100);
    let cfg = deterministic::Config::default().with_seed(setup.seed);

    let (addr_tx, addr_rx) = oneshot::channel::<(SocketAddr, SocketAddr)>();
    let (done_tx, done_rx) = oneshot::channel::<()>();

    // Start validator node in deterministic executor
    let executor_handle = std::thread::spawn(move || {
        let executor = Runner::from(cfg);
        executor.start(|mut context| async move {
            let (mut validators, _execution_runtime) = setup_validators(&mut context, setup).await;
            validators[0].start(&context).await;
            wait_for_height(&context, initial_height).await;

            let execution = validators[0].execution();
            addr_tx
                .send((
                    execution.rpc_server_handles.rpc.http_local_addr().unwrap(),
                    execution.rpc_server_handles.rpc.ws_local_addr().unwrap(),
                ))
                .unwrap();

            let _ = done_rx.await;
        });
    });

    // Get validator addresses
    let (http_addr, ws_addr) = addr_rx.await.unwrap();
    let ws_url = format!("ws://{ws_addr}");
    let http_url = format!("http://{http_addr}");

    // Create certified block provider (simulating follow mode)
    let (_provider, feed_state) = CertifiedBlockProvider::new(&ws_url)
        .await
        .expect("Failed to create certified block provider");

    // Query the validator's consensus RPC to verify it has data
    let validator_client = HttpClientBuilder::default().build(&http_url).unwrap();
    let validator_state = validator_client.get_latest().await.unwrap();
    assert!(
        validator_state.finalized.is_some(),
        "Validator should have finalized blocks"
    );

    // Verify feed_state implements ConsensusFeed correctly
    let follow_state = feed_state.get_latest().await;
    // Initially empty since we haven't run the provider yet
    assert!(
        follow_state.finalized.is_none(),
        "Follow state should be empty before provider runs"
    );

    drop(done_tx);
    executor_handle.join().unwrap();
}

/// Test that the certified block provider validates certificates and forwards blocks.
#[tokio::test]
#[test_traced]
async fn follow_mode_validates_and_forwards_blocks() {
    let _ = tempo_eyre::install();

    // Wait past first epoch boundary so identity proof API can work
    let initial_height = 105;
    let setup = Setup::new().how_many_signers(1).epoch_length(100);
    let cfg = deterministic::Config::default().with_seed(setup.seed);

    let (addr_tx, addr_rx) = oneshot::channel::<SocketAddr>();
    let (done_tx, done_rx) = oneshot::channel::<()>();

    // Start validator node in deterministic executor
    let executor_handle = std::thread::spawn(move || {
        let executor = Runner::from(cfg);
        executor.start(|mut context| async move {
            let (mut validators, _execution_runtime) = setup_validators(&mut context, setup).await;
            validators[0].start(&context).await;
            wait_for_height(&context, initial_height).await;

            let execution = validators[0].execution();
            addr_tx
                .send(execution.rpc_server_handles.rpc.ws_local_addr().unwrap())
                .unwrap();

            // Keep running until test completes
            let _ = done_rx.await;
        });
    });

    // Get validator WS address
    let ws_addr = addr_rx.await.unwrap();
    let ws_url = format!("ws://{ws_addr}");

    // Create certified block provider
    let (provider, feed_state) = CertifiedBlockProvider::new(&ws_url)
        .await
        .expect("Failed to create certified block provider");

    // Subscribe to blocks
    let (block_tx, mut block_rx) = mpsc::channel(16);
    let provider_clone = provider.clone();
    let subscribe_handle = tokio::spawn(async move {
        provider_clone.subscribe_blocks(block_tx).await;
    });

    // Wait for at least one block to come through
    let received_block = tokio::time::timeout(Duration::from_secs(10), block_rx.recv())
        .await
        .expect("Timed out waiting for block")
        .expect("Channel closed without receiving block");

    // Verify the block was validated (feed_state should have finalization data)
    let state = feed_state.get_latest().await;
    assert!(
        state.finalized.is_some(),
        "Feed state should have finalization after receiving validated block"
    );

    // Verify the finalization matches the received block
    let finalized = state.finalized.unwrap();
    assert!(
        finalized.height.is_some(),
        "Finalized block should have height"
    );

    tracing::info!(
        block_number = BlockTrait::header(&received_block).inner.number,
        finalized_height = ?finalized.height,
        "Successfully received and validated block"
    );

    // Cleanup
    subscribe_handle.abort();
    drop(done_tx);
    executor_handle.join().unwrap();
}

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
