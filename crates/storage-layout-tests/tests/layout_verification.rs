//! Integration tests for verifying Rust storage layouts match Solidity.

use alloy_primitives::{Address, U256};
use std::path::PathBuf;
use storage_layout_tests::{
    compare_layouts, compare_struct_members, load_solc_layout, RustStorageField,
};
use tempo_precompiles::{error, storage::Storable};

// Re-export storage module so tests can use it
mod storage {
    pub(super) use tempo_precompiles::storage::*;
}

use tempo_precompiles_macros::{contract, Storable};

// Helper struct for struct test (defined at module level)
#[derive(Debug, Clone, PartialEq, Eq, Storable)]
struct TestBlockInner {
    field1: U256,
    field2: U256,
    field3: u64,
}

fn testdata_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(filename)
}

#[test]
fn test_basic_types_layout() {
    #[contract]
    struct BasicTypes {
        field_a: U256,
        field_b: Address,
        field_c: bool,
        field_d: u64,
    }

    // Load expected layout from Solidity
    let solc_layout = load_solc_layout(&testdata_path("basic_types.layout.json"))
        .unwrap_or_else(|e| panic!("Failed to load expected layout: {}", e));

    // Extract Rust field slots
    let rust_fields = vec![
        RustStorageField {
            name: "field_a",
            slot: slots::FIELD_A,
            offset: slots::FIELD_A_OFFSET,
            bytes: slots::FIELD_A_BYTES,
        },
        RustStorageField {
            name: "field_b",
            slot: slots::FIELD_B,
            offset: slots::FIELD_B_OFFSET,
            bytes: slots::FIELD_B_BYTES,
        },
        RustStorageField {
            name: "field_c",
            slot: slots::FIELD_C,
            offset: slots::FIELD_C_OFFSET,
            bytes: slots::FIELD_C_BYTES,
        },
        RustStorageField {
            name: "field_d",
            slot: slots::FIELD_D,
            offset: slots::FIELD_D_OFFSET,
            bytes: slots::FIELD_D_BYTES,
        },
    ];

    // Compare layouts
    if let Err(errors) = compare_layouts(&solc_layout, &rust_fields) {
        panic!("Layout mismatch:\n{}", errors.join("\n"));
    }
}

#[test]
fn test_mixed_slots_layout() {
    #[contract]
    struct MixedSlots {
        field_a: U256,
        field_c: U256,
    }

    let expected_path = testdata_path("mixed_slots.layout.json");
    let solc_layout = load_solc_layout(&expected_path)
        .unwrap_or_else(|e| panic!("Failed to load expected layout: {}", e));

    let rust_fields = vec![
        RustStorageField {
            name: "field_a",
            slot: slots::FIELD_A,
            offset: slots::FIELD_A_OFFSET,
            bytes: slots::FIELD_A_BYTES,
        },
        RustStorageField {
            name: "field_c",
            slot: slots::FIELD_C,
            offset: slots::FIELD_C_OFFSET,
            bytes: slots::FIELD_C_BYTES,
        },
    ];

    if let Err(errors) = compare_layouts(&solc_layout, &rust_fields) {
        panic!("Layout mismatch:\n{}", errors.join("\n"));
    }
}

#[test]
fn test_arrays_layout() {
    #[contract]
    struct Arrays {
        field_a: U256,
        large_array: [U256; 5],
        field_b: U256,
    }

    let expected_path = testdata_path("arrays.layout.json");
    let solc_layout = load_solc_layout(&expected_path)
        .unwrap_or_else(|e| panic!("Failed to load expected layout: {}", e));

    let rust_fields = vec![
        RustStorageField {
            name: "field_a",
            slot: slots::FIELD_A,
            offset: slots::FIELD_A_OFFSET,
            bytes: slots::FIELD_A_BYTES,
        },
        RustStorageField {
            name: "large_array",
            slot: slots::LARGE_ARRAY,
            offset: slots::LARGE_ARRAY_OFFSET,
            bytes: slots::LARGE_ARRAY_BYTES,
        },
        RustStorageField {
            name: "field_b",
            slot: slots::FIELD_B,
            offset: slots::FIELD_B_OFFSET,
            bytes: slots::FIELD_B_BYTES,
        },
    ];

    if let Err(errors) = compare_layouts(&solc_layout, &rust_fields) {
        panic!("Layout mismatch:\n{}", errors.join("\n"));
    }
}

#[test]
fn test_mappings_layout() {
    #[contract]
    struct Mappings {
        field_a: U256,
        address_mapping: storage::Mapping<Address, U256, SlotDummy>,
        uint_mapping: storage::Mapping<u64, U256, SlotDummy>,
    }

    let expected_path = testdata_path("mappings.layout.json");
    let solc_layout = load_solc_layout(&expected_path)
        .unwrap_or_else(|e| panic!("Failed to load expected layout: {}", e));

    let rust_fields = vec![
        RustStorageField {
            name: "field_a",
            slot: slots::FIELD_A,
            offset: slots::FIELD_A_OFFSET,
            bytes: slots::FIELD_A_BYTES,
        },
        RustStorageField {
            name: "address_mapping",
            slot: slots::ADDRESS_MAPPING,
            offset: slots::ADDRESS_MAPPING_OFFSET,
            bytes: slots::ADDRESS_MAPPING_BYTES,
        },
        RustStorageField {
            name: "uint_mapping",
            slot: slots::UINT_MAPPING,
            offset: slots::UINT_MAPPING_OFFSET,
            bytes: slots::UINT_MAPPING_BYTES,
        },
    ];

    if let Err(errors) = compare_layouts(&solc_layout, &rust_fields) {
        panic!("Layout mismatch:\n{}", errors.join("\n"));
    }
}

// Test struct storage layout including individual struct member verification
#[test]
fn test_structs_layout() {
    use crate::__packing_test_block_inner::*;

    #[contract]
    struct Structs {
        field_a: U256,
        block_data: TestBlockInner,
        field_b: U256,
    }

    let expected_path = testdata_path("structs.layout.json");
    let solc_layout = load_solc_layout(&expected_path)
        .unwrap_or_else(|e| panic!("Failed to load expected layout: {}", e));

    // Verify top-level fields
    let rust_fields = vec![
        RustStorageField {
            name: "field_a",
            slot: slots::FIELD_A,
            offset: slots::FIELD_A_OFFSET,
            bytes: slots::FIELD_A_BYTES,
        },
        RustStorageField {
            name: "block_data",
            slot: slots::BLOCK_DATA,
            offset: slots::BLOCK_DATA_OFFSET,
            bytes: slots::BLOCK_DATA_BYTES,
        },
        RustStorageField {
            name: "field_b",
            slot: slots::FIELD_B,
            offset: slots::FIELD_B_OFFSET,
            bytes: slots::FIELD_B_BYTES,
        },
    ];

    if let Err(errors) = compare_layouts(&solc_layout, &rust_fields) {
        panic!("Layout mismatch:\n{}", errors.join("\n"));
    }

    // Verify struct member slots
    let base_slot = slots::BLOCK_DATA;
    let struct_member_slots = vec![
        RustStorageField {
            name: "field1",
            slot: base_slot + U256::from(FIELD_0_SLOT),
            offset: FIELD_0_OFFSET,
            bytes: FIELD_0_BYTES,
        },
        RustStorageField {
            name: "field2",
            slot: base_slot + U256::from(FIELD_1_SLOT),
            offset: FIELD_1_OFFSET,
            bytes: FIELD_1_BYTES,
        },
        RustStorageField {
            name: "field3",
            slot: base_slot + U256::from(FIELD_2_SLOT),
            offset: FIELD_2_OFFSET,
            bytes: FIELD_2_BYTES,
        },
    ];

    if let Err(errors) = compare_struct_members(&solc_layout, "block_data", &struct_member_slots) {
        panic!("Struct member layout mismatch:\n{}", errors.join("\n"));
    }
}

#[test]
fn test_double_mappings_layout() {
    use alloy_primitives::FixedBytes;

    #[contract]
    struct DoubleMappings {
        field_a: U256,
        account_role:
            storage::Mapping<Address, storage::Mapping<FixedBytes<32>, bool, SlotDummy>, SlotDummy>,
        allowances:
            storage::Mapping<Address, storage::Mapping<Address, U256, SlotDummy>, SlotDummy>,
    }

    let expected_path = testdata_path("double_mappings.layout.json");
    let solc_layout = load_solc_layout(&expected_path)
        .unwrap_or_else(|e| panic!("Failed to load expected layout: {}", e));

    let rust_fields = vec![
        RustStorageField {
            name: "field_a",
            slot: slots::FIELD_A,
            offset: slots::FIELD_A_OFFSET,
            bytes: slots::FIELD_A_BYTES,
        },
        RustStorageField {
            name: "account_role",
            slot: slots::ACCOUNT_ROLE,
            offset: slots::ACCOUNT_ROLE_OFFSET,
            bytes: slots::ACCOUNT_ROLE_BYTES,
        },
        RustStorageField {
            name: "allowances",
            slot: slots::ALLOWANCES,
            offset: slots::ALLOWANCES_OFFSET,
            bytes: slots::ALLOWANCES_BYTES,
        },
    ];

    if let Err(errors) = compare_layouts(&solc_layout, &rust_fields) {
        panic!("Layout mismatch:\n{}", errors.join("\n"));
    }
}
