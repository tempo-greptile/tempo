use super::executor::ExecutorMailbox;
use crate::{
    consensus::{Digest, block::Block},
    dkg::{self, PublicOutcome},
    epoch::SchemeProvider,
    subblocks,
};
use alloy_consensus::BlockHeader;
use alloy_primitives::{Address, B256, Bytes};
use alloy_rpc_types_engine::PayloadId;
use commonware_codec::{DecodeExt as _, Encode as _};
use commonware_consensus::{
    Block as _,
    marshal::{SchemeProvider as _, ingress::mailbox::AncestorStream},
    simplex::{signing_scheme::bls12381_threshold::Scheme, types::Context},
    types::Epoch,
    utils,
};
use commonware_cryptography::{
    Committable, bls12381::primitives::variant::MinSig, ed25519::PublicKey,
};
use commonware_runtime::{Clock, FutureExt as _, Metrics, Pacer, Spawner};
use commonware_utils::SystemTimeExt;
use eyre::{OptionExt, WrapErr as _, bail, eyre};
use futures::StreamExt as _;
use rand::Rng;
use reth_node_builder::ConsensusEngineHandle;
use std::{sync::Arc, time::Duration};
use tempo_node::{TempoExecutionData, TempoFullNode, TempoPayloadTypes};
use tempo_payload_types::TempoPayloadBuilderAttributes;
use tracing::{debug, info, instrument, warn};

#[derive(Clone)]
pub(crate) struct Application<E: Rng + Spawner + Metrics + Clock> {
    dkg_mailbox: dkg::manager::Mailbox,
    executor_mailbox: ExecutorMailbox,
    subblocks_mailbox: subblocks::Mailbox,

    epoch_length: u64,
    execution_node: Arc<TempoFullNode>,
    new_payload_wait_time: Duration,
    scheme_provider: SchemeProvider,

    genesis_block: Arc<Block>,
    fee_recipient: Address,

    _marker: std::marker::PhantomData<E>,
}

impl<E> Application<E>
where
    E: Rng + Spawner + Metrics + Clock,
{
    pub(crate) fn new(config: super::Config) -> Self {
        Self {
            dkg_mailbox: config.dkg,
            executor_mailbox: config.executor,
            subblocks_mailbox: config.subblocks,
            epoch_length: config.epoch_length,
            execution_node: Arc::new(config.execution_node),
            new_payload_wait_time: config.new_payload_wait_time,
            scheme_provider: config.scheme_provider,
            genesis_block: config.genesis_block,
            fee_recipient: config.fee_recipient,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<E> commonware_consensus::Application<E> for Application<E>
where
    E: Rng + Spawner + Metrics + Clock + Pacer,
{
    type SigningScheme = Scheme<PublicKey, MinSig>;
    type Context = Context<Digest, PublicKey>;
    type Block = Block;

    async fn genesis(&mut self) -> Self::Block {
        self.genesis_block.as_ref().clone()
    }

    async fn propose(
        &mut self,
        (runtime_context, consensus_context): (E, Self::Context),
        mut ancestry: AncestorStream<Self::SigningScheme, Self::Block>,
    ) -> Option<Self::Block> {
        // Fetch the parent from the ancestry stream
        let parent = ancestry.next().await?;

        if let Err(error) = self
            .executor_mailbox
            .canonicalize_head(parent.height(), parent.digest())
        {
            warn!(
                %error,
                parent.height = parent.height(),
                parent.digest = %parent.digest(),
                "failed updating canonical head to parent",
            );
        }

        // Query DKG manager for ceremony data before building payload
        // This data will be passed to the payload builder via attributes
        let extra_data = if utils::last_block_in_epoch(
            self.epoch_length,
            consensus_context.round.epoch(),
        ) == parent.height() + 1
        {
            // At epoch boundary: include public ceremony outcome
            let outcome_req = self
                .dkg_mailbox
                .get_public_ceremony_outcome(consensus_context.round.epoch())
                .await
                .transpose()
                .ok_or_eyre("public dkg ceremony outcome does not exist")
                .and_then(|this| this)
                .wrap_err("failed getting public dkg ceremony outcome");
            let outcome = match outcome_req {
                Ok(outcome) => outcome,
                Err(error) => {
                    warn!(
                        %error,
                        "failed getting dkg ceremony outcome",
                    );
                    return None;
                }
            };

            outcome.encode().freeze().into()
        } else {
            // Regular block: try to include intermediate dealing
            match self
                .dkg_mailbox
                .get_intermediate_dealing(consensus_context.round.epoch())
                .await
            {
                Err(error) => {
                    warn!(
                        %error,
                        "failed getting ceremony deal for current epoch because DKG manager went away",
                    );
                    Bytes::default()
                }
                Ok(None) => Bytes::default(),
                Ok(Some(deal_outcome)) => {
                    info!(
                        "found ceremony deal outcome; will include in payload builder attributes"
                    );
                    deal_outcome.encode().freeze().into()
                }
            }
        };

        let subblocks = self.subblocks_mailbox.clone();
        let parent_hash = parent.block_hash();
        let attrs = TempoPayloadBuilderAttributes::new(
            // XXX: derives the payload ID from the parent so that
            // overlong payload builds will eventually succeed on the
            // next iteration: if all other nodes take equally as long,
            // the consensus engine will kill the proposal task (see
            // also `response.cancellation` below). Then eventually
            // consensus will circle back to an earlier node, which then
            // has the chance of picking up the old payload.
            payload_id_from_block_hash(&parent_hash),
            parent_hash,
            self.fee_recipient,
            runtime_context.current().epoch_millis(),
            extra_data,
            move || subblocks.get_subblocks(parent_hash).unwrap_or_default(),
        );
        let interrupt_handle = attrs.interrupt_handle().clone();

        let payload_id_req = self
            .execution_node
            .payload_builder_handle
            .send_new_payload(attrs)
            .pace(&runtime_context, Duration::from_millis(20))
            .await
            .map_err(|_| eyre!("channel was closed before a response was returned"))
            .and_then(|ret| ret.wrap_err("execution layer rejected request"))
            .wrap_err("failed requesting new payload from the execution layer");
        let payload_id = match payload_id_req {
            Ok(id) => id,
            Err(err) => {
                tracing::error!(%err, "failed to request new payload");
                return None;
            }
        };

        debug!(
            timeout_ms = self.new_payload_wait_time.as_millis(),
            "sleeping for payload builder timeout"
        );
        runtime_context.sleep(self.new_payload_wait_time).await;

        interrupt_handle.interrupt();

        let payload_req = self
            .execution_node
            .payload_builder_handle
            .resolve_kind(payload_id, reth_node_builder::PayloadKind::WaitForPending)
            .pace(&runtime_context, Duration::from_millis(20))
            .await
            // XXX: this returns Option<Result<_, _>>; drilling into
            // resolve_kind this really seems to resolve to None if no
            // payload_id was found.
            .ok_or_eyre("no payload found under provided id")
            .and_then(|rsp| rsp.map_err(Into::<eyre::Report>::into))
            .wrap_err_with(|| format!("failed getting payload for payload ID `{payload_id}`"));
        let payload = match payload_req {
            Ok(payload) => payload,
            Err(err) => {
                tracing::error!(%err, %payload_id, "failed to get payload from builder");
                return None;
            }
        };

        let built_block = Block::from_execution_block(payload.block().clone());

        // Make sure reth sees the new payload so that in the next round we can verify blocks on top of it.
        let is_good_req = verify_block(
            runtime_context,
            consensus_context.round.epoch(),
            self.epoch_length,
            self.execution_node
                .add_ons_handle
                .beacon_engine_handle
                .clone(),
            &built_block,
            parent.commitment(),
            &self.scheme_provider,
        )
        .await
        .wrap_err("failed verifying block against execution layer");
        let is_good = match is_good_req {
            Ok(is_good) => is_good,
            Err(err) => {
                tracing::error!(
                    %err,
                    block_height = built_block.height(),
                    block_hash = %built_block.digest(),
                    "failed verifying newly built block against execution layer",
                );
                return None;
            }
        };

        if !is_good {
            warn!(
                block_height = built_block.height(),
                block_hash = %built_block.digest(),
                "newly built block was rejected by execution layer",
            );
            return None;
        }

        if let Err(error) = self
            .executor_mailbox
            .canonicalize_head(built_block.height(), built_block.commitment())
        {
            warn!(
                %error,
                proposal_digest = %built_block.commitment(),
                "failed making the proposal the head of the canonical chain",
            );
        }

        Some(built_block)
    }
}

impl<E> commonware_consensus::VerifyingApplication<E> for Application<E>
where
    E: Rng + Spawner + Metrics + Clock + Pacer,
{
    async fn verify(
        &mut self,
        (runtime_context, consensus_context): (E, Self::Context),
        mut ancestry: AncestorStream<Self::SigningScheme, Self::Block>,
    ) -> bool {
        let Some(block) = ancestry.next().await else {
            return false;
        };
        let Some(parent) = ancestry.next().await else {
            return false;
        };

        if utils::last_block_in_epoch(self.epoch_length, consensus_context.round.epoch())
            == block.height()
        {
            let our_outcome_req = self
                .dkg_mailbox
                .get_public_ceremony_outcome(consensus_context.round.epoch())
                .await
                .transpose()
                .ok_or_eyre("public dkg ceremony outcome does not exist")
                // TODO(janis): Result::flatten once msrv 1.89
                .and_then(|this| this)
                .wrap_err(
                    "failed getting public dkg ceremony outcome; cannot verify end of epoch block",
                );
            let our_outcome = match our_outcome_req {
                Ok(outcome) => outcome,
                Err(error) => {
                    warn!(
                        %error,
                        "failed getting dkg ceremony outcome; cannot verify end of epoch block",
                    );
                    return false;
                }
            };
            let block_outcome = match PublicOutcome::decode(block.header().extra_data().as_ref()) {
                Err(error) => {
                    warn!(
                        error = %eyre::Report::new(error),
                        "cannot decode extra data header field of boundary block as public ceremony outcome; failing block",
                    );
                    return false;
                }
                Ok(block_outcome) => block_outcome,
            };
            if our_outcome != block_outcome {
                warn!(
                    our.participants = ?our_outcome.participants,
                    our.public = ?our_outcome.public,
                    block.participants = ?block_outcome.participants,
                    block.public = ?block_outcome.public,
                    "our public dkg ceremont outcome does not match what's stored in the block; failing block",
                );
                return false;
            }
        }

        if let Err(error) = self
            .executor_mailbox
            .canonicalize_head(parent.height(), parent.digest())
        {
            tracing::warn!(
                %error,
                parent.height = parent.height(),
                parent.digest = %parent.digest(),
                "failed updating canonical head to parent",
            );
        }

        let verify_req = verify_block(
            runtime_context,
            consensus_context.round.epoch(),
            self.epoch_length,
            self.execution_node
                .add_ons_handle
                .beacon_engine_handle
                .clone(),
            &block,
            parent.commitment(),
            &self.scheme_provider,
        )
        .await;

        match verify_req {
            Ok(is_valid) => {
                if is_valid
                    && let Err(error) = self
                        .executor_mailbox
                        .canonicalize_head(block.height(), block.digest())
                {
                    warn!(
                        %error,
                        "failed making the verified proposal the head of the canonical chain",
                    );
                }
                is_valid
            }
            Err(error) => {
                warn!(
                    %error,
                    block.height = block.height(),
                    block.digest = %block.digest(),
                    "error occurred while verifying block; failing block",
                );
                false
            }
        }
    }
}

/// Constructs a [`PayloadId`] from the first 8 bytes of `block_hash`.
fn payload_id_from_block_hash(block_hash: &B256) -> PayloadId {
    PayloadId::new(
        <[u8; 8]>::try_from(&block_hash[0..8])
            .expect("a 32 byte array always has more than 8 bytes"),
    )
}

/// Verifies `block` given its `parent` against the execution layer.
///
/// Returns whether the block is valid or not. Returns an error if validation
/// was not possible, for example if communication with the execution layer
/// failed.
///
/// Reason the reason for why a block was not valid is communicated as a
/// tracing event.
#[instrument(
    skip_all,
    fields(
        epoch,
        epoch_length,
        block.parent_digest = %block.parent_digest(),
        block.digest = %block.digest(),
        block.height = block.height(),
        block.timestamp = block.timestamp(),
        parent.digest = %parent_digest,
    )
)]
async fn verify_block<TContext: Pacer>(
    context: TContext,
    epoch: Epoch,
    epoch_length: u64,
    engine: ConsensusEngineHandle<TempoPayloadTypes>,
    block: &Block,
    parent_digest: Digest,
    scheme_provider: &SchemeProvider,
) -> eyre::Result<bool> {
    use alloy_rpc_types_engine::PayloadStatusEnum;
    if utils::epoch(epoch_length, block.height()) != epoch {
        info!("block does not belong to this epoch");
        return Ok(false);
    }
    if block.parent_hash() != *parent_digest {
        info!(
            "parent digest stored in block must match the digest of the parent \
            argument but doesn't"
        );
        return Ok(false);
    }
    let scheme = scheme_provider
        .scheme(epoch)
        .ok_or_eyre("cannot determine participants in the current epoch")?;
    let block = block.clone().into_inner();
    let execution_data = TempoExecutionData {
        block,
        validator_set: Some(
            scheme
                .participants()
                .into_iter()
                .map(|p| B256::from_slice(p))
                .collect(),
        ),
    };
    let payload_status = engine
        .new_payload(execution_data)
        .pace(&context, Duration::from_millis(50))
        .await
        .wrap_err("failed sending `new payload` message to execution layer to validate block")?;
    match payload_status.status {
        PayloadStatusEnum::Valid | PayloadStatusEnum::Accepted => Ok(true),
        PayloadStatusEnum::Invalid { validation_error } => {
            info!(
                validation_error,
                "execution layer returned that the block was invalid"
            );
            Ok(false)
        }
        PayloadStatusEnum::Syncing => {
            // FIXME: is this error message correct?
            bail!(
                "failed validating block because payload is still syncing, \
                this means the parent block was available to the consensus
                layer but not the execution layer"
            )
        }
    }
}
