use crate::{
    Type,
    interface::{InterfaceError, InterfaceEvent, InterfaceFunction},
};
use quote::quote;
use syn::parse_quote;

pub(crate) fn get_functions(interface_type: &Type) -> Vec<InterfaceFunction> {
    vec![
        // Metadata functions (view, no parameters)
        InterfaceFunction {
            name: "name",
            params: vec![],
            return_type: parse_quote!(String),
            is_view: true,
            call_type_path: quote!(#interface_type::nameCall),
        },
        InterfaceFunction {
            name: "symbol",
            params: vec![],
            return_type: parse_quote!(String),
            is_view: true,
            call_type_path: quote!(#interface_type::symbolCall),
        },
        InterfaceFunction {
            name: "decimals",
            params: vec![],
            return_type: parse_quote!(u8),
            is_view: true,
            call_type_path: quote!(#interface_type::decimalsCall),
        },
        InterfaceFunction {
            name: "currency",
            params: vec![],
            return_type: parse_quote!(String),
            is_view: true,
            call_type_path: quote!(#interface_type::currencyCall),
        },
        InterfaceFunction {
            name: "total_supply",
            params: vec![],
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_type::totalSupplyCall),
        },
        InterfaceFunction {
            name: "supply_cap",
            params: vec![],
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_type::supplyCapCall),
        },
        InterfaceFunction {
            name: "transfer_policy_id",
            params: vec![],
            return_type: parse_quote!(u64),
            is_view: true,
            call_type_path: quote!(#interface_type::transferPolicyIdCall),
        },
        InterfaceFunction {
            name: "paused",
            params: vec![],
            return_type: parse_quote!(bool),
            is_view: true,
            call_type_path: quote!(#interface_type::pausedCall),
        },
        InterfaceFunction {
            name: "quote_token",
            params: vec![],
            return_type: parse_quote!(Address),
            is_view: true,
            call_type_path: quote!(#interface_type::quoteTokenCall),
        },
        InterfaceFunction {
            name: "next_quote_token",
            params: vec![],
            return_type: parse_quote!(Address),
            is_view: true,
            call_type_path: quote!(#interface_type::nextQuoteTokenCall),
        },
        // View functions with parameters
        InterfaceFunction {
            name: "balance_of",
            params: vec![("account", parse_quote!(Address))],
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_type::balanceOfCall),
        },
        InterfaceFunction {
            name: "allowance",
            params: vec![
                ("owner", parse_quote!(Address)),
                ("spender", parse_quote!(Address)),
            ],
            return_type: parse_quote!(U256),
            is_view: true,
            call_type_path: quote!(#interface_type::allowanceCall),
        },
        // Mutating functions (non-void)
        InterfaceFunction {
            name: "transfer",
            params: vec![
                ("to", parse_quote!(Address)),
                ("amount", parse_quote!(U256)),
            ],
            return_type: parse_quote!(bool),
            is_view: false,
            call_type_path: quote!(#interface_type::transferCall),
        },
        InterfaceFunction {
            name: "transfer_from",
            params: vec![
                ("from", parse_quote!(Address)),
                ("to", parse_quote!(Address)),
                ("amount", parse_quote!(U256)),
            ],
            return_type: parse_quote!(bool),
            is_view: false,
            call_type_path: quote!(#interface_type::transferFromCall),
        },
        InterfaceFunction {
            name: "approve",
            params: vec![
                ("spender", parse_quote!(Address)),
                ("amount", parse_quote!(U256)),
            ],
            return_type: parse_quote!(bool),
            is_view: false,
            call_type_path: quote!(#interface_type::approveCall),
        },
        InterfaceFunction {
            name: "transfer_from_with_memo",
            params: vec![
                ("from", parse_quote!(Address)),
                ("to", parse_quote!(Address)),
                ("amount", parse_quote!(U256)),
                ("memo", parse_quote!(B256)),
            ],
            return_type: parse_quote!(bool),
            is_view: false,
            call_type_path: quote!(#interface_type::transferFromWithMemoCall),
        },
        // Mutating functions (void)
        InterfaceFunction {
            name: "mint",
            params: vec![
                ("to", parse_quote!(Address)),
                ("amount", parse_quote!(U256)),
            ],
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_type::mintCall),
        },
        InterfaceFunction {
            name: "burn",
            params: vec![("amount", parse_quote!(U256))],
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_type::burnCall),
        },
        InterfaceFunction {
            name: "mint_with_memo",
            params: vec![
                ("to", parse_quote!(Address)),
                ("amount", parse_quote!(U256)),
                ("memo", parse_quote!(B256)),
            ],
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_type::mintWithMemoCall),
        },
        InterfaceFunction {
            name: "burn_with_memo",
            params: vec![("amount", parse_quote!(U256)), ("memo", parse_quote!(B256))],
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_type::burnWithMemoCall),
        },
        InterfaceFunction {
            name: "burn_blocked",
            params: vec![
                ("from", parse_quote!(Address)),
                ("amount", parse_quote!(U256)),
            ],
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_type::burnBlockedCall),
        },
        InterfaceFunction {
            name: "transfer_with_memo",
            params: vec![
                ("to", parse_quote!(Address)),
                ("amount", parse_quote!(U256)),
                ("memo", parse_quote!(B256)),
            ],
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_type::transferWithMemoCall),
        },
        // Admin functions (void)
        InterfaceFunction {
            name: "change_transfer_policy_id",
            params: vec![("new_policy_id", parse_quote!(u64))],
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_type::changeTransferPolicyIdCall),
        },
        InterfaceFunction {
            name: "set_supply_cap",
            params: vec![("new_supply_cap", parse_quote!(U256))],
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_type::setSupplyCapCall),
        },
        InterfaceFunction {
            name: "pause",
            params: vec![],
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_type::pauseCall),
        },
        InterfaceFunction {
            name: "unpause",
            params: vec![],
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_type::unpauseCall),
        },
        InterfaceFunction {
            name: "update_quote_token",
            params: vec![("new_quote_token", parse_quote!(Address))],
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_type::updateQuoteTokenCall),
        },
        InterfaceFunction {
            name: "finalize_quote_token_update",
            params: vec![],
            return_type: parse_quote!(()),
            is_view: false,
            call_type_path: quote!(#interface_type::finalizeQuoteTokenUpdateCall),
        },
    ]
}

pub(crate) fn get_events(interface_type: &Type) -> Vec<InterfaceEvent> {
    vec![
        // Core token events
        InterfaceEvent {
            name: "transfer",
            params: vec![
                ("from", parse_quote!(Address), true),
                ("to", parse_quote!(Address), true),
                ("amount", parse_quote!(U256), false),
            ],
            event_type_path: quote!(#interface_type::Transfer),
        },
        InterfaceEvent {
            name: "approval",
            params: vec![
                ("owner", parse_quote!(Address), true),
                ("spender", parse_quote!(Address), true),
                ("amount", parse_quote!(U256), false),
            ],
            event_type_path: quote!(#interface_type::Approval),
        },
        InterfaceEvent {
            name: "mint",
            params: vec![
                ("to", parse_quote!(Address), true),
                ("amount", parse_quote!(U256), false),
            ],
            event_type_path: quote!(#interface_type::Mint),
        },
        InterfaceEvent {
            name: "burn",
            params: vec![
                ("from", parse_quote!(Address), true),
                ("amount", parse_quote!(U256), false),
            ],
            event_type_path: quote!(#interface_type::Burn),
        },
        InterfaceEvent {
            name: "burn_blocked",
            params: vec![
                ("from", parse_quote!(Address), true),
                ("amount", parse_quote!(U256), false),
            ],
            event_type_path: quote!(#interface_type::BurnBlocked),
        },
        InterfaceEvent {
            name: "transfer_with_memo",
            params: vec![
                ("from", parse_quote!(Address), true),
                ("to", parse_quote!(Address), true),
                ("amount", parse_quote!(U256), false),
                ("memo", parse_quote!(B256), false),
            ],
            event_type_path: quote!(#interface_type::TransferWithMemo),
        },
        // Admin events
        InterfaceEvent {
            name: "transfer_policy_update",
            params: vec![
                ("updater", parse_quote!(Address), true),
                ("new_policy_id", parse_quote!(u64), true),
            ],
            event_type_path: quote!(#interface_type::TransferPolicyUpdate),
        },
        InterfaceEvent {
            name: "supply_cap_update",
            params: vec![
                ("updater", parse_quote!(Address), true),
                ("new_supply_cap", parse_quote!(U256), true),
            ],
            event_type_path: quote!(#interface_type::SupplyCapUpdate),
        },
        InterfaceEvent {
            name: "pause_state_update",
            params: vec![
                ("updater", parse_quote!(Address), true),
                ("is_paused", parse_quote!(bool), false),
            ],
            event_type_path: quote!(#interface_type::PauseStateUpdate),
        },
        InterfaceEvent {
            name: "update_quote_token",
            params: vec![
                ("updater", parse_quote!(Address), true),
                ("new_quote_token", parse_quote!(Address), true),
            ],
            event_type_path: quote!(#interface_type::UpdateQuoteToken),
        },
        InterfaceEvent {
            name: "quote_token_update_finalized",
            params: vec![
                ("updater", parse_quote!(Address), true),
                ("new_quote_token", parse_quote!(Address), true),
            ],
            event_type_path: quote!(#interface_type::QuoteTokenUpdateFinalized),
        },
    ]
}

pub(crate) fn get_errors(interface_type: &Type) -> Vec<InterfaceError> {
    vec![
        // Balance and allowance errors
        InterfaceError {
            name: "insufficient_balance",
            params: vec![],
            error_type_path: quote!(#interface_type::InsufficientBalance),
        },
        InterfaceError {
            name: "insufficient_allowance",
            params: vec![],
            error_type_path: quote!(#interface_type::InsufficientAllowance),
        },
        // Supply errors
        InterfaceError {
            name: "supply_cap_exceeded",
            params: vec![],
            error_type_path: quote!(#interface_type::SupplyCapExceeded),
        },
        // Payload errors
        InterfaceError {
            name: "invalid_payload",
            params: vec![],
            error_type_path: quote!(#interface_type::InvalidPayload),
        },
        InterfaceError {
            name: "string_too_long",
            params: vec![],
            error_type_path: quote!(#interface_type::StringTooLong),
        },
        // Transfer policy errors
        InterfaceError {
            name: "policy_forbids",
            params: vec![],
            error_type_path: quote!(#interface_type::PolicyForbids),
        },
        // Address errors
        InterfaceError {
            name: "invalid_recipient",
            params: vec![],
            error_type_path: quote!(#interface_type::InvalidRecipient),
        },
        // State errors
        InterfaceError {
            name: "contract_paused",
            params: vec![],
            error_type_path: quote!(#interface_type::ContractPaused),
        },
        // Currency errors
        InterfaceError {
            name: "invalid_currency",
            params: vec![],
            error_type_path: quote!(#interface_type::InvalidCurrency),
        },
        // Quote token errors
        InterfaceError {
            name: "invalid_quote_token",
            params: vec![],
            error_type_path: quote!(#interface_type::InvalidQuoteToken),
        },
        // Transfer errors
        InterfaceError {
            name: "transfers_disabled",
            params: vec![],
            error_type_path: quote!(#interface_type::TransfersDisabled),
        },
        // Amount errors
        InterfaceError {
            name: "invalid_amount",
            params: vec![],
            error_type_path: quote!(#interface_type::InvalidAmount),
        },
        // Access control errors
        InterfaceError {
            name: "unauthorized",
            params: vec![],
            error_type_path: quote!(#interface_type::Unauthorized),
        },
    ]
}
