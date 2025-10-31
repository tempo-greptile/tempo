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
        // Core trading functions (non-void returns)
        InterfaceFunction {
            name: "create_pair",
            params: params(vec![("base", parse_quote!(Address))]),
            return_type: parse_quote!(B256),
            is_view: false,
            call_type_path: quote!(#interface_ident::createPairCall),
        },
        InterfaceFunction {
            name: "place",
            params: params(vec![
                ("token", parse_quote!(Address)),
                ("amount", parse_quote!(u128)),
                ("is_bid", parse_quote!(bool)),
                ("tick", parse_quote!(i16)),
            ]),
            return_type: parse_quote!(u128),
            is_view: false,
            call_type_path: quote!(#interface_ident::placeCall),
        },
        InterfaceFunction {
            name: "place_flip",
            params: params(vec![
                ("token", parse_quote!(Address)),
                ("amount", parse_quote!(u128)),
                ("is_bid", parse_quote!(bool)),
                ("tick", parse_quote!(i16)),
                ("flip_tick", parse_quote!(i16)),
            ]),
            return_type: parse_quote!(u128),
            is_view: false,
            call_type_path: quote!(#interface_ident::placeFlipCall),
        },
        // Swap functions (non-void returns)
        InterfaceFunction {
            name: "swap_exact_amount_in",
            params: params(vec![
                ("token_in", parse_quote!(Address)),
                ("token_out", parse_quote!(Address)),
                ("amount_in", parse_quote!(u128)),
                ("min_amount_out", parse_quote!(u128)),
            ]),
            return_type: parse_quote!(u128),
            is_view: false,
            call_type_path: quote!(#interface_ident::swapExactAmountInCall),
        },
        InterfaceFunction {
            name: "swap_exact_amount_out",
            params: params(vec![
                ("token_in", parse_quote!(Address)),
                ("token_out", parse_quote!(Address)),
                ("amount_out", parse_quote!(u128)),
                ("max_amount_in", parse_quote!(u128)),
            ]),
            return_type: parse_quote!(u128),
            is_view: false,
            call_type_path: quote!(#interface_ident::swapExactAmountOutCall),
        },
        // View swap quote functions
        InterfaceFunction {
            name: "quote_swap_exact_amount_in",
            params: params(vec![
                ("token_in", parse_quote!(Address)),
                ("token_out", parse_quote!(Address)),
                ("amount_in", parse_quote!(u128)),
            ]),
            return_type: parse_quote!(u128),
            is_view: true,
            call_type_path: quote!(#interface_ident::quoteSwapExactAmountInCall),
        },
        InterfaceFunction {
            name: "quote_swap_exact_amount_out",
            params: params(vec![
                ("token_in", parse_quote!(Address)),
                ("token_out", parse_quote!(Address)),
                ("amount_out", parse_quote!(u128)),
            ]),
            return_type: parse_quote!(u128),
            is_view: true,
            call_type_path: quote!(#interface_ident::quoteSwapExactAmountOutCall),
        },
        // Balance management view functions
        InterfaceFunction {
            name: "balance_of",
            params: params(vec![
                ("user", parse_quote!(Address)),
                ("token", parse_quote!(Address)),
            ]),
            return_type: parse_quote!(u128),
            is_view: true,
            call_type_path: quote!(#interface_ident::balanceOfCall),
        },
        // View functions returning structs
        InterfaceFunction {
            name: "get_order",
            params: params(vec![("order_id", parse_quote!(u128))]),
            return_type: parse_quote!(#interface_ident::Order),
            is_view: true,
            call_type_path: quote!(#interface_ident::getOrderCall),
        },
        InterfaceFunction {
            name: "get_price_level",
            params: params(vec![
                ("base", parse_quote!(Address)),
                ("tick", parse_quote!(i16)),
                ("is_bid", parse_quote!(bool)),
            ]),
            return_type: parse_quote!(#interface_ident::PriceLevel),
            is_view: true,
            call_type_path: quote!(#interface_ident::getPriceLevelCall),
        },
        InterfaceFunction {
            name: "books",
            params: params(vec![("pair_key", parse_quote!(B256))]),
            return_type: parse_quote!(#interface_ident::Orderbook),
            is_view: true,
            call_type_path: quote!(#interface_ident::booksCall),
        },
        // Simple view functions
        InterfaceFunction {
            name: "pair_key",
            params: params(vec![
                ("token_a", parse_quote!(Address)),
                ("token_b", parse_quote!(Address)),
            ]),
            return_type: parse_quote!(B256),
            is_view: true,
            call_type_path: quote!(#interface_ident::pairKeyCall),
        },
        InterfaceFunction {
            name: "active_order_id",
            params: params(vec![]),
            return_type: parse_quote!(u128),
            is_view: true,
            call_type_path: quote!(#interface_ident::activeOrderIdCall),
        },
        InterfaceFunction {
            name: "pending_order_id",
            params: params(vec![]),
            return_type: parse_quote!(u128),
            is_view: true,
            call_type_path: quote!(#interface_ident::pendingOrderIdCall),
        },
        // Mutating functions (void)
        InterfaceFunction {
            name: "cancel",
            params: params(vec![("order_id", parse_quote!(u128))]),
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::cancelCall),
        },
        InterfaceFunction {
            name: "execute_block",
            params: params(vec![]),
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::executeBlockCall),
        },
        InterfaceFunction {
            name: "withdraw",
            params: params(vec![
                ("token", parse_quote!(Address)),
                ("amount", parse_quote!(u128)),
            ]),
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::withdrawCall),
        },
    ]
}

pub(crate) fn get_events(interface_ident: &Ident) -> Vec<InterfaceEvent> {
    vec![
        InterfaceEvent {
            name: "pair_created",
            params: vec![
                ("key", parse_quote!(B256), true),
                ("base", parse_quote!(Address), true),
                ("quote", parse_quote!(Address), true),
            ],
            event_type_path: quote!(#interface_ident::PairCreated),
        },
        InterfaceEvent {
            name: "order_placed",
            params: vec![
                ("order_id", parse_quote!(u128), true),
                ("maker", parse_quote!(Address), true),
                ("token", parse_quote!(Address), true),
                ("amount", parse_quote!(u128), false),
                ("is_bid", parse_quote!(bool), false),
                ("tick", parse_quote!(i16), false),
            ],
            event_type_path: quote!(#interface_ident::OrderPlaced),
        },
        InterfaceEvent {
            name: "flip_order_placed",
            params: vec![
                ("order_id", parse_quote!(u128), true),
                ("maker", parse_quote!(Address), true),
                ("token", parse_quote!(Address), true),
                ("amount", parse_quote!(u128), false),
                ("is_bid", parse_quote!(bool), false),
                ("tick", parse_quote!(i16), false),
                ("flip_tick", parse_quote!(i16), false),
            ],
            event_type_path: quote!(#interface_ident::FlipOrderPlaced),
        },
        InterfaceEvent {
            name: "order_filled",
            params: vec![
                ("order_id", parse_quote!(u128), true),
                ("maker", parse_quote!(Address), true),
                ("amount_filled", parse_quote!(u128), false),
                ("partial_fill", parse_quote!(bool), false),
            ],
            event_type_path: quote!(#interface_ident::OrderFilled),
        },
        InterfaceEvent {
            name: "order_cancelled",
            params: vec![("order_id", parse_quote!(u128), true)],
            event_type_path: quote!(#interface_ident::OrderCancelled),
        },
    ]
}

pub(crate) fn get_errors(interface_ident: &Ident) -> Vec<InterfaceError> {
    vec![
        InterfaceError {
            name: "unauthorized",
            params: vec![],
            error_type_path: quote!(#interface_ident::Unauthorized),
        },
        InterfaceError {
            name: "pair_does_not_exist",
            params: vec![],
            error_type_path: quote!(#interface_ident::PairDoesNotExist),
        },
        InterfaceError {
            name: "pair_already_exists",
            params: vec![],
            error_type_path: quote!(#interface_ident::PairAlreadyExists),
        },
        InterfaceError {
            name: "order_does_not_exist",
            params: vec![],
            error_type_path: quote!(#interface_ident::OrderDoesNotExist),
        },
        InterfaceError {
            name: "identical_tokens",
            params: vec![],
            error_type_path: quote!(#interface_ident::IdenticalTokens),
        },
        InterfaceError {
            name: "tick_out_of_bounds",
            params: vec![("tick", parse_quote!(i16))],
            error_type_path: quote!(#interface_ident::TickOutOfBounds),
        },
        InterfaceError {
            name: "invalid_tick",
            params: vec![],
            error_type_path: quote!(#interface_ident::InvalidTick),
        },
        InterfaceError {
            name: "invalid_flip_tick",
            params: vec![],
            error_type_path: quote!(#interface_ident::InvalidFlipTick),
        },
        InterfaceError {
            name: "insufficient_balance",
            params: vec![],
            error_type_path: quote!(#interface_ident::InsufficientBalance),
        },
        InterfaceError {
            name: "insufficient_liquidity",
            params: vec![],
            error_type_path: quote!(#interface_ident::InsufficientLiquidity),
        },
        InterfaceError {
            name: "insufficient_output",
            params: vec![],
            error_type_path: quote!(#interface_ident::InsufficientOutput),
        },
        InterfaceError {
            name: "max_input_exceeded",
            params: vec![],
            error_type_path: quote!(#interface_ident::MaxInputExceeded),
        },
    ]
}
