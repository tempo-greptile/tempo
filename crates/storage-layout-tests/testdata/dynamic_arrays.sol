// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Test contract with dynamic arrays to validate Vec<T> storage layout.
contract DynamicArrays {
    // uint8[] packs 32 elements per slot
    uint8[] public arr_u8;

    // uint256[] uses 1 slot per element (32 bytes each)
    uint256[] public arr_u256;

    // address[] uses 1 slot per element (20 bytes, but 32 % 20 != 0 so no packing)
    address[] public arr_address;

    // bool[] packs 32 elements per slot (1 byte each)
    bool[] public arr_bool;
}
