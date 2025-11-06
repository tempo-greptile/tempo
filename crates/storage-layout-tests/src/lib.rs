//! Storage layout testing utilities for comparing Solidity and Rust layouts.
//!
//! This crate provides infrastructure for verifying that Rust `#[contract]` macro
//! generates storage layouts that match Solidity's storage layout rules.

use alloy_primitives::U256;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Represents the full compiler output.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SolcOutput {
    contracts: HashMap<String, ContractOutput>,
    #[serde(default)]
    version: Option<String>,
}

/// Represents the full compiler output for a given contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContractOutput {
    #[serde(rename = "storage-layout")]
    storage_layout: StorageLayout,
}

/// Represents the storage layout for a contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageLayout {
    pub storage: Vec<StorageVariable>,
    pub types: HashMap<String, TypeDefinition>,
}

/// Represents a storage layout variable from solc's JSON output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageVariable {
    /// Contract name
    pub contract: String,
    /// Variable name
    pub label: String,
    /// Storage slot number
    pub slot: String,
    /// Byte offset within the storage slot
    pub offset: u64,
    /// Solidity type string: "t_uint256", "t_struct$_Block_$123_storage"
    #[serde(rename = "type")]
    pub ty: String,
}

/// Represents a type definition from Solidity compiler output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TypeDefinition {
    /// Encoding type: "inplace", "mapping", "dynamic_array"
    pub encoding: String,

    /// Human-readable label
    pub label: String,

    /// Number of bytes this type occupies
    #[serde(rename = "numberOfBytes")]
    pub number_of_bytes: String,

    /// Base type for arrays/mappings
    #[serde(default)]
    pub base: Option<String>,

    /// Key type for mappings
    #[serde(default)]
    pub key: Option<String>,

    /// Value type for mappings
    #[serde(default)]
    pub value: Option<String>,

    /// Struct members
    #[serde(default)]
    pub members: Option<Vec<StorageVariable>>,
}

/// Extracts storage layout from a Solidity file using solc.
pub fn extract_solc_layout(sol_file: &Path) -> Result<StorageLayout, String> {
    // Run solc with storage-layout output
    let output = Command::new("solc")
        .arg("--combined-json")
        .arg("storage-layout")
        .arg(sol_file)
        .output()
        .map_err(|e| format!("Failed to run solc: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "solc failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let solc_output: SolcOutput =
        serde_json::from_str(&stdout).map_err(|e| format!("Failed to parse solc output: {}", e))?;

    // Extract the first contract's storage layout
    let layout = solc_output
        .contracts
        .values()
        .next()
        .map(|contract| contract.storage_layout.clone())
        .ok_or_else(|| "No contracts found in solc output".to_string())?;

    Ok(layout)
}

/// Loads a expected storage layout file from disk.
pub fn load_solc_layout(json_file: &Path) -> Result<StorageLayout, String> {
    let content = std::fs::read_to_string(json_file)
        .map_err(|e| format!("Failed to read expected file: {}", e))?;
    let solc_output: SolcOutput = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse expected layout JSON: {}", e))?;

    // Extract the first contract's storage layout
    let layout = solc_output
        .contracts
        .values()
        .next()
        .map(|contract| contract.storage_layout.clone())
        .ok_or_else(|| "No contracts found in expected layout".to_string())?;

    Ok(layout)
}

/// Saves a storage layout as a expected file.
pub fn save_expected_layout(layout: &StorageLayout, json_file: &Path) -> Result<(), String> {
    let json = serde_json::to_string_pretty(layout)
        .map_err(|e| format!("Failed to serialize layout: {}", e))?;

    std::fs::write(json_file, json).map_err(|e| format!("Failed to write expected file: {}", e))
}

/// Discovers all `.sol` test files in the testdata directory.
pub fn discover_test_cases(testdata_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut sol_files = Vec::new();

    let entries = std::fs::read_dir(testdata_dir)
        .map_err(|e| format!("Failed to read testdata directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("sol") {
            sol_files.push(path);
        }
    }

    sol_files.sort();
    Ok(sol_files)
}

/// Represents a Rust storage field extracted from generated constants.
#[derive(Debug, Clone, PartialEq)]
pub struct RustStorageField {
    pub name: &'static str,
    pub slot: U256,
    pub offset: usize,
    pub bytes: usize,
}

/// Helper to convert Solidity slot string to U256.
pub fn parse_slot(slot_str: &str) -> Result<U256, String> {
    U256::from_str_radix(slot_str, 10)
        .map_err(|e| format!("Failed to parse slot '{}': {}", slot_str, e))
}

/// Compares two storage layouts and returns detailed differences.
pub fn compare_layouts(
    solc_layout: &StorageLayout,
    rust_fields: &[RustStorageField],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    // Build a map of Solidity field names to their storage info
    let solc_fields: HashMap<String, (&StorageVariable, U256)> = solc_layout
        .storage
        .iter()
        .filter_map(|var| {
            parse_slot(&var.slot)
                .ok()
                .map(|slot| (var.label.clone(), (var, slot)))
        })
        .collect();

    // Check that all Rust fields match Solidity fields
    for rust_field in rust_fields {
        match solc_fields.get(rust_field.name) {
            Some((solc_var, solc_slot)) => {
                // Compare slot
                if *solc_slot != rust_field.slot {
                    errors.push(format!(
                        "Field '{}': Solidity slot {} != Rust slot {}",
                        rust_field.name, solc_slot, rust_field.slot
                    ));
                }

                // Compare offset
                if solc_var.offset as usize != rust_field.offset {
                    errors.push(format!(
                        "Field '{}': Solidity offset {} != Rust offset {}",
                        rust_field.name, solc_var.offset, rust_field.offset
                    ));
                }

                // Compare bytes
                // Solidity stores number_of_bytes in the type definition
                if let Some(type_def) = solc_layout.types.get(&solc_var.ty) {
                    if let Ok(solc_bytes) = type_def.number_of_bytes.parse::<usize>() {
                        if solc_bytes != rust_field.bytes {
                            errors.push(format!(
                                "Field '{}': Solidity bytes {} != Rust bytes {}",
                                rust_field.name, solc_bytes, rust_field.bytes
                            ));
                        }
                    }
                }
            }
            None => {
                errors.push(format!(
                    "Field '{}' exists in Rust but not in Solidity layout",
                    rust_field.name
                ));
            }
        }
    }

    // Check for Solidity fields missing in Rust
    for (solc_field_name, _) in &solc_fields {
        if !rust_fields.iter().any(|rf| &rf.name == solc_field_name) {
            errors.push(format!(
                "Field '{}' exists in Solidity but not in Rust layout",
                solc_field_name
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Compares struct member layouts within a specific struct field.
///
/// This verifies that struct members have the correct relative offsets
/// from the base slot of the struct.
pub fn compare_struct_members(
    solc_layout: &StorageLayout,
    struct_field_name: &str,
    rust_member_slots: &[RustStorageField],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    // Find the struct field in the top-level storage
    let struct_var = solc_layout
        .storage
        .iter()
        .find(|v| v.label == struct_field_name)
        .ok_or_else(|| {
            vec![format!(
                "Struct field '{}' not found in Solidity layout",
                struct_field_name
            )]
        })?;

    // Get the base slot of the struct
    let struct_base_slot = parse_slot(&struct_var.slot).map_err(|e| vec![e])?;

    // Find the type definition for this struct
    let type_def = solc_layout.types.get(&struct_var.ty).ok_or_else(|| {
        vec![format!(
            "Type definition '{}' not found for struct field '{}'",
            struct_var.ty, struct_field_name
        )]
    })?;

    // Get the struct members
    let members = type_def.members.as_ref().ok_or_else(|| {
        vec![format!(
            "Type '{}' does not have members (not a struct?)",
            struct_var.ty
        )]
    })?;

    // Build a map of Solidity member names to their storage info
    // (absolute slot = base_slot + member's relative slot)
    let solc_member_info: HashMap<String, (&StorageVariable, U256)> = members
        .iter()
        .filter_map(|member| {
            parse_slot(&member.slot).ok().map(|relative_slot| {
                (
                    member.label.clone(),
                    (member, struct_base_slot + relative_slot),
                )
            })
        })
        .collect();

    // Compare Rust member slots against Solidity
    for rust_member in rust_member_slots {
        match solc_member_info.get(rust_member.name) {
            Some((solc_member, solc_slot)) => {
                // Compare slot
                if *solc_slot != rust_member.slot {
                    errors.push(format!(
                        "Struct member '{}.{}': Solidity slot {} != Rust slot {}",
                        struct_field_name, rust_member.name, solc_slot, rust_member.slot
                    ));
                }

                // Compare offset
                if solc_member.offset as usize != rust_member.offset {
                    errors.push(format!(
                        "Struct member '{}.{}': Solidity offset {} != Rust offset {}",
                        struct_field_name, rust_member.name, solc_member.offset, rust_member.offset
                    ));
                }

                // Compare bytes
                if let Some(member_type_def) = solc_layout.types.get(&solc_member.ty) {
                    if let Ok(solc_bytes) = member_type_def.number_of_bytes.parse::<usize>() {
                        if solc_bytes != rust_member.bytes {
                            errors.push(format!(
                                "Struct member '{}.{}': Solidity bytes {} != Rust bytes {}",
                                struct_field_name, rust_member.name, solc_bytes, rust_member.bytes
                            ));
                        }
                    }
                }
            }
            None => {
                errors.push(format!(
                    "Struct member '{}.{}' exists in Rust but not in Solidity",
                    struct_field_name, rust_member.name
                ));
            }
        }
    }

    // Check for Solidity members missing in Rust
    for (solc_member_name, _) in &solc_member_info {
        if !rust_member_slots
            .iter()
            .any(|rm| &rm.name == solc_member_name)
        {
            errors.push(format!(
                "Struct member '{}.{}' exists in Solidity but not in Rust",
                struct_field_name, solc_member_name
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_slot() {
        assert_eq!(parse_slot("0").unwrap(), U256::from(0));
        assert_eq!(parse_slot("10").unwrap(), U256::from(10));
        assert_eq!(parse_slot("255").unwrap(), U256::from(255));
    }

    #[test]
    fn test_load_expected_layout() {
        let testdata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata");
        let expected_path = testdata.join("basic_types.layout.json");

        let layout = load_solc_layout(&expected_path);
        assert!(layout.is_ok(), "Failed to load: {:?}", layout.err());

        let layout = layout.unwrap();
        assert_eq!(layout.storage.len(), 4); // 4 fields
        assert_eq!(layout.storage[0].label, "field_a");
        assert_eq!(layout.storage[0].slot, "0");
    }

    #[test]
    fn test_compare_layouts_matching() {
        let solc_layout = StorageLayout {
            storage: vec![
                StorageVariable {
                    contract: "Test".to_string(),
                    label: "field_a".to_string(),
                    offset: 0,
                    slot: "0".to_string(),
                    ty: "t_uint256".to_string(),
                },
                StorageVariable {
                    contract: "Test".to_string(),
                    label: "field_b".to_string(),
                    offset: 0,
                    slot: "1".to_string(),
                    ty: "t_uint256".to_string(),
                },
            ],
            types: HashMap::new(),
        };

        let rust_fields = vec![
            RustStorageField {
                name: "field_a",
                slot: U256::ZERO,
                offset: 0,
                bytes: 32,
            },
            RustStorageField {
                name: "field_b",
                slot: U256::from(1),
                offset: 0,
                bytes: 32,
            },
        ];

        assert!(compare_layouts(&solc_layout, &rust_fields).is_ok());
    }

    #[test]
    fn test_compare_layouts_mismatch() {
        let solc_layout = StorageLayout {
            storage: vec![StorageVariable {
                contract: "Test".to_string(),
                label: "field_a".to_string(),
                offset: 0,
                slot: "0".to_string(),
                ty: "t_uint256".to_string(),
            }],
            types: HashMap::new(),
        };

        let rust_fields = vec![RustStorageField {
            name: "field_a",
            slot: U256::from(5), // Wrong slot!
            offset: 0,
            bytes: 32,
        }];

        let result = compare_layouts(&solc_layout, &rust_fields);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Solidity slot 0 != Rust slot 5"));
    }

    #[test]
    fn test_compare_struct_members_matching() {
        let mut types = HashMap::new();
        types.insert(
            "t_struct_Block_storage".to_string(),
            TypeDefinition {
                encoding: "inplace".to_string(),
                label: "struct Block".to_string(),
                number_of_bytes: "96".to_string(),
                base: None,
                key: None,
                value: None,
                members: Some(vec![
                    StorageVariable {
                        contract: "Test".to_string(),
                        label: "field1".to_string(),
                        offset: 0,
                        slot: "0".to_string(), // Relative slot
                        ty: "t_uint256".to_string(),
                    },
                    StorageVariable {
                        contract: "Test".to_string(),
                        label: "field2".to_string(),
                        offset: 0,
                        slot: "1".to_string(), // Relative slot
                        ty: "t_uint256".to_string(),
                    },
                ]),
            },
        );

        let solc_layout = StorageLayout {
            storage: vec![StorageVariable {
                contract: "Test".to_string(),
                label: "my_struct".to_string(),
                offset: 0,
                slot: "5".to_string(), // Base slot
                ty: "t_struct_Block_storage".to_string(),
            }],
            types,
        };

        let rust_member_slots = vec![
            RustStorageField {
                name: "field1",
                slot: U256::from(5), // Base slot + 0
                offset: 0,
                bytes: 32,
            },
            RustStorageField {
                name: "field2",
                slot: U256::from(6), // Base slot + 1
                offset: 0,
                bytes: 32,
            },
        ];

        let result = compare_struct_members(&solc_layout, "my_struct", &rust_member_slots);
        assert!(
            result.is_ok(),
            "Expected success but got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_compare_struct_members_mismatch() {
        let mut types = HashMap::new();
        types.insert(
            "t_struct_Block_storage".to_string(),
            TypeDefinition {
                encoding: "inplace".to_string(),
                label: "struct Block".to_string(),
                number_of_bytes: "96".to_string(),
                base: None,
                key: None,
                value: None,
                members: Some(vec![StorageVariable {
                    contract: "Test".to_string(),
                    label: "field1".to_string(),
                    offset: 0,
                    slot: "0".to_string(), // Relative slot
                    ty: "t_uint256".to_string(),
                }]),
            },
        );

        let solc_layout = StorageLayout {
            storage: vec![StorageVariable {
                contract: "Test".to_string(),
                label: "my_struct".to_string(),
                offset: 0,
                slot: "5".to_string(), // Base slot
                ty: "t_struct_Block_storage".to_string(),
            }],
            types,
        };

        let rust_member_slots = vec![RustStorageField {
            name: "field1",
            slot: U256::from(10), // Wrong! Should be 5
            offset: 0,
            bytes: 32,
        }];

        let result = compare_struct_members(&solc_layout, "my_struct", &rust_member_slots);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Solidity slot 5 != Rust slot 10"));
    }
}
