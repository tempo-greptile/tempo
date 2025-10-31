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
        name: "get_currency_decimals",
        params: params(vec![("currency", parse_quote!(String))]),
        return_type: parse_quote!(u8),
        is_view: true,
        call_type_path: quote!(#interface_ident::getCurrencyDecimalsCall),
    }]
}

pub(crate) fn get_events(_interface_ident: &Ident) -> Vec<InterfaceEvent> {
    vec![]
}

pub(crate) fn get_errors(_interface_ident: &Ident) -> Vec<InterfaceError> {
    vec![]
}
