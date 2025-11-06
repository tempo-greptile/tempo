// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Test contract with nested mapping storage.
contract DoubleMappings {
    uint256 public field_a; // slot 0
    mapping(address => mapping(bytes32 => bool)) public account_role; // slot 1
    mapping(address => mapping(address => uint256)) public allowances; // slot 2
}
