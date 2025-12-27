//! Unified Error/Event enum code generation for the `#[solidity]` module macro.

use alloy_sol_macro_expander::{
    EventFieldInfo, SolErrorData, SolEventData, expand_from_into_tuples_simple,
    expand_tokenize_simple,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::utils::SolType;

use super::common;
use super::parser::{EnumVariantDef, SolEnumDef};
use super::registry::TypeRegistry;

/// Kind of variant enum being generated.
#[derive(Clone, Copy)]
pub(super) enum VariantEnumKind {
    Error,
    Event,
}

/// Generate code for Error or Event enum.
pub(super) fn generate_variant_enum(
    def: &SolEnumDef,
    registry: &TypeRegistry,
    kind: VariantEnumKind,
) -> syn::Result<TokenStream> {
    let variant_impls: syn::Result<Vec<TokenStream>> = def
        .variants
        .iter()
        .map(|v| generate_variant(v, registry, kind))
        .collect();
    let variant_impls = variant_impls?;

    let container_name = match kind {
        VariantEnumKind::Error => format_ident!("Error"),
        VariantEnumKind::Event => format_ident!("Event"),
    };

    let container = match kind {
        VariantEnumKind::Error => common::generate_error_container(&def.variants, registry)?,
        VariantEnumKind::Event => common::generate_event_container(&def.variants),
    };

    let constructors = common::generate_constructors(&container_name, &def.variants);

    Ok(quote! {
        #(#variant_impls)*
        #container
        #constructors
    })
}

/// Generate code for a single variant (Error or Event).
fn generate_variant(
    variant: &EnumVariantDef,
    registry: &TypeRegistry,
    kind: VariantEnumKind,
) -> syn::Result<TokenStream> {
    let struct_name = &variant.name;
    let signature = common::variant_signature(variant, registry)?;
    let field_names = common::extract_field_names(&variant.fields);
    let field_types = common::extract_field_types(&variant.fields);

    let doc_kind = match kind {
        VariantEnumKind::Error => "Custom error",
        VariantEnumKind::Event => "Event",
    };
    let doc = common::signature_doc(doc_kind, &signature);
    let variant_struct = common::generate_variant_struct(struct_name, &variant.fields, &doc);
    let from_tuple = expand_from_into_tuples_simple(struct_name, &field_names, &field_types);

    let trait_impl = match kind {
        VariantEnumKind::Error => generate_sol_error_impl(variant, &signature)?,
        VariantEnumKind::Event => generate_sol_event_impl(variant, &signature)?,
    };

    Ok(quote! {
        #variant_struct
        #from_tuple
        #trait_impl
    })
}

/// Generate SolError trait implementation.
fn generate_sol_error_impl(variant: &EnumVariantDef, signature: &str) -> syn::Result<TokenStream> {
    let struct_name = &variant.name;
    let field_names = common::extract_field_names(&variant.fields);
    let sol_types = common::fields_to_sol_types(&variant.fields)?;
    let param_tuple = common::make_param_tuple(&sol_types);
    let tokenize_impl = expand_tokenize_simple(&field_names, &sol_types);

    Ok(SolErrorData {
        param_tuple,
        tokenize_impl,
    }
    .expand(struct_name, signature))
}

/// Generate SolEvent trait implementation.
fn generate_sol_event_impl(variant: &EnumVariantDef, signature: &str) -> syn::Result<TokenStream> {
    let struct_name = &variant.name;

    let fields: syn::Result<Vec<EventFieldInfo>> = variant
        .fields
        .iter()
        .map(|f| {
            let sol_ty = SolType::from_syn(&f.ty)?;
            Ok(EventFieldInfo {
                name: f.name.clone(),
                sol_type: sol_ty.to_sol_data(),
                is_indexed: f.indexed,
                indexed_as_hash: f.indexed && sol_ty.is_dynamic(),
            })
        })
        .collect();

    Ok(SolEventData {
        anonymous: false,
        fields: fields?,
    }
    .expand(struct_name, signature))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solidity::test_utils::{
        empty_module, make_error_enum, make_event_enum, make_field, make_field_indexed,
        make_variant,
    };
    use syn::parse_quote;

    #[test]
    fn test_generate_error_and_event_enums() -> syn::Result<()> {
        let module = empty_module();
        let registry = TypeRegistry::from_module(&module)?;

        // Error enum
        let error_def = make_error_enum(vec![
            make_variant("Unauthorized", vec![]),
            make_variant(
                "InsufficientBalance",
                vec![
                    make_field("available", parse_quote!(U256)),
                    make_field("required", parse_quote!(U256)),
                ],
            ),
        ]);
        let error_code =
            generate_variant_enum(&error_def, &registry, VariantEnumKind::Error)?.to_string();
        assert!(
            error_code.contains("struct Unauthorized")
                && error_code.contains("struct InsufficientBalance")
        );
        assert!(error_code.contains("enum Error") && error_code.contains("fn unauthorized"));

        // Event enum
        let event_def = make_event_enum(vec![make_variant(
            "Transfer",
            vec![
                make_field_indexed("from", parse_quote!(Address), true),
                make_field_indexed("to", parse_quote!(Address), true),
                make_field_indexed("amount", parse_quote!(U256), false),
            ],
        )]);
        let event_code =
            generate_variant_enum(&event_def, &registry, VariantEnumKind::Event)?.to_string();
        assert!(event_code.contains("struct Transfer") && event_code.contains("enum Event"));
        assert!(event_code.contains("IntoLogData") && event_code.contains("fn transfer"));
        Ok(())
    }
}
