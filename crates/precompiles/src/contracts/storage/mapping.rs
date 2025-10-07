use crate::contracts::{
    StorageProvider,
    storage::slots::{double_mapping_slot, mapping_slot},
};
use alloy::primitives::{Address, U256};
use std::{borrow::Borrow, cell::RefCell, marker::PhantomData, ops::Deref, rc::Rc};

/// Helper type to access a mapping in the storage.
#[derive(Debug)]
pub struct Mapping<'a, S: StorageProvider, K> {
    address: Address,
    storage: Rc<RefCell<&'a mut S>>,
    base_slot: U256,
    _pd: PhantomData<K>,
}

impl<'a, S: StorageProvider, K: AsRef<[u8]>> Mapping<'a, S, K> {
    /// Create a new mapping.
    pub fn new(address: Address, storage: Rc<RefCell<&'a mut S>>, base_slot: U256) -> Self {
        Self {
            address,
            storage,
            base_slot,
            _pd: PhantomData,
        }
    }

    /// Get the value of the mapping for the given key.
    pub fn get(&self, key: impl Borrow<K>) -> Result<U256, S::Error> {
        let slot = mapping_slot(key.borrow(), self.base_slot);
        self.storage.borrow_mut().sload(self.address, slot)
    }

    /// Get a mutable reference to the value of the mapping for the given key.
    pub fn get_mut(&mut self, key: K) -> Result<BorrowedValue<'a, '_, S>, S::Error> {
        let slot = mapping_slot(key.borrow(), self.base_slot);
        let value = self.storage.borrow_mut().sload(self.address, slot)?;
        Ok(BorrowedValue {
            slot,
            contract: &self.address,
            value,
            storage: &mut self.storage,
        })
    }

    /// Set the value of the mapping for the given key.
    pub fn set(&self, key: impl Borrow<K>, value: U256) -> Result<(), S::Error> {
        let slot = mapping_slot(key.borrow(), self.base_slot);
        self.storage.borrow_mut().sstore(self.address, slot, value)
    }
}

/// Helper type to access a double mapping in the storage.
#[derive(Debug)]
pub struct DoubleMapping<'a, S: StorageProvider, K1, K2> {
    address: Address,
    storage: Rc<RefCell<&'a mut S>>,
    base_slot: U256,
    _pd: PhantomData<(K1, K2)>,
}

impl<'a, S: StorageProvider, K1: AsRef<[u8]>, K2: AsRef<[u8]>> DoubleMapping<'a, S, K1, K2> {
    /// Create a new double mapping.
    pub fn new(address: Address, storage: Rc<RefCell<&'a mut S>>, base_slot: U256) -> Self {
        Self {
            address,
            storage,
            base_slot,
            _pd: PhantomData,
        }
    }

    /// Get the value of the double mapping for the given keys.
    pub fn get(&self, key1: impl Borrow<K1>, key2: impl Borrow<K2>) -> Result<U256, S::Error> {
        let slot = double_mapping_slot(key1.borrow(), key2.borrow(), self.base_slot);
        self.storage.borrow_mut().sload(self.address, slot)
    }

    /// Get a mutable reference to the value of the double mapping for the given keys.
    pub fn get_mut(&mut self, key1: K1, key2: K2) -> Result<BorrowedValue<'a, '_, S>, S::Error> {
        let slot = double_mapping_slot(key1.borrow(), key2.borrow(), self.base_slot);
        let value = self.storage.borrow_mut().sload(self.address, slot)?;
        Ok(BorrowedValue {
            slot,
            contract: &self.address,
            value,
            storage: &mut self.storage,
        })
    }

    /// Set the value of the double mapping for the given keys.
    pub fn set(
        &self,
        key1: impl Borrow<K1>,
        key2: impl Borrow<K2>,
        value: U256,
    ) -> Result<(), S::Error> {
        let slot = double_mapping_slot(key1.borrow(), key2.borrow(), self.base_slot);
        self.storage.borrow_mut().sstore(self.address, slot, value)
    }
}

/// A value that is borrowed from a mapping.
///
/// This is used to allow mutating the value of the mapping without calculating the slot each time.
pub struct BorrowedValue<'a, 'b, S: StorageProvider> {
    slot: U256,
    contract: &'b Address,
    storage: &'b mut Rc<RefCell<&'a mut S>>,
    value: U256,
}

impl<'a, 'b, S: StorageProvider> Deref for BorrowedValue<'a, 'b, S> {
    type Target = U256;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<'a, 'b, S: StorageProvider> BorrowedValue<'a, 'b, S> {
    pub fn inc(&mut self, amount: U256) -> Result<(), S::Error> {
        self.value += amount;
        self.storage
            .borrow_mut()
            .sstore(*self.contract, self.slot, self.value)
    }
}
