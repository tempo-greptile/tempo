use super::*;

pub(super) fn transfer(
    signer: PrivateKeySigner,
    nonce: u64,
    chain_id: ChainId,
    token_address: Address,
) -> eyre::Result<Vec<u8>> {
    let mut tx = TxLegacy {
        chain_id: Some(chain_id),
        nonce,
        gas_price: TEMPO_BASE_FEE as u128,
        gas_limit: GAS_LIMIT,
        to: TxKind::Call(token_address),
        value: U256::ZERO,
        input: ITIP20::transferCall {
            to: Address::random(),
            amount: U256::ONE,
        }
        .abi_encode()
        .into(),
    };

    let signature = signer
        .sign_transaction_sync(&mut tx)
        .map_err(|e| eyre::eyre!("Failed to sign transaction: {e}"))?;
    let mut payload = Vec::new();
    tx.into_signed(signature).eip2718_encode(&mut payload);
    Ok(payload)
}
