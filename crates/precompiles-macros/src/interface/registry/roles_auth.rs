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

    vec![
        // View functions
        InterfaceFunction {
            name: "has_role",
            params: params(vec![
                ("account", parse_quote!(Address)),
                ("role", parse_quote!(B256)),
            ]),
            return_type: parse_quote!(bool),
            is_view: true,
            call_type_path: quote!(#interface_ident::hasRoleCall),
        },
        InterfaceFunction {
            name: "get_role_admin",
            params: params(vec![("role", parse_quote!(B256))]),
            return_type: parse_quote!(B256),
            is_view: true,
            call_type_path: quote!(#interface_ident::getRoleAdminCall),
        },
        // Mutating functions (void)
        InterfaceFunction {
            name: "grant_role",
            params: params(vec![
                ("role", parse_quote!(B256)),
                ("account", parse_quote!(Address)),
            ]),
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::grantRoleCall),
        },
        InterfaceFunction {
            name: "revoke_role",
            params: params(vec![
                ("role", parse_quote!(B256)),
                ("account", parse_quote!(Address)),
            ]),
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::revokeRoleCall),
        },
        InterfaceFunction {
            name: "renounce_role",
            params: params(vec![("role", parse_quote!(B256))]),
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::renounceRoleCall),
        },
        InterfaceFunction {
            name: "set_role_admin",
            params: params(vec![
                ("role", parse_quote!(B256)),
                ("adminRole", parse_quote!(B256)),
            ]),
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::setRoleAdminCall),
        },
    ]
}

pub(crate) fn get_events(interface_ident: &Ident) -> Vec<InterfaceEvent> {
    vec![
        InterfaceEvent {
            name: "role_membership_updated",
            params: vec![
                ("role", parse_quote!(B256), true),
                ("account", parse_quote!(Address), true),
                ("sender", parse_quote!(Address), true),
                ("hasRole", parse_quote!(bool), false),
            ],
            event_type_path: quote!(#interface_ident::RoleMembershipUpdated),
        },
        InterfaceEvent {
            name: "role_admin_updated",
            params: vec![
                ("role", parse_quote!(B256), true),
                ("newAdminRole", parse_quote!(B256), true),
                ("sender", parse_quote!(Address), true),
            ],
            event_type_path: quote!(#interface_ident::RoleAdminUpdated),
        },
    ]
}

pub(crate) fn get_errors(interface_ident: &Ident) -> Vec<InterfaceError> {
    vec![InterfaceError {
        name: "unauthorized",
        params: vec![],
        error_type_path: quote!(#interface_ident::Unauthorized),
    }]
}
