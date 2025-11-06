// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Test contract with fixed-size array storage.
contract Arrays {
    uint256 public field_a; // slot 0
    uint256[5] public large_array; // slots 1-5
    uint256 public field_b; // slot 6
}
