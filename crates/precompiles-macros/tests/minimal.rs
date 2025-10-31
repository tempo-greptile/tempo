//! Minimal test to debug trait vs type issue

mod storage {
    pub(super) use tempo_precompiles::storage::*;
}

use alloy::{
    primitives::U256,
    sol,
};
use storage::{ContractStorage, hashmap::HashMapStorageProvider};
use tempo_precompiles_macros::contract;

sol! {
    interface ISimple {
        function value() external view returns (uint256);
    }
}

pub use tempo_precompiles::{
    METADATA_GAS, Precompile, VIEW_FUNC_GAS, error, metadata, view,
};

#[contract(ISimple)]
pub struct SimpleContract {
    pub value: U256,
}

impl<S: storage::PrecompileStorageProvider> SimpleContractCall for SimpleContract<'_, S> {
    // value() is auto-generated
}

#[test]
fn test_minimal() {
    let mut storage = HashMapStorageProvider::new(1);
    let addr = alloy::primitives::Address::ZERO;
    let mut contract = SimpleContract::_new(addr, &mut storage);

    contract._set_value(U256::from(42)).unwrap();
    assert_eq!(contract._get_value().unwrap(), U256::from(42));
}
