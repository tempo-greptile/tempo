//! Drives the execution engine by forwarding consensus messages.

use std::{sync::Arc, time::Duration};
use tempo_node::TempoFullNode;

mod core;
pub(crate) use core::Application;

pub(super) mod executor;

use crate::{
    consensus::{application::executor::ExecutorMailbox, block::Block},
    dkg,
    epoch::SchemeProvider,
    subblocks,
};

pub(crate) struct Config {
    /// A handle to the dkg/reshare manager.
    pub(crate) dkg: dkg::manager::Mailbox,

    /// A handle to the execution service.
    pub(crate) executor: ExecutorMailbox,

    /// A handle to the subblocks service to get subblocks for proposals.
    pub(crate) subblocks: subblocks::Mailbox,

    /// The number of heights H in an epoch. For a given epoch E, all heights
    /// `E*H+1` to and including `(E+1)*H` make up the epoch. The block at
    /// `E*H` is said to be the genesis (or parent) of the epoch.
    pub(crate) epoch_length: u64,

    /// A handle to the execution node to verify and create new payloads.
    pub(crate) execution_node: TempoFullNode,

    /// The minimum amount of time to wait before resolving a new payload from the builder
    pub(crate) new_payload_wait_time: Duration,

    /// The scheme provider to use for the application.
    pub(crate) scheme_provider: SchemeProvider,

    /// Used as PayloadAttributes.suggested_fee_recipient
    pub(crate) fee_recipient: alloy_primitives::Address,

    /// The genesis block of the chain.
    pub(crate) genesis_block: Arc<Block>,
}
