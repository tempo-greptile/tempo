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
        // Reward functions
        InterfaceFunction {
            name: "start_reward",
            params: params(vec![
                ("amount", parse_quote!(U256)),
                ("seconds", parse_quote!(u128)),
            ]),
            return_type: parse_quote!(u64),
            is_view: false,
            call_type_path: quote!(#interface_ident::startRewardCall),
        },
        InterfaceFunction {
            name: "set_reward_recipient",
            params: params(vec![("recipient", parse_quote!(Address))]),
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_ident::setRewardRecipientCall),
        },
        InterfaceFunction {
            name: "cancel_reward",
            params: params(vec![("id", parse_quote!(u64))]),
            return_type: parse_quote!(U256),
            is_view: false,
            call_type_path: quote!(#interface_ident::cancelRewardCall),
        },
        InterfaceFunction {
            name: "get_stream",
            params: params(vec![("id", parse_quote!(u64))]),
            return_type: parse_quote!(ITIP20Rewards::RewardStream),
            is_view: true,
            call_type_path: quote!(#interface_ident::getStreamCall),
        },
        InterfaceFunction {
            name: "total_reward_per_second",
            params: params(vec![]),
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_ident::totalRewardPerSecondCall),
        },
    ]
}

pub(crate) fn get_events(interface_ident: &Ident) -> Vec<InterfaceEvent> {
    vec![
        // Reward events
        InterfaceEvent {
            name: "reward_scheduled",
            params: vec![
                ("funder", parse_quote!(Address), true),
                ("id", parse_quote!(u64), true),
                ("amount", parse_quote!(U256), false),
                ("durationSeconds", parse_quote!(u32), false),
            ],
            event_type_path: quote!(#interface_ident::RewardScheduled),
        },
        InterfaceEvent {
            name: "reward_canceled",
            params: vec![
                ("funder", parse_quote!(Address), true),
                ("id", parse_quote!(u64), true),
                ("refund", parse_quote!(U256), false),
            ],
            event_type_path: quote!(#interface_ident::RewardCanceled),
        },
        InterfaceEvent {
            name: "reward_recipient_set",
            params: vec![
                ("holder", parse_quote!(Address), true),
                ("recipient", parse_quote!(Address), true),
            ],
            event_type_path: quote!(#interface_ident::RewardRecipientSet),
        },
    ]
}

pub(crate) fn get_errors(interface_ident: &Ident) -> Vec<InterfaceError> {
    vec![
        // Reward stream errors
        InterfaceError {
            name: "not_stream_funder",
            params: vec![],
            error_type_path: quote!(#interface_ident::NotStreamFunder),
        },
        InterfaceError {
            name: "stream_inactive",
            params: vec![],
            error_type_path: quote!(#interface_ident::StreamInactive),
        },
        InterfaceError {
            name: "no_opted_in_supply",
            params: vec![],
            error_type_path: quote!(#interface_ident::NoOptedInSupply),
        },
    ]
}
