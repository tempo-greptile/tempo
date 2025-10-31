use crate::{
    Type,
    interface::{InterfaceError, InterfaceEvent, InterfaceFunction},
};
use quote::quote;
use syn::parse_quote;

pub(crate) fn get_functions(interface_type: &Type) -> Vec<InterfaceFunction> {
    vec![
        // Reward functions
        InterfaceFunction {
            name: "start_reward",
            params: vec![
                ("amount", parse_quote!(U256)),
                ("seconds", parse_quote!(u128)),
            ],
            return_type: parse_quote!(u64),
            is_view: false,
            call_type_path: quote!(#interface_type::startRewardCall),
        },
        InterfaceFunction {
            name: "set_reward_recipient",
            params: vec![("recipient", parse_quote!(Address))],
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_type::setRewardRecipientCall),
        },
        InterfaceFunction {
            name: "cancel_reward",
            params: vec![("id", parse_quote!(u64))],
            return_type: parse_quote!(U256),
            is_view: false,
            call_type_path: quote!(#interface_type::cancelRewardCall),
        },
        InterfaceFunction {
            name: "finalize_streams",
            params: vec![],
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_type::finalizeStreamsCall),
        },
        InterfaceFunction {
            name: "get_stream",
            params: vec![("id", parse_quote!(u64))],
            return_type: parse_quote!(RewardStream),
            is_view: true,
            call_type_path: quote!(#interface_type::getStreamCall),
        },
        InterfaceFunction {
            name: "total_reward_per_second",
            params: vec![],
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_type::totalRewardPerSecondCall),
        },
    ]
}

pub(crate) fn get_events(interface_type: &Type) -> Vec<InterfaceEvent> {
    vec![
        // Reward events
        InterfaceEvent {
            name: "reward_scheduled",
            params: vec![
                ("funder", parse_quote!(Address), true),
                ("id", parse_quote!(u64), true),
                ("amount", parse_quote!(U256), false),
                ("duration_seconds", parse_quote!(u32), false),
            ],
            event_type_path: quote!(#interface_type::RewardScheduled),
        },
        InterfaceEvent {
            name: "reward_canceled",
            params: vec![
                ("funder", parse_quote!(Address), true),
                ("id", parse_quote!(u64), true),
                ("refund", parse_quote!(U256), false),
            ],
            event_type_path: quote!(#interface_type::RewardCanceled),
        },
        InterfaceEvent {
            name: "reward_recipient_set",
            params: vec![
                ("holder", parse_quote!(Address), true),
                ("recipient", parse_quote!(Address), true),
            ],
            event_type_path: quote!(#interface_type::RewardRecipientSet),
        },
    ]
}

pub(crate) fn get_errors(interface_type: &Type) -> Vec<InterfaceError> {
    vec![
        // Reward stream errors
        InterfaceError {
            name: "not_stream_funder",
            params: vec![],
            error_type_path: quote!(#interface_type::NotStreamFunder),
        },
        InterfaceError {
            name: "stream_inactive",
            params: vec![],
            error_type_path: quote!(#interface_type::StreamInactive),
        },
        InterfaceError {
            name: "no_opted_in_supply",
            params: vec![],
            error_type_path: quote!(#interface_type::NoOptedInSupply),
        },
    ]
}
