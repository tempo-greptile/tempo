use std::time::Duration;
use tempo_e2e_tests::{
    node::setup_validators,
    utils::{wait_for_block, wait_for_blocks, wait_for_live_rpc},
};

#[tokio::test]
async fn basic_spin_up() {
    let validators = setup_validators(4).await;

    let provider = validators.first().unwrap().get_eth_provider().await;
    assert!(
        wait_for_live_rpc(provider.clone(), Duration::from_secs(45)).await,
        "network failed to start"
    );
    assert!(
        wait_for_block(provider, 4, Duration::from_secs(45)).await,
        "network failed to produce blocks"
    );
}

#[tokio::test]
async fn network_survives_one_validator_down() {
    let validators = setup_validators(4).await;

    let provider = validators.first().unwrap().get_eth_provider().await;

    assert!(
        wait_for_live_rpc(provider.clone(), Duration::from_secs(45)).await,
        "network failed to start"
    );
    assert!(
        wait_for_block(provider.clone(), 4, Duration::from_secs(45)).await,
        "network failed to produce blocks"
    );

    let container = validators.get(1).unwrap().container();
    container.stop().await.unwrap();

    assert!(
        wait_for_blocks(provider.clone(), 5, Duration::from_secs(30)).await,
        "network stopped producing blocks"
    );
}
