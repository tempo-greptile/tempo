//! Getter multi-trait integration test.
#![cfg(feature = "precompile")]

use alloy::primitives::{Address, U256};
use tempo_precompiles::storage::Handler;
use tempo_precompiles_macros::{abi, contract};

pub mod storage {
    pub use tempo_precompiles::storage::*;
}
pub mod dispatch {
    pub use tempo_precompiles::{
        dispatch_call, input_cost, metadata, metadata_with_sender, mutate, mutate_no_sender,
        mutate_void, mutate_void_no_sender, unknown_selector, view, view_with_sender,
    };
}
pub use tempo_precompiles::error;

type Result<T> = crate::error::Result<T>;

#[abi]
mod getter_traits {
    use super::*;

    pub trait IFeeManager {
        #[getter]
        fn foo(&self) -> Result<Address>;
    }

    pub trait IFeeAMM {
        #[getter]
        fn bar(&self, key1: Address) -> Result<U256>;
    }
}

#[contract(abi = getter_traits)]
pub struct GetterMultiTraitContract {
    foo: Address,
    bar: storage::Mapping<Address, U256>,
}

#[test]
fn test_getter_multi_trait_impls_compile() {
    let contract = GetterMultiTraitContract::__new(Address::ZERO);
    let _ = contract._foo();
    let _ = contract._bar(Address::ZERO);
}
