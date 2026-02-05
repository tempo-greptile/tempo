//! Certified follow mode for Tempo nodes.
//!
//! This module provides a block provider that wraps RPC block fetching with
//! finalization certificate verification and storage. This enables follow-mode
//! nodes to:
//!
//! 1. Fetch blocks from an upstream RPC endpoint
//! 2. Fetch and store finalization certificates for each block
//! 3. Serve consensus RPCs (`consensus_getFinalization`, `consensus_getLatest`)
//!
//! ## Architecture
//!
//! - [`CertifiedBlockProvider`]: Wraps `RpcBlockProvider` to fetch and store certificates
//! - [`FollowFeedState`]: Implements `ConsensusFeed` to serve consensus RPC queries
//!
//! ## Usage
//!
//! ```ignore
//! let (provider, feed_state) = CertifiedBlockProvider::new(rpc_url, data_dir).await?;
//! builder
//!     .launch_with_debug_capabilities()
//!     .with_debug_block_provider(provider)
//!     .await?;
//! ```

mod provider;
mod state;

pub use provider::CertifiedBlockProvider;
pub use state::FollowFeedState;
