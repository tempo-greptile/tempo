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
        // Constants (pure functions)
        InterfaceFunction {
            name: "basis_points",
            params: vec![],
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_ident::BASIS_POINTSCall),
        },
        InterfaceFunction {
            name: "fee_bps",
            params: vec![],
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_ident::FEE_BPSCall),
        },
        // User preference view functions
        InterfaceFunction {
            name: "user_tokens",
            params: params(vec![("user", parse_quote!(Address))]),
            return_type: parse_quote!(Address),
            is_view: true,
            call_type_path: quote!(#interface_ident::userTokensCall),
        },
        InterfaceFunction {
            name: "validator_tokens",
            params: params(vec![("validator", parse_quote!(Address))]),
            return_type: parse_quote!(Address),
            is_view: true,
            call_type_path: quote!(#interface_ident::validatorTokensCall),
        },
        // Fee view function
        InterfaceFunction {
            name: "get_fee_token_balance",
            params: params(vec![
                ("sender", parse_quote!(Address)),
                ("validator", parse_quote!(Address)),
            ]),
            return_type: parse_quote!((Address, U256)),
            is_view: true,
            call_type_path: quote!(#interface_ident::getFeeTokenBalanceCall),
        },
        // Mutating functions (void)
        InterfaceFunction {
            name: "set_user_token",
            params: params(vec![("token", parse_quote!(Address))]),
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::setUserTokenCall),
        },
        InterfaceFunction {
            name: "set_validator_token",
            params: params(vec![("token", parse_quote!(Address))]),
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::setValidatorTokenCall),
        },
        InterfaceFunction {
            name: "execute_block",
            params: vec![],
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::executeBlockCall),
        },
    ]
}

pub(crate) fn get_events(interface_ident: &Ident) -> Vec<InterfaceEvent> {
    vec![
        InterfaceEvent {
            name: "user_token_set",
            params: vec![
                ("user", parse_quote!(Address), true),
                ("token", parse_quote!(Address), true),
            ],
            event_type_path: quote!(#interface_ident::UserTokenSet),
        },
        InterfaceEvent {
            name: "validator_token_set",
            params: vec![
                ("validator", parse_quote!(Address), true),
                ("token", parse_quote!(Address), true),
            ],
            event_type_path: quote!(#interface_ident::ValidatorTokenSet),
        },
    ]
}

pub(crate) fn get_errors(interface_ident: &Ident) -> Vec<InterfaceError> {
    vec![
        InterfaceError {
            name: "only_validator",
            params: vec![],
            error_type_path: quote!(#interface_ident::OnlyValidator),
        },
        InterfaceError {
            name: "only_system_contract",
            params: vec![],
            error_type_path: quote!(#interface_ident::OnlySystemContract),
        },
        InterfaceError {
            name: "invalid_token",
            params: vec![],
            error_type_path: quote!(#interface_ident::InvalidToken),
        },
        InterfaceError {
            name: "pool_does_not_exist",
            params: vec![],
            error_type_path: quote!(#interface_ident::PoolDoesNotExist),
        },
        InterfaceError {
            name: "insufficient_liquidity",
            params: vec![],
            error_type_path: quote!(#interface_ident::InsufficientLiquidity),
        },
        InterfaceError {
            name: "insufficient_fee_token_balance",
            params: vec![],
            error_type_path: quote!(#interface_ident::InsufficientFeeTokenBalance),
        },
        InterfaceError {
            name: "internal_error",
            params: vec![],
            error_type_path: quote!(#interface_ident::InternalError),
        },
        InterfaceError {
            name: "cannot_change_within_block",
            params: vec![],
            error_type_path: quote!(#interface_ident::CannotChangeWithinBlock),
        },
        InterfaceError {
            name: "token_policy_forbids",
            params: vec![],
            error_type_path: quote!(#interface_ident::TokenPolicyForbids),
        },
    ]
}
