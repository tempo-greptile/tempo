//! Shared test utilities for the solidity module.

#![cfg(test)]

use proc_macro2::Span;
use quote::format_ident;
use syn::{Type, Visibility};

use super::parser::{
    EnumVariantDef, FieldDef, InterfaceDef, MethodDef, SolEnumDef, SolStructDef, SolidityModule,
    UnitEnumDef,
};

pub(super) fn make_field(name: &str, ty: Type) -> FieldDef {
    FieldDef {
        name: format_ident!("{}", name),
        ty,
        indexed: false,
        vis: Visibility::Public(syn::token::Pub {
            span: Span::call_site(),
        }),
    }
}

pub(super) fn make_field_indexed(name: &str, ty: Type, indexed: bool) -> FieldDef {
    FieldDef {
        indexed,
        ..make_field(name, ty)
    }
}

pub(super) fn make_struct(name: &str, fields: Vec<FieldDef>) -> SolStructDef {
    SolStructDef {
        name: format_ident!("{}", name),
        fields,
        derives: vec![],
        attrs: vec![],
        vis: Visibility::Public(syn::token::Pub {
            span: Span::call_site(),
        }),
    }
}

pub(super) fn make_variant(name: &str, fields: Vec<FieldDef>) -> EnumVariantDef {
    EnumVariantDef {
        name: format_ident!("{}", name),
        fields,
    }
}

pub(super) fn make_unit_enum(name: &str, variants: Vec<&str>) -> UnitEnumDef {
    UnitEnumDef {
        name: format_ident!("{}", name),
        variants: variants.iter().map(|v| format_ident!("{}", v)).collect(),
        attrs: vec![],
        vis: Visibility::Public(syn::token::Pub {
            span: Span::call_site(),
        }),
    }
}

pub(super) fn make_error_enum(variants: Vec<EnumVariantDef>) -> SolEnumDef {
    SolEnumDef {
        name: format_ident!("Error"),
        variants,
        attrs: vec![],
        vis: Visibility::Public(syn::token::Pub {
            span: Span::call_site(),
        }),
    }
}

pub(super) fn make_event_enum(variants: Vec<EnumVariantDef>) -> SolEnumDef {
    SolEnumDef {
        name: format_ident!("Event"),
        variants,
        attrs: vec![],
        vis: Visibility::Public(syn::token::Pub {
            span: Span::call_site(),
        }),
    }
}

pub(super) fn make_method(
    name: &str,
    sol_name: &str,
    params: Vec<(proc_macro2::Ident, Type)>,
    return_type: Option<Type>,
    is_mutable: bool,
) -> MethodDef {
    MethodDef {
        name: format_ident!("{}", name),
        sol_name: sol_name.to_string(),
        params,
        return_type,
        is_mutable,
    }
}

pub(super) fn make_interface(methods: Vec<MethodDef>) -> InterfaceDef {
    InterfaceDef {
        name: format_ident!("Interface"),
        methods,
        attrs: vec![],
        vis: Visibility::Public(syn::token::Pub {
            span: Span::call_site(),
        }),
    }
}

pub(super) fn empty_module() -> SolidityModule {
    SolidityModule {
        name: format_ident!("test"),
        vis: Visibility::Public(syn::token::Pub {
            span: Span::call_site(),
        }),
        imports: vec![],
        structs: vec![],
        unit_enums: vec![],
        error: None,
        event: None,
        interface: None,
        other_items: vec![],
    }
}
