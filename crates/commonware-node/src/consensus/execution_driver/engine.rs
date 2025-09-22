//! Drives the execution engine by forwarding consensus messages.

use std::{sync::Arc, time::SystemTime};

use alloy_primitives::{B64, B256};
use alloy_rpc_types_engine::ForkchoiceState;
use commonware_consensus::{Automaton, Block as _, Relay, Reporter, marshal};
use commonware_runtime::{Clock, Handle, Metrics, Spawner, Storage};

use commonware_utils::SystemTimeExt;
use eyre::{OptionExt, WrapErr as _, bail, ensure, eyre};
use futures_channel::{mpsc, oneshot};
use futures_util::{
    SinkExt as _, StreamExt as _, TryFutureExt,
    future::{Either, try_join},
};
use rand::{CryptoRng, Rng};
use reth::{network::SyncState, payload::EthPayloadBuilderAttributes, rpc::types::Withdrawals};
use reth_node_builder::ConsensusEngineHandle;
use reth_primitives_traits::SealedBlock;
use tempo_node::{TempoExecutionData, TempoFullNode, TempoPayloadTypes};

use reth_provider::BlockReader as _;
use tokio::sync::RwLock;
use tracing::{Level, info, instrument};

use tempo_commonware_node_cryptography::{BlsScheme, Digest};

use super::{
    super::{View, block::Block},
    finalizer,
};

pub struct Builder<TContext> {
    /// The execution context of the commonwarexyz application (tokio runtime, etc).
    pub context: TContext,

    /// Used as PayloadAttributes.suggested_fee_recipient
    pub fee_recipient: alloy_primitives::Address,

    /// Number of messages from consensus to hold in our backlog
    /// before blocking.
    pub mailbox_size: usize,

    /// The syncer for subscribing to blocks distributed via the consensus
    /// p2p network.
    pub syncer_mailbox: marshal::Mailbox<BlsScheme, Block>,

    /// A handle to the execution node to verify and create new payloads.
    pub execution_node: TempoFullNode,
}

impl<TContext> Builder<TContext>
where
    TContext: Clock + governor::clock::Clock + Rng + CryptoRng + Spawner + Storage + Metrics,
{
    pub(super) fn try_init(self) -> eyre::Result<ExecutionDriver<TContext>> {
        let (tx, rx) = mpsc::channel(self.mailbox_size);
        let my_mailbox = ExecutionDriverMailbox::from_sender(tx);

        let block = self
            .execution_node
            .provider
            .block_by_number(0)
            .map_err(Into::<eyre::Report>::into)
            .and_then(|maybe| maybe.ok_or_eyre("block reader returned empty genesis block"))
            .wrap_err("failed reading genesis block from execution node")?;

        let finalizer = finalizer::Builder {
            execution_node: self.execution_node.clone(),
        }
        .build();
        let to_finalizer = finalizer.mailbox().clone();

        self.context
            .with_label("finalizer")
            .spawn(move |_| finalizer.run());
        Ok(ExecutionDriver {
            context: self.context,

            fee_recipient: self.fee_recipient,

            from_consensus: rx,
            my_mailbox,
            syncer_mailbox: self.syncer_mailbox,

            genesis_block: Arc::new(Block::from_execution_block(SealedBlock::seal_slow(block))),

            latest_proposed_block: Arc::new(RwLock::new(None)),

            execution_node: self.execution_node,

            to_finalizer,
        })
    }
}

/// Manages access to the execution engine.
///
/// # Nomenclature
///
/// -**Canonical Chain**: The sequence of blocks that was agreed upon. For example this chain exposed to end users via RPC: [0..Latest]
/// -**(Latest) Head**: The highest canonical block, aka the canonical head/tip
/// -**Pending block**: The block that isn't part of the canonical chain (yet) but is canonical + 1
///
/// The engine reacts to messages emitted by consensus [`Automaton`] and is responsible for "blocks in, blocks out", which includes all of:
///
/// # Payload(Block) processing
///
/// On [`Automaton::verify`] the corresponding payload must be validated via the EL.
/// `verify` provides the digest and the parent digest of the payload, the marshaller is responsible for providing the corresponding payload.
/// If the payload was obtained from the marshaller, the payload is handed to the EL for validation.
/// Validation is only possible if this payload can be attached to the canonical chain: e.g. payload `N+1` and canonical head `N`.
///
/// If validation returns `SYNCING`, then the EL was unable to attach the payload and we can try to fill the gap.
///
/// On [`Automaton::verify`] the parent digest is _always_ notarized.
///
/// After a payload is validated by the EL (`VALID`) is is considered a candidate, it is the "pending" candidate if it is `canonical block + 1`
///
/// # Syncing
///
/// Syncing effectively means "advance the EL chain to the head".
///
/// ## Live Sync
/// Regular mode of operation after the chain was fully synced and the chain advances entirely by validating and appending payloads received from consensus.
///
/// ## Backfill Sync
///
/// If the node fell behind (e.g. after downtime) or during initial sync, the node must first catch up to the tip of the advancing chain before transitioning into live sync. On new payload or forkchoice update that resulted in a `SYNCING` response, the node has a gap of `(local tip..head]` that must be filled first before it can validate and propose new blocks.
/// Filling this gap can be done in two ways:
/// * filling gap one block at a time: the EL node can source missing blocks from the EL p2p or the consensus can forward payloads it receives from the marshaller.
/// * filling larger gaps in chunks: if the gap is larger (>100 blocks), the EL can enter EL p2p backfill sync where the entire missing range is downloaded and executed. This process can be repeated multiple times before it fully caught up with the tip and is able to transition to live sync.
///
/// Advancing the chain is done by setting a new `head` hash via forkchoice update, the head hash can be "unsafe" (can be reorged), finalized must be actually final and reorgs below that block height must be ruled out.
///
/// # Proposing
///
/// A node is only able to propose a block on top of the canonical chain, if the node doesn't have the parent block, it can't build the payload. In this case it is unable to propose.
///
/// # Commonware x EL
///
/// The EL is purely reactive and listens for:
///
/// `newPayload`: request for validating a payload.
/// `newPayload` maps to [`Automaton::verify`] if consensus has the payload, but it can also be invoked to provide the EL with missing blocks (e.g. on restart).
///
/// `forkchoiceUpdated`: request to advance the chain to a new head block and update the finalized block.
/// Normal mode (e.g. live sync), a valid `newPayload` is followed by a FCU that sets this block as valid if it extends the canonical chain.
/// In commonware finalized blocks are emitted by the marshaller, these blocks must be tracked for the finalized height. If the EL doesn't have the payload, the chain can be advanced via `newPayload(finalized) + fcu(head=finalized)` combination.
/// Advancing the head can also be done on [`Automaton::verify`], where the parent is already notarized and if the payload is valid in can be made part of the canonical chain via `fcu(head=payload)` or `fcu(head=parent)`.
///
/// ## Caveats
///
/// Unwinding the canonical chain will be ignored by the EL. This can be requested via a `fcu(head)` where the head hash is already part of the canonical chain: `current tip N` and `fcu(head=N-2)`, will ignored.
/// Reorgs below the finalized height must be avoided: The section `(finalized..head]` can safely be reorged, but `finalized` is considered final by the EL.
/// The execution layer processes requests sequentially, this type must avoid introducing blockers that would result in a buildup of pending messages, so that new incoming consensus messages aren't dropped or message processing stalls.
/// This should also avoid blocking IO (`provider` calls) in the hotpath.
// TODO make this type more like a dispatcher, this could operate similar to lighthouse router https://github.com/sigp/lighthouse/blob/3cb7e59be2ebcf66836dabae2c771b455822f654/beacon_node/network/src/router.rs#L29-L30 and `Beaconchain` type https://github.com/sigp/lighthouse/blob/3cb7e59be2ebcf66836dabae2c771b455822f654/beacon_node/beacon_chain/src/beacon_chain.rs#L371-L371, e.g. this type handles ingress from consensus and dispatches messages accordingly, but in our case this is a lot simpler and we can probably do everything in one type as well
pub struct ExecutionDriver<TContext> {
    context: TContext,

    fee_recipient: alloy_primitives::Address,

    from_consensus: mpsc::Receiver<Message>,
    my_mailbox: ExecutionDriverMailbox,

    syncer_mailbox: marshal::Mailbox<BlsScheme, Block>,

    // TODO: move into state
    genesis_block: Arc<Block>,
    latest_proposed_block: Arc<RwLock<Option<Block>>>,

    execution_node: TempoFullNode,
}

impl<TContext> ExecutionDriver<TContext>
where
    TContext: Clock + governor::clock::Clock + Rng + CryptoRng + Spawner + Storage + Metrics,
{
    pub(super) fn mailbox(&self) -> &ExecutionDriverMailbox {
        &self.my_mailbox
    }

    async fn run(mut self) {
        loop {
            tokio::select!(
                // NOTE: biased because we prefer running finalizations above all else.
                biased;

                Some(msg) = self.from_consensus.next() => {
                    if let Err(error) =  self.handle_message(msg) {
                        tracing::error_span!("handle message").in_scope(|| tracing::error!(
                            %error,
                            "critical error occurred while handling message; exiting"
                        ));
                        break;
                    }
                }

                else => break,
            )
        }
    }

    pub(super) fn start(mut self) -> Handle<()> {
        self.context.spawn_ref()(self.run())
    }

    fn handle_message(&mut self, msg: Message) -> eyre::Result<()> {
        match msg {
            Message::Broadcast(broadcast) => self.handle_broadcast(broadcast),
            Message::Finalized(finalized) => {
                self.handle_finalized(*finalized);
            }
            Message::Genesis(genesis) => _ = self.handle_genesis(genesis),
            Message::Propose(propose) => self.handle_propose(propose),
            Message::Verify(verify) => self.handle_verify(verify),
        }
        Ok(())
    }

    fn handle_broadcast(&mut self, broadcast: Broadcast) {}

    /// Pushes a `finalized` request to the back of the finalization queue.
    fn handle_finalized(&self, finalized: Finalized) {
        // TODO: update finalized tracker
    }

    fn handle_genesis(&mut self, genesis: Genesis) {
        let genesis_digest = self.genesis_block.digest();
        let _ = genesis.response.send(genesis_digest);
    }

    fn handle_propose(&mut self, propose: Propose) {
        // TODO: check if we can build a new payload on top of this block (has parent block)
        //  spawn a task that takes care of this
    }

    fn handle_verify(&self, verify: Verify) {
        // TODO: spawn task that obtains the corresponding payload -> BlockProcessor
        //  if newpayload results in syncing, this must kickoff marshaller payload forwarding,
    }
}

impl Automaton for ExecutionDriverMailbox {
    type Context = super::super::Context;

    type Digest = Digest;

    async fn genesis(&mut self) -> Self::Digest {
        let (tx, rx) = oneshot::channel();
        // TODO: panicking here really is not good. there's actually no requirement on `Self::Context` nor `Self::Digest` to fulfill
        // any invariants, so we could just turn them into `Result<Context, Error>` and be happy.
        self.to_execution_driver
            .send(Genesis { response: tx }.into())
            .await
            .expect("application is present and ready to receive genesis");
        rx.await
            .expect("application returns the digest of the genesis")
    }

    async fn propose(&mut self, context: Self::Context) -> oneshot::Receiver<Self::Digest> {
        // TODO: panicking here really is not good. there's actually no requirement on `Self::Context` nor `Self::Digest` to fulfill
        // any invariants, so we could just turn them into `Result<Context, Error>` and be happy.
        //
        // XXX: comment taken from alto - what does this mean? is this relevant to us?
        // > If we linked payloads to their parent, we would verify
        // > the parent included in the payload matches the provided `Context`.
        let (tx, rx) = oneshot::channel();
        self.to_execution_driver
            .send(
                Propose {
                    view: context.view,
                    parent: context.parent,
                    response: tx,
                }
                .into(),
            )
            .await
            .expect("application is present and ready to receive proposals");
        rx
    }

    async fn verify(
        &mut self,
        context: Self::Context,
        payload: Self::Digest,
    ) -> oneshot::Receiver<bool> {
        // TODO: panicking here really is not good. there's actually no requirement on `Self::Context` nor `Self::Digest` to fulfill
        // any invariants, so we could just turn them into `Result<Context, Error>` and be happy.
        //
        // XXX: comment taken from alto - what does this mean? is this relevant to us?
        // > If we linked payloads to their parent, we would verify
        // > the parent included in the payload matches the provided `Context`.
        let (tx, rx) = oneshot::channel();
        self.to_execution_driver
            .send(
                Verify {
                    view: context.view,
                    parent: context.parent,
                    payload,
                    response: tx,
                }
                .into(),
            )
            .await
            .expect("application is present and ready to receive verify requests");
        rx
    }
}

impl Relay for ExecutionDriverMailbox {
    type Digest = Digest;

    async fn broadcast(&mut self, digest: Self::Digest) {
        // TODO: panicking here is really not necessary. Just log at the ERROR or WARN levels instead?
        self.to_execution_driver
            .send(Broadcast { payload: digest }.into())
            .await
            .expect("application is present and ready to receive broadcasts");
    }
}

impl Reporter for ExecutionDriverMailbox {
    type Activity = Block;

    async fn report(&mut self, block: Self::Activity) {
        let (response, rx) = oneshot::channel();
        // TODO: panicking here is really not necessary. Just log at the ERROR or WARN levels instead?
        self.to_execution_driver
            .send(Finalized { block, response }.into())
            .await
            .expect("application is present and ready to receive broadcasts");

        // XXX: This is used as an acknowledgement that the application
        // finalized the block:
        // Response on this channel -> future returns -> marshaller gets an ack
        //
        // TODO(janis): report if this channel gets dropped?
        let _ = rx.await;
    }
}

/// Communication channel to the [`ExecutionDriver`].
///
/// This is used to forward messages from the consensus to the execution engine.
#[derive(Clone)]
pub struct ExecutionDriverMailbox {
    to_execution_driver: mpsc::Sender<Message>,
}

impl ExecutionDriverMailbox {
    fn from_sender(to_execution_driver: mpsc::Sender<Message>) -> Self {
        Self {
            to_execution_driver,
        }
    }
}

// TODO: maybe this doesnt even need to be a separate type?
struct BlockProcessor {
    /// Message channel from the execution driver.
    from_driver_rx: (),

    state: EngineState,

    // TODO: some queue where newPayloads are spawned

    // TODO: some kind of receiver from the marshaller task that fetches missing payloads if newpayload resulted in SYNCING

    // TODO: on successful verify this should then also perform the FCU with the newest head and current finalized, must ensure that head>=finalized, likely need some lock for updating the FCU: https://github.com/sigp/lighthouse/blob/3cb7e59be2ebcf66836dabae2c771b455822f654/beacon_node/beacon_chain/src/beacon_chain.rs#L6158-L6172

}

/// Keeps track of engine related state.
struct EngineState {
    inner: Arc<EngineStateInner>,
}

struct EngineStateInner {
    // TODO: perhaps we should add a cache here so that we can easily lookup previous payloads,
    //  in case we need this, maybe when handling finalized messages.
    /// latest valid forkchoice state
    latest_forkchoice_state: RwLock<Option<ForkchoiceState>>,
    /// Syncing state of the node
    sync_state: RwLock<SyncState>,
}

/// Messages forwarded from consensus to execution driver.
// TODO: add trace spans into all of these messages.
enum Message {
    Broadcast(Broadcast),
    Finalized(Box<Finalized>),
    Genesis(Genesis),
    Propose(Propose),
    Verify(Verify),
}

struct Genesis {
    response: oneshot::Sender<Digest>,
}

impl From<Genesis> for Message {
    fn from(value: Genesis) -> Self {
        Self::Genesis(value)
    }
}

struct Propose {
    view: View,
    parent: (View, Digest),
    response: oneshot::Sender<Digest>,
}

impl From<Propose> for Message {
    fn from(value: Propose) -> Self {
        Self::Propose(value)
    }
}

struct Broadcast {
    payload: Digest,
}

impl From<Broadcast> for Message {
    fn from(value: Broadcast) -> Self {
        Self::Broadcast(value)
    }
}

struct Verify {
    view: View,
    parent: (View, Digest),
    payload: Digest,
    response: oneshot::Sender<bool>,
}

impl From<Verify> for Message {
    fn from(value: Verify) -> Self {
        Self::Verify(value)
    }
}

#[derive(Debug)]
struct Finalized {
    block: Block,
    response: oneshot::Sender<()>,
}

impl From<Finalized> for Message {
    fn from(value: Finalized) -> Self {
        Self::Finalized(value.into())
    }
}
