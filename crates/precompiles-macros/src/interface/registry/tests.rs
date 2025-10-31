use crate::interface::{InterfaceError, InterfaceEvent, InterfaceFunction, ParamName};
use quote::quote;
use syn::{Ident, Type, parse_quote};

// Test interface for E2E dispatcher tests
pub(crate) fn get_itest_token_functions(interface_ident: &Ident) -> Vec<InterfaceFunction> {
    // Helper to convert parameter tuples to ParamName
    let params = |p: Vec<(&'static str, Type)>| -> Vec<(ParamName, Type)> {
        p.into_iter()
            .map(|(name, ty)| (ParamName::new(name), ty))
            .collect()
    };

    vec![
        // Metadata functions (view, no parameters)
        InterfaceFunction {
            name: "name",
            params: params(vec![]),
            return_type: parse_quote!(String),
            is_view: true,
            call_type_path: quote!(#interface_ident::nameCall),
        },
        InterfaceFunction {
            name: "symbol",
            params: params(vec![]),
            return_type: parse_quote!(String),
            is_view: true,
            call_type_path: quote!(#interface_ident::symbolCall),
        },
        InterfaceFunction {
            name: "decimals",
            params: params(vec![]),
            return_type: parse_quote!(u8),
            is_view: true,
            call_type_path: quote!(#interface_ident::decimalsCall),
        },
        // View functions (with parameters)
        InterfaceFunction {
            name: "balance_of",
            params: params(vec![("account", parse_quote!(Address))]),
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_ident::balanceOfCall),
        },
        InterfaceFunction {
            name: "allowance",
            params: params(vec![
                ("owner", parse_quote!(Address)),
                ("spender", parse_quote!(Address)),
            ]),
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_ident::allowanceCall),
        },
        // Mutating functions (non-void)
        InterfaceFunction {
            name: "transfer",
            params: params(vec![
                ("to", parse_quote!(Address)),
                ("amount", parse_quote!(U256)),
            ]),
            return_type: parse_quote!(bool),
            is_view: false,
            call_type_path: quote!(#interface_ident::transferCall),
        },
        InterfaceFunction {
            name: "approve",
            params: params(vec![
                ("spender", parse_quote!(Address)),
                ("amount", parse_quote!(U256)),
            ]),
            return_type: parse_quote!(bool),
            is_view: false,
            call_type_path: quote!(#interface_ident::approveCall),
        },
        // Mutating functions (void)
        InterfaceFunction {
            name: "mint",
            params: params(vec![
                ("to", parse_quote!(Address)),
                ("amount", parse_quote!(U256)),
            ]),
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::mintCall),
        },
        InterfaceFunction {
            name: "burn",
            params: params(vec![("amount", parse_quote!(U256))]),
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::burnCall),
        },
    ]
}

// Test interface for multi-interface testing
pub(crate) fn get_imetadata_functions(interface_ident: &Ident) -> Vec<InterfaceFunction> {
    // Helper to convert parameter tuples to ParamName
    let params = |p: Vec<(&'static str, Type)>| -> Vec<(ParamName, Type)> {
        p.into_iter()
            .map(|(name, ty)| (ParamName::new(name), ty))
            .collect()
    };

    vec![
        InterfaceFunction {
            name: "version",
            params: params(vec![]),
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_ident::versionCall),
        },
        InterfaceFunction {
            name: "owner",
            params: params(vec![]),
            return_type: parse_quote!(Address),
            is_view: true,
            call_type_path: quote!(#interface_ident::ownerCall),
        },
    ]
}

// Mini token test interface for event emission testing
pub(crate) fn get_imini_token_functions(interface_ident: &Ident) -> Vec<InterfaceFunction> {
    // Helper to convert parameter tuples to ParamName
    let params = |p: Vec<(&'static str, Type)>| -> Vec<(ParamName, Type)> {
        p.into_iter()
            .map(|(name, ty)| (ParamName::new(name), ty))
            .collect()
    };

    vec![InterfaceFunction {
        name: "mint",
        params: params(vec![
            ("to", parse_quote!(Address)),
            ("amount", parse_quote!(U256)),
        ]),
        return_type: parse_quote!(()),
        is_view: false,
        call_type_path: quote!(#interface_ident::mintCall),
    }]
}

pub(crate) fn get_imini_token_events(interface_ident: &Ident) -> Vec<InterfaceEvent> {
    vec![
        InterfaceEvent {
            name: "transfer",
            params: vec![
                ("from", parse_quote!(Address), true),
                ("to", parse_quote!(Address), true),
                ("amount", parse_quote!(U256), false),
            ],
            event_type_path: quote!(#interface_ident::Transfer),
        },
        InterfaceEvent {
            name: "mint",
            params: vec![
                ("to", parse_quote!(Address), true),
                ("amount", parse_quote!(U256), false),
            ],
            event_type_path: quote!(#interface_ident::Mint),
        },
    ]
}

// Test interface for error constructor generation
pub(crate) fn get_ierror_test_functions(interface_ident: &Ident) -> Vec<InterfaceFunction> {
    // Helper to convert parameter tuples to ParamName
    let params = |p: Vec<(&'static str, Type)>| -> Vec<(ParamName, Type)> {
        p.into_iter()
            .map(|(name, ty)| (ParamName::new(name), ty))
            .collect()
    };

    vec![InterfaceFunction {
        name: "dummy",
        params: params(vec![]),
        return_type: parse_quote!(()),
        is_view: false,
        call_type_path: quote!(#interface_ident::dummyCall),
    }]
}

pub(crate) fn get_ierror_test_errors(interface_ident: &Ident) -> Vec<InterfaceError> {
    vec![
        InterfaceError {
            name: "simple_error",
            params: vec![],
            error_type_path: quote!(#interface_ident::SimpleError),
        },
        InterfaceError {
            name: "parameterized_error",
            params: vec![
                ("code", parse_quote!(U256)),
                ("addr", parse_quote!(Address)),
            ],
            error_type_path: quote!(#interface_ident::ParameterizedError),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interface::{InterfaceError, InterfaceEvent, InterfaceFunction, ParamName, FunctionKind, get_event_enum_path, parse_interface};
    use crate::utils::try_extract_type_ident;
    use quote::quote;
    use syn::{Ident, Type, parse_quote};

    #[test]
    fn test_extract_interface_ident() {
        let ty: Type = parse_quote!(ITIP20);
        let ident = try_extract_type_ident(&ty).unwrap();
        assert_eq!(ident.to_string(), "ITIP20");

        let ty: Type = parse_quote!(crate::ITIP20);
        let ident = try_extract_type_ident(&ty).unwrap();
        assert_eq!(ident.to_string(), "ITIP20");
    }

    #[test]
    fn test_get_event_enum_path() {
        // Simple path
        let ty: Type = parse_quote!(ITIP20);
        let ident = try_extract_type_ident(&ty).unwrap();
        let path = get_event_enum_path(&ident).unwrap();
        assert_eq!(path.to_string(), "ITIP20Events");

        // Qualified path
        let ty: Type = parse_quote!(crate::ITIP20);
        let ident = try_extract_type_ident(&ty).unwrap();
        let path = get_event_enum_path(&ident).unwrap();
        assert_eq!(path.to_string(), "ITIP20Events");

        // Test with ITestToken
        let ty: Type = parse_quote!(ITestToken);
        let ident = try_extract_type_ident(&ty).unwrap();
        let path = get_event_enum_path(&ident).unwrap();
        assert_eq!(path.to_string(), "ITestTokenEvents");
    }

    #[test]
    fn test_parse_interface_itip20() {
        let ty: Type = parse_quote!(ITIP20);
        let ident = try_extract_type_ident(&ty).unwrap();
        let parsed = parse_interface(&ident).unwrap();

        // Should have 28 functions
        assert_eq!(parsed.functions.len(), 28);

        // Should have 11 events (matching sol! interface)
        assert_eq!(parsed.events.len(), 11);

        // Should have 13 errors
        assert_eq!(parsed.errors.len(), 13);

        // Check a few specific functions
        let name_fn = parsed.functions.iter().find(|f| f.name == "name");
        assert!(name_fn.is_some());
        assert!(name_fn.unwrap().is_view);
        assert!(name_fn.unwrap().params.is_empty());

        let balance_of_fn = parsed.functions.iter().find(|f| f.name == "balance_of");
        assert!(balance_of_fn.is_some());
        assert_eq!(balance_of_fn.unwrap().params.len(), 1);

        // Check a few specific events
        let transfer_event = parsed.events.iter().find(|e| e.name == "transfer");
        assert!(transfer_event.is_some());
        assert_eq!(transfer_event.unwrap().params.len(), 3);

        // Check a few specific errors
        let insufficient_balance_error = parsed
            .errors
            .iter()
            .find(|e| e.name == "insufficient_balance");
        assert!(insufficient_balance_error.is_some());
        assert_eq!(insufficient_balance_error.unwrap().params.len(), 0);
    }

    #[test]
    fn test_parse_unknown_interface() {
        let ty: Type = parse_quote!(UnknownInterface);
        let ident = try_extract_type_ident(&ty).unwrap();
        let parsed = parse_interface(&ident).unwrap();

        // Should return empty vecs for unknown interfaces
        assert!(parsed.functions.is_empty());
        assert!(parsed.events.is_empty());
        assert!(parsed.errors.is_empty());
    }

    #[test]
    fn test_fn_kind() {
        let new_fn = |name: &'static str,
                      params: Vec<(&'static str, Type)>,
                      return_type: Type,
                      is_view: bool|
         -> InterfaceFunction {
            InterfaceFunction {
                name,
                params: params
                    .into_iter()
                    .map(|(name, ty)| (ParamName::new(name), ty))
                    .collect(),
                return_type,
                is_view,
                call_type_path: quote::quote!(ITIP20::testCall),
            }
        };

        let func = new_fn("name", vec![], parse_quote!(String), true);
        assert_eq!(func.kind(), FunctionKind::Metadata);

        let func = new_fn(
            "balance_of",
            vec![("account", parse_quote!(Address))],
            parse_quote!(U256),
            true,
        );
        assert_eq!(func.kind(), FunctionKind::View);

        let func = new_fn(
            "transfer",
            vec![
                ("to", parse_quote!(Address)),
                ("amount", parse_quote!(U256)),
            ],
            parse_quote!(bool),
            false,
        );
        assert_eq!(func.kind(), FunctionKind::Mutate);

        let func = new_fn(
            "mint",
            vec![
                ("to", parse_quote!(Address)),
                ("amount", parse_quote!(U256)),
            ],
            parse_quote!(()),
            false,
        );
        assert_eq!(func.kind(), FunctionKind::MutateVoid);
    }
}
