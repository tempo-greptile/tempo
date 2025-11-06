use alloy::primitives::{U256, keccak256};

fn left_pad_to_32(data: &[u8]) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[32 - data.len()..].copy_from_slice(data);
    buf
}

/// Compute storage slot for a mapping
#[inline]
pub fn mapping_slot<T: AsRef<[u8]>>(key: T, mapping_slot: U256) -> U256 {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(&left_pad_to_32(key.as_ref()));
    buf[32..].copy_from_slice(&mapping_slot.to_be_bytes::<32>());
    U256::from_be_bytes(keccak256(buf).0)
}

/// Compute storage slot for a double mapping (mapping\[key1\]\[key2\])
#[inline]
pub fn double_mapping_slot<T: AsRef<[u8]>, U: AsRef<[u8]>>(
    key1: T,
    key2: U,
    base_slot: U256,
) -> U256 {
    let intermediate_slot = mapping_slot(key1, base_slot);
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(&left_pad_to_32(key2.as_ref()));
    buf[32..].copy_from_slice(&intermediate_slot.to_be_bytes::<32>());
    U256::from_be_bytes(keccak256(buf).0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{B256, address};

    #[test]
    fn test_mapping_slot_deterministic() {
        let key: B256 = U256::from(123).into();
        let slot1 = mapping_slot(key, U256::ZERO);
        let slot2 = mapping_slot(key, U256::ZERO);

        assert_eq!(slot1, slot2);
    }

    #[test]
    fn test_different_keys_different_slots() {
        let key1: B256 = U256::from(123).into();
        let key2: B256 = U256::from(456).into();

        let slot1 = mapping_slot(key1, U256::ZERO);
        let slot2 = mapping_slot(key2, U256::ZERO);

        assert_ne!(slot1, slot2);
    }

    #[test]
    fn test_tip20_balance_slots() {
        // Test balance slot calculation for TIP20 tokens (slot 10)
        let alice = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        let bob = address!("0x70997970C51812dc3A010C7d01b50e0d17dc79C8");

        let alice_balance_slot = mapping_slot(alice, U256::from(10));
        let bob_balance_slot = mapping_slot(bob, U256::from(10));

        println!("Alice balance slot: 0x{alice_balance_slot:064x}");
        println!("Bob balance slot: 0x{bob_balance_slot:064x}");

        // Verify they're different
        assert_ne!(alice_balance_slot, bob_balance_slot);
    }

    #[test]
    fn test_tip20_allowance_slots() {
        // Test allowance slot calculation for TIP20 tokens (slot 11)
        let alice = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        let tip_fee_mgr = address!("0xfeec000000000000000000000000000000000000");

        let allowance_slot = double_mapping_slot(alice, tip_fee_mgr, U256::from(11));

        println!("Alice->TipFeeManager allowance slot: 0x{allowance_slot:064x}");

        // Just verify it's calculated consistently
        let allowance_slot2 = double_mapping_slot(alice, tip_fee_mgr, U256::from(11));
        assert_eq!(allowance_slot, allowance_slot2);
    }

    #[test]
    fn test_double_mapping_different_keys() {
        let alice = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        let bob = address!("0x70997970C51812dc3A010C7d01b50e0d17dc79C8");
        let spender = address!("0xfeec000000000000000000000000000000000000");

        let alice_allowance = double_mapping_slot(alice, spender, U256::from(11));
        let bob_allowance = double_mapping_slot(bob, spender, U256::from(11));

        assert_ne!(alice_allowance, bob_allowance);
    }

    #[test]
    fn test_left_padding_correctness() {
        let addr = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        let bytes: &[u8] = addr.as_ref();
        let padded = left_pad_to_32(bytes);

        // First 12 bytes should be zeros (left padding)
        assert_eq!(&padded[..12], &[0u8; 12]);
        // Last 20 bytes should be the address
        assert_eq!(&padded[12..], bytes);
    }

    #[test]
    fn test_mapping_slot_encoding() {
        let key = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        let base_slot = U256::from(10);

        // Manual computation to validate
        let mut buf = [0u8; 64];
        // Left-pad the address to 32 bytes
        buf[12..32].copy_from_slice(key.as_ref());
        // Slot in big-endian
        buf[32..].copy_from_slice(&base_slot.to_be_bytes::<32>());

        let expected = U256::from_be_bytes(keccak256(buf).0);
        let computed = mapping_slot(key, base_slot);

        assert_eq!(computed, expected, "mapping_slot encoding mismatch");
    }

    #[test]
    fn test_double_mapping_account_role() {
        let account = address!("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        let role: B256 = U256::from(1).into();
        let base_slot = U256::from(1).into();

        let slot = double_mapping_slot(account, role, base_slot);

        // Verify deterministic
        let slot2 = double_mapping_slot(account, role, base_slot);
        assert_eq!(slot, slot2);

        // Verify different role yields different slot
        let different_role: B256 = U256::from(2).into();
        let different_slot = double_mapping_slot(account, different_role, base_slot);
        assert_ne!(slot, different_slot);
    }
}
