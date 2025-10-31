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

    vec![InterfaceFunction {
        name: "finalize_streams",
        params: params(vec![]),
        return_type: parse_quote!(()),
        is_view: false,
        call_type_path: quote!(#interface_ident::finalizeStreamsCall),
    }]
}

pub(crate) fn get_events(_interface_ident: &Ident) -> Vec<InterfaceEvent> {
    vec![]
}

pub(crate) fn get_errors(interface_ident: &Ident) -> Vec<InterfaceError> {
    vec![
        InterfaceError {
            name: "unauthorized",
            params: vec![],
            error_type_path: quote!(#interface_ident::Unauthorized),
        },
        InterfaceError {
            name: "streams_already_finalized",
            params: vec![],
            error_type_path: quote!(#interface_ident::StreamsAlreadyFinalized),
        },
    ]
}
