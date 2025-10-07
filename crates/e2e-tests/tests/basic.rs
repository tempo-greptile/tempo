use alloy::providers::{Provider, ProviderBuilder};
use std::time::Duration;
use tempo_e2e_tests::utils::setup_validators;
use tokio::time::sleep;
use url::Url;

#[tokio::test]
async fn basic_spin_up() {
    let validators = setup_validators(4).await;

    let first_node_url = validators
        .first()
        .unwrap()
        .get_eth_rpc_url()
        .await
        .expect("there's more than one node");

    let provider = ProviderBuilder::new()
        .connect_http(Url::parse(&first_node_url).unwrap());

    for _ in 1..30 {
        sleep(Duration::from_secs(1)).await;

        let block_number_result = provider.get_block_number().await;

        match block_number_result {
            Ok(block_number) => {
                if block_number > 4 {
                    break;
                }
            }
            Err(_) => continue,
        }
    }

    assert!(provider.get_block_number().await.is_ok_and(|i| i > 4));
}
