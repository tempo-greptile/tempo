//! Unit enum code generation for the `#[solidity]` module macro.
//!
//! Generates Solidity-compatible unit enums that encode as `uint8`.
//! These correspond to Solidity enums like `enum Status { Pending, Filled, Cancelled }`.
//!
//! # Generated Code
//!
//! For each unit enum:
//! - `#[repr(u8)]` with explicit discriminants (0, 1, 2, ...)
//! - `From<Enum> for u8`
//! - `TryFrom<u8> for Enum`
//! - `SolType` implementation (encodes as uint8)
//! - `SolTypeValue` implementation

use proc_macro2::TokenStream;
use quote::quote;

use super::common;
use super::parser::UnitEnumDef;

/// Generate code for a unit enum definition.
pub(super) fn generate_unit_enum(def: &UnitEnumDef) -> TokenStream {
    let enum_name = &def.name;
    let vis = &def.vis;
    let attrs = &def.attrs;

    let variants_with_discriminants: Vec<TokenStream> = def
        .variants
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let idx = i as u8;
            quote! { #v = #idx }
        })
        .collect();

    let from_u8_arms: Vec<TokenStream> = def
        .variants
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let idx = i as u8;
            quote! { #idx => Ok(Self::#v) }
        })
        .collect();

    let enum_def = quote! {
        #(#attrs)*
        #[repr(u8)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #vis enum #enum_name {
            #(#variants_with_discriminants),*
        }
    };

    let trait_impls = common::expand_unit_enum_traits(
        enum_name,
        def.variants.len() as u8,
        &from_u8_arms,
        def.variants.first(),
    );

    quote! {
        #enum_def
        #trait_impls
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solidity::test_utils::make_unit_enum;

    #[test]
    fn test_generate_unit_enum() {
        let def = make_unit_enum("OrderStatus", vec!["Pending", "Filled", "Cancelled"]);
        let tokens = generate_unit_enum(&def);
        let code = tokens.to_string();

        assert!(code.contains("repr"));
        assert!(code.contains("u8"));
        assert!(code.contains("enum OrderStatus"));
        assert!(code.contains("Pending"));
        assert!(code.contains("Filled"));
        assert!(code.contains("Cancelled"));
        assert!(code.contains("From"));
        assert!(code.contains("TryFrom"));
        assert!(code.contains("SolType"));
    }

    #[test]
    fn test_generate_unit_enum_single_variant() {
        let def = make_unit_enum("SingleVariant", vec!["Only"]);
        let tokens = generate_unit_enum(&def);
        let code = tokens.to_string();

        assert!(code.contains("Only = 0u8"));
        assert!(code.contains("value < 1u8"));
    }
}
