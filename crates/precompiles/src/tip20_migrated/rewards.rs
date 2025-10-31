use crate::{
    TIP20_REWARDS_REGISTRY_ADDRESS,
    error::{Result, TempoPrecompileError},
    storage::{PrecompileStorageProvider, Storable, slots::mapping_slot},
    tip20_migrated::{TIP20Token, TIP20Token_ITIP20Rewards},
    tip20_rewards_registry::TIP20RewardsRegistry,
};
use alloy::primitives::{Address, IntoLogData, U256, uint};
use revm::interpreter::instructions::utility::{IntoAddress, IntoU256};
use tempo_contracts::precompiles::{
    ITIP20, ITIP20Rewards, TIP20Error, TIP20Event, TIP20RewardsError, TIP20RewardsEvent,
};
use tempo_precompiles_macros::Storable;

pub const ACC_PRECISION: U256 = uint!(1000000000000000000_U256);

/// Reward stream data structure occupying 5 consecutive storage slots.
/// Fields are stored in declaration order, one field per slot.
#[derive(Debug, Clone, PartialEq, Eq, Storable)]
pub struct RewardStream {
    pub funder: Address,
    pub start_time: u64,
    pub end_time: u64,
    pub rate_per_second_scaled: U256,
    pub amount_total: U256,
}

impl RewardStream {
    /// Creates a new RewardStream instance.
    pub fn new(
        funder: Address,
        start_time: u64,
        end_time: u64,
        rate_per_second_scaled: U256,
        amount_total: U256,
    ) -> Self {
        Self {
            funder,
            start_time,
            end_time,
            rate_per_second_scaled,
            amount_total,
        }
    }
}

impl From<RewardStream> for ITIP20Rewards::RewardStream {
    fn from(value: RewardStream) -> Self {
        Self {
            funder: value.funder,
            startTime: value.start_time,
            endTime: value.end_time,
            ratePerSecondScaled: value.rate_per_second_scaled,
            amountTotal: value.amount_total,
        }
    }
}

impl<'a, S: PrecompileStorageProvider> TIP20Token_ITIP20Rewards for TIP20Token<'a, S> {
    /// Starts a new reward stream for the token contract.
    ///
    /// This function allows an authorized user to fund a reward stream that distributes
    /// tokens to opted-in recipients either immediately if seconds=0, or over the specified
    /// duration.
    fn start_reward(&mut self, msg_sender: Address, amount: U256, seconds: u128) -> Result<u64> {
        self.check_not_paused()?;
        let token_address = self.address;
        self.ensure_transfer_authorized(msg_sender, token_address)?;

        if amount == U256::ZERO {
            return Err(TIP20Error::invalid_amount().into());
        }

        self._transfer(msg_sender, token_address, amount)?;

        if seconds == 0 {
            let opted_in_supply = self.get_opted_in_supply()?;
            if opted_in_supply.is_zero() {
                return Err(TIP20RewardsError::no_opted_in_supply().into());
            }

            let delta_rpt = amount
                .checked_mul(ACC_PRECISION)
                .and_then(|v| v.checked_div(opted_in_supply))
                .ok_or(TempoPrecompileError::under_overflow())?;
            let current_rpt = self.get_reward_per_token_stored()?;
            let new_rpt = current_rpt
                .checked_add(delta_rpt)
                .ok_or(TempoPrecompileError::under_overflow())?;
            self.set_reward_per_token_stored(new_rpt)?;

            // Emit reward scheduled event for immediate payout
            self._emit_reward_scheduled(msg_sender, 0, amount, 0)?;

            return Ok(0);
        }

        let rate = amount
            .checked_mul(ACC_PRECISION)
            .and_then(|v| v.checked_div(U256::from(seconds)))
            .ok_or(TempoPrecompileError::under_overflow())?;
        let stream_id = self.get_next_stream_id()?;
        let next_stream_id = stream_id
            .checked_add(1)
            .ok_or(TempoPrecompileError::under_overflow())?;
        self.set_next_stream_id(next_stream_id)?;

        let current_total = self.get_total_reward_per_second()?;
        let new_total = current_total
            .checked_add(rate)
            .ok_or(TempoPrecompileError::under_overflow())?;
        self.set_total_reward_per_second(new_total)?;

        let current_time = self.storage.timestamp().to::<u128>();
        let end_time = current_time
            .checked_add(seconds)
            .ok_or(TempoPrecompileError::under_overflow())?;

        self._set_streams(
            stream_id,
            RewardStream::new(
                msg_sender,
                current_time as u64,
                end_time as u64,
                rate,
                amount,
            ),
        )?;

        let current_decrease = self.get_scheduled_rate_decrease_at(end_time);
        let new_decrease = current_decrease
            .checked_add(rate)
            .ok_or(TempoPrecompileError::under_overflow())?;
        self.set_scheduled_rate_decrease_at(end_time, new_decrease)?;

        // Add stream to registry
        let mut registry = TIP20RewardsRegistry::new(self.storage);
        registry.add_stream(self.address, end_time)?;

        // Emit reward scheduled event for streaming reward
        self._emit_reward_scheduled(msg_sender, stream_id, amount, seconds as u32)?;

        Ok(stream_id)
    }
}

impl<'a, S: PrecompileStorageProvider> TIP20Token<'a, S> {
    /// Handles reward accounting when tokens are transferred from an address.
    ///
    /// This function updates the reward state for the sender's reward recipient,
    /// reducing their delegated balance and returns the resulting opted in supply delta if changed
    fn handle_sender_rewards(&mut self, from: Address, amount: U256) -> Result<Option<U256>> {
        let from_recipient = self.get_reward_recipient_of(from)?;
        if from_recipient != Address::ZERO {
            self.update_rewards(from_recipient)?;

            let delegated = self
                .get_delegated_balance(from_recipient)?
                .checked_sub(amount)
                .ok_or(TempoPrecompileError::under_overflow())?;
            self.set_delegated_balance(from_recipient, delegated)?;

            Ok(Some(amount))
        } else {
            Ok(None)
        }
    }

    /// Handles reward accounting when tokens are transferred to an address.
    ///
    /// This function updates the reward state for the receiver's reward recipient,
    /// increasing their delegated balance and returns the resulting opted in supply delta if changed
    fn handle_receiver_rewards(&mut self, to: Address, amount: U256) -> Result<Option<U256>> {
        let to_recipient = self.get_reward_recipient_of(to)?;
        if to_recipient != Address::ZERO {
            self.update_rewards(to_recipient)?;

            let delegated = self
                .get_delegated_balance(to_recipient)?
                .checked_add(amount)
                .ok_or(TempoPrecompileError::under_overflow())?;
            self.set_delegated_balance(to_recipient, delegated)?;

            Ok(Some(amount))
        } else {
            Ok(None)
        }
    }

    /// Accrues rewards based on elapsed time since last update.
    ///
    /// This function calculates and updates the reward per token stored based on
    /// the total reward rate and the time elapsed since the last update.
    /// Only processes rewards if there is an opted-in supply.
    pub fn accrue(&mut self, accrue_to_timestamp: U256) -> Result<()> {
        let elapsed = accrue_to_timestamp - U256::from(self.get_last_update_time()?);
        if elapsed.is_zero() {
            return Ok(());
        }

        self.set_last_update_time(accrue_to_timestamp)?;

        let opted_in_supply = self.get_opted_in_supply()?;
        if opted_in_supply == U256::ZERO {
            return Ok(());
        }

        let total_reward_per_second = self.get_total_reward_per_second()?;
        if total_reward_per_second > U256::ZERO {
            let delta_rpt = total_reward_per_second
                .checked_mul(elapsed)
                .and_then(|v| v.checked_div(opted_in_supply))
                .ok_or(TempoPrecompileError::under_overflow())?;
            let current_rpt = self.get_reward_per_token_stored()?;
            let new_rpt = current_rpt
                .checked_add(delta_rpt)
                .ok_or(TempoPrecompileError::under_overflow())?;
            self.set_reward_per_token_stored(new_rpt)?;
        }

        Ok(())
    }

    /// Updates and distributes accrued rewards for a specific recipient.
    ///
    /// This function calculates the rewards earned by a recipient based on their
    /// delegated balance and the reward per token difference since their last update.
    /// It then transfers the accrued rewards from the contract to the recipient.
    fn update_rewards(&mut self, recipient: Address) -> Result<()> {
        if recipient == Address::ZERO {
            return Ok(());
        }

        let delegated = self.get_delegated_balance(recipient)?;
        let reward_per_token_stored = self.get_reward_per_token_stored()?;
        let user_reward_per_token_paid = self.get_user_reward_per_token_paid(recipient)?;

        let mut accrued = reward_per_token_stored
            .checked_sub(user_reward_per_token_paid)
            .and_then(|diff| delegated.checked_mul(diff))
            .and_then(|v| v.checked_div(ACC_PRECISION))
            .ok_or(TempoPrecompileError::under_overflow())?;

        self.set_user_reward_per_token_paid(recipient, reward_per_token_stored)?;

        if accrued > U256::ZERO {
            let token_address = self.address;
            let contract_balance = self._get_balances(token_address)?;

            if accrued > contract_balance {
                accrued = contract_balance;
            }

            let new_contract_balance = contract_balance
                .checked_sub(accrued)
                .ok_or(TempoPrecompileError::under_overflow())?;
            self._set_balances(token_address, new_contract_balance)?;

            let recipient_balance = self
                ._get_balances(recipient)?
                .checked_add(accrued)
                .ok_or(TempoPrecompileError::under_overflow())?;
            self._set_balances(recipient, recipient_balance)?;

            // Since rewards are being claimed, we need to increase the delegated balance
            // and opted-in supply to reflect that these tokens are now part of the reward pool.
            let delegated_balance = self
                .get_delegated_balance(recipient)?
                .checked_add(accrued)
                .ok_or(TempoPrecompileError::under_overflow())?;
            self.set_delegated_balance(recipient, delegated_balance)?;

            let opted_in_supply = self
                .get_opted_in_supply()?
                .checked_add(accrued)
                .ok_or(TempoPrecompileError::under_overflow())?;
            self.set_opted_in_supply(opted_in_supply)?;

            self.storage.emit_event(
                self.address,
                TIP20Event::Transfer(ITIP20::Transfer {
                    from: token_address,
                    to: recipient,
                    amount: accrued,
                })
                .into_log_data(),
            )?;
        }

        Ok(())
    }

    /// Sets or changes the reward recipient for a token holder.
    ///
    /// This function allows a token holder to designate who should receive their
    /// share of rewards. Setting to zero address opts out of rewards.
    pub fn set_reward_recipient(
        &mut self,
        msg_sender: Address,
        call: ITIP20Rewards::setRewardRecipientCall,
    ) -> Result<()> {
        self.check_not_paused()?;
        if recipient != Address::ZERO {
            self.ensure_transfer_authorized(msg_sender, recipient)?;
        }

        let timestamp = self.storage.timestamp();
        self.accrue(timestamp)?;

        let current_recipient = self.get_reward_recipient_of(msg_sender)?;
        if recipient == current_recipient {
            return Ok(());
        }

        let holder_balance = self._get_balances(msg_sender)?;
        if current_recipient != Address::ZERO {
            self.update_rewards(current_recipient)?;
            let delegated_balance = self
                .get_delegated_balance(current_recipient)?
                .checked_sub(holder_balance)
                .ok_or(TempoPrecompileError::under_overflow())?;
            self.set_delegated_balance(current_recipient, delegated_balance)?;
        }

        self.set_reward_recipient_of(msg_sender, recipient)?;
        if recipient == Address::ZERO {
            let opted_in_supply = self
                .get_opted_in_supply()?
                .checked_sub(holder_balance)
                .ok_or(TempoPrecompileError::under_overflow())?;
            self.set_opted_in_supply(opted_in_supply)?;
        } else {
            let delegated = self.get_delegated_balance(recipient)?;
            if delegated > U256::ZERO {
                self.update_rewards(recipient)?;
            }

            let new_delegated = delegated
                .checked_add(holder_balance)
                .ok_or(TempoPrecompileError::under_overflow())?;
            self.set_delegated_balance(recipient, new_delegated)?;

            if current_recipient.is_zero() {
                let opted_in = self
                    .get_opted_in_supply()?
                    .checked_add(holder_balance)
                    .ok_or(TempoPrecompileError::under_overflow())?;
                self.set_opted_in_supply(opted_in)?;
            }

            let rpt = self.get_reward_per_token_stored()?;
            self.set_user_reward_per_token_paid(recipient, rpt)?;
        }

        // Emit reward recipient set event
        self.storage.emit_event(
            self.address,
            TIP20RewardsEvent::RewardRecipientSet(ITIP20Rewards::RewardRecipientSet {
                holder: msg_sender,
                recipient: recipient,
            })
            .into_log_data(),
        )?;

        Ok(())
    }

    /// Cancels an active reward stream and refunds remaining tokens.
    ///
    /// This function allows the funder of a reward stream to cancel it early,
    /// stopping future reward distribution and refunding unused tokens.
    pub fn cancel_reward(
        &mut self,
        msg_sender: Address,
        call: ITIP20Rewards::cancelRewardCall,
    ) -> Result<U256> {
        let stream_id = id;
        let stream = RewardStream::from_storage(stream_id, self.storage, self.address)?;

        if stream.funder.is_zero() {
            return Err(TIP20RewardsError::stream_inactive().into());
        }

        if stream.funder != msg_sender {
            return Err(TIP20RewardsError::not_stream_funder().into());
        }

        let current_time = self.storage.timestamp();
        if current_time >= stream.end_time {
            return Err(TIP20RewardsError::stream_inactive().into());
        }

        let timestamp = self.storage.timestamp();
        self.accrue(timestamp)?;

        let elapsed = if current_time > stream.start_time {
            current_time - U256::from(stream.start_time)
        } else {
            U256::ZERO
        };

        let mut distributed = stream
            .rate_per_second_scaled
            .checked_mul(elapsed)
            .and_then(|v| v.checked_div(ACC_PRECISION))
            .ok_or(TempoPrecompileError::under_overflow())?;
        distributed = distributed.min(stream.amount_total);
        let remaining = stream
            .amount_total
            .checked_sub(distributed)
            .ok_or(TempoPrecompileError::under_overflow())?;

        let total_rps = self
            .get_total_reward_per_second()?
            .checked_sub(stream.rate_per_second_scaled)
            .ok_or(TempoPrecompileError::under_overflow())?;
        self.set_total_reward_per_second(total_rps)?;

        // Update the rate decrease and remove the stream
        let end_time = stream.end_time as u128;
        let rate_decrease = self
            .get_scheduled_rate_decrease_at(end_time)
            .checked_sub(stream.rate_per_second_scaled)
            .ok_or(TempoPrecompileError::under_overflow())?;
        self.set_scheduled_rate_decrease_at(end_time, rate_decrease)?;

        stream.delete(self.storage, self.address)?;

        // Attempt to transfer remaining funds to funder
        let mut refund = U256::ZERO;
        if remaining > U256::ZERO {
            // Check if transfer is authorized
            if self.is_transfer_authorized(stream.funder, stream.funder)? {
                let contract_address = self.address;
                let contract_balance = self
                    ._get_balances(contract_address)?
                    .checked_sub(remaining)
                    .ok_or(TempoPrecompileError::under_overflow())?;
                self._set_balances(contract_address, contract_balance)?;

                let funder_balance = self
                    ._get_balances(stream.funder)?
                    .checked_add(remaining)
                    .ok_or(TempoPrecompileError::under_overflow())?;
                self._set_balances(stream.funder, funder_balance)?;

                self.storage.emit_event(
                    self.address,
                    TIP20Event::Transfer(ITIP20::Transfer {
                        from: self.address,
                        to: stream.funder,
                        amount: remaining,
                    })
                    .into_log_data(),
                )?;

                refund = remaining;
            }
        }

        // Emit reward canceled event
        self.storage.emit_event(
            self.address,
            TIP20RewardsEvent::RewardCanceled(ITIP20Rewards::RewardCanceled {
                funder: stream.funder,
                id: stream_id,
                refund,
            })
            .into_log_data(),
        )?;

        Ok(refund)
    }

    /// Finalizes expired reward streams by updating the total reward rate.
    ///
    /// This function is called to clean up streams that have reached their end time,
    /// reducing the total reward per second rate by the amount of the expired streams.
    pub fn finalize_streams(&mut self, msg_sender: Address, end_time: u128) -> Result<()> {
        if msg_sender != TIP20_REWARDS_REGISTRY_ADDRESS {
            return Err(TIP20Error::unauthorized().into());
        }

        let rate_decrease = self.get_scheduled_rate_decrease_at(end_time);

        if rate_decrease == U256::ZERO {
            return Ok(());
        }

        self.accrue(U256::from(end_time))?;

        let total_rps = self
            .get_total_reward_per_second()?
            .checked_sub(rate_decrease)
            .ok_or(TempoPrecompileError::under_overflow())?;
        self.set_total_reward_per_second(total_rps)?;

        self.set_scheduled_rate_decrease_at(end_time, U256::ZERO)?;

        Ok(())
    }

    /// Gets the last recorded reward per token for a user.
    fn get_user_reward_per_token_paid(&mut self, account: Address) -> Result<U256> {
        let slot = mapping_slot(account, slots::USER_REWARD_PER_TOKEN_PAID);
        self.storage.sload(self.address, slot)
    }

    /// Sets the last recorded reward per token for a user.
    fn set_user_reward_per_token_paid(&mut self, account: Address, value: U256) -> Result<()> {
        let slot = mapping_slot(account, slots::USER_REWARD_PER_TOKEN_PAID);
        self.storage.sstore(self.address, slot, value)
    }

    /// Gets the next available stream ID (minimum 1).
    fn get_next_stream_id(&mut self) -> Result<u64> {
        let id = self
            .storage
            .sload(self.address, slots::NEXT_STREAM_ID)?
            .to::<u64>();

        Ok(id.max(1))
    }

    /// Sets the next stream ID counter.
    fn set_next_stream_id(&mut self, value: u64) -> Result<()> {
        self.storage
            .sstore(self.address, slots::NEXT_STREAM_ID, U256::from(value))
    }

    /// Gets the accumulated reward per token stored.
    fn get_reward_per_token_stored(&mut self) -> Result<U256> {
        self.storage
            .sload(self.address, slots::REWARD_PER_TOKEN_STORED)
    }

    /// Sets the accumulated reward per token in storage.
    fn set_reward_per_token_stored(&mut self, value: U256) -> Result<()> {
        self.storage
            .sstore(self.address, slots::REWARD_PER_TOKEN_STORED, value)
    }

    /// Gets the timestamp of the last reward update from storage.
    fn get_last_update_time(&mut self) -> Result<u64> {
        Ok(self
            .storage
            .sload(self.address, slots::LAST_UPDATE_TIME)?
            .to::<u64>())
    }

    /// Sets the timestamp of the last reward update in storage.
    fn set_last_update_time(&mut self, value: U256) -> Result<()> {
        self.storage
            .sstore(self.address, slots::LAST_UPDATE_TIME, value)
    }

    /// Gets the total supply of tokens opted into rewards from storage.
    pub fn get_opted_in_supply(&mut self) -> Result<U256> {
        self.storage.sload(self.address, slots::OPTED_IN_SUPPLY)
    }

    /// Sets the total supply of tokens opted into rewards in storage.
    pub fn set_opted_in_supply(&mut self, value: U256) -> Result<()> {
        self.storage
            .sstore(self.address, slots::OPTED_IN_SUPPLY, value)
    }

    /// Gets the reward recipient address for an account from storage.
    fn get_reward_recipient_of(&mut self, account: Address) -> Result<Address> {
        let slot = mapping_slot(account, slots::REWARD_RECIPIENT_OF);
        Ok(self.storage.sload(self.address, slot)?.into_address())
    }

    /// Sets the reward recipient address for an account in storage.
    fn set_reward_recipient_of(&mut self, account: Address, recipient: Address) -> Result<()> {
        let slot = mapping_slot(account, slots::REWARD_RECIPIENT_OF);
        self.storage
            .sstore(self.address, slot, recipient.into_u256())
    }

    /// Gets the delegated balance for an account from storage.
    fn get_delegated_balance(&mut self, account: Address) -> Result<U256> {
        let slot = mapping_slot(account, slots::DELEGATED_BALANCE);
        self.storage.sload(self.address, slot)
    }

    /// Sets the delegated balance for an account in storage.
    fn set_delegated_balance(&mut self, account: Address, amount: U256) -> Result<()> {
        let slot = mapping_slot(account, slots::DELEGATED_BALANCE);
        self.storage.sstore(self.address, slot, amount)
    }

    /// Gets the scheduled rate decrease at a specific time from storage.
    fn get_scheduled_rate_decrease_at(&mut self, end_time: u128) -> U256 {
        let slot = mapping_slot(end_time.to_be_bytes(), slots::SCHEDULED_RATE_DECREASE);
        self.storage.sload(self.address, slot).unwrap_or(U256::ZERO)
    }

    /// Sets the scheduled rate decrease at a specific time in storage.
    fn set_scheduled_rate_decrease_at(&mut self, end_time: u128, value: U256) -> Result<()> {
        let slot = mapping_slot(end_time.to_be_bytes(), slots::SCHEDULED_RATE_DECREASE);
        self.storage.sstore(self.address, slot, value)
    }

    /// Gets the total reward per second rate from storage.
    pub fn get_total_reward_per_second(&mut self) -> Result<U256> {
        self.storage
            .sload(self.address, slots::TOTAL_REWARD_PER_SECOND)
    }

    /// Sets the total reward per second rate in storage.
    fn set_total_reward_per_second(&mut self, value: U256) -> Result<()> {
        self.storage
            .sstore(self.address, slots::TOTAL_REWARD_PER_SECOND, value)
    }

    /// Handles reward accounting for both sender and receiver during token transfers.
    ///
    /// This function manages the opted-in supply adjustments when tokens are transferred
    /// between addresses with different reward recipient settings. It returns the net
    /// change to the opted-in supply.
    pub fn handle_rewards_on_transfer(
        &mut self,
        from: Address,
        to: Address,
        amount: U256,
    ) -> Result<()> {
        let mut opted_in_delta = alloy::primitives::I256::ZERO;

        if let Some(delta) = self.handle_sender_rewards(from, amount)? {
            opted_in_delta = alloy::primitives::I256::from(delta);
        }

        if let Some(delta) = self.handle_receiver_rewards(to, amount)? {
            opted_in_delta -= alloy::primitives::I256::from(delta);
        }

        if opted_in_delta > alloy::primitives::I256::ZERO {
            let opted_in_supply = self
                .get_opted_in_supply()?
                .checked_sub(U256::from(opted_in_delta))
                .ok_or(crate::error::TempoPrecompileError::under_overflow())?;
            self.set_opted_in_supply(opted_in_supply)?;
        } else if opted_in_delta < alloy::primitives::I256::ZERO {
            let opted_in_supply = self
                .get_opted_in_supply()?
                .checked_add(U256::from(-opted_in_delta))
                .ok_or(crate::error::TempoPrecompileError::under_overflow())?;
            self.set_opted_in_supply(opted_in_supply)?;
        }

        Ok(())
    }

    /// Handles reward accounting when tokens are minted to an address.
    ///
    /// This function manages the opted-in supply adjustments when tokens are minted
    /// to an address with a reward recipient setting. It only handles receiver rewards
    /// since tokens are minted from the zero address.
    pub fn handle_rewards_on_mint(&mut self, to: Address, amount: U256) -> Result<()> {
        if let Some(delta) = self.handle_receiver_rewards(to, amount)? {
            let opted_in_supply = self
                .get_opted_in_supply()?
                .checked_add(delta)
                .ok_or(crate::error::TempoPrecompileError::under_overflow())?;
            self.set_opted_in_supply(opted_in_supply)?;
        }

        Ok(())
    }
}
