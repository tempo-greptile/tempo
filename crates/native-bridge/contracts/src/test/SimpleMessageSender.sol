// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

/// @title SimpleMessageSender
/// @notice Minimal contract for testing - just emits MessageSent event
contract SimpleMessageSender {
    event MessageSent(address indexed sender, bytes32 indexed messageHash, uint64 indexed destinationChainId);
    
    mapping(address => mapping(bytes32 => bool)) public sent;

    function send(bytes32 messageHash, uint64 destinationChainId) external {
        require(messageHash != bytes32(0), "ZeroMessageHash");
        require(!sent[msg.sender][messageHash], "MessageAlreadySent");
        
        sent[msg.sender][messageHash] = true;
        emit MessageSent(msg.sender, messageHash, destinationChainId);
    }
}
