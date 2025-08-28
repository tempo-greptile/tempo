use reth_rpc::eth::{EthApi, RpcNodeCore};
use reth_rpc_convert::RpcConvert;
use reth_rpc_eth_api::{EthApiTypes, helpers::FullEthApi};
use reth_rpc_eth_types::EthApiError;
use std::ops::Deref;

/// Tempo `Eth` API implementation.
///
/// This type provides the functionality for handling `eth_` related requests.
///
/// This wraps a default `Eth` implementation, and provides additional functionality where the
/// Tempo spec deviates from the default ethereum spec, e.g. gas estimation denominated in
/// `feeToken`
///
/// This type implements the [`FullEthApi`](reth_rpc_eth_api::helpers::FullEthApi) by implemented
/// all the `Eth` helper traits and prerequisite traits.
#[derive(Clone)]
pub struct TempoEthApi<N: RpcNodeCore, Rpc: RpcConvert> {
    /// Gateway to node's core components.
    inner: EthApi<N, Rpc>,
}

impl<N: RpcNodeCore, Rpc: RpcConvert> TempoEthApi<N, Rpc> {
    /// Creates a new `TempoEthApi`.
    pub fn new(eth_api: EthApi<N, Rpc>) -> Self {
        Self { inner: eth_api }
    }
}

// Delegate all methods to the inner EthApi
impl<N: RpcNodeCore, Rpc: RpcConvert> Deref for TempoEthApi<N, Rpc> {
    type Target = EthApi<N, Rpc>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<N, Rpc> EthApiTypes for TempoEthApi<N, Rpc>
where
    N: RpcNodeCore,
    Rpc: RpcConvert<Primitives = N::Primitives>,
{
    type Error = EthApiError;
    type NetworkTypes = Rpc::Network;
    type RpcConvert = Rpc;

    fn tx_resp_builder(&self) -> &Self::RpcConvert {
        self.inner.tx_resp_builder()
    }
}
