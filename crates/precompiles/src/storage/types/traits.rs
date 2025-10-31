use alloy::primitives::{Address, B256, FixedBytes, U256};
use revm::interpreter::instructions::utility::{IntoAddress, IntoU256};

use crate::{
    error::{Result, TempoPrecompileError},
    storage::StorageOps,
};

/// Trait for types that can be stored/loaded from EVM storage.
///
/// This trait provides a flexible abstraction for reading and writing Rust types
/// to EVM storage. Types can occupy one or more consecutive storage slots, enabling
/// support for both simple values (Address, U256, bool) and complex multi-slot types
/// (structs, fixed arrays).
///
/// # Storage Layout
///
/// For a type with `SLOT_COUNT = 3` starting at `base_slot`:
/// - Slot 0: `base_slot + 0`
/// - Slot 1: `base_slot + 1`
/// - Slot 2: `base_slot + 2`
///
/// # Safety
///
/// Implementations must ensure that:
/// - Round-trip conversions preserve data: `load(store(x)) == Ok(x)`
/// - `SLOT_COUNT` accurately reflects the number of slots used
/// - `store` and `load` access exactly `SLOT_COUNT` consecutive slots
pub trait Storable: Sized {
    /// Number of consecutive storage slots this type occupies.
    ///
    /// For single-word types (Address, U256, bool), this is `1`.
    /// For multi-slot types (structs, arrays), this equals the number of fields/elements.
    const SLOT_COUNT: usize;

    /// Load this type from storage starting at the given base slot.
    ///
    /// Reads `SLOT_COUNT` consecutive slots starting from `base_slot`.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Storage read fails
    /// - Data cannot be decoded into this type
    fn load<S: StorageOps>(storage: &mut S, base_slot: U256) -> Result<Self>;

    /// Store this type to storage starting at the given base slot.
    ///
    /// Writes `SLOT_COUNT` consecutive slots starting from `base_slot`.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage write fails.
    fn store<S: StorageOps>(&self, storage: &mut S, base_slot: U256) -> Result<()>;

    /// Delete this type from storage (set all slots to zero).
    ///
    /// Sets `SLOT_COUNT` consecutive slots to zero, starting from `base_slot`.
    ///
    /// The default implementation sets each slot to zero individually.
    /// Types may override this for optimized bulk deletion.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage write fails.
    fn delete<S: StorageOps>(storage: &mut S, base_slot: U256) -> Result<()> {
        for offset in 0..Self::SLOT_COUNT {
            storage.sstore(base_slot + U256::from(offset), U256::ZERO)?;
        }
        Ok(())
    }
}

/// Trait for types that can be used as storage mapping keys.
///
/// Keys are hashed using keccak256 along with the mapping's base slot
/// to determine the final storage location. This trait provides the
/// byte representation used in that hash.
pub trait StorageKey {
    fn as_storage_bytes(&self) -> &[u8];
}

// -- STORAGE TYPE IMPLEMENTATIONS ---------------------------------------------

impl Storable for U256 {
    const SLOT_COUNT: usize = 1;

    #[inline]
    fn load<S: StorageOps>(storage: &mut S, base_slot: U256) -> Result<Self> {
        storage.sload(base_slot)
    }

    #[inline]
    fn store<S: StorageOps>(&self, storage: &mut S, base_slot: U256) -> Result<()> {
        storage.sstore(base_slot, *self)
    }
}

impl Storable for Address {
    const SLOT_COUNT: usize = 1;

    #[inline]
    fn load<S: StorageOps>(storage: &mut S, base_slot: U256) -> Result<Self> {
        let value = storage.sload(base_slot)?;
        Ok(value.into_address())
    }

    #[inline]
    fn store<S: StorageOps>(&self, storage: &mut S, base_slot: U256) -> Result<()> {
        storage.sstore(base_slot, self.into_u256())
    }
}

impl Storable for B256 {
    const SLOT_COUNT: usize = 1;

    #[inline]
    fn load<S: StorageOps>(storage: &mut S, base_slot: U256) -> Result<Self> {
        let value = storage.sload(base_slot)?;
        Ok(Self::from(value.to_be_bytes::<32>()))
    }

    #[inline]
    fn store<S: StorageOps>(&self, storage: &mut S, base_slot: U256) -> Result<()> {
        storage.sstore(base_slot, U256::from_be_bytes(self.0))
    }
}

impl Storable for bool {
    const SLOT_COUNT: usize = 1;

    #[inline]
    fn load<S: StorageOps>(storage: &mut S, base_slot: U256) -> Result<Self> {
        let value = storage.sload(base_slot)?;
        Ok(value != U256::ZERO)
    }

    #[inline]
    fn store<S: StorageOps>(&self, storage: &mut S, base_slot: U256) -> Result<()> {
        let value = if *self { U256::ONE } else { U256::ZERO };
        storage.sstore(base_slot, value)
    }
}

impl Storable for u64 {
    const SLOT_COUNT: usize = 1;

    #[inline]
    fn load<S: StorageOps>(storage: &mut S, base_slot: U256) -> Result<Self> {
        let value = storage.sload(base_slot)?;
        Ok(value.to::<Self>())
    }

    #[inline]
    fn store<S: StorageOps>(&self, storage: &mut S, base_slot: U256) -> Result<()> {
        storage.sstore(base_slot, U256::from(*self))
    }
}

impl Storable for u128 {
    const SLOT_COUNT: usize = 1;

    #[inline]
    fn load<S: StorageOps>(storage: &mut S, base_slot: U256) -> Result<Self> {
        let value = storage.sload(base_slot)?;
        Ok(value.to::<Self>())
    }

    #[inline]
    fn store<S: StorageOps>(&self, storage: &mut S, base_slot: U256) -> Result<()> {
        storage.sstore(base_slot, U256::from(*self))
    }
}

impl Storable for i16 {
    const SLOT_COUNT: usize = 1;

    #[inline]
    fn load<S: StorageOps>(storage: &mut S, base_slot: U256) -> Result<Self> {
        let value = storage.sload(base_slot)?;
        // Read as u16 then cast to i16 (preserves bit pattern)
        Ok(value.to::<u16>() as Self)
    }

    #[inline]
    fn store<S: StorageOps>(&self, storage: &mut S, base_slot: U256) -> Result<()> {
        // Cast to u16 to preserve bit pattern, then extend to U256
        storage.sstore(base_slot, U256::from(*self as u16))
    }
}

/// String storage using Solidity's short string optimization.
///
/// Strings up to 31 bytes are stored inline in a single slot with the format:
/// - Bytes 0..len: UTF-8 string data
/// - Byte 31: length * 2 (LSB indicates short string encoding)
///
/// Strings longer than 31 bytes are not currently supported and will panic.
impl Storable for String {
    const SLOT_COUNT: usize = 1;

    fn load<S: StorageOps>(storage: &mut S, base_slot: U256) -> Result<Self> {
        let value = storage.sload(base_slot)?;
        let bytes = value.to_be_bytes::<32>();
        let len = bytes[31] as usize / 2; // Length stored as len * 2

        if len > 31 {
            return Err(TempoPrecompileError::Fatal(
                "String too long for short string encoding".into(),
            ));
        }

        let utf8_bytes = &bytes[..len];
        Self::from_utf8(utf8_bytes.to_vec()).map_err(|e| {
            TempoPrecompileError::Fatal(format!("Invalid UTF-8 in stored string: {e}"))
        })
    }

    fn store<S: StorageOps>(&self, storage: &mut S, base_slot: U256) -> Result<()> {
        let bytes = self.as_bytes();

        if bytes.len() > 31 {
            return Err(TempoPrecompileError::Fatal(format!(
                "String too long for storage slot: {} bytes",
                bytes.len()
            )));
        }

        let mut storage_bytes = [0u8; 32];
        storage_bytes[..bytes.len()].copy_from_slice(bytes);
        storage_bytes[31] = (bytes.len() * 2) as u8; // Store length * 2

        storage.sstore(base_slot, U256::from_be_bytes(storage_bytes))
    }
}

// -- STORAGE KEY IMPLEMENTATIONS ---------------------------------------------

impl StorageKey for Address {
    #[inline]
    fn as_storage_bytes(&self) -> &[u8] {
        self.as_slice()
    }
}

impl StorageKey for U256 {
    #[inline]
    fn as_storage_bytes(&self) -> &[u8] {
        // U256 needs to be converted to bytes; we'll use a thread-local buffer
        // This is safe because the lifetime is tied to the borrow
        thread_local! {
            static BUFFER: std::cell::RefCell<[u8; 32]> = const { std::cell::RefCell::new([0u8; 32]) };
        }

        BUFFER.with(|buf| {
            let mut buffer = buf.borrow_mut();
            *buffer = self.to_be_bytes();
            // SAFETY: The buffer lives in TLS and we're returning a reference
            // that cannot outlive this function call. The caller must use it
            // immediately before any other code can access the TLS buffer.
            unsafe { std::slice::from_raw_parts(buffer.as_ptr(), 32) }
        })
    }
}

impl StorageKey for B256 {
    #[inline]
    fn as_storage_bytes(&self) -> &[u8] {
        self.as_slice()
    }
}

impl StorageKey for FixedBytes<4> {
    #[inline]
    fn as_storage_bytes(&self) -> &[u8] {
        self.as_slice()
    }
}

impl StorageKey for u64 {
    #[inline]
    fn as_storage_bytes(&self) -> &[u8] {
        thread_local! {
            static BUFFER: std::cell::RefCell<[u8; 8]> = const { std::cell::RefCell::new([0u8; 8]) };
        }

        BUFFER.with(|buf| {
            let mut buffer = buf.borrow_mut();
            *buffer = self.to_be_bytes();
            // SAFETY: The buffer lives in TLS and we're returning a reference
            // that cannot outlive this function call. The caller must use it
            // immediately before any other code can access the TLS buffer.
            unsafe { std::slice::from_raw_parts(buffer.as_ptr(), 8) }
        })
    }
}

impl StorageKey for u128 {
    #[inline]
    fn as_storage_bytes(&self) -> &[u8] {
        thread_local! {
            static BUFFER: std::cell::RefCell<[u8; 16]> = const { std::cell::RefCell::new([0u8; 16]) };
        }

        BUFFER.with(|buf| {
            let mut buffer = buf.borrow_mut();
            *buffer = self.to_be_bytes();
            // SAFETY: The buffer lives in TLS and we're returning a reference
            // that cannot outlive this function call. The caller must use it
            // immediately before any other code can access the TLS buffer.
            unsafe { std::slice::from_raw_parts(buffer.as_ptr(), 16) }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{PrecompileStorageProvider, hashmap::HashMapStorageProvider};
    use alloy::primitives::address;

    // Test helper that implements StorageOps
    struct TestContract<'a, S> {
        address: Address,
        storage: &'a mut S,
    }

    impl<'a, S: PrecompileStorageProvider> StorageOps for TestContract<'a, S> {
        fn sstore(&mut self, slot: U256, value: U256) -> Result<()> {
            self.storage.sstore(self.address, slot, value)
        }

        fn sload(&mut self, slot: U256) -> Result<U256> {
            self.storage.sload(self.address, slot)
        }
    }

    #[test]
    fn test_u256_round_trip() {
        let mut storage = HashMapStorageProvider::new(1);
        let addr = Address::random();
        let mut contract = TestContract {
            address: addr,
            storage: &mut storage,
        };

        let value = U256::from(12345u64);
        let slot = U256::from(0);

        value.store(&mut contract, slot).unwrap();
        let loaded = U256::load(&mut contract, slot).unwrap();
        assert_eq!(value, loaded);
    }

    #[test]
    fn test_address_round_trip() {
        let mut storage = HashMapStorageProvider::new(1);
        let contract_addr = Address::random();
        let mut contract = TestContract {
            address: contract_addr,
            storage: &mut storage,
        };

        let addr = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        let slot = U256::from(1);

        addr.store(&mut contract, slot).unwrap();
        let loaded = Address::load(&mut contract, slot).unwrap();
        assert_eq!(addr, loaded);
    }

    #[test]
    fn test_b256_round_trip() {
        let mut storage = HashMapStorageProvider::new(1);
        let addr = Address::random();
        let mut contract = TestContract {
            address: addr,
            storage: &mut storage,
        };

        let value = B256::from([0x42u8; 32]);
        let slot = U256::from(2);

        value.store(&mut contract, slot).unwrap();
        let loaded = B256::load(&mut contract, slot).unwrap();
        assert_eq!(value, loaded);
    }

    #[test]
    fn test_bool_conversions() {
        let mut storage = HashMapStorageProvider::new(1);
        let addr = Address::random();
        let mut contract = TestContract {
            address: addr,
            storage: &mut storage,
        };

        let slot = U256::from(3);

        // Test true
        true.store(&mut contract, slot).unwrap();
        assert!(bool::load(&mut contract, slot).unwrap());

        // Test false
        false.store(&mut contract, slot).unwrap();
        assert!(!bool::load(&mut contract, slot).unwrap());

        // Test that any non-zero value is true
        contract.storage.sstore(addr, slot, U256::from(42)).unwrap();
        assert!(bool::load(&mut contract, slot).unwrap());
    }

    #[test]
    fn test_u64_round_trip() {
        let mut storage = HashMapStorageProvider::new(1);
        let addr = Address::random();
        let mut contract = TestContract {
            address: addr,
            storage: &mut storage,
        };

        let value = u64::MAX;
        let slot = U256::from(4);

        value.store(&mut contract, slot).unwrap();
        let loaded = u64::load(&mut contract, slot).unwrap();
        assert_eq!(value, loaded);
    }

    #[test]
    fn test_u128_round_trip() {
        let mut storage = HashMapStorageProvider::new(1);
        let addr = Address::random();
        let mut contract = TestContract {
            address: addr,
            storage: &mut storage,
        };

        let value = u128::MAX;
        let slot = U256::from(5);

        value.store(&mut contract, slot).unwrap();
        let loaded = u128::load(&mut contract, slot).unwrap();
        assert_eq!(value, loaded);
    }

    #[test]
    fn test_i16_round_trip() {
        let mut storage = HashMapStorageProvider::new(1);
        let addr = Address::random();
        let mut contract = TestContract {
            address: addr,
            storage: &mut storage,
        };

        let slot = U256::from(6);

        // Positive value
        let pos = i16::MAX;
        pos.store(&mut contract, slot).unwrap();
        assert_eq!(i16::load(&mut contract, slot).unwrap(), pos);

        // Negative value (two's complement)
        let neg = i16::MIN;
        neg.store(&mut contract, slot).unwrap();
        assert_eq!(i16::load(&mut contract, slot).unwrap(), neg);

        // Zero
        let zero = 0i16;
        zero.store(&mut contract, slot).unwrap();
        assert_eq!(i16::load(&mut contract, slot).unwrap(), zero);
    }

    #[test]
    fn test_string_empty() {
        let mut storage = HashMapStorageProvider::new(1);
        let addr = Address::random();
        let mut contract = TestContract {
            address: addr,
            storage: &mut storage,
        };

        let s = String::new();
        let slot = U256::from(7);

        s.store(&mut contract, slot).unwrap();
        let loaded = String::load(&mut contract, slot).unwrap();
        assert_eq!(s, loaded);
    }

    #[test]
    fn test_string_short() {
        let mut storage = HashMapStorageProvider::new(1);
        let addr = Address::random();
        let mut contract = TestContract {
            address: addr,
            storage: &mut storage,
        };

        let s = "Hello, Tempo!".to_string();
        assert!(s.len() <= 31, "Test string must be <= 31 bytes");

        let slot = U256::from(8);
        s.store(&mut contract, slot).unwrap();
        let loaded = String::load(&mut contract, slot).unwrap();
        assert_eq!(s, loaded);
    }

    #[test]
    fn test_string_max_length() {
        let mut storage = HashMapStorageProvider::new(1);
        let addr = Address::random();
        let mut contract = TestContract {
            address: addr,
            storage: &mut storage,
        };

        // 31 bytes is the maximum for short string encoding
        let s = "a".repeat(31);
        assert_eq!(s.len(), 31);

        let slot = U256::from(9);
        s.store(&mut contract, slot).unwrap();
        let loaded = String::load(&mut contract, slot).unwrap();
        assert_eq!(s, loaded);
    }

    #[test]
    fn test_string_too_long_errors() {
        let mut storage = HashMapStorageProvider::new(1);
        let addr = Address::random();
        let mut contract = TestContract {
            address: addr,
            storage: &mut storage,
        };

        let s = "a".repeat(32); // 32 bytes > 31 byte limit
        let slot = U256::from(10);

        let result = s.store(&mut contract, slot);
        assert!(result.is_err());
    }

    #[test]
    fn test_string_unicode() {
        let mut storage = HashMapStorageProvider::new(1);
        let addr = Address::random();
        let mut contract = TestContract {
            address: addr,
            storage: &mut storage,
        };

        let s = "Hello 世界 🌍".to_string();
        assert!(s.len() <= 31, "Test string too long");

        let slot = U256::from(11);
        s.store(&mut contract, slot).unwrap();
        let loaded = String::load(&mut contract, slot).unwrap();
        assert_eq!(s, loaded);
    }

    #[test]
    fn test_string_storage_format() {
        let mut storage = HashMapStorageProvider::new(1);
        let addr = Address::random();
        let mut contract = TestContract {
            address: addr,
            storage: &mut storage,
        };

        let s = "test".to_string(); // 4 bytes
        let slot = U256::from(12);

        s.store(&mut contract, slot).unwrap();
        let raw_value = contract.storage.sload(addr, slot).unwrap();
        let bytes = raw_value.to_be_bytes::<32>();

        // Check first 4 bytes contain "test"
        assert_eq!(&bytes[0..4], b"test");

        // Check rest is zeros
        assert!(bytes[4..31].iter().all(|&b| b == 0));

        // Check length byte: 4 * 2 = 8
        assert_eq!(bytes[31], 8);
    }

    #[test]
    fn test_address_as_storage_bytes() {
        let addr = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        let bytes = addr.as_storage_bytes();
        assert_eq!(bytes.len(), 20);
        assert_eq!(bytes, addr.as_slice());
    }

    #[test]
    fn test_u256_as_storage_bytes() {
        let value = U256::from(0x123456789abcdef_u64);
        let bytes = value.as_storage_bytes();
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn test_b256_as_storage_bytes() {
        let value = B256::from([0x42u8; 32]);
        let bytes = value.as_storage_bytes();
        assert_eq!(bytes.len(), 32);
        assert_eq!(bytes, value.as_slice());
    }

    #[test]
    fn test_u64_as_storage_bytes() {
        let value = 0x123456789abcdef_u64;
        let bytes = value.as_storage_bytes();
        assert_eq!(bytes.len(), 8);
        assert_eq!(bytes, &value.to_be_bytes());
    }
}
