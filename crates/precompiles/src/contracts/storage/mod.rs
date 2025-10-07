pub mod evm;
pub mod hashmap;
pub mod mapping;
pub mod slots;

use std::{cell::RefCell, fmt::Debug, rc::Rc};

use alloy::primitives::{Address, LogData, U256};
use revm::state::{AccountInfo, Bytecode};

pub trait StorageProvider {
    type Error: Debug;

    fn chain_id(&self) -> u64;
    fn set_code(&mut self, address: Address, code: Bytecode) -> Result<(), Self::Error>;
    fn get_account_info(&mut self, address: Address) -> Result<AccountInfo, Self::Error>;
    fn sstore(&mut self, address: Address, key: U256, value: U256) -> Result<(), Self::Error>;
    fn sload(&mut self, address: Address, key: U256) -> Result<U256, Self::Error>;
    fn emit_event(&mut self, address: Address, event: LogData) -> Result<(), Self::Error>;
}

pub trait StorageOps {
    // TODO: error handling
    fn sstore(&mut self, slot: U256, value: U256);
    fn sload(&mut self, slot: U256) -> U256;
}

impl<S: StorageProvider> StorageProvider for Rc<RefCell<&'_ mut S>> {
    type Error = S::Error;

    fn chain_id(&self) -> u64 {
        self.borrow().chain_id()
    }

    fn set_code(&mut self, address: Address, code: Bytecode) -> Result<(), Self::Error> {
        self.borrow_mut().set_code(address, code)
    }

    fn get_account_info(&mut self, address: Address) -> Result<AccountInfo, Self::Error> {
        self.borrow_mut().get_account_info(address)
    }

    fn sstore(&mut self, address: Address, key: U256, value: U256) -> Result<(), Self::Error> {
        self.borrow_mut().sstore(address, key, value)
    }

    fn sload(&mut self, address: Address, key: U256) -> Result<U256, Self::Error> {
        self.borrow_mut().sload(address, key)
    }

    fn emit_event(&mut self, address: Address, event: LogData) -> Result<(), Self::Error> {
        self.borrow_mut().emit_event(address, event)
    }
}
