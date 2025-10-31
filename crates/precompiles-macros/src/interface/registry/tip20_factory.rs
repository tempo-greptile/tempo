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
            name: "create_token",
            params: params(vec![
                ("name", parse_quote!(String)),
                ("symbol", parse_quote!(String)),
                ("currency", parse_quote!(String)),
                ("quote_token", parse_quote!(Address)),
                ("admin", parse_quote!(Address)),
            ]),
            return_type: parse_quote!(U256),
            is_view: false,
            call_type_path: quote!(#interface_ident::createTokenCall),
        },
        InterfaceFunction {
            name: "token_id_counter",
            params: params(vec![]),
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_ident::tokenIdCounterCall),
        },
    ]
}

pub(crate) fn get_events(interface_ident: &Ident) -> Vec<InterfaceEvent> {
    vec![InterfaceEvent {
        name: "token_created",
        params: vec![
            ("token", parse_quote!(Address), true),
            ("token_id", parse_quote!(U256), true),
            ("name", parse_quote!(String), false),
            ("symbol", parse_quote!(String), false),
            ("currency", parse_quote!(String), false),
            ("admin", parse_quote!(Address), false),
        ],
        event_type_path: quote!(#interface_ident::TokenCreated),
    }]
}

pub(crate) fn get_errors(_interface_ident: &Ident) -> Vec<InterfaceError> {
    vec![]
}
