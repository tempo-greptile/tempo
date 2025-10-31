//! Unit tests for the #[derive(Storable)] macro in isolation.
//! These tests verify that user-defined structs properly implement store, load, and delete operations.

// Re-export `tempo_precompiles::storage` as a local module so `crate::storage` works
mod storage {
    pub(super) use tempo_precompiles::storage::*;
}

use alloy::primitives::{Address, U256};
use storage::{
    ContractStorage, PrecompileStorageProvider, Storable, hashmap::HashMapStorageProvider,
};
use tempo_precompiles::error;
use tempo_precompiles_macros::Storable;

// Test wrapper that combines address + storage provider to implement ContractStorage
struct TestStorage<S> {
    address: Address,
    storage: S,
}

impl<S: PrecompileStorageProvider> ContractStorage for TestStorage<S> {
    type Storage = S;
    fn address(&self) -> Address {
        self.address
    }
    fn storage(&mut self) -> &mut Self::Storage {
        &mut self.storage
    }
}

// Helper to generate addresses
fn test_address(byte: u8) -> Address {
    let mut bytes = [0u8; 20];
    bytes[19] = byte;
    Address::from(bytes)
}

// Test structs with various field configurations

#[derive(Debug, Clone, PartialEq, Eq, Storable)]
struct SingleField {
    pub value: U256,
}

#[derive(Debug, Clone, PartialEq, Eq, Storable)]
struct ThreeFields {
    pub field1: U256,
    pub field2: U256,
    pub field3: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Storable)]
struct MixedTypes {
    pub owner: Address,
    pub active: bool,
    pub balance: U256,
    pub nonce: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Storable)]
struct FiveFields {
    pub f1: U256,
    pub f2: U256,
    pub f3: U256,
    pub f4: U256,
    pub f5: U256,
}

#[test]
fn test_slot_count_calculation() {
    // Verify SLOT_COUNT is correctly calculated for various struct sizes
    assert_eq!(SingleField::SLOT_COUNT, 1);
    assert_eq!(ThreeFields::SLOT_COUNT, 3);
    assert_eq!(MixedTypes::SLOT_COUNT, 4);
    assert_eq!(FiveFields::SLOT_COUNT, 5);
}

#[test]
fn test_single_field_round_trip() {
    let mut storage = TestStorage {
        address: test_address(1),
        storage: HashMapStorageProvider::new(1),
    };
    let base_slot = U256::from(100);

    let original = SingleField {
        value: U256::from(42),
    };

    // Store
    original.store(&mut storage, base_slot).unwrap();

    // Verify storage slot
    assert_eq!(
        storage.storage.sload(storage.address, base_slot),
        Ok(U256::from(42))
    );

    // Load and verify
    let loaded = SingleField::load(&mut storage, base_slot).unwrap();
    assert_eq!(loaded, original);
}

#[test]
fn test_three_fields_round_trip() {
    let mut storage = TestStorage {
        address: test_address(1),
        storage: HashMapStorageProvider::new(1),
    };
    let base_slot = U256::from(50);

    let original = ThreeFields {
        field1: U256::from(1000),
        field2: U256::from(2000),
        field3: 3000,
    };

    // Store
    original.store(&mut storage, base_slot).unwrap();

    // Verify individual slots
    assert_eq!(
        storage.storage.sload(storage.address, base_slot),
        Ok(U256::from(1000))
    ); // field1
    assert_eq!(
        storage
            .storage
            .sload(storage.address, base_slot + U256::from(1)),
        Ok(U256::from(2000))
    ); // field2
    assert_eq!(
        storage
            .storage
            .sload(storage.address, base_slot + U256::from(2)),
        Ok(U256::from(3000))
    ); // field3

    // Load and verify
    let loaded = ThreeFields::load(&mut storage, base_slot).unwrap();
    assert_eq!(loaded, original);
}

#[test]
fn test_mixed_types_round_trip() {
    let mut storage = TestStorage {
        address: test_address(1),
        storage: HashMapStorageProvider::new(1),
    };
    let base_slot = U256::from(200);

    let owner_addr = test_address(42);
    let original = MixedTypes {
        owner: owner_addr,
        active: true,
        balance: U256::from(1_000_000),
        nonce: 5,
    };

    // Store
    original.store(&mut storage, base_slot).unwrap();

    // Verify individual slots
    assert_eq!(
        storage.storage.sload(storage.address, base_slot),
        Ok(U256::from_be_bytes(owner_addr.into_word().into()))
    ); // owner
    assert_eq!(
        storage
            .storage
            .sload(storage.address, base_slot + U256::from(1)),
        Ok(U256::from(1))
    ); // active (true)
    assert_eq!(
        storage
            .storage
            .sload(storage.address, base_slot + U256::from(2)),
        Ok(U256::from(1_000_000))
    ); // balance
    assert_eq!(
        storage
            .storage
            .sload(storage.address, base_slot + U256::from(3)),
        Ok(U256::from(5))
    ); // nonce

    // Load and verify
    let loaded = MixedTypes::load(&mut storage, base_slot).unwrap();
    assert_eq!(loaded, original);
}

#[test]
fn test_five_fields_round_trip() {
    let mut storage = TestStorage {
        address: test_address(1),
        storage: HashMapStorageProvider::new(1),
    };
    let base_slot = U256::from(1000);

    let original = FiveFields {
        f1: U256::from(111),
        f2: U256::from(222),
        f3: U256::from(333),
        f4: U256::from(444),
        f5: U256::from(555),
    };

    // Store
    original.store(&mut storage, base_slot).unwrap();

    // Load and verify
    let loaded = FiveFields::load(&mut storage, base_slot).unwrap();
    assert_eq!(loaded, original);
}

#[test]
fn test_delete_single_field() {
    let mut storage = TestStorage {
        address: test_address(1),
        storage: HashMapStorageProvider::new(1),
    };
    let base_slot = U256::from(100);

    let data = SingleField {
        value: U256::from(999),
    };

    // Store data
    data.store(&mut storage, base_slot).unwrap();
    assert_eq!(
        storage.storage.sload(storage.address, base_slot),
        Ok(U256::from(999))
    );

    // Delete
    SingleField::delete(&mut storage, base_slot).unwrap();

    // Verify slot is zeroed
    assert_eq!(
        storage.storage.sload(storage.address, base_slot),
        Ok(U256::ZERO)
    );

    // Verify load returns default values
    let loaded = SingleField::load(&mut storage, base_slot).unwrap();
    assert_eq!(loaded, SingleField { value: U256::ZERO });
}

#[test]
fn test_delete_three_fields() {
    let mut storage = TestStorage {
        address: test_address(1),
        storage: HashMapStorageProvider::new(1),
    };
    let base_slot = U256::from(50);

    let data = ThreeFields {
        field1: U256::from(1000),
        field2: U256::from(2000),
        field3: 3000,
    };

    // Store data
    data.store(&mut storage, base_slot).unwrap();

    // Verify data is stored
    assert_eq!(
        storage.storage.sload(storage.address, base_slot),
        Ok(U256::from(1000))
    );
    assert_eq!(
        storage
            .storage
            .sload(storage.address, base_slot + U256::from(1)),
        Ok(U256::from(2000))
    );
    assert_eq!(
        storage
            .storage
            .sload(storage.address, base_slot + U256::from(2)),
        Ok(U256::from(3000))
    );

    // Delete
    ThreeFields::delete(&mut storage, base_slot).unwrap();

    // Verify all slots are zeroed
    assert_eq!(
        storage.storage.sload(storage.address, base_slot),
        Ok(U256::ZERO)
    );
    assert_eq!(
        storage
            .storage
            .sload(storage.address, base_slot + U256::from(1)),
        Ok(U256::ZERO)
    );
    assert_eq!(
        storage
            .storage
            .sload(storage.address, base_slot + U256::from(2)),
        Ok(U256::ZERO)
    );

    // Verify load returns default values
    let loaded = ThreeFields::load(&mut storage, base_slot).unwrap();
    assert_eq!(
        loaded,
        ThreeFields {
            field1: U256::ZERO,
            field2: U256::ZERO,
            field3: 0,
        }
    );
}

#[test]
fn test_delete_mixed_types() {
    let mut storage = TestStorage {
        address: test_address(1),
        storage: HashMapStorageProvider::new(1),
    };
    let base_slot = U256::from(200);

    let data = MixedTypes {
        owner: test_address(42),
        active: true,
        balance: U256::from(1_000_000),
        nonce: 5,
    };

    // Store data
    data.store(&mut storage, base_slot).unwrap();

    // Delete
    MixedTypes::delete(&mut storage, base_slot).unwrap();

    // Verify all 4 slots are zeroed
    for offset in 0..4 {
        assert_eq!(
            storage
                .storage
                .sload(storage.address, base_slot + U256::from(offset)),
            Ok(U256::ZERO)
        );
    }

    // Verify load returns default values
    let loaded = MixedTypes::load(&mut storage, base_slot).unwrap();
    assert_eq!(
        loaded,
        MixedTypes {
            owner: Address::ZERO,
            active: false,
            balance: U256::ZERO,
            nonce: 0,
        }
    );
}

#[test]
fn test_delete_five_fields() {
    let mut storage = TestStorage {
        address: test_address(1),
        storage: HashMapStorageProvider::new(1),
    };
    let base_slot = U256::from(1000);

    let data = FiveFields {
        f1: U256::from(111),
        f2: U256::from(222),
        f3: U256::from(333),
        f4: U256::from(444),
        f5: U256::from(555),
    };

    // Store data
    data.store(&mut storage, base_slot).unwrap();

    // Delete
    FiveFields::delete(&mut storage, base_slot).unwrap();

    // Verify all 5 slots are zeroed
    for offset in 0..5 {
        assert_eq!(
            storage
                .storage
                .sload(storage.address, base_slot + U256::from(offset)),
            Ok(U256::ZERO)
        );
    }

    // Verify load returns default values
    let loaded = FiveFields::load(&mut storage, base_slot).unwrap();
    assert_eq!(
        loaded,
        FiveFields {
            f1: U256::ZERO,
            f2: U256::ZERO,
            f3: U256::ZERO,
            f4: U256::ZERO,
            f5: U256::ZERO,
        }
    );
}

#[test]
fn test_load_from_uninitialized_storage() {
    let mut storage = TestStorage {
        address: test_address(1),
        storage: HashMapStorageProvider::new(1),
    };
    let base_slot = U256::from(500);

    // Load from empty storage should return default values
    let single = SingleField::load(&mut storage, base_slot).unwrap();
    assert_eq!(single, SingleField { value: U256::ZERO });

    let three = ThreeFields::load(&mut storage, base_slot).unwrap();
    assert_eq!(
        three,
        ThreeFields {
            field1: U256::ZERO,
            field2: U256::ZERO,
            field3: 0,
        }
    );

    let mixed = MixedTypes::load(&mut storage, base_slot).unwrap();
    assert_eq!(
        mixed,
        MixedTypes {
            owner: Address::ZERO,
            active: false,
            balance: U256::ZERO,
            nonce: 0,
        }
    );
}

#[test]
fn test_idempotent_delete() {
    let mut storage = TestStorage {
        address: test_address(1),
        storage: HashMapStorageProvider::new(1),
    };
    let base_slot = U256::from(100);

    let data = ThreeFields {
        field1: U256::from(1000),
        field2: U256::from(2000),
        field3: 3000,
    };

    // Store data
    data.store(&mut storage, base_slot).unwrap();

    // Delete once
    ThreeFields::delete(&mut storage, base_slot).unwrap();

    // Verify slots are zeroed
    assert_eq!(
        storage.storage.sload(storage.address, base_slot),
        Ok(U256::ZERO)
    );
    assert_eq!(
        storage
            .storage
            .sload(storage.address, base_slot + U256::from(1)),
        Ok(U256::ZERO)
    );
    assert_eq!(
        storage
            .storage
            .sload(storage.address, base_slot + U256::from(2)),
        Ok(U256::ZERO)
    );

    // Delete again
    ThreeFields::delete(&mut storage, base_slot).unwrap();

    // Verify slots are still zeroed (idempotent)
    assert_eq!(
        storage.storage.sload(storage.address, base_slot),
        Ok(U256::ZERO)
    );
    assert_eq!(
        storage
            .storage
            .sload(storage.address, base_slot + U256::from(1)),
        Ok(U256::ZERO)
    );
    assert_eq!(
        storage
            .storage
            .sload(storage.address, base_slot + U256::from(2)),
        Ok(U256::ZERO)
    );

    // Delete a third time on already-zeroed storage
    ThreeFields::delete(&mut storage, base_slot).unwrap();

    // Still zeroed
    assert_eq!(
        storage.storage.sload(storage.address, base_slot),
        Ok(U256::ZERO)
    );
    assert_eq!(
        storage
            .storage
            .sload(storage.address, base_slot + U256::from(1)),
        Ok(U256::ZERO)
    );
    assert_eq!(
        storage
            .storage
            .sload(storage.address, base_slot + U256::from(2)),
        Ok(U256::ZERO)
    );
}

#[test]
fn test_partial_update_after_delete() {
    let mut storage = TestStorage {
        address: test_address(1),
        storage: HashMapStorageProvider::new(1),
    };
    let base_slot = U256::from(50);

    // Store initial data
    let initial = ThreeFields {
        field1: U256::from(100),
        field2: U256::from(200),
        field3: 300,
    };
    initial.store(&mut storage, base_slot).unwrap();

    // Delete
    ThreeFields::delete(&mut storage, base_slot).unwrap();

    // Store new data
    let new_data = ThreeFields {
        field1: U256::from(999),
        field2: U256::from(888),
        field3: 777,
    };
    new_data.store(&mut storage, base_slot).unwrap();

    // Load and verify new data
    let loaded = ThreeFields::load(&mut storage, base_slot).unwrap();
    assert_eq!(loaded, new_data);
}

#[test]
fn test_address_and_bool_encoding() {
    let mut storage = TestStorage {
        address: test_address(1),
        storage: HashMapStorageProvider::new(1),
    };
    let base_slot = U256::from(300);

    // Test with true
    let data_true = MixedTypes {
        owner: test_address(123),
        active: true,
        balance: U256::from(500),
        nonce: 10,
    };
    data_true.store(&mut storage, base_slot).unwrap();
    let loaded_true = MixedTypes::load(&mut storage, base_slot).unwrap();
    assert_eq!(loaded_true, data_true);
    assert!(loaded_true.active);

    // Test with false
    let data_false = MixedTypes {
        owner: test_address(99),
        active: false,
        balance: U256::from(700),
        nonce: 20,
    };
    data_false.store(&mut storage, base_slot).unwrap();
    let loaded_false = MixedTypes::load(&mut storage, base_slot).unwrap();
    assert_eq!(loaded_false, data_false);
    assert!(!loaded_false.active);
}
