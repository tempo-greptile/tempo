use crate::{
    Precompile, fill_precompile_output, input_cost, metadata, path_usd::PathUSD,
    storage::ContractStorage, tip20::ITIP20,
};

use alloy::{primitives::Address, sol_types::SolCall};
use revm::precompile::{PrecompileError, PrecompileResult};

impl Precompile for PathUSD {
    fn call(&mut self, calldata: &[u8], msg_sender: Address) -> PrecompileResult {
        let selector: [u8; 4] = if let Some(bytes) = calldata.get(..4) {
            bytes.try_into().unwrap()
        } else {
            self.token
                .storage()
                .deduct_gas(input_cost(calldata.len()))
                .map_err(|_| PrecompileError::OutOfGas)?;

            return Err(PrecompileError::Other(
                "Invalid input: missing function selector".into(),
            ));
        };

        if ![ITIP20::nameCall::SELECTOR, ITIP20::symbolCall::SELECTOR].contains(&selector) {
            // Foraward all calls to the TIP20Token impl except for `name()` and `symbol()`
            return self.token.call(calldata, msg_sender);
        }

        self.token
            .storage()
            .deduct_gas(input_cost(calldata.len()))
            .map_err(|_| PrecompileError::OutOfGas)?;

        let result = match selector {
            // Metadata
            ITIP20::nameCall::SELECTOR => metadata::<ITIP20::nameCall>(|| self.name()),
            ITIP20::symbolCall::SELECTOR => metadata::<ITIP20::symbolCall>(|| self.symbol()),
            _ => unreachable!("call forwarded to the TIP20Token impl"),
        };

        result.map(|res| fill_precompile_output(res, self.token.storage()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        storage::{StorageCtx, hashmap::HashMapStorageProvider},
        test_util::{assert_full_coverage, check_selector_coverage, setup_storage},
        tip20::tests::initialize_path_usd,
    };
    use alloy::{
        primitives::{Bytes, U256},
        sol_types::SolInterface,
    };
    use tempo_chainspec::hardfork::TempoHardfork;
    use tempo_contracts::precompiles::{
        IRolesAuth::IRolesAuthCalls, ITIP20::ITIP20Calls, TIP20Error,
    };

    #[test]
    fn path_usd_test_selector_coverage_pre_allegretto() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1).with_spec(TempoHardfork::Moderato);

        StorageCtx::enter(&mut storage, || {
            initialize_path_usd(Address::random())?;

            let mut path_usd = PathUSD::new();
            let itip20_unsupported =
                check_selector_coverage(&mut path_usd, ITIP20Calls::SELECTORS, "ITIP20", |s| {
                    ITIP20Calls::name_by_selector(s)
                });

            let roles_unsupported = check_selector_coverage(
                &mut path_usd,
                IRolesAuthCalls::SELECTORS,
                "IRolesAuth",
                IRolesAuthCalls::name_by_selector,
            );

            assert_full_coverage([itip20_unsupported, roles_unsupported]);

            Ok(())
        })
    }

    #[test]
    fn path_usd_test_selector_coverage_post_allegretto() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1).with_spec(TempoHardfork::Allegretto);

        StorageCtx::enter(&mut storage, || {
            initialize_path_usd(Address::random())?;

            let mut path_usd = PathUSD::new();
            let itip20_unsupported =
                check_selector_coverage(&mut path_usd, ITIP20Calls::SELECTORS, "ITIP20", |s| {
                    ITIP20Calls::name_by_selector(s)
                });

            let roles_unsupported = check_selector_coverage(
                &mut path_usd,
                IRolesAuthCalls::SELECTORS,
                "IRolesAuth",
                IRolesAuthCalls::name_by_selector,
            );

            assert_full_coverage([itip20_unsupported, roles_unsupported]);
            Ok(())
        })
    }

    #[test]
    fn test_start_reward_disabled_post_moderato() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1).with_spec(TempoHardfork::Moderato);
        let sender = Address::random();

        StorageCtx::enter(&mut storage, || {
            let mut token = PathUSD::new();
            token.initialize(sender)?;

            let calldata = ITIP20::startRewardCall {
                amount: U256::from(1000),
                secs: 100,
            }
            .abi_encode();

            let output = token.call(&calldata, sender)?;
            assert!(output.reverted);
            let expected: Bytes = TIP20Error::rewards_disabled().selector().into();
            assert_eq!(output.bytes, expected);

            Ok(())
        })
    }

    #[test]
    fn test_set_reward_recipient_disabled_post_moderato() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1).with_spec(TempoHardfork::Moderato);
        let sender = Address::random();
        let recipient = Address::random();

        StorageCtx::enter(&mut storage, || {
            let mut token = PathUSD::new();
            token.initialize(sender)?;

            let calldata = ITIP20::setRewardRecipientCall { recipient }.abi_encode();
            let output = token.call(&calldata, sender)?;
            assert!(output.reverted);
            let expected: Bytes = TIP20Error::rewards_disabled().selector().into();
            assert_eq!(output.bytes, expected);

            Ok(())
        })
    }

    #[test]
    fn test_cancel_reward_disabled_post_moderato() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1).with_spec(TempoHardfork::Moderato);
        let sender = Address::random();

        StorageCtx::enter(&mut storage, || {
            let mut token = PathUSD::new();
            token.initialize(sender)?;

            let calldata = ITIP20::cancelRewardCall { id: 1 }.abi_encode();

            let output = token.call(&calldata, sender)?;
            assert!(output.reverted);
            let expected: Bytes = TIP20Error::rewards_disabled().selector().into();
            assert_eq!(output.bytes, expected);

            Ok(())
        })
    }

    #[test]
    fn test_claim_rewards_disabled_post_moderato() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1).with_spec(TempoHardfork::Moderato);
        let sender = Address::random();

        StorageCtx::enter(&mut storage, || {
            let mut token = PathUSD::new();
            token.initialize(sender)?;

            let calldata = ITIP20::claimRewardsCall {}.abi_encode();

            let output = token.call(&calldata, sender)?;
            assert!(output.reverted);
            let expected: Bytes = TIP20Error::rewards_disabled().selector().into();
            assert_eq!(output.bytes, expected);

            Ok(())
        })
    }

    #[test]
    fn test_pre_allegretto_name_symbol() -> eyre::Result<()> {
        let (mut storage, sender) = setup_storage();
        storage.set_spec(TempoHardfork::Moderato);

        StorageCtx::enter(&mut storage, || {
            let mut token = PathUSD::new();
            token.initialize(sender)?;

            let name_calldata = ITIP20::nameCall {}.abi_encode();
            let name_output = token.call(&Bytes::from(name_calldata), sender)?;
            let name = ITIP20::nameCall::abi_decode_returns(&name_output.bytes)?;
            assert_eq!(name, "linkingUSD");

            let symbol_calldata = ITIP20::symbolCall {}.abi_encode();
            let symbol_output = token.call(&Bytes::from(symbol_calldata), sender)?;
            let symbol = ITIP20::symbolCall::abi_decode_returns(&symbol_output.bytes)?;
            assert_eq!(symbol, "linkingUSD");

            Ok(())
        })
    }

    #[test]
    fn test_post_allegretto_name_symbol() -> eyre::Result<()> {
        let (mut storage, sender) = setup_storage();
        storage.set_spec(TempoHardfork::Allegretto);

        StorageCtx::enter(&mut storage, || {
            let mut token = PathUSD::new();
            token.initialize(sender)?;

            let name_calldata = ITIP20::nameCall {}.abi_encode();
            let name_output = token.call(&Bytes::from(name_calldata), sender)?;
            let name = ITIP20::nameCall::abi_decode_returns(&name_output.bytes)?;
            assert_eq!(name, "pathUSD");

            let symbol_calldata = ITIP20::symbolCall {}.abi_encode();
            let symbol_output = token.call(&Bytes::from(symbol_calldata), sender)?;
            let symbol = ITIP20::symbolCall::abi_decode_returns(&symbol_output.bytes)?;
            assert_eq!(symbol, "pathUSD");

            Ok(())
        })
    }
}
