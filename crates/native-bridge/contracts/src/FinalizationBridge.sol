// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {IFinalizationBridge} from "./interfaces/IFinalizationBridge.sol";
import {BLS12381} from "./BLS12381.sol";
import {BlockHeaderDecoder} from "./libraries/BlockHeaderDecoder.sol";
import {ReceiptDecoder} from "./libraries/ReceiptDecoder.sol";
import {MerklePatricia, StorageValue} from "solidity-merkle-trees/MerklePatricia.sol";

/// @title FinalizationBridge
/// @notice Cross-chain messaging using finalization certificates and receipt proofs
/// @dev Uses BLS12-381 threshold signatures to verify Tempo block finalization,
///      then Merkle Patricia Trie proofs to verify message events in receipts.
contract FinalizationBridge is IFinalizationBridge {
    using BlockHeaderDecoder for bytes;
    using ReceiptDecoder for bytes;

    //=============================================================
    //                          CONSTANTS
    //=============================================================

    /// @notice Namespace for finalization signatures (consensus uses "TEMPO_FINALIZE")
    /// @dev Validators sign: varint(len(namespace)) || namespace || proposal.encode()
    bytes public constant FINALIZATION_NAMESPACE = "TEMPO_FINALIZE";

    /// @notice Domain separator for key rotation
    bytes public constant KEY_ROTATION_DOMAIN = "TEMPO_BRIDGE_KEY_ROTATION_V1";

    /// @notice BLS Domain Separation Tag for hash-to-curve (MinSig variant, targets G1)
    /// @dev This is the standard DST used by commonware-cryptography for BLS12-381 MinSig
    bytes public constant BLS_DST = "BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_POP_";

    /// @notice Expected length for uncompressed G1 point (signature in MinSig)
    uint256 internal constant G1_POINT_LENGTH = 128;

    /// @notice Expected length for uncompressed G2 point (public key in MinSig)
    uint256 internal constant G2_POINT_LENGTH = 256;

    //=============================================================
    //                          STORAGE
    //=============================================================

    /// @notice Contract owner
    address public owner;

    /// @notice Pause state
    bool public paused;

    /// @notice This chain's ID (destination)
    uint64 public immutable chainId;

    /// @notice Origin chain ID (Tempo)
    uint64 public immutable originChainId;

    /// @notice Current validator epoch
    uint64 public epoch;

    /// @notice Previous epoch (for grace period)
    uint64 public previousEpoch;

    /// @notice BLS group public key for current epoch (G2 point, 256 bytes for MinSig)
    bytes public groupPublicKey;

    /// @notice BLS group public key for previous epoch (G2 point)
    bytes public previousGroupPublicKey;

    /// @notice Sent messages: sender => messageHash => sent
    mapping(address => mapping(bytes32 => bool)) public sent;

    /// @notice Received messages: originChainId => sender => messageHash => timestamp
    mapping(uint64 => mapping(address => mapping(bytes32 => uint256))) public received;

    //=============================================================
    //                        MODIFIERS
    //=============================================================

    modifier onlyOwner() {
        if (msg.sender != owner) revert Unauthorized();
        _;
    }

    modifier whenNotPaused() {
        if (paused) revert ContractPaused();
        _;
    }

    //=============================================================
    //                       CONSTRUCTOR
    //=============================================================

    /// @param _owner Contract owner
    /// @param _originChainId The Tempo chain ID (origin of messages)
    /// @param _initialEpoch Initial epoch number
    /// @param _initialPublicKey Initial BLS group public key (G2, 256 bytes for MinSig)
    constructor(address _owner, uint64 _originChainId, uint64 _initialEpoch, bytes memory _initialPublicKey) {
        if (_initialPublicKey.length != G2_POINT_LENGTH) revert InvalidPublicKeyLength();
        if (!BLS12381.isValidPublicKey(_initialPublicKey)) revert PublicKeyIsInfinity();

        owner = _owner;
        chainId = uint64(block.chainid);
        originChainId = _originChainId;
        epoch = _initialEpoch;
        groupPublicKey = _initialPublicKey;
    }

    //=============================================================
    //                      SEND FUNCTION
    //=============================================================

    /// @inheritdoc IFinalizationBridge
    function send(bytes32 messageHash, uint64 destinationChainId) external whenNotPaused {
        if (messageHash == bytes32(0)) revert ZeroMessageHash();

        if (sent[msg.sender][messageHash]) {
            revert MessageAlreadySent(msg.sender, messageHash);
        }

        sent[msg.sender][messageHash] = true;

        emit MessageSent(msg.sender, messageHash, destinationChainId);
    }

    //=============================================================
    //                      WRITE FUNCTIONS
    //=============================================================

    /// @inheritdoc IFinalizationBridge
    function write(
        bytes calldata blockHeader,
        bytes calldata finalizationProposal,
        bytes calldata finalizationSignature,
        bytes[] calldata receiptProof,
        uint256 receiptIndex,
        uint256 logIndex
    ) external whenNotPaused {
        if (finalizationSignature.length != G1_POINT_LENGTH) revert InvalidSignatureLength();
        if (receiptProof.length == 0) revert EmptyProof();

        // 1. Decode block header and get blockHash + receiptsRoot
        (bytes32 blockHash, bytes32 receiptsRoot) = BlockHeaderDecoder.decode(blockHeader);

        // 2. Verify finalization signature over proposal (which contains the blockHash)
        // Consensus signs: varint(len(namespace)) || namespace || proposal.encode()
        bytes memory signedMessage = _computeFinalizationMessage(finalizationProposal);
        bool valid = _verifyBLSSignatureRaw(groupPublicKey, signedMessage, finalizationSignature);

        if (!valid && previousGroupPublicKey.length > 0) {
            valid = _verifyBLSSignatureRaw(previousGroupPublicKey, signedMessage, finalizationSignature);
        }

        if (!valid) revert InvalidFinalizationSignature();

        // 3. Verify the proposal contains the correct block hash
        // The proposal payload (last 32 bytes) should match the block hash
        _verifyProposalMatchesBlock(finalizationProposal, blockHash);

        // 4. Verify receipt proof and extract message
        _verifyAndStoreMessage(receiptsRoot, receiptProof, receiptIndex, logIndex);
    }

    /// @inheritdoc IFinalizationBridge
    function writeBatch(
        bytes calldata blockHeader,
        bytes calldata finalizationProposal,
        bytes calldata finalizationSignature,
        bytes[][] calldata receiptProofs,
        uint256[] calldata receiptIndices,
        uint256[] calldata logIndices
    ) external whenNotPaused {
        if (finalizationSignature.length != G1_POINT_LENGTH) revert InvalidSignatureLength();
        if (receiptProofs.length == 0) revert EmptyProof();
        if (receiptProofs.length != receiptIndices.length || receiptIndices.length != logIndices.length) {
            revert ArrayLengthMismatch();
        }

        // 1. Decode block header and get blockHash + receiptsRoot
        (bytes32 blockHash, bytes32 receiptsRoot) = BlockHeaderDecoder.decode(blockHeader);

        // 2. Verify finalization signature over proposal (once for the batch)
        bytes memory signedMessage = _computeFinalizationMessage(finalizationProposal);
        bool valid = _verifyBLSSignatureRaw(groupPublicKey, signedMessage, finalizationSignature);

        if (!valid && previousGroupPublicKey.length > 0) {
            valid = _verifyBLSSignatureRaw(previousGroupPublicKey, signedMessage, finalizationSignature);
        }

        if (!valid) revert InvalidFinalizationSignature();

        // 3. Verify the proposal contains the correct block hash
        _verifyProposalMatchesBlock(finalizationProposal, blockHash);

        // 4. Process each message
        for (uint256 i = 0; i < receiptProofs.length; i++) {
            _verifyAndStoreMessage(receiptsRoot, receiptProofs[i], receiptIndices[i], logIndices[i]);
        }
    }

    //=============================================================
    //                      READ FUNCTIONS
    //=============================================================

    /// @inheritdoc IFinalizationBridge
    function receivedAt(uint64 _originChainId, address sender, bytes32 messageHash) external view returns (uint256) {
        return received[_originChainId][sender][messageHash];
    }

    /// @inheritdoc IFinalizationBridge
    function isSent(address sender, bytes32 messageHash) external view returns (bool) {
        return sent[sender][messageHash];
    }

    //=============================================================
    //                   KEY ROTATION FUNCTIONS
    //=============================================================

    /// @inheritdoc IFinalizationBridge
    function rotateKey(
        uint64 newEpoch,
        bytes calldata newPublicKey,
        bytes calldata authSignature
    ) external whenNotPaused {
        if (newPublicKey.length != G2_POINT_LENGTH) revert InvalidPublicKeyLength();
        if (authSignature.length != G1_POINT_LENGTH) revert InvalidSignatureLength();
        if (newEpoch <= epoch) revert EpochMustIncrease(epoch, newEpoch);
        if (groupPublicKey.length == 0) revert NoActivePublicKey();

        bytes32 rotationHash = _computeKeyRotationHash(epoch, newEpoch, newPublicKey);

        bool valid = _verifyBLSSignature(groupPublicKey, rotationHash, authSignature);
        if (!valid) revert KeyTransitionNotAuthorized();

        emit KeyRotationAuthorized(epoch, newEpoch, newPublicKey);

        _rotateKey(newEpoch, newPublicKey);
    }

    /// @inheritdoc IFinalizationBridge
    function computeKeyRotationHash(
        uint64 oldEpoch,
        uint64 newEpoch,
        bytes calldata newPublicKey
    ) external pure returns (bytes32) {
        return _computeKeyRotationHash(oldEpoch, newEpoch, newPublicKey);
    }

    //=============================================================
    //                      ADMIN FUNCTIONS
    //=============================================================

    /// @inheritdoc IFinalizationBridge
    function forceSetGroupPublicKey(uint64 newEpoch, bytes calldata publicKey) external onlyOwner {
        if (publicKey.length != G2_POINT_LENGTH) revert InvalidPublicKeyLength();
        if (newEpoch <= epoch) revert EpochMustIncrease(epoch, newEpoch);

        _rotateKey(newEpoch, publicKey);
    }

    /// @inheritdoc IFinalizationBridge
    function pause() external onlyOwner {
        paused = true;
    }

    /// @inheritdoc IFinalizationBridge
    function unpause() external onlyOwner {
        paused = false;
    }

    /// @inheritdoc IFinalizationBridge
    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0), "Invalid owner");
        owner = newOwner;
    }

    //=============================================================
    //                      INTERNAL FUNCTIONS
    //=============================================================

    /// @notice Verify receipt proof and store the message
    function _verifyAndStoreMessage(
        bytes32 receiptsRoot,
        bytes[] calldata receiptProof,
        uint256 receiptIndex,
        uint256 logIndex
    ) internal {
        // Build the key for the receipt (RLP-encoded index)
        bytes memory key = _rlpEncodeUint(receiptIndex);

        // Prepare keys array for MPT verification
        bytes[] memory keys = new bytes[](1);
        keys[0] = key;

        // Verify receipt exists in the trie
        StorageValue[] memory values = MerklePatricia.VerifyEthereumProof(receiptsRoot, receiptProof, keys);

        if (values.length == 0 || values[0].value.length == 0) {
            revert InvalidReceiptProof();
        }

        // Decode the MessageSent log from the receipt
        ReceiptDecoder.MessageSentLog memory log = ReceiptDecoder.decodeMessageSentLog(values[0].value, logIndex);

        // Verify destination chain matches this chain
        if (log.destinationChainId != chainId) {
            revert WrongDestinationChain(chainId, log.destinationChainId);
        }

        // Check not already received
        if (received[originChainId][log.sender][log.messageHash] != 0) {
            revert MessageAlreadyReceived(originChainId, log.sender, log.messageHash);
        }

        // Store the message
        uint256 timestamp = block.timestamp;
        received[originChainId][log.sender][log.messageHash] = timestamp;

        emit MessageReceived(originChainId, log.sender, log.messageHash, timestamp);
    }

    /// @notice Construct the message that validators sign for finalization
    /// @dev Format: varint(len(namespace)) || namespace || proposal
    /// For "TEMPO_FINALIZE" (14 bytes), varint is 0x0E
    function _computeFinalizationMessage(bytes calldata proposal) internal pure returns (bytes memory) {
        // Namespace length as varint (14 = 0x0E, fits in one byte)
        return abi.encodePacked(uint8(FINALIZATION_NAMESPACE.length), FINALIZATION_NAMESPACE, proposal);
    }

    /// @notice Verify the proposal's payload matches the block hash
    /// @dev The proposal structure is: Round (epoch u64 + view u64) + parent u64 + payload (32 bytes)
    /// Payload is the last 32 bytes of the encoded proposal
    function _verifyProposalMatchesBlock(bytes calldata proposal, bytes32 blockHash) internal pure {
        if (proposal.length < 32) revert InvalidBlockHeader();
        bytes32 proposalPayload = bytes32(proposal[proposal.length - 32:]);
        if (proposalPayload != blockHash) revert BlockHashMismatch(blockHash, proposalPayload);
    }

    /// @notice Compute key rotation authorization hash
    function _computeKeyRotationHash(
        uint64 oldEpoch,
        uint64 newEpoch,
        bytes memory newPublicKey
    ) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(KEY_ROTATION_DOMAIN, oldEpoch, newEpoch, newPublicKey));
    }

    /// @notice Internal key rotation
    function _rotateKey(uint64 newEpoch, bytes memory newPublicKey) internal {
        if (!BLS12381.isValidPublicKey(newPublicKey)) revert PublicKeyIsInfinity();

        bytes memory oldKey = groupPublicKey;
        uint64 oldEpoch = epoch;

        previousEpoch = epoch;
        previousGroupPublicKey = groupPublicKey;

        epoch = newEpoch;
        groupPublicKey = newPublicKey;

        emit KeyRotated(oldEpoch, newEpoch, oldKey, newPublicKey);
    }

    /// @notice Verify a BLS signature over raw message bytes (MinSig variant)
    /// @dev Used for finalization signatures where message is varint(len) || namespace || proposal
    function _verifyBLSSignatureRaw(
        bytes memory publicKey,
        bytes memory message,
        bytes calldata signature
    ) internal view returns (bool) {
        return BLS12381.verify(publicKey, message, BLS_DST, bytes(signature));
    }

    /// @notice Verify a BLS signature using RFC 9380 hash-to-curve (MinSig variant)
    /// @dev Used for key rotation where message is a keccak256 hash
    function _verifyBLSSignature(
        bytes memory publicKey,
        bytes32 messageHash,
        bytes calldata signature
    ) internal view returns (bool) {
        return BLS12381.verifyHash(publicKey, messageHash, BLS_DST, bytes(signature));
    }

    /// @notice RLP encode a uint (for receipt index key)
    /// @dev Simplified RLP encoding for small integers
    function _rlpEncodeUint(uint256 value) internal pure returns (bytes memory) {
        if (value == 0) {
            return hex"80";
        } else if (value < 128) {
            return abi.encodePacked(uint8(value));
        } else if (value < 256) {
            return abi.encodePacked(uint8(0x81), uint8(value));
        } else if (value < 65536) {
            return abi.encodePacked(uint8(0x82), uint16(value));
        } else if (value < 16777216) {
            return abi.encodePacked(uint8(0x83), uint8(value >> 16), uint8(value >> 8), uint8(value));
        } else {
            return abi.encodePacked(uint8(0x84), uint32(value));
        }
    }
}
