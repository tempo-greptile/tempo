pub mod dispatch;

use crate::{error::Result, storage::StorageCtx, tip20::TIP20Token};
use alloy::primitives::{Address, B256, keccak256};
use std::sync::LazyLock;
pub use tempo_contracts::precompiles::IPathUSD;

pub static TRANSFER_ROLE: LazyLock<B256> = LazyLock::new(|| keccak256(b"TRANSFER_ROLE"));
pub static RECEIVE_WITH_MEMO_ROLE: LazyLock<B256> =
    LazyLock::new(|| keccak256(b"RECEIVE_WITH_MEMO_ROLE"));
const NAME: &str = "pathUSD";
const CURRENCY: &str = "USD";

pub struct PathUSD {
    pub token: TIP20Token,
    storage: StorageCtx,
}

impl Default for PathUSD {
    fn default() -> Self {
        Self::new()
    }
}

impl PathUSD {
    pub fn new() -> Self {
        Self {
            token: TIP20Token::new(0),
            storage: StorageCtx::default(),
        }
    }

    pub fn initialize(&mut self, admin: Address) -> Result<()> {
        self.token
            .initialize(NAME, NAME, CURRENCY, Address::ZERO, admin, Address::ZERO)
    }

    pub fn name(&self) -> Result<String> {
        Ok(NAME.to_string())
    }

    pub fn symbol(&self) -> Result<String> {
        Ok(NAME.to_string())
    }

    /// Returns the TRANSFER_ROLE constant
    ///
    /// This role identifier grants permission to transfer pathUSD tokens.
    /// The role is computed as `keccak256("TRANSFER_ROLE")`.
    pub fn transfer_role() -> B256 {
        *TRANSFER_ROLE
    }

    /// Returns the RECEIVE_WITH_MEMO_ROLE constant
    ///
    /// This role identifier grants permission to receive pathUSD tokens.
    /// The role is computed as `keccak256("RECEIVE_WITH_MEMO_ROLE")`.
    pub fn receive_with_memo_role() -> B256 {
        *RECEIVE_WITH_MEMO_ROLE
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::uint;
    use tempo_chainspec::hardfork::TempoHardfork;
    use tempo_contracts::precompiles::RolesAuthError;

    use super::*;
    use crate::{
        error::TempoPrecompileError,
        storage::hashmap::HashMapStorageProvider,
        test_util::{TIP20Setup, setup_storage},
        tip20::{IRolesAuth, ISSUER_ROLE, PAUSE_ROLE, UNPAUSE_ROLE},
        tip403_registry::{ITIP403Registry, TIP403Registry},
    };

    fn transfer_test_setup(admin: Address) -> Result<PathUSD> {
        let mut path_usd = PathUSD::new();
        path_usd.initialize(admin)?;
        path_usd.token.grant_role_internal(admin, *ISSUER_ROLE)?;

        Ok(path_usd)
    }

    #[test]
    fn test_metadata() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        let admin = Address::random();

        StorageCtx::enter(&mut storage, || {
            let path_usd = TIP20Setup::path_usd(admin).apply()?;
            assert_eq!(path_usd.name()?, NAME);
            assert_eq!(path_usd.symbol()?, NAME);
            assert_eq!(path_usd.currency()?, "USD");

            Ok(())
        })
    }

    #[test]
    fn test_mint() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        let admin = Address::random();
        let owner = Address::random();
        let amount = U256::from(1000);

        StorageCtx::enter(&mut storage, || {
            let mut path_usd = TIP20Setup::path_usd(admin).with_issuer(admin).apply()?;

            let balance_before = path_usd.balance_of(ITIP20::balanceOfCall { account: owner })?;

            path_usd.mint(admin, ITIP20::mintCall { to: owner, amount })?;

            let balance_after = path_usd.balance_of(ITIP20::balanceOfCall { account: owner })?;
            assert_eq!(balance_after, balance_before + amount);
            Ok(())
        })
    }

    #[test]
    fn test_burn() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        let admin = Address::random();
        let owner = Address::random();
        let amount = U256::from(1000);

        StorageCtx::enter(&mut storage, || {
            let mut path_usd = TIP20Setup::path_usd(admin)
                .with_issuer(admin)
                .with_mint(owner, amount)
                .apply()?;

            let balance_before = path_usd.balance_of(ITIP20::balanceOfCall { account: owner })?;

            path_usd.burn(owner, ITIP20::burnCall { amount })?;

            let balance_after = path_usd.balance_of(ITIP20::balanceOfCall { account: owner })?;
            assert_eq!(balance_after, balance_before - amount);
            Ok(())
        })
    }

    #[test]
    fn test_approve() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        let admin = Address::random();
        let owner = Address::random();
        let spender = Address::random();
        let amount = U256::from(1000);

        StorageCtx::enter(&mut storage, || {
            let mut path_usd = TIP20Setup::path_usd(admin).with_issuer(admin).apply()?;

            path_usd.approve(owner, ITIP20::approveCall { spender, amount })?;

            let allowance = path_usd.allowance(ITIP20::allowanceCall { owner, spender })?;
            assert_eq!(allowance, amount);
            Ok(())
        })
    }

    #[test]
    fn test_pause_and_unpause() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        let admin = Address::random();
        let pauser = Address::random();
        let unpauser = Address::random();

        StorageCtx::enter(&mut storage, || {
            let mut path_usd = TIP20Setup::path_usd(admin)
                // Grant PAUSE_ROLE and UNPAUSE_ROLE
                .with_role(pauser, *PAUSER)
                .with_role(unpauser, *UNPAUSE_ROLE)
                .apply()?;

            assert!(!path_usd.paused()?);

            path_usd.pause(pauser, ITIP20::pauseCall {})?;
            assert!(path_usd.paused()?);

            path_usd.unpause(unpauser, ITIP20::unpauseCall {})?;
            assert!(!path_usd.paused()?);
            Ok(())
        })
    }

    #[test]
    fn test_role_management() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        let admin = Address::random();
        let user = Address::random();

        StorageCtx::enter(&mut storage, || {
            let mut path_usd = TIP20Setup::path_usd(admin).apply()?;

            // Grant ISSUER_ROLE to user
            path_usd.token.grant_role(
                admin,
                IRolesAuth::grantRoleCall {
                    role: *ISSUER_ROLE,
                    account: user,
                },
            )?;

            // Check that user has the role
            assert!(path_usd.token.has_role(IRolesAuth::hasRoleCall {
                role: *ISSUER_ROLE,
                account: user,
            })?);

            // Revoke the role
            path_usd.token.revoke_role(
                admin,
                IRolesAuth::revokeRoleCall {
                    role: *ISSUER_ROLE,
                    account: user,
                },
            )?;

            // Check that user no longer has the role
            assert!(!path_usd.token.has_role(IRolesAuth::hasRoleCall {
                role: *ISSUER_ROLE,
                account: user,
            })?);
            Ok(())
        })
    }

    #[test]
    fn test_supply_cap() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        let admin = Address::random();
        let recipient = Address::random();
        let supply_cap = U256::from(1000);

        StorageCtx::enter(&mut storage, || {
            let mut path_usd = TIP20Setup::path_usd(admin).with_issuer(admin).apply()?;

            // Set supply cap
            path_usd.token.set_supply_cap(
                admin,
                ITIP20::setSupplyCapCall {
                    newSupplyCap: supply_cap,
                },
            )?;
            assert_eq!(path_usd.token.supply_cap()?, supply_cap);

            // Try to mint more than supply cap
            let result = path_usd.mint(
                admin,
                ITIP20::mintCall {
                    to: recipient,
                    amount: U256::from(1001),
                },
            );

            assert_eq!(
                result.unwrap_err(),
                TempoPrecompileError::TIP20(TIP20Error::supply_cap_exceeded())
            );
            Ok(())
        })
    }

    #[test]
    fn test_invalid_supply_caps() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        let admin = Address::random();
        let recipient = Address::random();
        let supply_cap = U256::from(1000);
        let bad_supply_cap = U256::from(u128::MAX) + U256::ONE;

        StorageCtx::enter(&mut storage, || {
            let mut path_usd = TIP20Setup::path_usd(admin).with_issuer(admin).apply()?;

            // Set supply cap to u128 max plus one
            let result = path_usd.token.set_supply_cap(
                admin,
                ITIP20::setSupplyCapCall {
                    newSupplyCap: bad_supply_cap,
                },
            );

            assert_eq!(
                result.unwrap_err(),
                TempoPrecompileError::TIP20(TIP20Error::supply_cap_exceeded())
            );

            // Set supply cap
            path_usd.token.set_supply_cap(
                admin,
                ITIP20::setSupplyCapCall {
                    newSupplyCap: supply_cap,
                },
            )?;

            // Try to mint the exact supply cap
            path_usd.mint(
                admin,
                ITIP20::mintCall {
                    to: recipient,
                    amount: supply_cap,
                },
            )?;

            // Try to set the supply cap to something lower than the total supply
            let smaller_supply_cap = supply_cap - U256::ONE;
            let result = path_usd.token.set_supply_cap(
                admin,
                ITIP20::setSupplyCapCall {
                    newSupplyCap: smaller_supply_cap,
                },
            );

            assert_eq!(
                result.unwrap_err(),
                TempoPrecompileError::TIP20(TIP20Error::invalid_supply_cap())
            );
            Ok(())
        })
    }

    #[test]
    fn test_change_transfer_policy_id() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        let admin = Address::random();
        let non_admin = Address::random();

        StorageCtx::enter(&mut storage, || {
            let mut path_usd = TIP20Setup::path_usd(admin).apply()?;

            // Initialize TIP403 registry
            let mut registry = TIP403Registry::new();
            registry.initialize()?;

            // Create a valid policy
            let new_policy_id = registry.create_policy(
                admin,
                ITIP403Registry::createPolicyCall {
                    admin,
                    policyType: ITIP403Registry::PolicyType::WHITELIST,
                },
            )?;

            // Admin can change transfer policy ID
            path_usd.token.change_transfer_policy_id(
                admin,
                ITIP20::changeTransferPolicyIdCall {
                    newPolicyId: new_policy_id,
                },
            )?;

            assert_eq!(path_usd.token.transfer_policy_id()?, new_policy_id);

            // Non-admin cannot change transfer policy ID
            let result = path_usd.token.change_transfer_policy_id(
                non_admin,
                ITIP20::changeTransferPolicyIdCall { newPolicyId: 100 },
            );

            assert_eq!(
                result.unwrap_err(),
                TempoPrecompileError::RolesAuthError(RolesAuthError::unauthorized())
            );
            Ok(())
        })
    }

    #[test]
    fn test_transfer() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        let admin = Address::random();
        let sender = Address::random();
        let recipient = Address::random();
        let amount = U256::from(1000);

        StorageCtx::enter(&mut storage, || {
            let mut path_usd = TIP20Setup::path_usd(admin)
                .with_issuer(admin)
                .with_mint(sender, amount)
                .apply()?;

            let result = path_usd.transfer(
                sender,
                ITIP20::transferCall {
                    to: recipient,
                    amount,
                },
            )?;

            assert!(result);

            let sender_balance = path_usd.balance_of(ITIP20::balanceOfCall { account: sender })?;
            let recipient_balance =
                path_usd.balance_of(ITIP20::balanceOfCall { account: recipient })?;

            assert_eq!(sender_balance, U256::ZERO);
            assert_eq!(recipient_balance, amount);
            Ok(())
        })
    }

    #[test]
    fn test_transfer_from() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        let admin = Address::random();
        let sender = Address::random();
        let recipient = Address::random();
        let amount = U256::from(1000);

        StorageCtx::enter(&mut storage, || {
            let mut path_usd = TIP20Setup::path_usd(admin)
                .with_issuer(admin)
                .with_mint(sender, amount)
                .with_approval(sender, recipient, amount)
                .apply()?;

            let success = path_usd.transfer_from(
                spender,
                ITIP20::transferFromCall {
                    from: owner,
                    to: recipient,
                    amount,
                },
            )?;
            assert!(success);

            let owner_balance = path_usd.balance_of(ITIP20::balanceOfCall { account: owner })?;
            let recipient_balance =
                path_usd.balance_of(ITIP20::balanceOfCall { account: recipient })?;

            assert_eq!(owner_balance, U256::ZERO);
            assert_eq!(recipient_balance, amount);
            Ok(())
        })
    }

    #[test]
    fn test_transfer_with_memo() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        let admin = Address::random();
        let sender = Address::random();
        let recipient = Address::random();
        let amount = U256::from(1000);
        let memo = [1u8; 32];

        StorageCtx::enter(&mut storage, || {
            let mut path_usd = TIP20Setup::path_usd(admin)
                .with_issuer(admin)
                .with_mint(sender, amount)
                .apply()?;

            path_usd.transfer_with_memo(
                sender,
                ITIP20::transferWithMemoCall {
                    to: recipient,
                    amount,
                    memo: memo.into(),
                },
            )?;

            let sender_balance = path_usd.balance_of(ITIP20::balanceOfCall { account: sender })?;
            let recipient_balance =
                path_usd.balance_of(ITIP20::balanceOfCall { account: recipient })?;

            assert_eq!(sender_balance, U256::ZERO);
            assert_eq!(recipient_balance, amount);
            Ok(())
        })
    }

    #[test]
    fn test_transfer_from_with_memo() -> eyre::Result<()> {
        let mut storage = HashMapStorageProvider::new(1);
        let admin = Address::random();
        let sender = Address::random();
        let recipient = Address::random();
        let amount = U256::from(1000);
        let memo = [1u8; 32];

        StorageCtx::enter(&mut storage, || {
            let mut path_usd = TIP20Setup::path_usd(admin)
                .with_issuer(admin)
                .with_mint(sender, amount)
                .with_approval(sender, recipient, amount)
                .apply()?;

            let success = path_usd.transfer_from_with_memo(
                spender,
                ITIP20::transferFromWithMemoCall {
                    from: owner,
                    to: recipient,
                    amount,
                    memo: memo.into(),
                },
            )?;
            assert!(success);

            let owner_balance = path_usd.balance_of(ITIP20::balanceOfCall { account: owner })?;
            let recipient_balance =
                path_usd.balance_of(ITIP20::balanceOfCall { account: recipient })?;

            assert_eq!(owner_balance, U256::ZERO);
            assert_eq!(recipient_balance, amount);
            Ok(())
        })
    }
}
