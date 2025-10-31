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
            name: "policy_id_counter",
            params: params(vec![]),
            return_type: parse_quote!(u64),
            is_view: true,
            call_type_path: quote!(#interface_ident::policyIdCounterCall),
        },
        InterfaceFunction {
            name: "policy_data",
            params: params(vec![("policy_id", parse_quote!(u64))]),
            return_type: parse_quote!((#interface_ident::PolicyType, Address)),
            is_view: true,
            call_type_path: quote!(#interface_ident::policyDataCall),
        },
        InterfaceFunction {
            name: "is_authorized",
            params: params(vec![
                ("policy_id", parse_quote!(u64)),
                ("user", parse_quote!(Address)),
            ]),
            return_type: parse_quote!(bool),
            is_view: true,
            call_type_path: quote!(#interface_ident::isAuthorizedCall),
        },
        // State-changing functions (non-void returns)
        InterfaceFunction {
            name: "create_policy",
            params: params(vec![
                ("admin", parse_quote!(Address)),
                ("policy_type", parse_quote!(#interface_ident::PolicyType)),
            ]),
            return_type: parse_quote!(u64),
            is_view: false,
            call_type_path: quote!(#interface_ident::createPolicyCall),
        },
        InterfaceFunction {
            name: "create_policy_with_accounts",
            params: params(vec![
                ("admin", parse_quote!(Address)),
                ("policy_type", parse_quote!(#interface_ident::PolicyType)),
                ("accounts", parse_quote!(Vec<Address>)),
            ]),
            return_type: parse_quote!(u64),
            is_view: false,
            call_type_path: quote!(#interface_ident::createPolicyWithAccountsCall),
        },
        // State-changing functions (void)
        InterfaceFunction {
            name: "set_policy_admin",
            params: params(vec![
                ("policy_id", parse_quote!(u64)),
                ("admin", parse_quote!(Address)),
            ]),
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::setPolicyAdminCall),
        },
        InterfaceFunction {
            name: "modify_policy_whitelist",
            params: params(vec![
                ("policy_id", parse_quote!(u64)),
                ("account", parse_quote!(Address)),
                ("allowed", parse_quote!(bool)),
            ]),
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::modifyPolicyWhitelistCall),
        },
        InterfaceFunction {
            name: "modify_policy_blacklist",
            params: params(vec![
                ("policy_id", parse_quote!(u64)),
                ("account", parse_quote!(Address)),
                ("restricted", parse_quote!(bool)),
            ]),
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::modifyPolicyBlacklistCall),
        },
    ]
}

pub(crate) fn get_events(interface_ident: &Ident) -> Vec<InterfaceEvent> {
    vec![
        InterfaceEvent {
            name: "policy_admin_updated",
            params: vec![
                ("policy_id", parse_quote!(u64), true),
                ("updater", parse_quote!(Address), true),
                ("admin", parse_quote!(Address), true),
            ],
            event_type_path: quote!(#interface_ident::PolicyAdminUpdated),
        },
        InterfaceEvent {
            name: "policy_created",
            params: vec![
                ("policy_id", parse_quote!(u64), true),
                ("updater", parse_quote!(Address), true),
                (
                    "policy_type",
                    parse_quote!(#interface_ident::PolicyType),
                    false,
                ),
            ],
            event_type_path: quote!(#interface_ident::PolicyCreated),
        },
        InterfaceEvent {
            name: "whitelist_updated",
            params: vec![
                ("policy_id", parse_quote!(u64), true),
                ("updater", parse_quote!(Address), true),
                ("account", parse_quote!(Address), true),
                ("allowed", parse_quote!(bool), false),
            ],
            event_type_path: quote!(#interface_ident::WhitelistUpdated),
        },
        InterfaceEvent {
            name: "blacklist_updated",
            params: vec![
                ("policy_id", parse_quote!(u64), true),
                ("updater", parse_quote!(Address), true),
                ("account", parse_quote!(Address), true),
                ("restricted", parse_quote!(bool), false),
            ],
            event_type_path: quote!(#interface_ident::BlacklistUpdated),
        },
    ]
}

pub(crate) fn get_errors(interface_ident: &Ident) -> Vec<InterfaceError> {
    vec![
        InterfaceError {
            name: "unauthorized",
            params: vec![],
            error_type_path: quote!(#interface_ident::Unauthorized),
        },
        InterfaceError {
            name: "incompatible_policy_type",
            params: vec![],
            error_type_path: quote!(#interface_ident::IncompatiblePolicyType),
        },
        InterfaceError {
            name: "self_owned_policy_must_be_whitelist",
            params: vec![],
            error_type_path: quote!(#interface_ident::SelfOwnedPolicyMustBeWhitelist),
        },
    ]
}
