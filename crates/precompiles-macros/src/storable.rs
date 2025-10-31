//! Implementation of the `#[derive(Storable)]` macro.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Type};

/// Implements the `Storable` derive macro for a struct.
pub(crate) fn derive_impl(input: DeriveInput) -> syn::Result<TokenStream> {
    // Extract struct name and generics
    let struct_name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Parse struct fields
    let fields = match &input.data {
        Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields_named) => &fields_named.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    &input.ident,
                    "`Storable` can only be derived for structs with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "`Storable` can only be derived for structs",
            ));
        }
    };

    // Extract field information
    let field_count = fields.len();
    let field_names: Vec<_> = fields.iter().map(|f| f.ident.as_ref().unwrap()).collect();
    let _field_types: Vec<&Type> = fields.iter().map(|f| &f.ty).collect();

    // Generate load implementation (uses Storable::load for each field)
    let load_fields = field_names.iter().enumerate().map(|(offset, name)| {
        quote! {
            let #name = <_>::load(storage, base_slot + ::alloy::primitives::U256::from(#offset))?;
        }
    });

    let construct_self = quote! {
        Ok(Self {
            #(#field_names),*
        })
    };

    // Generate store implementation (uses Storable::store for each field)
    let store_fields = field_names.iter().enumerate().map(|(offset, name)| {
        quote! {
            self.#name.store(storage, base_slot + ::alloy::primitives::U256::from(#offset))?;
        }
    });

    // Generate the trait implementation
    let expanded = quote! {
        impl #impl_generics crate::storage::Storable for #struct_name #ty_generics #where_clause {
            const SLOT_COUNT: usize = #field_count;

            fn load<S>(
                storage: &mut S,
                base_slot: ::alloy::primitives::U256,
            ) -> Result<Self, crate::error::TempoPrecompileError>
            where
                S: crate::storage::StorageOps,
            {
                use crate::storage::Storable;

                #(#load_fields)*

                #construct_self
            }

            fn store<S>(
                &self,
                storage: &mut S,
                base_slot: ::alloy::primitives::U256,
            ) -> Result<(), crate::error::TempoPrecompileError>
            where
                S: crate::storage::StorageOps,
            {
                use crate::storage::Storable;

                #(#store_fields)*

                Ok(())
            }
        }
    };

    Ok(expanded)
}
