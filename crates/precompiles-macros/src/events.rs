//! Event emission helper generation for contract macro.
//!
//! This module generates private `_emit_*` methods for each event defined in the
//! contract's interfaces.

use crate::{
    interface::{InterfaceEvent, get_event_enum_path},
    utils::try_extract_type_ident,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type, parse2};

/// Extracts the final identifier from an event type path.
fn extract_event_variant_name(event_type_path: &TokenStream) -> Ident {
    let type_path: Type =
        parse2(event_type_path.clone()).expect("event_type_path should be a valid type path");

    try_extract_type_ident(&type_path).expect("Failed to extract event variant name from type path")
}

/// Generates event emission helper methods for a contract.
///
/// Creates private `_emit_<event_name>()` methods that take event parameters
/// and call `self.storage.emit_event()` with the properly constructed event.
pub(crate) fn gen_event_helpers(
    contract_ident: &Ident,
    interface_ident: &Ident,
    events: &[InterfaceEvent],
) -> TokenStream {
    if events.is_empty() {
        return quote! {};
    }

    // Get the event enum type path (e.g., ITIP20Events)
    let event_enum_path = match get_event_enum_path(interface_ident) {
        Ok(path) => path,
        Err(_) => return quote! {}, // Skip if we can't determine the event enum
    };

    let methods: Vec<_> = events
        .iter()
        .map(|event| gen_emission_helper(&event_enum_path, event))
        .collect();

    quote! {
        impl<'a, S: crate::storage::PrecompileStorageProvider> #contract_ident<'a, S> {
            #(#methods)*
        }
    }
}

/// Generates a single event emission helper method.
fn gen_emission_helper(event_enum_path: &TokenStream, event: &InterfaceEvent) -> TokenStream {
    let method_name = format!("_emit_{}", event.name);
    let method_ident: Ident = syn::parse_str(&method_name).expect("Valid identifier");

    // Extract the event variant name from the event_type_path
    let variant_ident = extract_event_variant_name(&event.event_type_path);

    let event_type_path = &event.event_type_path;

    // Generate parameter list and struct field assignments
    let (params, field_assignments): (Vec<_>, Vec<_>) = event
        .params
        .iter()
        .map(|(param_name, param_type, _indexed)| {
            let param_ident: Ident = syn::parse_str(param_name).expect("Valid identifier");
            (
                quote! { #param_ident: #param_type },
                quote! { #param_ident },
            )
        })
        .unzip();

    quote! {
        fn #method_ident(&mut self, #(#params),*) -> crate::error::Result<()> {
            use ::alloy::primitives::IntoLogData;
            self.storage.emit_event(
                self.address(),
                #event_enum_path::#variant_ident(#event_type_path {
                    #(#field_assignments),*
                })
                .into_log_data(),
            )
        }
    }
}
