use alloy::providers::Provider;
use std::time::{Duration, Instant};
use tokio::time::sleep;

async fn wait_for<F, Fut>(mut cond: F, timeout: Duration, poll_every: Duration) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = Instant::now() + timeout;

    loop {
        if cond().await {
            return true;
        }

        let now = Instant::now();
        if now >= deadline {
            return false;
        }

        let sleep_for = poll_every.min(deadline.saturating_duration_since(now));
        sleep(sleep_for).await;
    }
}

pub async fn wait_for_block<P: Provider>(
    provider: P,
    block_number: u64,
    timeout: Duration,
) -> bool {
    wait_for(
        async || {
            match provider.get_block_number().await {
                Ok(num) => num >= block_number,
                Err(_) => false,
            }
        },
        timeout,
        Duration::from_secs(1),
    )
    .await
}

pub async fn wait_for_blocks<P: Provider>(
    provider: P,
    blocks_to_pass: u64,
    timeout: Duration,
) -> bool {
    let current_block_number = provider.get_block_number().await.unwrap();

    wait_for(
        async || {
            match provider.get_block_number().await {
                Ok(num) => {
                    num >= current_block_number + blocks_to_pass
                },
                Err(_) => false,
            }
        },
        timeout,
        Duration::from_secs(1),
    )
    .await
}

pub async fn wait_for_live_rpc<P: Provider>(provider: P, timeout: Duration) -> bool {
    wait_for(
        async || {
            match provider.get_block_number().await {
                Ok(_) => true,
                Err(e) => {
                    println!("{:?}",e);
                    false
                },
            }
        },
        timeout,
        Duration::from_secs(1),
    )
    .await
}
