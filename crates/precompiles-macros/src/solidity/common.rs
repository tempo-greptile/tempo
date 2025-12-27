//! Shared utilities for code generation.

use alloy_sol_macro_expander::{
    SolInterfaceData, SolInterfaceKind, expand_sol_interface, selector,
};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::Type;

use crate::utils::{SolType, to_snake_case};

use super::parser::{EnumVariantDef, FieldDef};
use super::registry::TypeRegistry;

/// Extract field names from a slice of FieldDef.
pub(super) fn extract_field_names(fields: &[FieldDef]) -> Vec<Ident> {
    fields.iter().map(|f| f.name.clone()).collect()
}

/// Extract field types as TokenStreams.
pub(super) fn extract_field_types(fields: &[FieldDef]) -> Vec<TokenStream> {
    fields
        .iter()
        .map(|f| {
            let ty = &f.ty;
            quote! { #ty }
        })
        .collect()
}

/// Extract param names from method params.
pub(super) fn extract_param_names(params: &[(Ident, Type)]) -> Vec<Ident> {
    params.iter().map(|(n, _)| n.clone()).collect()
}

/// Extract param types.
pub(super) fn extract_param_types(params: &[(Ident, Type)]) -> Vec<Type> {
    params.iter().map(|(_, t)| t.clone()).collect()
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
    types
        .iter()
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
    let field_pairs: Vec<_> = fields.iter().map(|f| (&f.name, &f.ty)).collect();
    generate_simple_struct(name, &field_pairs, doc)
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
    registry.compute_signature_from_fields(&variant.name.to_string(), &variant.fields)
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

/// Generate Error container enum from variants.
pub(super) fn generate_error_container(
    variants: &[EnumVariantDef],
    registry: &TypeRegistry,
) -> syn::Result<TokenStream> {
    let names: Vec<Ident> = variants.iter().map(|v| v.name.clone()).collect();
    let signatures: syn::Result<Vec<String>> = variants
        .iter()
        .map(|v| variant_signature(v, registry))
        .collect();
    let field_counts: Vec<usize> = variants.iter().map(|v| v.fields.len()).collect();
    Ok(generate_sol_interface_container(
        "Error",
        &names,
        &names,
        &signatures?,
        &field_counts,
        SolInterfaceKind::Error,
    ))
}

/// Generate Event container enum with IntoLogData impl and From conversions.
pub(super) fn generate_event_container(variants: &[EnumVariantDef]) -> TokenStream {
    let names: Vec<&Ident> = variants.iter().map(|v| &v.name).collect();
    let container = format_ident!("Event");

    let from_impls: TokenStream = names
        .iter()
        .map(|name| generate_from_impl(name, &container))
        .collect();

    quote! {
        /// Container enum for all event types.
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub enum Event {
            #(#[allow(missing_docs)] #names(#names),)*
        }

        #[automatically_derived]
        impl ::alloy::primitives::IntoLogData for Event {
            fn to_log_data(&self) -> ::alloy::primitives::LogData {
                match self { #(Self::#names(inner) => inner.to_log_data(),)* }
            }
            fn into_log_data(self) -> ::alloy::primitives::LogData {
                match self { #(Self::#names(inner) => inner.into_log_data(),)* }
            }
        }

        #from_impls
    }
}

/// Generate all trait implementations for a unit enum (uint8-encoded).
///
/// Generates: From<Enum> for u8, TryFrom<u8>, SolType, SolTypeValue, SolValue, Default
pub(super) fn expand_unit_enum_traits(
    enum_name: &Ident,
    variant_count: u8,
    from_u8_arms: &[TokenStream],
    first_variant: Option<&Ident>,
) -> TokenStream {
    let from_impl = quote! {
        #[automatically_derived]
        impl ::core::convert::From<#enum_name> for u8 {
            #[inline]
            fn from(value: #enum_name) -> u8 {
                value as u8
            }
        }
    };

    let try_from_impl = quote! {
        #[automatically_derived]
        impl ::core::convert::TryFrom<u8> for #enum_name {
            type Error = ();

            #[inline]
            fn try_from(value: u8) -> ::core::result::Result<Self, ()> {
                match value {
                    #(#from_u8_arms,)*
                    _ => Err(()),
                }
            }
        }
    };

    let sol_type_impl = quote! {
        #[automatically_derived]
        impl alloy_sol_types::SolType for #enum_name {
            type RustType = Self;
            type Token<'a> = <alloy_sol_types::sol_data::Uint<8> as alloy_sol_types::SolType>::Token<'a>;

            const SOL_NAME: &'static str = "uint8";
            const ENCODED_SIZE: Option<usize> = Some(32);
            const PACKED_ENCODED_SIZE: Option<usize> = Some(1);

            #[inline]
            fn valid_token(token: &Self::Token<'_>) -> bool {
                let value: u8 = token.0.to();
                value < #variant_count
            }

            #[inline]
            fn detokenize(token: Self::Token<'_>) -> Self::RustType {
                let value: u8 = token.0.to();
                // SAFETY: Returns default variant for invalid values (defensive, should not occur with valid_token check)
                Self::try_from(value).unwrap_or_default()
            }
        }
    };

    let sol_type_value_impl = quote! {
        #[automatically_derived]
        impl alloy_sol_types::private::SolTypeValue<#enum_name> for #enum_name {
            #[inline]
            fn stv_to_tokens(&self) -> <#enum_name as alloy_sol_types::SolType>::Token<'_> {
                alloy_sol_types::Word::from(alloy::primitives::U256::from(*self as u8))
            }

            #[inline]
            fn stv_abi_encode_packed_to(&self, out: &mut alloy_sol_types::private::Vec<u8>) {
                out.push(*self as u8);
            }

            #[inline]
            fn stv_eip712_data_word(&self) -> alloy_sol_types::Word {
                <alloy_sol_types::sol_data::Uint<8> as alloy_sol_types::SolType>::tokenize(&(*self as u8)).0
            }
        }
    };

    let sol_value_impl = quote! {
        #[automatically_derived]
        impl alloy_sol_types::SolValue for #enum_name {
            type SolType = Self;
        }
    };

    let default_impl = first_variant.map(|fv| {
        quote! {
            #[automatically_derived]
            impl ::core::default::Default for #enum_name {
                #[inline]
                fn default() -> Self {
                    Self::#fv
                }
            }
        }
    });

    quote! {
        #from_impl
        #try_from_impl
        #sol_type_impl
        #sol_type_value_impl
        #sol_value_impl
        #default_impl
    }
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
