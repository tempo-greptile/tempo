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
        InterfaceFunction {
            name: "get_nonce",
            params: params(vec![
                ("account", parse_quote!(Address)),
                ("nonce_key", parse_quote!(U256)),
            ]),
            return_type: parse_quote!(u64),
            is_view: true,
            call_type_path: quote!(#interface_ident::getNonceCall),
        },
        InterfaceFunction {
            name: "get_active_nonce_key_count",
            params: params(vec![("account", parse_quote!(Address))]),
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_ident::getActiveNonceKeyCountCall),
        },
    ]
}

pub(crate) fn get_events(interface_ident: &Ident) -> Vec<InterfaceEvent> {
    vec![
        InterfaceEvent {
            name: "nonce_incremented",
            params: vec![
                ("account", parse_quote!(Address), true),
                ("nonce_key", parse_quote!(U256), true),
                ("new_nonce", parse_quote!(u64), false),
            ],
            event_type_path: quote!(#interface_ident::NonceIncremented),
        },
        InterfaceEvent {
            name: "active_key_count_changed",
            params: vec![
                ("account", parse_quote!(Address), true),
                ("new_count", parse_quote!(U256), false),
            ],
            event_type_path: quote!(#interface_ident::ActiveKeyCountChanged),
        },
    ]
}

pub(crate) fn get_errors(interface_ident: &Ident) -> Vec<InterfaceError> {
    vec![
        InterfaceError {
            name: "protocol_nonce_not_supported",
            params: vec![],
            error_type_path: quote!(#interface_ident::ProtocolNonceNotSupported),
        },
        InterfaceError {
            name: "invalid_nonce_key",
            params: vec![],
            error_type_path: quote!(#interface_ident::InvalidNonceKey),
        },
        InterfaceError {
            name: "nonce_overflow",
            params: vec![],
            error_type_path: quote!(#interface_ident::NonceOverflow),
        },
    ]
}
