use alloy::primitives::{Address, B256, U256};

use crate::{
    error::Result,
    storage::PrecompileStorageProvider,
    tip20_migrated::{RolesAuthError, TIP20Token, TIP20Token_IRolesAuth},
};

pub const DEFAULT_ADMIN_ROLE: B256 = B256::ZERO;
pub const UNGRANTABLE_ROLE: B256 = B256::new([0xff; 32]);

impl<'a, S: PrecompileStorageProvider> TIP20Token_IRolesAuth for TIP20Token<'a, S> {
    fn grant_role(&mut self, msg_sender: Address, role: B256, account: Address) -> Result<()> {
        let admin_role = self.get_role_admin_internal(role)?;
        self.check_role_internal(msg_sender, admin_role)?;
        self.grant_role_internal(account, role)?;
        self._emit_role_membership_updated(role, account, msg_sender, true)
    }

    fn revoke_role(&mut self, msg_sender: Address, role: B256, account: Address) -> Result<()> {
        let admin_role = self.get_role_admin_internal(role)?;
        self.check_role_internal(msg_sender, admin_role)?;
        self.revoke_role_internal(account, role)?;
        self._emit_role_membership_updated(role, account, msg_sender, false)
    }

    fn renounce_role(&mut self, msg_sender: Address, role: B256) -> Result<()> {
        self.check_role_internal(msg_sender, role)?;
        self.revoke_role_internal(msg_sender, role)?;
        self._emit_role_membership_updated(role, msg_sender, msg_sender, false)
    }

    fn set_role_admin(&mut self, msg_sender: Address, role: B256, admin_role: B256) -> Result<()> {
        let current_admin_role = self.get_role_admin_internal(role)?;
        self.check_role_internal(msg_sender, current_admin_role)?;

        self.set_role_admin_internal(role, admin_role)?;
        self._emit_role_admin_updated(role, admin_role, msg_sender)
    }
}

impl<'a, S: PrecompileStorageProvider> TIP20Token<'a, S> {
    /// Initialize the UNGRANTABLE_ROLE to be self-administered
    pub(crate) fn roles_initialize(&mut self) -> Result<()> {
        self.set_role_admin_internal(UNGRANTABLE_ROLE, UNGRANTABLE_ROLE)
    }

    /// Grant the default admin role to an account
    pub fn roles_grant_default_admin(&mut self, admin: Address) -> Result<()> {
        self.grant_role_internal(admin, DEFAULT_ADMIN_ROLE)
    }

    // Utility functions for checking roles without calldata
    pub fn check_role(&mut self, account: Address, role: B256) -> Result<()> {
        self.check_role_internal(account, role)
    }

    pub fn grant_role_internal(&mut self, account: Address, role: B256) -> Result<()> {
        self._set_roles(account, role, true)
    }

    fn revoke_role_internal(&mut self, account: Address, role: B256) -> Result<()> {
        self._set_roles(account, role, false)
    }

    fn get_role_admin_internal(&mut self, role: B256) -> Result<B256> {
        let admin = self._get_role_admins(role)?;
        Ok(B256::from(admin)) // If sloads 0, will be equal to DEFAULT_ADMIN_ROLE
    }

    fn set_role_admin_internal(&mut self, role: B256, admin_role: B256) -> Result<()> {
        self._set_role_admins(role, admin_role)
    }

    fn check_role_internal(&mut self, account: Address, role: B256) -> Result<()> {
        if !self._get_roles(account, role)? {
            return Err(RolesAuthError::unauthorized()).into();
        }
        Ok(())
    }
}
