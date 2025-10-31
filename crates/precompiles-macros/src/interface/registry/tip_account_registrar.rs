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
            name: "delegate_to_default",
            params: params(vec![
                ("hash", parse_quote!(B256)),
                ("signature", parse_quote!(Bytes)),
            ]),
            return_type: parse_quote!(Address),
            is_view: false,
            call_type_path: quote!(#interface_ident::delegateToDefaultCall),
        },
        InterfaceFunction {
            name: "get_delegation_message",
            params: params(vec![]),
            return_type: parse_quote!(String),
            is_view: true,
            call_type_path: quote!(#interface_ident::getDelegationMessageCall),
        },
    ]
}

pub(crate) fn get_events(_interface_ident: &Ident) -> Vec<InterfaceEvent> {
    vec![]
}

pub(crate) fn get_errors(interface_ident: &Ident) -> Vec<InterfaceError> {
    vec![
        InterfaceError {
            name: "invalid_signature",
            params: vec![],
            error_type_path: quote!(#interface_ident::InvalidSignature),
        },
        InterfaceError {
            name: "code_not_empty",
            params: vec![],
            error_type_path: quote!(#interface_ident::CodeNotEmpty),
        },
        InterfaceError {
            name: "nonce_not_zero",
            params: vec![],
            error_type_path: quote!(#interface_ident::NonceNotZero),
        },
    ]
}
