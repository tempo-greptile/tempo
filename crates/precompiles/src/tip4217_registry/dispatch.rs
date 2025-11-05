use crate::{Precompile, input_cost, storage::PrecompileStorageProvider, view};
use alloy::{primitives::Address, sol_types::SolCall};
use revm::precompile::{PrecompileError, PrecompileResult};

use crate::tip4217_registry::{ITIP4217Registry, TIP4217Registry};

impl<'a, S: PrecompileStorageProvider> Precompile for TIP4217Registry<'a, S> {
    fn call(&mut self, calldata: &[u8], _msg_sender: Address) -> PrecompileResult {
        self.storage
            .deduct_gas(input_cost(calldata.len()))
            .map_err(|_| PrecompileError::OutOfGas)?;

        let selector: [u8; 4] = calldata
            .get(..4)
            .ok_or_else(|| {
                PrecompileError::Other("Invalid input: missing function selector".to_string())
            })?
            .try_into()
            .unwrap();

        let result = match selector {
            ITIP4217Registry::getCurrencyDecimalsCall::SELECTOR => {
                view::<ITIP4217Registry::getCurrencyDecimalsCall>(calldata, |call| {
                    Ok(self.get_currency_decimals(call))
                })
            }
            _ => Err(PrecompileError::Other(
                "Unknown function selector".to_string(),
            )),
        };

        result.map(|mut res| {
            res.gas_used = self.storage.gas_remaining();
            res
        })
    }
}
