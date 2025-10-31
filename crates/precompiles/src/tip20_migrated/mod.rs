pub mod rewards;
pub mod roles;

pub use tempo_contracts::precompiles::{
    IRolesAuth, ITIP20, ITIP20Rewards, RolesAuthError, RolesAuthEvent, TIP20Error, TIP20Event,
    TIP20RewardsError, TIP20RewardsEvent,
};

use crate::{
    LINKING_USD_ADDRESS, TIP_FEE_MANAGER_ADDRESS,
    error::{Result, TempoPrecompileError},
    storage::PrecompileStorageProvider,
    tip20_factory::TIP20Factory,
    tip403_registry::{ITIP403Registry, TIP403Registry},
    tip4217_registry::{ITIP4217Registry, TIP4217Registry},
};
use alloy::{
    hex,
    primitives::{Address, B256, Bytes, IntoLogData, U256, keccak256},
};
use revm::state::Bytecode;
use std::sync::LazyLock;
use tempo_precompiles_macros::contract;
use tracing::trace;

/// TIP20 token address prefix (12 bytes for token ID encoding)
const TIP20_TOKEN_PREFIX: [u8; 12] = hex!("20C000000000000000000000");

/// TIP20 payment address prefix (14 bytes for payment classification)
/// Same as TIP20_TOKEN_PREFIX but extended to 14 bytes for payment classification
pub const TIP20_PAYMENT_PREFIX: [u8; 14] = hex!("20C0000000000000000000000000");

pub fn is_tip20(token: Address) -> bool {
    token.as_slice().starts_with(&TIP20_TOKEN_PREFIX)
}

/// Converts a token ID to its corresponding contract address
/// Uses the pattern: TIP20_TOKEN_PREFIX ++ token_id
pub fn token_id_to_address(token_id: u64) -> Address {
    let mut address_bytes = [0u8; 20];
    address_bytes[..12].copy_from_slice(&TIP20_TOKEN_PREFIX);
    address_bytes[12..20].copy_from_slice(&token_id.to_be_bytes());
    Address::from(address_bytes)
}

pub fn address_to_token_id_unchecked(address: Address) -> u64 {
    u64::from_be_bytes(address.as_slice()[12..20].try_into().unwrap())
}

#[contract(ITIP20, ITIP20Rewards, IRolesAuth)]
pub struct TIP20Token {
    // RolesAuth
    #[map = "has_role"]
    roles: Mapping<Address, Mapping<B256, bool>>, // slot 0
    #[map = "get_role_admin"]
    role_admins: Mapping<B256, B256>, // slot 1

    // TIP20 Metadata
    name: String,              // slot 2
    symbol: String,            // slot 3
    currency: String,          // slot 4
    domain_separator: B256,    // slot 5
    quote_token: Address,      // slot 6
    next_quote_token: Address, // slot 7
    transfer_policy_id: u64,   // slot 8

    // TIP20 Token
    total_supply: U256, // slot 9
    #[map = "balance_of"]
    balances: Mapping<Address, U256>, // slot 10
    #[map = "allowance"]
    allowances: Mapping<Address, Mapping<Address, U256>>, // slot 11
    nonces: Mapping<Address, U256>, // slot 12
    paused: bool,       // slot 13
    supply_cap: U256,   // slot 14
    salts: Mapping<B256, bool>, // slot 15

    // TIP20 Rewards
    last_update_time: u64, // slot 16
    opted_in_supply: U256, // slot 17
    next_stream_id: u64,   // slot 18
    #[map = "get_stream"]
    streams: Mapping<u64, rewards::RewardStream>, // slot 19
    scheduled_rate_decrease: Mapping<u128, U256>, // slot 20
    reward_recipient_of: Mapping<Address, Address>, // slot 21
    user_reward_per_token_paid: Mapping<Address, U256>, // slot 22
    delegated_balance: Mapping<Address, U256>, // slot 23
    reward_per_token_stored: U256, // slot 24
    total_reward_per_second: U256, // slot 25
}

pub static PAUSE_ROLE: LazyLock<B256> = LazyLock::new(|| keccak256(b"PAUSE_ROLE"));
pub static UNPAUSE_ROLE: LazyLock<B256> = LazyLock::new(|| keccak256(b"UNPAUSE_ROLE"));
pub static ISSUER_ROLE: LazyLock<B256> = LazyLock::new(|| keccak256(b"ISSUER_ROLE"));
pub static BURN_BLOCKED_ROLE: LazyLock<B256> = LazyLock::new(|| keccak256(b"BURN_BLOCKED_ROLE"));

// Re-export role constants from roles module for convenience
pub use roles::{DEFAULT_ADMIN_ROLE, UNGRANTABLE_ROLE};

impl<'a, S: PrecompileStorageProvider> TIP20Token_ITIP20 for TIP20Token<'a, S> {
    // Metadata functions
    fn decimals(&mut self) -> Result<u8> {
        let currency = self.currency()?;
        Ok(TIP4217Registry::default()
            .get_currency_decimals(ITIP4217Registry::getCurrencyDecimalsCall { currency }))
    }

    // Admin functions
    fn change_transfer_policy_id(
        &mut self,
        msg_sender: Address,
        new_policy_id: u64,
    ) -> crate::error::Result<()> {
        self.check_role(msg_sender, DEFAULT_ADMIN_ROLE)?;
        self._set_transfer_policy_id(new_policy_id)?;
        self._emit_transfer_policy_update(msg_sender, new_policy_id)
    }

    fn set_supply_cap(
        &mut self,
        msg_sender: Address,
        new_supply_cap: U256,
    ) -> crate::error::Result<()> {
        self.check_role(msg_sender, DEFAULT_ADMIN_ROLE)?;
        if new_supply_cap < self.total_supply()? {
            return Err(TIP20Error::supply_cap_exceeded().into());
        }

        self._set_supply_cap(new_supply_cap)?;
        self._emit_supply_cap_update(msg_sender, new_supply_cap)
    }

    fn pause(&mut self, msg_sender: Address) -> crate::error::Result<()> {
        self.check_role(msg_sender, *PAUSE_ROLE)?;
        self._set_paused(true);
        self._emit_pause_state_update(msg_sender, true)
    }
    fn unpause(&mut self, msg_sender: Address) -> crate::error::Result<()> {
        self.check_role(msg_sender, *PAUSE_ROLE)?;
        self._set_paused(false);
        self._emit_pause_state_update(msg_sender, false)
    }
    fn update_quote_token(
        &mut self,
        msg_sender: Address,
        new_quote_token: Address,
    ) -> crate::error::Result<()> {
        self.check_role(msg_sender, DEFAULT_ADMIN_ROLE)?;

        // Verify the new quote token is a valid TIP20 token that has been deployed
        if !is_tip20(new_quote_token) {
            return Err(TIP20Error::invalid_quote_token().into());
        }

        let new_token_id = address_to_token_id_unchecked(new_quote_token);
        let factory_token_id_counter = TIP20Factory::new(self.storage)
            .token_id_counter()?
            .to::<u64>();

        // Ensure the quote token has been deployed (token_id < counter)
        if new_token_id >= factory_token_id_counter {
            return Err(TIP20Error::invalid_quote_token().into());
        }

        self._set_quote_token(new_quote_token)?;
        self._emit_update_quote_token(msg_sender, new_quote_token)
    }

    fn finalize_quote_token_update(&mut self, msg_sender: Address) -> crate::error::Result<()> {
        self.check_role(msg_sender, DEFAULT_ADMIN_ROLE)?;

        let next_quote_token = self.next_quote_token()?;

        // Check that this does not create a loop
        // Loop through quote tokens until we reach the root (LinkingUSD)
        let mut current = next_quote_token;
        while current != LINKING_USD_ADDRESS {
            if current == self.address {
                return Err(TIP20Error::invalid_quote_token().into());
            }

            current = TIP20Token::from_address(current, self.storage).quote_token()?;
        }

        // Update the quote token
        self._set_next_quote_token(next_quote_token)?;
        self._emit_quote_token_update_finalized(msg_sender, next_quote_token)
    }

    // Token operations
    /// Mints new tokens to specified address
    fn mint(&mut self, msg_sender: Address, to: Address, amount: U256) -> crate::error::Result<()> {
        self._mint(msg_sender, to, amount)
    }

    /// Mints new tokens to specified address with memo attached
    fn mint_with_memo(
        &mut self,
        msg_sender: Address,
        to: Address,
        amount: U256,
        memo: B256,
    ) -> crate::error::Result<()> {
        self._mint(msg_sender, to, amount)?;
        self._emit_transfer_with_memo(msg_sender, to, amount, memo)
    }

    /// Burns tokens from sender's balance and reduces total supply
    fn burn(&mut self, msg_sender: Address, amount: U256) -> crate::error::Result<()> {
        self._burn(msg_sender, amount)
    }

    /// Burns tokens from sender's balance with memo attached
    fn burn_with_memo(
        &mut self,
        msg_sender: Address,
        amount: U256,
        memo: B256,
    ) -> crate::error::Result<()> {
        self._burn(msg_sender, amount)?;
        self._emit_transfer_with_memo(msg_sender, Address::ZERO, amount, memo)
    }

    /// Burns tokens from blocked addresses that cannot transfer
    fn burn_blocked(
        &mut self,
        msg_sender: Address,
        from: Address,
        amount: U256,
    ) -> crate::error::Result<()> {
        self.check_role(msg_sender, *BURN_BLOCKED_ROLE)?;

        // Check if the address is blocked from transferring
        let transfer_policy_id = self.transfer_policy_id()?;
        let mut registry = TIP403Registry::new(self.storage);
        // TODO(rusowsky): use flattened version once migrated
        if registry.is_authorized(ITIP403Registry::isAuthorizedCall {
            policyId: transfer_policy_id,
            user: from,
        })? {
            // Only allow burning from addresses that are blocked from transferring
            return Err(TIP20Error::policy_forbids().into());
        }

        self._transfer(from, Address::ZERO, amount)?;

        let total_supply = self.total_supply()?;
        let new_supply = total_supply
            .checked_sub(amount)
            .ok_or(TIP20Error::insufficient_balance())?;
        self._set_total_supply(new_supply)?;

        self._emit_burn_blocked(from, amount)
    }

    // Standard token functions
    fn approve(
        &mut self,
        msg_sender: Address,
        spender: Address,
        amount: U256,
    ) -> crate::error::Result<bool> {
        self._set_allowances(msg_sender, spender, amount)?;
        self._emit_approval(msg_sender, spender, amount)?;
        Ok(true)
    }

    fn transfer(
        &mut self,
        msg_sender: Address,
        to: Address,
        amount: U256,
    ) -> crate::error::Result<bool> {
        trace!(%msg_sender, ?to, ?amount, "transferring TIP20");
        self.check_not_paused()?;
        self.check_not_token_address(to)?;
        self.ensure_transfer_authorized(msg_sender, to)?;
        self._transfer(msg_sender, to, amount)?;
        Ok(true)
    }

    fn transfer_from(
        &mut self,
        msg_sender: Address,
        from: Address,
        to: Address,
        amount: U256,
    ) -> crate::error::Result<bool> {
        self._transfer_from(msg_sender, from, to, amount)
    }

    // TIP20 extension functions
    fn transfer_with_memo(
        &mut self,
        msg_sender: Address,
        to: Address,
        amount: U256,
        memo: B256,
    ) -> crate::error::Result<()> {
        self.check_not_paused()?;
        self.check_not_token_address(to)?;
        self.ensure_transfer_authorized(msg_sender, to)?;

        self._transfer(msg_sender, to, amount)?;
        self._emit_transfer_with_memo(msg_sender, to, amount, memo)
    }

    /// Transfer from `from` to `to` address with memo attached
    fn transfer_from_with_memo(
        &mut self,
        msg_sender: Address,
        from: Address,
        to: Address,
        amount: U256,
        memo: B256,
    ) -> crate::error::Result<bool> {
        self._transfer_from(msg_sender, from, to, amount)?;
        self._emit_transfer_with_memo(msg_sender, to, amount, memo)?;
        Ok(true)
    }
}

impl<'a, S: PrecompileStorageProvider> TIP20Token<'a, S> {
    /// Internal helper to mint new tokens and update balances
    fn _mint(&mut self, msg_sender: Address, to: Address, amount: U256) -> Result<()> {
        self.check_role(msg_sender, *ISSUER_ROLE)?;
        let total_supply = self.total_supply()?;

        let new_supply = total_supply
            .checked_add(amount)
            .ok_or(TempoPrecompileError::under_overflow())?;

        let supply_cap = self.supply_cap()?;
        if new_supply > supply_cap {
            return Err(TIP20Error::supply_cap_exceeded().into());
        }

        let timestamp = self.storage.timestamp();
        self.accrue(timestamp)?;

        self.handle_rewards_on_mint(to, amount)?;

        self._set_total_supply(new_supply)?;
        let to_balance = self._get_balances(to)?;
        let new_to_balance: alloy::primitives::Uint<256, 4> = to_balance
            .checked_add(amount)
            .ok_or(TempoPrecompileError::under_overflow())?;
        self._set_balances(to, new_to_balance)?;

        self._emit_transfer(Address::ZERO, to, amount)?;
        self._emit_mint(to, amount)
    }

    /// Internal helper to burn tokens and update balances
    fn _burn(&mut self, msg_sender: Address, amount: U256) -> Result<()> {
        self.check_role(msg_sender, *ISSUER_ROLE)?;

        self._transfer(msg_sender, Address::ZERO, amount)?;

        let total_supply = self.total_supply()?;
        let new_supply = total_supply
            .checked_sub(amount)
            .ok_or(TIP20Error::insufficient_balance())?;
        self._set_total_supply(new_supply)?;

        self._emit_burn(msg_sender, amount)
    }

    /// Transfer from `from` to `to` address without approval requirement
    /// This function is not exposed via the public interface and should only be invoked by precompiles
    pub fn system_transfer_from(
        &mut self,
        from: Address,
        to: Address,
        amount: U256,
    ) -> Result<bool> {
        self.check_not_paused()?;
        self.check_not_token_address(to)?;
        self.ensure_transfer_authorized(from, to)?;

        self._transfer(from, to, amount)?;

        Ok(true)
    }

    fn _transfer_from(
        &mut self,
        msg_sender: Address,
        from: Address,
        to: Address,
        amount: U256,
    ) -> Result<bool> {
        self.check_not_paused()?;
        self.check_not_token_address(to)?;
        self.ensure_transfer_authorized(from, to)?;

        let allowed = self._get_allowances(from, msg_sender)?;
        if amount > allowed {
            return Err(TIP20Error::insufficient_allowance().into());
        }

        if allowed != U256::MAX {
            let new_allowance = allowed
                .checked_sub(amount)
                .ok_or(TIP20Error::insufficient_allowance())?;
            self._set_allowances(from, msg_sender, new_allowance)?;
        }

        self._transfer(from, to, amount)?;

        Ok(true)
    }
}

// Utility functions
impl<'a, S: PrecompileStorageProvider> TIP20Token<'a, S> {
    pub fn new(token_id: u64, storage: &'a mut S) -> Self {
        let token_address = token_id_to_address(token_id);
        Self::_new(token_address, storage)
    }

    /// Create a TIP20Token from an address
    pub fn from_address(address: Address, storage: &'a mut S) -> Self {
        let token_id = address_to_token_id_unchecked(address);
        Self::new(token_id, storage)
    }

    /// Only called internally from the factory, which won't try to re-initialize a token.
    pub fn initialize(
        &mut self,
        name: &str,
        symbol: &str,
        currency: &str,
        quote_token: Address,
        admin: Address,
    ) -> Result<()> {
        trace!(%name, address=%self.address, "Initializing token");

        // must ensure the account is not empty, by setting some code
        self.storage.set_code(
            self.address,
            Bytecode::new_legacy(Bytes::from_static(&[0xef])),
        )?;

        self._set_name(name.to_string())?;
        self._set_symbol(symbol.to_string())?;
        self._set_currency(currency.to_string())?;
        self._set_quote_token(quote_token)?;
        // Initialize nextQuoteToken to the same value as quoteToken
        self._set_next_quote_token(quote_token)?;

        // Validate currency via TIP4217 registry
        if self.decimals()? == 0 {
            return Err(TIP20Error::invalid_currency().into());
        }

        // Set default values
        self._set_supply_cap(U256::MAX)?;
        self._set_transfer_policy_id(1)?;

        // Initialize roles system and grant admin role
        self.roles_initialize()?;
        self.roles_grant_default_admin(admin)
    }

    fn check_not_paused(&mut self) -> Result<()> {
        if self.paused()? {
            return Err(TIP20Error::contract_paused().into());
        }
        Ok(())
    }

    fn check_not_token_address(&self, to: Address) -> Result<()> {
        // Don't allow sending to other precompiled tokens
        if is_tip20(to) {
            return Err(TIP20Error::invalid_recipient().into());
        }
        Ok(())
    }

    /// Checks if the transfer is authorized.
    pub fn is_transfer_authorized(&mut self, from: Address, to: Address) -> Result<bool> {
        let transfer_policy_id = self.transfer_policy_id()?;
        let mut registry = TIP403Registry::new(self.storage);

        // Check if 'from' address is authorized
        let from_authorized = registry.is_authorized(ITIP403Registry::isAuthorizedCall {
            policyId: transfer_policy_id,
            user: from,
        })?;

        // Check if 'to' address is authorized
        let to_authorized = registry.is_authorized(ITIP403Registry::isAuthorizedCall {
            policyId: transfer_policy_id,
            user: to,
        })?;

        Ok(from_authorized && to_authorized)
    }

    /// Ensures the transfer is authorized.
    pub fn ensure_transfer_authorized(&mut self, from: Address, to: Address) -> Result<()> {
        if !self.is_transfer_authorized(from, to)? {
            return Err(TIP20Error::policy_forbids().into());
        }

        Ok(())
    }

    fn _transfer(&mut self, from: Address, to: Address, amount: U256) -> Result<()> {
        // Accrue before balance changes
        let timestamp = self.storage.timestamp();
        self.accrue(timestamp)?;
        self.handle_rewards_on_transfer(from, to, amount)?;

        let from_balance = self._get_balances(from)?;
        if amount > from_balance {
            return Err(TIP20Error::insufficient_balance().into());
        }

        // Adjust balances
        let from_balance = self._get_balances(from)?;
        let new_from_balance = from_balance
            .checked_sub(amount)
            .ok_or(TempoPrecompileError::under_overflow())?;

        self._set_balances(from, new_from_balance)?;

        if to != Address::ZERO {
            let to_balance = self._get_balances(to)?;
            let new_to_balance = to_balance
                .checked_add(amount)
                .ok_or(TempoPrecompileError::under_overflow())?;

            self._set_balances(to, new_to_balance)?;
        }

        self._emit_transfer(from, to, amount)
    }

    /// Transfers fee tokens from user to fee manager before transaction execution
    pub fn transfer_fee_pre_tx(&mut self, from: Address, amount: U256) -> Result<()> {
        let from_balance = self._get_balances(from)?;
        if amount > from_balance {
            return Err(TIP20Error::insufficient_balance().into());
        }

        let new_from_balance = from_balance
            .checked_sub(amount)
            .ok_or(TIP20Error::insufficient_balance())?;

        self._set_balances(from, new_from_balance)?;

        let to_balance = self._get_balances(TIP_FEE_MANAGER_ADDRESS)?;
        let new_to_balance = to_balance
            .checked_add(amount)
            .ok_or(TIP20Error::supply_cap_exceeded())?;
        self._set_balances(TIP_FEE_MANAGER_ADDRESS, new_to_balance)?;

        Ok(())
    }

    /// Refunds unused fee tokens to user and emits transfer event for gas amount used
    pub fn transfer_fee_post_tx(
        &mut self,
        to: Address,
        refund: U256,
        actual_used: U256,
    ) -> Result<()> {
        let from_balance = self._get_balances(TIP_FEE_MANAGER_ADDRESS)?;
        if refund > from_balance {
            return Err(TIP20Error::insufficient_balance().into());
        }

        let new_from_balance = from_balance
            .checked_sub(refund)
            .ok_or(TIP20Error::insufficient_balance())?;

        self._set_balances(TIP_FEE_MANAGER_ADDRESS, new_from_balance)?;

        let to_balance = self._get_balances(to)?;
        let new_to_balance = to_balance
            .checked_add(refund)
            .ok_or(TIP20Error::supply_cap_exceeded())?;
        self._set_balances(to, new_to_balance)?;

        self._emit_transfer(to, TIP_FEE_MANAGER_ADDRESS, actual_used)
    }
}
