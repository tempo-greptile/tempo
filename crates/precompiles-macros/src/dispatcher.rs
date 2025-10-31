//! Dispatcher generation for contract macro.
//!
//! This module generates the `trait Precompile` implementation that routes
//! EVM calldata to trait methods based on function selectors.

use crate::{
    FieldInfo,
    interface::{FunctionKind, InterfaceFunction},
    utils,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, Type};

/// Generates the `Precompile` implementation for the contract.
pub(crate) fn gen_dispatcher(
    strukt: &Ident,
    interface_types: &[Type],
    all_funcs: &[InterfaceFunction],
    _fields: &[FieldInfo],
) -> TokenStream {
    // Create a mapping from interface type to interface identifier
    let num_interfaces = interface_types.len();
    let interface_map: Vec<(String, Ident)> = interface_types
        .iter()
        .map(|ty| {
            let interface_ident =
                utils::try_extract_type_ident(ty).expect("Failed to extract interface identifier");

            let trait_name = if num_interfaces == 1 {
                // single interface: `<ContractName>Call`
                format_ident!("{}Call", strukt)
            } else {
                // multiple interfaces: `<ContractName>_<InterfaceName>Call`
                format_ident!("{}_{}", strukt, interface_ident)
            };
            (format!("{}", quote!(#ty)), trait_name)
        })
        .collect();

    // Generate match arms for each function
    let match_arms: Vec<TokenStream> = all_funcs
        .iter()
        .map(|func| gen_match_arm(strukt, &interface_map, func))
        .collect();

    quote! {
        impl<'a, S: crate::storage::PrecompileStorageProvider> crate::Precompile for #strukt<'a, S> {
            fn call(&mut self, calldata: &[u8], msg_sender: ::alloy::primitives::Address) -> ::revm::precompile::PrecompileResult {
                let selector: [u8; 4] = calldata
                    .get(..4)
                    .ok_or_else(|| {
                        ::revm::precompile::PrecompileError::Other(
                            "Invalid input: missing function selector".to_string()
                        )
                    })?
                    .try_into()
                    .map_err(|_| {
                        ::revm::precompile::PrecompileError::Other(
                            "Invalid selector format".to_string()
                        )
                    })?;

                match selector {
                    #(#match_arms)*
                    _ => Err(::revm::precompile::PrecompileError::Other(
                        "Unknown function selector".to_string()
                    )),
                }
            }
        }
    }
}

/// Generates an individual match arm for a function.
fn gen_match_arm(
    struct_name: &Ident,
    interface_map: &[(String, Ident)],
    func: &InterfaceFunction,
) -> TokenStream {
    let call_type = &func.call_type_path;
    let method_name = format_ident!("{}", func.name);

    // Determine which interface trait this function belongs to
    let call_type_str = format!("{call_type}");
    let trait_name = interface_map
        .iter()
        .find(|(interface_path, _)| call_type_str.starts_with(interface_path))
        .map(|(_, trait_name)| trait_name.clone())
        .unwrap_or_else(|| {
            // Fallback: extract interface from call_type
            let call_type_str = format!("{call_type}");
            let interface_name = call_type_str.split("::").next().unwrap_or("Unknown");
            format_ident!("{}_{}", struct_name, interface_name)
        });

    // Extract parameter field accessors from the call struct
    let param_fields = func.params.iter().map(|(name, _)| {
        let field_name = format_ident!("{}", name);
        quote! { call.#field_name }
    });

    match func.kind() {
        FunctionKind::Metadata => {
            quote! {
                #call_type::SELECTOR => {
                    crate::metadata::<#call_type>(|| #trait_name::#method_name(self))
                }
            }
        }
        FunctionKind::View => {
            let call_expr = if func.params.is_empty() {
                quote! { #trait_name::#method_name(self) }
            } else {
                quote! { #trait_name::#method_name(self, #(#param_fields),*) }
            };
            quote! {
                #call_type::SELECTOR => {
                    crate::view::<#call_type>(calldata, |call| {
                        #call_expr
                    })
                }
            }
        }
        FunctionKind::Mutate => {
            let call_expr = if func.params.is_empty() {
                quote! { #trait_name::#method_name(self, s) }
            } else {
                quote! { #trait_name::#method_name(self, s, #(#param_fields),*) }
            };
            quote! {
                #call_type::SELECTOR => {
                    crate::mutate::<#call_type>(
                        calldata,
                        msg_sender,
                        |s, call| #call_expr
                    )
                }
            }
        }
        FunctionKind::MutateVoid => {
            let call_expr = if func.params.is_empty() {
                quote! { #trait_name::#method_name(self, s) }
            } else {
                quote! { #trait_name::#method_name(self, s, #(#param_fields),*) }
            };
            quote! {
                #call_type::SELECTOR => {
                    crate::mutate_void::<#call_type>(
                        calldata,
                        msg_sender,
                        |s, call| #call_expr
                    )
                }
            }
        }
    }
}
