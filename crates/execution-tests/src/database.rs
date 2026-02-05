//! Database seeding for test vectors.
//!
//! This module provides utilities to create an in-memory CacheDB
//! populated with the prestate defined in a test vector.
//!
//! The seeding system uses `FieldMetadata.seed` function pointers from the
//! precompile resolver. Each field type has its own compile-time generated
//! seed function. Struct types with custom encoding (`AuthorizedKey`, `PolicyData`)
//! are handled via field-specific overrides.

use crate::vector::{PrecompileState, Prestate};
use alloy_primitives::{Address, B256, Bytes, U256};
use revm::{
    DatabaseRef,
    database::{CacheDB, EmptyDB},
    primitives::KECCAK_EMPTY,
    state::{AccountInfo, Bytecode},
};
use tempo_precompiles::{
    account_keychain::AuthorizedKey,
    resolver::{FieldMetadata, SeedError, metadata_for},
    tip403_registry::PolicyData,
};

/// Marker bytecode for precompile accounts (invalid opcode, won't execute).
/// TIP20 tokens check `is_initialized()` which requires non-empty code hash.
const PRECOMPILE_MARKER_BYTECODE: u8 = 0xEF;

/// A database seeded from a test vector's prestate.
pub struct VectorDatabase {
    /// The underlying CacheDB
    pub db: CacheDB<EmptyDB>,
}

impl VectorDatabase {
    /// Create a new database from a prestate definition.
    pub fn from_prestate(prestate: &Prestate) -> eyre::Result<Self> {
        let mut db = CacheDB::new(EmptyDB::default());

        // Insert accounts (balance, nonce)
        for (address, account) in &prestate.accounts {
            let code_hash = prestate
                .code
                .get(address)
                .map(hash_bytes)
                .unwrap_or(KECCAK_EMPTY);

            let info = AccountInfo {
                balance: U256::ZERO,
                nonce: account.nonce,
                code_hash,
                code: prestate
                    .code
                    .get(address)
                    .map(|c| Bytecode::new_raw(c.clone())),
                ..Default::default()
            };

            db.insert_account_info(*address, info);
        }

        // Insert code for addresses not in accounts
        for (address, code) in &prestate.code {
            if !prestate.accounts.contains_key(address) {
                let info = AccountInfo {
                    balance: U256::ZERO,
                    nonce: 0,
                    code_hash: hash_bytes(code),
                    code: Some(Bytecode::new_raw(code.clone())),
                    ..Default::default()
                };
                db.insert_account_info(*address, info);
            }
        }

        // Insert storage
        for (address, slots) in &prestate.storage {
            // Ensure account exists
            if !prestate.accounts.contains_key(address) && !prestate.code.contains_key(address) {
                db.insert_account_info(*address, AccountInfo::default());
            }

            for (slot, value) in slots {
                db.insert_account_storage(*address, *slot, *value)?;
            }
        }

        let mut vector_db = Self { db };
        vector_db.seed_precompiles(&prestate.precompiles)?;
        Ok(vector_db)
    }

    /// Seeds precompile state from test vector definitions.
    fn seed_precompiles(&mut self, precompiles: &[PrecompileState]) -> eyre::Result<()> {
        for precompile in precompiles {
            self.seed_precompile(precompile)?;
        }
        Ok(())
    }

    /// Seeds a single precompile's state using the generic field seeder.
    fn seed_precompile(&mut self, precompile: &PrecompileState) -> eyre::Result<()> {
        let address = precompile.address;
        let contract = &precompile.name;

        // Ensure the precompile account exists with bytecode.
        let marker_code = Bytecode::new_raw(Bytes::from_static(&[PRECOMPILE_MARKER_BYTECODE]));
        let info = AccountInfo {
            code_hash: alloy_primitives::keccak256([PRECOMPILE_MARKER_BYTECODE]),
            code: Some(marker_code),
            ..Default::default()
        };
        self.db.insert_account_info(address, info);

        let fields = precompile
            .fields
            .as_object()
            .ok_or_else(|| eyre::eyre!("fields must be an object"))?;

        for (field_name, value) in fields {
            self.seed_field(address, contract, field_name, value)?;
        }

        Ok(())
    }

    /// Generic entry point for seeding a single field.
    /// Handles scalars, mappings (nested or flat), and arrays based on JSON shape.
    fn seed_field(
        &mut self,
        address: Address,
        contract: &str,
        field: &str,
        value: &serde_json::Value,
    ) -> eyre::Result<()> {
        // Check if this is a mapping by trying to get metadata without keys
        let base_meta = metadata_for(contract, field, &[]);

        match base_meta {
            Ok(meta) if meta.is_mapping => {
                // This is a mapping field - traverse the JSON object
                self.seed_mapping(address, contract, field, value, vec![], meta.nesting_depth)?;
            }
            Ok(_) => {
                // Non-mapping field - seed as scalar
                self.seed_scalar(address, contract, field, &[], value)?;
            }
            Err(tempo_precompiles::resolver::ResolverError::MissingKey(_)) => {
                // Needs keys - it's a mapping
                let meta =
                    metadata_for(contract, field, &["0x0000000000000000000000000000000000000000"])?;
                self.seed_mapping(address, contract, field, value, vec![], meta.nesting_depth)?;
            }
            Err(e) => return Err(eyre::eyre!("field resolution failed: {}", e)),
        }

        Ok(())
    }

    /// Recursively traverses a mapping's JSON object and seeds each leaf value.
    fn seed_mapping(
        &mut self,
        address: Address,
        contract: &str,
        field: &str,
        value: &serde_json::Value,
        keys: Vec<String>,
        remaining_depth: u8,
    ) -> eyre::Result<()> {
        let map = value
            .as_object()
            .ok_or_else(|| eyre::eyre!("{} must be an object", field))?;

        for (key, inner_value) in map {
            let mut new_keys = keys.clone();
            new_keys.push(key.clone());

            if remaining_depth > 1 && inner_value.is_object() {
                // More nesting levels to go
                self.seed_mapping(
                    address,
                    contract,
                    field,
                    inner_value,
                    new_keys,
                    remaining_depth - 1,
                )?;
            } else {
                // Leaf value - seed as scalar
                let key_refs: Vec<&str> = new_keys.iter().map(|s| s.as_str()).collect();
                self.seed_scalar(address, contract, field, &key_refs, inner_value)?;
            }
        }

        Ok(())
    }

    /// Seeds a scalar value at a specific storage slot.
    ///
    /// Uses `meta.seed()` to encode the JSON value, with overrides for struct types
    /// that have custom encoding (`AuthorizedKey`, `PolicyData`).
    fn seed_scalar(
        &mut self,
        address: Address,
        contract: &str,
        field: &str,
        keys: &[&str],
        value: &serde_json::Value,
    ) -> eyre::Result<()> {
        let meta = metadata_for(contract, field, keys)?;

        // Try to use the generated seed function, with overrides for struct types
        let word = self.encode_value(contract, field, &meta, value)?;

        // Handle packing for fields that don't occupy a full slot
        let final_value = if meta.offset > 0 || meta.bytes < 32 {
            let current = self.db.storage_ref(address, meta.slot).unwrap_or(U256::ZERO);
            pack_word(current, word, meta.offset, meta.bytes)?
        } else {
            word
        };

        self.db
            .insert_account_storage(address, meta.slot, final_value)?;
        Ok(())
    }

    /// Encodes a JSON value to a U256 word.
    ///
    /// Uses the `meta.seed` function for primitives, with overrides for struct types.
    fn encode_value(
        &self,
        contract: &str,
        field: &str,
        meta: &FieldMetadata,
        value: &serde_json::Value,
    ) -> eyre::Result<U256> {
        // Try the generated seed function first
        match (meta.seed)(value) {
            Ok(word) => Ok(word),
            Err(SeedError::Parse(msg)) if msg.contains("struct types require") => {
                // Fallback to struct-specific encoders
                self.encode_struct(contract, field, value)
            }
            Err(e) => Err(eyre::eyre!("seed error for {}.{}: {}", contract, field, e)),
        }
    }

    /// Encodes struct types that have custom `encode_to_slot()` methods.
    fn encode_struct(
        &self,
        contract: &str,
        field: &str,
        value: &serde_json::Value,
    ) -> eyre::Result<U256> {
        let obj = value
            .as_object()
            .ok_or_else(|| eyre::eyre!("{}.{} must be an object", contract, field))?;

        match (contract, field) {
            ("AccountKeychain", "keys") => encode_authorized_key(obj),
            ("TIP403Registry", "policy_records") => encode_policy_data(obj),
            _ => Err(eyre::eyre!(
                "no custom encoder for struct field {}.{}",
                contract,
                field
            )),
        }
    }

    /// Get a reference to the underlying database.
    pub fn inner(&self) -> &CacheDB<EmptyDB> {
        &self.db
    }

    /// Read a storage slot from the database.
    pub fn storage(&self, address: Address, slot: U256) -> eyre::Result<U256> {
        self.db
            .storage_ref(address, slot)
            .map_err(|e| eyre::eyre!("storage read failed: {:?}", e))
    }

    /// Read account info from the database.
    pub fn account(&self, address: Address) -> eyre::Result<Option<AccountInfo>> {
        self.db
            .basic_ref(address)
            .map_err(|e| eyre::eyre!("account read failed: {:?}", e))
    }
}

/// Compute keccak256 hash of bytes
fn hash_bytes(data: &Bytes) -> B256 {
    alloy_primitives::keccak256(data)
}

/// Pack a word into a slot at the given offset.
fn pack_word(current: U256, word: U256, offset: usize, bytes: usize) -> eyre::Result<U256> {
    use tempo_precompiles::storage::packing::create_element_mask;

    let shift_bits = offset * 8;
    let mask = create_element_mask(bytes);

    // Clear the bits for this field in the current slot value
    let clear_mask = !(mask << shift_bits);
    let cleared = current & clear_mask;

    // Position the new value and combine with cleared slot
    let positioned = (word & mask) << shift_bits;
    Ok(cleared | positioned)
}

/// Encodes an AuthorizedKey struct from JSON into a packed U256.
fn encode_authorized_key(info: &serde_json::Map<String, serde_json::Value>) -> eyre::Result<U256> {
    let signature_type = info
        .get("signature_type")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u8;

    let expiry = info
        .get("expiry")
        .map(parse_u64_value)
        .transpose()?
        .unwrap_or(0);

    let enforce_limits = info
        .get("enforce_limits")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let is_revoked = info
        .get("is_revoked")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let key = AuthorizedKey {
        signature_type,
        expiry,
        enforce_limits,
        is_revoked,
    };

    Ok(key.encode_to_slot())
}

/// Encodes a PolicyData struct from JSON into a packed U256.
fn encode_policy_data(info: &serde_json::Map<String, serde_json::Value>) -> eyre::Result<U256> {
    let policy_type = info
        .get("policy_type")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u8;

    let admin_str = info
        .get("admin")
        .and_then(|v| v.as_str())
        .unwrap_or("0x0000000000000000000000000000000000000000");
    let admin: Address = admin_str.parse()?;

    let data = PolicyData { policy_type, admin };

    Ok(data.encode_to_slot())
}

/// Parses a JSON value as u64 (accepts number or decimal/hex string).
fn parse_u64_value(value: &serde_json::Value) -> eyre::Result<u64> {
    if let Some(n) = value.as_u64() {
        return Ok(n);
    }
    let s = value
        .as_str()
        .ok_or_else(|| eyre::eyre!("expected number or string for u64"))?;
    if s.starts_with("0x") || s.starts_with("0X") {
        u64::from_str_radix(&s[2..], 16).map_err(|e| eyre::eyre!("invalid hex u64: {}", e))
    } else {
        s.parse()
            .map_err(|e| eyre::eyre!("invalid decimal u64: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::AccountState;
    use alloy_primitives::{address, uint};
    use std::collections::BTreeMap;
    use tempo_contracts::precompiles::DEFAULT_FEE_TOKEN;

    #[test]
    fn test_empty_prestate() {
        let prestate = Prestate::default();
        let db = VectorDatabase::from_prestate(&prestate).unwrap();
        assert!(db.db.cache.accounts.is_empty());
    }

    #[test]
    fn test_account_seeding() {
        let mut prestate = Prestate::default();
        let addr = address!("1111111111111111111111111111111111111111");
        prestate.accounts.insert(addr, AccountState { nonce: 5 });

        let db = VectorDatabase::from_prestate(&prestate).unwrap();
        let account = db.account(addr).unwrap().unwrap();

        assert_eq!(account.balance, U256::ZERO);
        assert_eq!(account.nonce, 5);
    }

    #[test]
    fn test_storage_seeding() {
        let mut prestate = Prestate::default();
        let addr = address!("2222222222222222222222222222222222222222");

        let mut slots = BTreeMap::new();
        slots.insert(U256::from(0), U256::from(42));
        slots.insert(U256::from(1), U256::from(100));
        prestate.storage.insert(addr, slots);

        let db = VectorDatabase::from_prestate(&prestate).unwrap();

        assert_eq!(db.storage(addr, U256::from(0)).unwrap(), U256::from(42));
        assert_eq!(db.storage(addr, U256::from(1)).unwrap(), U256::from(100));
        assert_eq!(db.storage(addr, U256::from(2)).unwrap(), U256::ZERO);
    }

    #[test]
    fn test_code_seeding() {
        let mut prestate = Prestate::default();
        let addr = address!("3333333333333333333333333333333333333333");
        let code = Bytes::from(vec![0x60, 0x00, 0x60, 0x00, 0xf3]); // PUSH 0, PUSH 0, RETURN

        prestate.code.insert(addr, code);

        let db = VectorDatabase::from_prestate(&prestate).unwrap();
        let account = db.account(addr).unwrap().unwrap();

        assert_ne!(account.code_hash, KECCAK_EMPTY);
        assert!(account.code.is_some());
    }

    #[test]
    fn test_precompile_seeding() {
        let mut prestate = Prestate::default();

        let sender = address!("abcdef0000000000000000000000000000000001");
        prestate.precompiles.push(PrecompileState {
            name: "TIP20Token".to_string(),
            address: DEFAULT_FEE_TOKEN,
            fields: serde_json::json!({
                "currency": "USD",
                "transfer_policy_id": 1,
                "balances": {
                    "0xabcdef0000000000000000000000000000000001": "1000000000000"
                }
            }),
        });

        let db = VectorDatabase::from_prestate(&prestate).unwrap();

        // Check that fee token has USD currency set
        let currency_slot = metadata_for("TIP20Token", "currency", &[]).unwrap().slot;
        let currency = db.storage(DEFAULT_FEE_TOKEN, currency_slot).unwrap();
        let expected_usd =
            uint!(0x5553440000000000000000000000000000000000000000000000000000000006_U256);
        assert_eq!(currency, expected_usd);

        // Check that sender has balance in fee token
        let sender_str = format!("{sender:?}");
        let balance_slot = metadata_for("TIP20Token", "balances", &[&sender_str])
            .unwrap()
            .slot;
        let balance = db.storage(DEFAULT_FEE_TOKEN, balance_slot).unwrap();
        assert_eq!(balance, U256::from(1_000_000_000_000u64));
    }

    #[test]
    fn test_transfer_policy_id_packing() {
        let mut prestate = Prestate::default();

        prestate.precompiles.push(PrecompileState {
            name: "TIP20Token".to_string(),
            address: DEFAULT_FEE_TOKEN,
            fields: serde_json::json!({
                "transfer_policy_id": 1
            }),
        });

        let db = VectorDatabase::from_prestate(&prestate).unwrap();

        let slot = metadata_for("TIP20Token", "transfer_policy_id", &[])
            .unwrap()
            .slot;
        let value = db.storage(DEFAULT_FEE_TOKEN, slot).unwrap();

        // Policy ID = 1 shifted left by 160 bits
        let expected =
            uint!(0x0000000000000000000000010000000000000000000000000000000000000000_U256);
        assert_eq!(value, expected);
    }
}
