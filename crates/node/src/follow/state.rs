//! Follow mode feed state for serving consensus RPCs.

use crate::rpc::consensus::{
    CertifiedBlock, ConsensusFeed, ConsensusState, Event, IdentityProofError,
    IdentityTransitionResponse, Query,
};
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;

const BROADCAST_CHANNEL_SIZE: usize = 1024;

/// Internal state for the follow feed.
struct FeedState {
    latest_finalized: Option<CertifiedBlock>,
}

/// Feed state for follow mode nodes.
///
/// This implements `ConsensusFeed` to serve consensus RPC queries
/// using data fetched and stored by the `CertifiedBlockProvider`.
#[derive(Clone)]
pub struct FollowFeedState {
    state: Arc<RwLock<FeedState>>,
    events_tx: broadcast::Sender<Event>,
}

impl FollowFeedState {
    /// Create a new follow feed state.
    pub fn new() -> Self {
        let (events_tx, _) = broadcast::channel(BROADCAST_CHANNEL_SIZE);
        Self {
            state: Arc::new(RwLock::new(FeedState {
                latest_finalized: None,
            })),
            events_tx,
        }
    }

    /// Update the latest finalized block.
    pub fn set_finalized(&self, block: CertifiedBlock) {
        if let Ok(mut state) = self.state.write() {
            state.latest_finalized = Some(block.clone());
        }
        let _ = self.events_tx.send(Event::Finalized {
            block,
            seen: now_millis(),
        });
    }

    /// Get the events sender for broadcasting.
    pub fn events_tx(&self) -> &broadcast::Sender<Event> {
        &self.events_tx
    }
}

impl Default for FollowFeedState {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for FollowFeedState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug_struct = f.debug_struct("FollowFeedState");
        if let Ok(state) = self.state.read() {
            debug_struct.field("latest_finalized", &state.latest_finalized);
        }
        debug_struct
            .field("subscriber_count", &self.events_tx.receiver_count())
            .finish()
    }
}

impl ConsensusFeed for FollowFeedState {
    async fn get_finalization(&self, query: Query) -> Option<CertifiedBlock> {
        match query {
            Query::Latest => self
                .state
                .read()
                .ok()
                .and_then(|s| s.latest_finalized.clone()),
            Query::Height(_height) => {
                // TODO: Query from stored archive once implemented
                // For now, only support latest
                None
            }
        }
    }

    async fn get_latest(&self) -> ConsensusState {
        let finalized = self
            .state
            .read()
            .ok()
            .and_then(|s| s.latest_finalized.clone());
        ConsensusState {
            finalized,
            // Follow mode doesn't track notarizations
            notarized: None,
        }
    }

    async fn subscribe(&self) -> Option<broadcast::Receiver<Event>> {
        Some(self.events_tx.subscribe())
    }

    async fn get_identity_transition_proof(
        &self,
        _from_epoch: Option<u64>,
        _full: bool,
    ) -> Result<IdentityTransitionResponse, IdentityProofError> {
        // TODO: Implement once we store epoch identity data
        Err(IdentityProofError::NotReady)
    }
}

/// Get current Unix timestamp in milliseconds.
fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
