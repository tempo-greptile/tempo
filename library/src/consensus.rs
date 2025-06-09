use reth_chainspec::ChainSpec;
use reth_consensus::{Consensus, ConsensusError, HeaderValidator };
use reth_node_builder::{Block, components::ConsensusBuilder, BuilderContext, FullNodeTypes};
use reth_primitives::{SealedHeader, SealedBlock};

use std::sync::Arc;

#[derive(Debug, Default, Clone)]
pub struct MalachiteConsensus {
    chain_spec: Arc<ChainSpec>,
}

impl MalachiteConsensus {
    pub fn new(chain_spec: Arc<ChainSpec>) -> Self {
        Self { chain_spec }
    }
}

impl<B> Consensus<B> for MalachiteConsensus
where B: Block {
    type Error = ConsensusError;

    fn validate_body_against_header(&self, body: &B::Body, header: &SealedHeader<B::Header> ,) -> Result<(),Self::Error> {
        Ok(())
    }

    fn validate_block_pre_execution(&self, block: &SealedBlock<B>) -> Result<(),Self::Error> {
        Ok(())
    }
}
#[derive(Debug)]
pub struct MalachiteConsensusBuilder<B> {}

impl<Node, B> ConsensusBuilder<Node> for MalachiteConsensusBuilder<B>
where
    Node: FullNodeTypes,
    B: Block + HeaderValidator<B::Header>,
{
    type Consensus = Arc<MalachiteConsensus>;

    async fn build_consensus(self, ctx: &BuilderContext<Node>) -> eyre::Result<Self::Consensus> {
       Ok(())
    }
}