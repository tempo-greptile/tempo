//! Shared utilities for code generation.

use alloy_sol_macro_expander::{SolInterfaceData, SolInterfaceKind, expand_sol_interface, selector};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::Type;

use crate::utils::{to_snake_case, SolType};

use super::parser::{EnumVariantDef, FieldDef};
use super::registry::TypeRegistry;

/// Extract field names from a slice of FieldDef.
pub(super) fn extract_field_names(fields: &[FieldDef]) -> Vec<Ident> {
    fields.iter().map(|f| f.name.clone()).collect()
}

/// Extract field types as TokenStreams.
pub(super) fn extract_field_types(fields: &[FieldDef]) -> Vec<TokenStream> {
    fields.iter().map(|f| { let ty = &f.ty; quote! { #ty } }).collect()
}

/// Convert fields to sol_data types.
pub(super) fn fields_to_sol_types(fields: &[FieldDef]) -> syn::Result<Vec<TokenStream>> {
    let types: Vec<_> = fields.iter().map(|f| f.ty.clone()).collect();
    types_to_sol_types(&types)
}

/// Generate param tuple from sol_types.
pub(super) fn make_param_tuple(sol_types: &[TokenStream]) -> TokenStream {
    if sol_types.is_empty() {
        quote! { () }
    } else {
        quote! { (#(#sol_types,)*) }
    }
}

/// Convert types to sol_data types.
pub(super) fn types_to_sol_types(types: &[syn::Type]) -> syn::Result<Vec<TokenStream>> {
    types.iter()
        .map(|ty| Ok(SolType::from_syn(ty)?.to_sol_data()))
        .collect()
}

/// Generate From impl for converting variant to container enum.
pub(super) fn generate_from_impl(variant_name: &Ident, container: &Ident) -> TokenStream {
    quote! {
        #[automatically_derived]
        impl ::core::convert::From<#variant_name> for #container {
            #[inline]
            fn from(value: #variant_name) -> Self {
                Self::#variant_name(value)
            }
        }
    }
}

/// Generate variant struct (unit or with fields).
pub(super) fn generate_variant_struct(name: &Ident, fields: &[FieldDef], doc: &str) -> TokenStream {
    if fields.is_empty() {
        quote! {
            #[doc = #doc]
            #[derive(Clone, Debug, PartialEq, Eq)]
            pub struct #name;
        }
    } else {
        let names: Vec<_> = fields.iter().map(|f| &f.name).collect();
        let types: Vec<_> = fields.iter().map(|f| &f.ty).collect();
        quote! {
            #[doc = #doc]
            #[derive(Clone, Debug, PartialEq, Eq)]
            pub struct #name {
                #(pub #names: #types),*
            }
        }
    }
}

/// Generate signature doc string with selector.
pub(super) fn signature_doc(kind: &str, signature: &str) -> String {
    let sel = selector(signature);
    format!(
        "{} with signature `{}` and selector `0x{}`.",
        kind,
        signature,
        hex::encode(sel)
    )
}

/// Compute variant signature using registry.
pub(super) fn variant_signature(
    variant: &EnumVariantDef,
    registry: &TypeRegistry,
) -> syn::Result<String> {
    let param_types: Vec<_> = variant.fields.iter().map(|f| f.ty.clone()).collect();
    registry.compute_signature(&variant.name.to_string(), &param_types)
}

/// Generate a SolInterface container enum (Calls, Error, or Event).
///
/// Takes variant names, type names, signatures, and field counts to build
/// the `SolInterfaceData` and expand it.
pub(super) fn generate_sol_interface_container(
    container_name: &str,
    variants: &[Ident],
    types: &[Ident],
    signatures: &[String],
    field_counts: &[usize],
    kind: SolInterfaceKind,
) -> TokenStream {
    let data = SolInterfaceData {
        name: format_ident!("{}", container_name),
        variants: variants.to_vec(),
        types: types.to_vec(),
        selectors: signatures.iter().map(selector).collect(),
        min_data_len: field_counts.iter().copied().min().unwrap_or(0) * 32,
        signatures: signatures.to_vec(),
        kind,
    };
    expand_sol_interface(data)
}

/// Generate simple struct (unit or with named fields).
///
/// Unlike `generate_variant_struct`, this takes raw (name, type) pairs
/// instead of FieldDef, useful for interface call structs.
pub(super) fn generate_simple_struct(
    name: &Ident,
    fields: &[(&Ident, &Type)],
    doc: &str,
) -> TokenStream {
    if fields.is_empty() {
        quote! {
            #[doc = #doc]
            #[derive(Clone, Debug, PartialEq, Eq)]
            pub struct #name;
        }
    } else {
        let names: Vec<_> = fields.iter().map(|(n, _)| *n).collect();
        let types: Vec<_> = fields.iter().map(|(_, t)| *t).collect();
        quote! {
            #[doc = #doc]
            #[derive(Clone, Debug, PartialEq, Eq)]
            pub struct #name {
                #(pub #names: #types),*
            }
        }
    }
}

/// Generate constructor methods for container enum.
/// Empty variants use `const fn`, variants with fields use regular `fn`.
pub(super) fn generate_constructors(container: &Ident, variants: &[EnumVariantDef]) -> TokenStream {
    let constructors: Vec<TokenStream> = variants
        .iter()
        .map(|v| {
            let variant_name = &v.name;
            let fn_name = format_ident!("{}", to_snake_case(&v.name.to_string()));

            if v.fields.is_empty() {
                quote! {
                    #[doc = concat!("Creates a new `", stringify!(#variant_name), "`.")]
                    pub const fn #fn_name() -> Self {
                        Self::#variant_name(#variant_name)
                    }
                }
            } else {
                let param_names: Vec<_> = v.fields.iter().map(|f| &f.name).collect();
                let param_types: Vec<_> = v.fields.iter().map(|f| &f.ty).collect();

                quote! {
                    #[doc = concat!("Creates a new `", stringify!(#variant_name), "`.")]
                    pub fn #fn_name(#(#param_names: #param_types),*) -> Self {
                        Self::#variant_name(#variant_name { #(#param_names),* })
                    }
                }
            }
        })
        .collect();

    quote! {
        impl #container {
            #(#constructors)*
        }
    }
}
