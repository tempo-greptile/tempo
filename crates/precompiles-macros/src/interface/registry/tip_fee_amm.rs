use crate::interface::{InterfaceError, InterfaceEvent, InterfaceFunction, ParamName};
use quote::quote;
use syn::{Ident, Type, parse_quote};

pub(crate) fn get_functions(interface_ident: &Ident) -> Vec<InterfaceFunction> {
    // Helper to convert parameter tuples to ParamName
    let params = |p: Vec<(&'static str, Type)>| -> Vec<(ParamName, Type)> {
        p.into_iter()
            .map(|(name, ty)| (ParamName::new(name), ty))
            .collect()
    };

    vec![
        // Pure functions
        InterfaceFunction {
            name: "get_pool_id",
            params: params(vec![
                ("user_token", parse_quote!(Address)),
                ("validator_token", parse_quote!(Address)),
            ]),
            return_type: parse_quote!(B256),
            is_view: true,
            call_type_path: quote!(#interface_ident::getPoolIdCall),
        },
        InterfaceFunction {
            name: "calculate_liquidity",
            params: params(vec![("x", parse_quote!(U256)), ("y", parse_quote!(U256))]),
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_ident::calculateLiquidityCall),
        },
        // View functions returning structs
        InterfaceFunction {
            name: "get_pool",
            params: params(vec![
                ("user_token", parse_quote!(Address)),
                ("validator_token", parse_quote!(Address)),
            ]),
            return_type: parse_quote!(#interface_ident::Pool),
            is_view: true,
            call_type_path: quote!(#interface_ident::getPoolCall),
        },
        InterfaceFunction {
            name: "pools",
            params: params(vec![("pool_id", parse_quote!(B256))]),
            return_type: parse_quote!(#interface_ident::Pool),
            is_view: true,
            call_type_path: quote!(#interface_ident::poolsCall),
        },
        // View functions returning primitives
        InterfaceFunction {
            name: "total_supply",
            params: params(vec![("pool_id", parse_quote!(B256))]),
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_ident::totalSupplyCall),
        },
        InterfaceFunction {
            name: "liquidity_balances",
            params: params(vec![
                ("pool_id", parse_quote!(B256)),
                ("user", parse_quote!(Address)),
            ]),
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_ident::liquidityBalancesCall),
        },
        // Mutating functions (non-void returns)
        InterfaceFunction {
            name: "mint",
            params: params(vec![
                ("user_token", parse_quote!(Address)),
                ("validator_token", parse_quote!(Address)),
                ("amount_user_token", parse_quote!(U256)),
                ("amount_validator_token", parse_quote!(U256)),
                ("to", parse_quote!(Address)),
            ]),
            return_type: parse_quote!(U256),
            is_view: false,
            call_type_path: quote!(#interface_ident::mintCall),
        },
        InterfaceFunction {
            name: "burn",
            params: params(vec![
                ("user_token", parse_quote!(Address)),
                ("validator_token", parse_quote!(Address)),
                ("liquidity", parse_quote!(U256)),
                ("to", parse_quote!(Address)),
            ]),
            return_type: parse_quote!((U256, U256)),
            is_view: false,
            call_type_path: quote!(#interface_ident::burnCall),
        },
        InterfaceFunction {
            name: "rebalance_swap",
            params: params(vec![
                ("user_token", parse_quote!(Address)),
                ("validator_token", parse_quote!(Address)),
                ("amount_out", parse_quote!(U256)),
                ("to", parse_quote!(Address)),
            ]),
            return_type: parse_quote!(U256),
            is_view: false,
            call_type_path: quote!(#interface_ident::rebalanceSwapCall),
        },
    ]
}

pub(crate) fn get_events(interface_ident: &Ident) -> Vec<InterfaceEvent> {
    vec![
        InterfaceEvent {
            name: "mint",
            params: vec![
                ("sender", parse_quote!(Address), true),
                ("user_token", parse_quote!(Address), true),
                ("validator_token", parse_quote!(Address), true),
                ("amount_user_token", parse_quote!(U256), false),
                ("amount_validator_token", parse_quote!(U256), false),
                ("liquidity", parse_quote!(U256), false),
            ],
            event_type_path: quote!(#interface_ident::Mint),
        },
        InterfaceEvent {
            name: "burn",
            params: vec![
                ("sender", parse_quote!(Address), true),
                ("user_token", parse_quote!(Address), true),
                ("validator_token", parse_quote!(Address), true),
                ("amount_user_token", parse_quote!(U256), false),
                ("amount_validator_token", parse_quote!(U256), false),
                ("liquidity", parse_quote!(U256), false),
                ("to", parse_quote!(Address), false),
            ],
            event_type_path: quote!(#interface_ident::Burn),
        },
        InterfaceEvent {
            name: "rebalance_swap",
            params: vec![
                ("user_token", parse_quote!(Address), true),
                ("validator_token", parse_quote!(Address), true),
                ("swapper", parse_quote!(Address), true),
                ("amount_in", parse_quote!(U256), false),
                ("amount_out", parse_quote!(U256), false),
            ],
            event_type_path: quote!(#interface_ident::RebalanceSwap),
        },
        InterfaceEvent {
            name: "fee_swap",
            params: vec![
                ("user_token", parse_quote!(Address), true),
                ("validator_token", parse_quote!(Address), true),
                ("amount_in", parse_quote!(U256), false),
                ("amount_out", parse_quote!(U256), false),
            ],
            event_type_path: quote!(#interface_ident::FeeSwap),
        },
    ]
}

pub(crate) fn get_errors(interface_ident: &Ident) -> Vec<InterfaceError> {
    vec![
        InterfaceError {
            name: "identical_addresses",
            params: vec![],
            error_type_path: quote!(#interface_ident::IdenticalAddresses),
        },
        InterfaceError {
            name: "zero_address",
            params: vec![],
            error_type_path: quote!(#interface_ident::ZeroAddress),
        },
        InterfaceError {
            name: "pool_exists",
            params: vec![],
            error_type_path: quote!(#interface_ident::PoolExists),
        },
        InterfaceError {
            name: "pool_does_not_exist",
            params: vec![],
            error_type_path: quote!(#interface_ident::PoolDoesNotExist),
        },
        InterfaceError {
            name: "invalid_token",
            params: vec![],
            error_type_path: quote!(#interface_ident::InvalidToken),
        },
        InterfaceError {
            name: "insufficient_liquidity",
            params: vec![],
            error_type_path: quote!(#interface_ident::InsufficientLiquidity),
        },
        InterfaceError {
            name: "only_protocol",
            params: vec![],
            error_type_path: quote!(#interface_ident::OnlyProtocol),
        },
        InterfaceError {
            name: "insufficient_pool_balance",
            params: vec![],
            error_type_path: quote!(#interface_ident::InsufficientPoolBalance),
        },
        InterfaceError {
            name: "insufficient_reserves",
            params: vec![],
            error_type_path: quote!(#interface_ident::InsufficientReserves),
        },
        InterfaceError {
            name: "insufficient_liquidity_balance",
            params: vec![],
            error_type_path: quote!(#interface_ident::InsufficientLiquidityBalance),
        },
        InterfaceError {
            name: "must_deposit_lower_balance_token",
            params: vec![],
            error_type_path: quote!(#interface_ident::MustDepositLowerBalanceToken),
        },
        InterfaceError {
            name: "invalid_amount",
            params: vec![],
            error_type_path: quote!(#interface_ident::InvalidAmount),
        },
        InterfaceError {
            name: "invalid_rebalance_state",
            params: vec![],
            error_type_path: quote!(#interface_ident::InvalidRebalanceState),
        },
        InterfaceError {
            name: "invalid_rebalance_direction",
            params: vec![],
            error_type_path: quote!(#interface_ident::InvalidRebalanceDirection),
        },
        InterfaceError {
            name: "invalid_new_reserves",
            params: vec![],
            error_type_path: quote!(#interface_ident::InvalidNewReserves),
        },
        InterfaceError {
            name: "cannot_support_pending_swaps",
            params: vec![],
            error_type_path: quote!(#interface_ident::CannotSupportPendingSwaps),
        },
        InterfaceError {
            name: "division_by_zero",
            params: vec![],
            error_type_path: quote!(#interface_ident::DivisionByZero),
        },
        InterfaceError {
            name: "invalid_swap_calculation",
            params: vec![],
            error_type_path: quote!(#interface_ident::InvalidSwapCalculation),
        },
        InterfaceError {
            name: "insufficient_liquidity_for_pending",
            params: vec![],
            error_type_path: quote!(#interface_ident::InsufficientLiquidityForPending),
        },
        InterfaceError {
            name: "token_transfer_failed",
            params: vec![],
            error_type_path: quote!(#interface_ident::TokenTransferFailed),
        },
        InterfaceError {
            name: "internal_error",
            params: vec![],
            error_type_path: quote!(#interface_ident::InternalError),
        },
    ]
}
