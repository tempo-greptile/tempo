// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Test contract with struct storage.
contract Structs {
    struct TestBlock {
        uint256 field1;
        uint256 field2;
        uint64 field3;
    }

    uint256 public field_a; // slot 0
    TestBlock public block_data; // slots 1-3
    uint256 public field_b; // slot 4
}
