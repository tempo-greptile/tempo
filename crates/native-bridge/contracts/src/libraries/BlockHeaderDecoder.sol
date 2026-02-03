// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {RLPReader} from "solidity-merkle-trees/trie/ethereum/RLPReader.sol";

/// @title BlockHeaderDecoder
/// @notice Decodes Tempo block headers to extract the receiptsRoot
/// @dev Tempo headers are encoded as: rlp([generalGasLimit, sharedGasLimit, timestampMillisPart, inner])
///      where `inner` is a standard Ethereum header list.
library BlockHeaderDecoder {
    using RLPReader for bytes;
    using RLPReader for RLPReader.RLPItem;

    /// @notice Tempo header outer field indices
    /// @dev Outer structure: [generalGasLimit, sharedGasLimit, timestampMillisPart, inner]
    uint256 internal constant INNER_HEADER_INDEX = 3;

    /// @notice Ethereum header field indices (inside the inner list)
    /// @dev Inner fields: [parentHash, unclesHash, coinbase, stateRoot, txRoot, receiptsRoot, ...]
    uint256 internal constant RECEIPTS_ROOT_INDEX = 5;

    error InvalidBlockHeaderFormat();
    error NotEnoughHeaderFields();
    error InvalidInnerHeaderFormat();

    /// @notice Decode a Tempo block header and extract key fields
    /// @param rlpHeader RLP-encoded Tempo block header
    /// @return blockHash keccak256 hash of the RLP-encoded header
    /// @return receiptsRoot The receipts trie root from the inner header
    function decode(bytes calldata rlpHeader) internal pure returns (bytes32 blockHash, bytes32 receiptsRoot) {
        blockHash = keccak256(rlpHeader);

        RLPReader.RLPItem memory item = rlpHeader.toRlpItem();
        if (!item.isList()) revert InvalidBlockHeaderFormat();

        RLPReader.RLPItem[] memory outerFields = item.toList();
        if (outerFields.length <= INNER_HEADER_INDEX) revert NotEnoughHeaderFields();

        // Get the inner Ethereum header (4th element, index 3)
        RLPReader.RLPItem memory innerItem = outerFields[INNER_HEADER_INDEX];
        if (!innerItem.isList()) revert InvalidInnerHeaderFormat();

        RLPReader.RLPItem[] memory innerFields = innerItem.toList();
        if (innerFields.length <= RECEIPTS_ROOT_INDEX) revert NotEnoughHeaderFields();

        receiptsRoot = bytes32(innerFields[RECEIPTS_ROOT_INDEX].toUint());
    }

    /// @notice Decode and return just the block hash
    /// @param rlpHeader RLP-encoded block header
    /// @return blockHash keccak256 hash of the header
    function hashHeader(bytes calldata rlpHeader) internal pure returns (bytes32 blockHash) {
        blockHash = keccak256(rlpHeader);
    }

    /// @notice Extract receiptsRoot from a Tempo block header
    /// @param rlpHeader RLP-encoded Tempo block header
    /// @return receiptsRoot The receipts trie root
    function extractReceiptsRoot(bytes calldata rlpHeader) internal pure returns (bytes32 receiptsRoot) {
        RLPReader.RLPItem memory item = rlpHeader.toRlpItem();
        if (!item.isList()) revert InvalidBlockHeaderFormat();

        RLPReader.RLPItem[] memory outerFields = item.toList();
        if (outerFields.length <= INNER_HEADER_INDEX) revert NotEnoughHeaderFields();

        RLPReader.RLPItem memory innerItem = outerFields[INNER_HEADER_INDEX];
        if (!innerItem.isList()) revert InvalidInnerHeaderFormat();

        RLPReader.RLPItem[] memory innerFields = innerItem.toList();
        if (innerFields.length <= RECEIPTS_ROOT_INDEX) revert NotEnoughHeaderFields();

        receiptsRoot = bytes32(innerFields[RECEIPTS_ROOT_INDEX].toUint());
    }
}
