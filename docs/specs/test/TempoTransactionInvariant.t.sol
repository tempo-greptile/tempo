// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {Vm} from "forge-std/Vm.sol";

import {InvariantBase} from "./helpers/InvariantBase.sol";
import {TxBuilder} from "./helpers/TxBuilder.sol";
import {InitcodeHelper, SimpleStorage, Counter} from "./helpers/TestContracts.sol";
import {TIP20} from "../src/TIP20.sol";
import {INonce} from "../src/interfaces/INonce.sol";
import {IAccountKeychain} from "../src/interfaces/IAccountKeychain.sol";
import {ITIP20} from "../src/interfaces/ITIP20.sol";

import {VmRlp, VmExecuteTransaction} from "tempo-std/StdVm.sol";
import {TempoTransaction, TempoCall, TempoTransactionLib} from "./helpers/tx/TempoTransactionLib.sol";
import {LegacyTransaction, LegacyTransactionLib} from "./helpers/tx/LegacyTransactionLib.sol";

/// @title Tempo Transaction Invariant Tests
/// @notice Comprehensive Foundry invariant tests for Tempo transaction behavior
/// @dev Tests nonce management, CREATE operations, fee collection, and access keys
contract TempoTransactionInvariantTest is InvariantBase {
    using TempoTransactionLib for TempoTransaction;
    using LegacyTransactionLib for LegacyTransaction;
    using TxBuilder for *;

    // ============ Additional Ghost State ============

    mapping(address => uint256) public ghost_previousProtocolNonce;
    mapping(address => mapping(uint256 => uint256)) public ghost_previous2dNonce;

    // ============ Setup ============

    function setUp() public override {
        super.setUp();

        // Target this contract for handler functions
        targetContract(address(this));

        // Define which handlers the fuzzer should call
        bytes4[] memory selectors = new bytes4[](14);
        // Legacy transaction handlers
        selectors[0] = this.handler_transfer.selector;
        selectors[1] = this.handler_sequentialTransfers.selector;
        selectors[2] = this.handler_create.selector;
        selectors[3] = this.handler_createReverting.selector;
        // 2D nonce handlers
        selectors[4] = this.handler_2dNonceIncrement.selector;
        selectors[5] = this.handler_multipleNonceKeys.selector;
        // Tempo transaction handlers
        selectors[6] = this.handler_tempoTransfer.selector;
        selectors[7] = this.handler_tempoTransferProtocolNonce.selector;
        selectors[8] = this.handler_tempoUseAccessKey.selector;
        selectors[9] = this.handler_tempoUseP256AccessKey.selector;
        // Access key handlers
        selectors[10] = this.handler_authorizeKey.selector;
        selectors[11] = this.handler_revokeKey.selector;
        selectors[12] = this.handler_useAccessKey.selector;
        selectors[13] = this.handler_insufficientBalanceTransfer.selector;
        targetSelector(FuzzSelector({addr: address(this), selectors: selectors}));

        // Initialize previous nonce tracking for secp256k1 actors
        for (uint256 i = 0; i < actors.length; i++) {
            ghost_previousProtocolNonce[actors[i]] = 0;
        }

        // Fund P256-derived addresses with fee tokens and initialize nonce tracking
        vm.startPrank(admin);
        for (uint256 i = 0; i < actors.length; i++) {
            address p256Addr = actorP256Addresses[i];
            feeToken.mint(p256Addr, 100_000_000e6);
            ghost_feeTokenBalance[p256Addr] = 100_000_000e6;
            ghost_previousProtocolNonce[p256Addr] = 0;
        }
        vm.stopPrank();
    }

    /*//////////////////////////////////////////////////////////////
                        SIGNING PARAMS HELPER
    //////////////////////////////////////////////////////////////*/

    /// @notice Build SigningParams for the given actor and signature type
    function _getSigningParams(uint256 actorIndex, SignatureType sigType, uint256 keySeed)
        internal
        view
        returns (TxBuilder.SigningParams memory params, address sender)
    {
        if (sigType == SignatureType.Secp256k1) {
            sender = actors[actorIndex];
            params = TxBuilder.SigningParams({
                strategy: TxBuilder.SigningStrategy.Secp256k1,
                privateKey: actorKeys[actorIndex],
                pubKeyX: bytes32(0),
                pubKeyY: bytes32(0),
                userAddress: address(0)
            });
        } else if (sigType == SignatureType.P256) {
            (address p256Addr, uint256 p256Key, bytes32 pubKeyX, bytes32 pubKeyY) = _getActorP256(actorIndex);
            sender = p256Addr;
            params = TxBuilder.SigningParams({
                strategy: TxBuilder.SigningStrategy.P256,
                privateKey: p256Key,
                pubKeyX: pubKeyX,
                pubKeyY: pubKeyY,
                userAddress: address(0)
            });
        } else if (sigType == SignatureType.WebAuthn) {
            (address p256Addr, uint256 p256Key, bytes32 pubKeyX, bytes32 pubKeyY) = _getActorP256(actorIndex);
            sender = p256Addr;
            params = TxBuilder.SigningParams({
                strategy: TxBuilder.SigningStrategy.WebAuthn,
                privateKey: p256Key,
                pubKeyX: pubKeyX,
                pubKeyY: pubKeyY,
                userAddress: address(0)
            });
        } else {
            // AccessKey
            (, uint256 keyPk) = _getActorAccessKey(actorIndex, keySeed);
            sender = actors[actorIndex];
            params = TxBuilder.SigningParams({
                strategy: TxBuilder.SigningStrategy.KeychainSecp256k1,
                privateKey: keyPk,
                pubKeyX: bytes32(0),
                pubKeyY: bytes32(0),
                userAddress: actors[actorIndex]
            });
        }
    }

    /*//////////////////////////////////////////////////////////////
                        TRANSACTION BUILDING
    //////////////////////////////////////////////////////////////*/

    function _buildAndSignLegacyTransferWithSigType(
        uint256 actorIndex,
        address to,
        uint256 amount,
        uint64 txNonce,
        uint256 sigTypeSeed
    ) internal view returns (bytes memory signedTx, address sender) {
        SignatureType sigType = _getRandomSignatureType(sigTypeSeed);
        (TxBuilder.SigningParams memory params, address senderAddr) = _getSigningParams(actorIndex, sigType, sigTypeSeed);
        sender = senderAddr;

        LegacyTransaction memory tx_ = LegacyTransactionLib.create()
            .withNonce(txNonce)
            .withGasPrice(TxBuilder.DEFAULT_GAS_PRICE)
            .withGasLimit(TxBuilder.DEFAULT_GAS_LIMIT)
            .withTo(address(feeToken))
            .withData(abi.encodeCall(ITIP20.transfer, (to, amount)));

        signedTx = TxBuilder.signLegacy(vmRlp, vm, tx_, params);
    }

    function _buildAndSignLegacyTransfer(uint256 actorIndex, address to, uint256 amount, uint64 txNonce)
        internal
        view
        returns (bytes memory)
    {
        return TxBuilder.buildLegacyCall(vmRlp, vm, address(feeToken), abi.encodeCall(ITIP20.transfer, (to, amount)), txNonce, actorKeys[actorIndex]);
    }

    function _buildAndSignLegacyCreateWithSigType(
        uint256 actorIndex,
        bytes memory initcode,
        uint64 txNonce,
        uint256 sigTypeSeed
    ) internal view returns (bytes memory signedTx, address sender) {
        SignatureType sigType = _getRandomSignatureType(sigTypeSeed);
        (TxBuilder.SigningParams memory params, address senderAddr) = _getSigningParams(actorIndex, sigType, sigTypeSeed);
        sender = senderAddr;

        LegacyTransaction memory tx_ = LegacyTransactionLib.create()
            .withNonce(txNonce)
            .withGasPrice(TxBuilder.DEFAULT_GAS_PRICE)
            .withGasLimit(TxBuilder.DEFAULT_CREATE_GAS_LIMIT)
            .withTo(address(0))
            .withData(initcode);

        signedTx = TxBuilder.signLegacy(vmRlp, vm, tx_, params);
    }

    function _buildAndSignLegacyCreate(uint256 actorIndex, bytes memory initcode, uint64 txNonce)
        internal
        view
        returns (bytes memory)
    {
        return TxBuilder.buildLegacyCreate(vmRlp, vm, initcode, txNonce, actorKeys[actorIndex]);
    }

    function _buildAndSignTempoTransferWithSigType(
        uint256 actorIndex,
        address to,
        uint256 amount,
        uint64 nonceKey,
        uint64 txNonce,
        uint256 sigTypeSeed
    ) internal view returns (bytes memory signedTx, address sender) {
        SignatureType sigType = _getRandomSignatureType(sigTypeSeed);
        (TxBuilder.SigningParams memory params, address senderAddr) = _getSigningParams(actorIndex, sigType, sigTypeSeed);
        sender = senderAddr;

        TempoCall[] memory calls = new TempoCall[](1);
        calls[0] = TempoCall({to: address(feeToken), value: 0, data: abi.encodeCall(ITIP20.transfer, (to, amount))});

        TempoTransaction memory tx_ = TempoTransactionLib.create()
            .withChainId(uint64(block.chainid))
            .withMaxFeePerGas(TxBuilder.DEFAULT_GAS_PRICE)
            .withGasLimit(TxBuilder.DEFAULT_GAS_LIMIT)
            .withCalls(calls)
            .withNonceKey(nonceKey)
            .withNonce(txNonce);

        signedTx = TxBuilder.signTempo(vmRlp, vm, tx_, params);
    }

    /*//////////////////////////////////////////////////////////////
                    NONCE HANDLERS (N1-N5, N12-N15)
    //////////////////////////////////////////////////////////////*/

    /// @notice Handler: Execute a transfer from a random actor with random signature type
    /// @dev Tests N1 (monotonicity) and N2 (bump on call) across all signature types
    function handler_transfer(uint256 actorSeed, uint256 recipientSeed, uint256 amount, uint256 sigTypeSeed) external {
        uint256 senderIdx = actorSeed % actors.length;
        uint256 recipientIdx = recipientSeed % actors.length;
        if (senderIdx == recipientIdx) {
            recipientIdx = (recipientIdx + 1) % actors.length;
        }

        SignatureType sigType = _getRandomSignatureType(sigTypeSeed);
        address sender = _getSenderForSigType(senderIdx, sigType);
        address recipient = actors[recipientIdx];

        amount = bound(amount, 1e6, 100e6);

        // Build tx first to get actual sender (may differ for P256/WebAuthn)
        uint64 currentNonce = uint64(ghost_protocolNonce[sender]);
        (bytes memory signedTx, address actualSender) = _buildAndSignLegacyTransferWithSigType(senderIdx, recipient, amount, currentNonce, sigTypeSeed);

        // Use actualSender for all checks and ghost state
        uint256 balance = feeToken.balanceOf(actualSender);
        if (balance < amount) {
            return;
        }

        // Re-check nonce with actual sender if different
        if (actualSender != sender) {
            currentNonce = uint64(ghost_protocolNonce[actualSender]);
            (signedTx,) = _buildAndSignLegacyTransferWithSigType(senderIdx, recipient, amount, currentNonce, sigTypeSeed);
        }

        ghost_previousProtocolNonce[actualSender] = ghost_protocolNonce[actualSender];

        vm.coinbase(validator);

        // Nonce is consumed when tx is included, regardless of execution success/revert
        try vmExec.executeTransaction(signedTx) {
            ghost_protocolNonce[actualSender]++;
            ghost_totalTxExecuted++;
            ghost_totalCallsExecuted++;
            ghost_totalProtocolNonceTxs++;
        } catch {
            // Check if nonce was actually incremented on-chain
            uint256 actualNonce = vm.getNonce(actualSender);
            if (actualNonce > ghost_protocolNonce[actualSender]) {
                ghost_protocolNonce[actualSender]++;
                ghost_totalProtocolNonceTxs++;
            }
            ghost_totalTxReverted++;
        }
    }

    /// @notice Handler: Execute multiple transfers in sequence from same actor with random sig types
    /// @dev Tests sequential nonce bumping across all signature types
    function handler_sequentialTransfers(uint256 actorSeed, uint256 count, uint256 sigTypeSeed) external {
        count = bound(count, 1, 5);
        uint256 senderIdx = actorSeed % actors.length;
        uint256 recipientIdx = (senderIdx + 1) % actors.length;

        SignatureType sigType = _getRandomSignatureType(sigTypeSeed);
        address sender = _getSenderForSigType(senderIdx, sigType);
        address recipient = actors[recipientIdx];

        // Get actual sender from build function (may differ for P256/WebAuthn)
        (, address actualSender) = _buildAndSignLegacyTransferWithSigType(senderIdx, recipient, 1e6, 0, sigTypeSeed);

        uint256 amountPerTx = 10e6;
        uint256 balance = feeToken.balanceOf(actualSender);

        if (balance < amountPerTx * count) {
            return;
        }

        for (uint256 i = 0; i < count; i++) {
            ghost_previousProtocolNonce[actualSender] = ghost_protocolNonce[actualSender];
            uint64 currentNonce = uint64(ghost_protocolNonce[actualSender]);

            (bytes memory signedTx,) = _buildAndSignLegacyTransferWithSigType(senderIdx, recipient, amountPerTx, currentNonce, sigTypeSeed);

            vm.coinbase(validator);

            // Nonce is consumed when tx is included, regardless of execution success/revert
            try vmExec.executeTransaction(signedTx) {
                ghost_protocolNonce[actualSender]++;
                ghost_totalTxExecuted++;
                ghost_totalCallsExecuted++;
                ghost_totalProtocolNonceTxs++;
            } catch {
                // Check if nonce was actually incremented on-chain
                uint256 actualNonce = vm.getNonce(actualSender);
                if (actualNonce > ghost_protocolNonce[actualSender]) {
                    ghost_protocolNonce[actualSender]++;
                    ghost_totalProtocolNonceTxs++;
                }
                ghost_totalTxReverted++;
                break;
            }
        }
    }

    /// @notice Handler: Deploy a contract via CREATE with random signature type
    /// @dev Tests N3 (nonce bumps on tx inclusion) and C5-C6 (address derivation) across all sig types
    function handler_create(uint256 actorSeed, uint256 initValue, uint256 sigTypeSeed) external {
        uint256 senderIdx = actorSeed % actors.length;
        SignatureType sigType = _getRandomSignatureType(sigTypeSeed);
        address sender = _getSenderForSigType(senderIdx, sigType);

        initValue = bound(initValue, 0, 1000);

        // Build tx first to get actual sender (may differ for P256/WebAuthn)
        uint64 currentNonce = uint64(ghost_protocolNonce[sender]);
        bytes memory initcode = InitcodeHelper.simpleStorageInitcode(initValue);
        (bytes memory signedTx, address actualSender) = _buildAndSignLegacyCreateWithSigType(senderIdx, initcode, currentNonce, sigTypeSeed);

        // Re-check nonce with actual sender if different
        if (actualSender != sender) {
            currentNonce = uint64(ghost_protocolNonce[actualSender]);
            (signedTx,) = _buildAndSignLegacyCreateWithSigType(senderIdx, initcode, currentNonce, sigTypeSeed);
        }

        // Compute expected CREATE address BEFORE nonce is incremented
        address expectedAddress = TxBuilder.computeCreateAddress(actualSender, currentNonce);

        ghost_previousProtocolNonce[actualSender] = ghost_protocolNonce[actualSender];

        vm.coinbase(validator);

        // Nonce is consumed when tx is included, regardless of execution success/revert
        try vmExec.executeTransaction(signedTx) {
            ghost_protocolNonce[actualSender]++;
            ghost_totalTxExecuted++;
            ghost_totalCreatesExecuted++;
            ghost_totalProtocolNonceTxs++;

            // Record the deployed address
            bytes32 key = keccak256(abi.encodePacked(actualSender, uint256(currentNonce)));
            ghost_createAddresses[key] = expectedAddress;
            ghost_createCount[actualSender]++;
        } catch {
            // Check if nonce was actually incremented on-chain
            uint256 actualNonce = vm.getNonce(actualSender);
            if (actualNonce > ghost_protocolNonce[actualSender]) {
                ghost_protocolNonce[actualSender]++;
                ghost_totalTxExecuted++;
                ghost_totalCreatesExecuted++;
                ghost_totalProtocolNonceTxs++;
            }
            ghost_totalTxReverted++;
        }
    }

    /// @notice Handler: Attempt to deploy a reverting contract
    /// @dev Tests that reverting initcode causes tx rejection (no nonce consumed)
    function handler_createReverting(uint256 actorSeed, uint256 sigTypeSeed) external {
        uint256 senderIdx = actorSeed % actors.length;
        SignatureType sigType = _getRandomSignatureType(sigTypeSeed);
        address sender = _getSenderForSigType(senderIdx, sigType);

        // Build tx first to get actual sender (may differ for P256/WebAuthn)
        uint64 currentNonce = uint64(ghost_protocolNonce[sender]);
        bytes memory initcode = InitcodeHelper.revertingContractInitcode();
        (bytes memory signedTx, address actualSender) = _buildAndSignLegacyCreateWithSigType(senderIdx, initcode, currentNonce, sigTypeSeed);

        // Re-check nonce with actual sender if different
        if (actualSender != sender) {
            currentNonce = uint64(ghost_protocolNonce[actualSender]);
            (signedTx,) = _buildAndSignLegacyCreateWithSigType(senderIdx, initcode, currentNonce, sigTypeSeed);
        }

        ghost_previousProtocolNonce[actualSender] = ghost_protocolNonce[actualSender];

        vm.coinbase(validator);

        // Ethereum nonce semantics: nonce is consumed when tx is INCLUDED in a block,
        // regardless of whether execution succeeds or reverts.
        // vmExec.executeTransaction processes the tx - if it reverts, the tx was still included.
        try vmExec.executeTransaction(signedTx) {
            ghost_protocolNonce[actualSender]++;
            ghost_totalTxExecuted++;
            ghost_totalCreatesExecuted++;
            ghost_totalProtocolNonceTxs++;
        } catch {
            // Check if nonce was actually incremented on-chain
            uint256 actualNonce = vm.getNonce(actualSender);
            if (actualNonce > ghost_protocolNonce[actualSender]) {
                ghost_protocolNonce[actualSender]++;
                ghost_totalTxExecuted++;
                ghost_totalCreatesExecuted++;
                ghost_totalProtocolNonceTxs++;
            }
            ghost_totalTxReverted++;
        }
    }

    /*//////////////////////////////////////////////////////////////
                    2D NONCE HANDLERS (N6-N11)
    //////////////////////////////////////////////////////////////*/

    /// @notice Handler: Increment a 2D nonce key
    /// @dev Tests N6 (independence) and N7 (monotonicity)
    function handler_2dNonceIncrement(uint256 actorSeed, uint256 nonceKey) external {
        uint256 actorIdx = actorSeed % actors.length;
        address actor = actors[actorIdx];

        // Bound nonce key to reasonable range (1-100, key 0 is protocol nonce)
        nonceKey = bound(nonceKey, 1, 100);

        // Store previous nonce for monotonicity check
        ghost_previous2dNonce[actor][nonceKey] = ghost_2dNonce[actor][nonceKey];

        // Increment via storage manipulation (simulates protocol behavior)
        _incrementNonceViaStorage(actor, nonceKey);
    }

    /// @notice Handler: Increment multiple different nonce keys for same actor
    /// @dev Tests N6 (keys are independent)
    function handler_multipleNonceKeys(uint256 actorSeed, uint256 key1, uint256 key2, uint256 key3) external {
        uint256 actorIdx = actorSeed % actors.length;
        address actor = actors[actorIdx];

        // Bound keys to different values
        key1 = bound(key1, 1, 33);
        key2 = bound(key2, 34, 66);
        key3 = bound(key3, 67, 100);

        // Track previous values
        ghost_previous2dNonce[actor][key1] = ghost_2dNonce[actor][key1];
        ghost_previous2dNonce[actor][key2] = ghost_2dNonce[actor][key2];
        ghost_previous2dNonce[actor][key3] = ghost_2dNonce[actor][key3];

        // Increment each key
        _incrementNonceViaStorage(actor, key1);
        _incrementNonceViaStorage(actor, key2);
        _incrementNonceViaStorage(actor, key3);
    }

    /*//////////////////////////////////////////////////////////////
                    TEMPO TRANSACTION HANDLERS (TX1-TX6)
    //////////////////////////////////////////////////////////////*/

    /// @notice Handler: Execute a Tempo transfer with random signature type
    /// @dev Tests that Tempo transactions work with all signature types (secp256k1, P256, WebAuthn, Keychain)
    function handler_tempoTransfer(uint256 actorSeed, uint256 recipientSeed, uint256 amount, uint256 nonceKeySeed, uint256 sigTypeSeed) external {
        uint256 senderIdx = actorSeed % actors.length;
        uint256 recipientIdx = recipientSeed % actors.length;
        if (senderIdx == recipientIdx) {
            recipientIdx = (recipientIdx + 1) % actors.length;
        }

        SignatureType sigType = _getRandomSignatureType(sigTypeSeed);
        address sender = _getSenderForSigType(senderIdx, sigType);
        address recipient = actors[recipientIdx];

        amount = bound(amount, 1e6, 100e6);

        // Use 2D nonce key (nonceKey > 0 for Tempo tx)
        uint64 nonceKey = uint64(bound(nonceKeySeed, 1, 100));

        // Build tx first to get actual sender (may differ for P256/WebAuthn)
        uint64 currentNonce = uint64(ghost_2dNonce[sender][nonceKey]);
        (bytes memory signedTx, address actualSender) = _buildAndSignTempoTransferWithSigType(senderIdx, recipient, amount, nonceKey, currentNonce, sigTypeSeed);

        // Use actualSender for all checks and ghost state
        uint256 balance = feeToken.balanceOf(actualSender);
        if (balance < amount) {
            return;
        }

        // Re-check nonce with actual sender if different
        if (actualSender != sender) {
            currentNonce = uint64(ghost_2dNonce[actualSender][nonceKey]);
            (signedTx,) = _buildAndSignTempoTransferWithSigType(senderIdx, recipient, amount, nonceKey, currentNonce, sigTypeSeed);
        }

        // Store previous nonce for monotonicity check
        ghost_previous2dNonce[actualSender][nonceKey] = ghost_2dNonce[actualSender][nonceKey];

        vm.coinbase(validator);

        // Nonce is consumed when tx is included, regardless of execution success/revert
        try vmExec.executeTransaction(signedTx) {
            // Update 2D nonce (not protocol nonce for Tempo tx with nonceKey > 0)
            ghost_2dNonce[actualSender][nonceKey]++;
            ghost_2dNonceUsed[actualSender][nonceKey] = true;
            ghost_totalTxExecuted++;
            ghost_totalCallsExecuted++;
            ghost_total2dNonceTxs++;
        } catch {
            // Check if 2D nonce was actually incremented
            uint64 actualNonce = nonce.getNonce(actualSender, nonceKey);
            if (actualNonce > ghost_2dNonce[actualSender][nonceKey]) {
                ghost_2dNonce[actualSender][nonceKey]++;
                ghost_2dNonceUsed[actualSender][nonceKey] = true;
                ghost_totalTxExecuted++;
                ghost_totalCallsExecuted++;
                ghost_total2dNonceTxs++;
            }
            ghost_totalTxReverted++;
        }
    }

    /// @notice Handler: Execute a Tempo transfer using protocol nonce (nonceKey = 0)
    /// @dev Tests that Tempo transactions with nonceKey=0 use the protocol nonce
    function handler_tempoTransferProtocolNonce(uint256 actorSeed, uint256 recipientSeed, uint256 amount, uint256 sigTypeSeed) external {
        uint256 senderIdx = actorSeed % actors.length;
        uint256 recipientIdx = recipientSeed % actors.length;
        if (senderIdx == recipientIdx) {
            recipientIdx = (recipientIdx + 1) % actors.length;
        }

        SignatureType sigType = _getRandomSignatureType(sigTypeSeed);
        address sender = _getSenderForSigType(senderIdx, sigType);
        address recipient = actors[recipientIdx];

        amount = bound(amount, 1e6, 100e6);

        // Use protocol nonce (nonceKey = 0)
        uint64 nonceKey = 0;

        // Build tx first to get actual sender (may differ for P256/WebAuthn)
        uint64 currentNonce = uint64(ghost_protocolNonce[sender]);
        (bytes memory signedTx, address actualSender) = _buildAndSignTempoTransferWithSigType(senderIdx, recipient, amount, nonceKey, currentNonce, sigTypeSeed);

        // Use actualSender for all checks and ghost state
        uint256 balance = feeToken.balanceOf(actualSender);
        if (balance < amount) {
            return;
        }

        // Re-check nonce with actual sender if different
        if (actualSender != sender) {
            currentNonce = uint64(ghost_protocolNonce[actualSender]);
            (signedTx,) = _buildAndSignTempoTransferWithSigType(senderIdx, recipient, amount, nonceKey, currentNonce, sigTypeSeed);
        }

        ghost_previousProtocolNonce[actualSender] = ghost_protocolNonce[actualSender];

        vm.coinbase(validator);

        // Nonce is consumed when tx is included, regardless of execution success/revert
        try vmExec.executeTransaction(signedTx) {
            ghost_protocolNonce[actualSender]++;
            ghost_totalTxExecuted++;
            ghost_totalCallsExecuted++;
            ghost_totalProtocolNonceTxs++; // Tempo with nonceKey=0 uses protocol nonce
        } catch {
            // Check if nonce was actually incremented on-chain
            uint256 actualNonce = vm.getNonce(actualSender);
            if (actualNonce > ghost_protocolNonce[actualSender]) {
                ghost_protocolNonce[actualSender]++;
                ghost_totalTxExecuted++;
                ghost_totalCallsExecuted++;
                ghost_totalProtocolNonceTxs++;
            }
            ghost_totalTxReverted++;
        }
    }

    /// @notice Handler: Use access key with Tempo transaction
    /// @dev Tests access keys with Tempo transactions (K5, K9 with Tempo tx type)
    function handler_tempoUseAccessKey(uint256 actorSeed, uint256 keySeed, uint256 recipientSeed, uint256 amount, uint256 nonceKeySeed) external {
        uint256 actorIdx = actorSeed % actors.length;
        address owner = actors[actorIdx];
        uint256 recipientIdx = recipientSeed % actors.length;
        if (actorIdx == recipientIdx) {
            recipientIdx = (recipientIdx + 1) % actors.length;
        }
        address recipient = actors[recipientIdx];

        // Get a secp256k1 access key
        (address keyId, uint256 keyPk) = _getActorAccessKey(actorIdx, keySeed);

        // Only use if authorized
        if (!ghost_keyAuthorized[owner][keyId]) {
            return;
        }

        // Check if key is expired
        if (ghost_keyExpiry[owner][keyId] <= block.timestamp) {
            return;
        }

        amount = bound(amount, 1e6, 50e6);

        // Check balance
        uint256 balance = feeToken.balanceOf(owner);
        if (balance < amount) {
            return;
        }

        // Check spending limit if enforced
        if (ghost_keyEnforceLimits[owner][keyId]) {
            uint256 limit = ghost_keySpendingLimit[owner][keyId][address(feeToken)];
            uint256 spent = ghost_keySpentAmount[owner][keyId][address(feeToken)];
            if (spent + amount > limit) {
                return; // Would exceed limit
            }
        }

        uint64 nonceKey = uint64(bound(nonceKeySeed, 1, 100));
        ghost_previous2dNonce[owner][nonceKey] = ghost_2dNonce[owner][nonceKey];
        uint64 currentNonce = uint64(ghost_2dNonce[owner][nonceKey]);

        // Build Tempo transaction signed by access key
        bytes memory signedTx = TxBuilder.buildTempoCallKeychain(
            vmRlp,
            vm,
            address(feeToken),
            abi.encodeCall(ITIP20.transfer, (recipient, amount)),
            nonceKey,
            currentNonce,
            keyPk,
            owner
        );

        vm.coinbase(validator);

        // Nonce is consumed when tx is included, regardless of execution success/revert
        try vmExec.executeTransaction(signedTx) {
            ghost_2dNonce[owner][nonceKey]++;
            ghost_2dNonceUsed[owner][nonceKey] = true;
            ghost_totalTxExecuted++;
            ghost_totalCallsExecuted++;
            ghost_total2dNonceTxs++;

            // Track spending for K9 invariant
            if (ghost_keyEnforceLimits[owner][keyId]) {
                _recordKeySpending(owner, keyId, address(feeToken), amount);
            }
        } catch {
            // Check if 2D nonce was actually incremented
            uint64 actualNonce = nonce.getNonce(owner, nonceKey);
            if (actualNonce > ghost_2dNonce[owner][nonceKey]) {
                ghost_2dNonce[owner][nonceKey]++;
                ghost_2dNonceUsed[owner][nonceKey] = true;
                ghost_totalTxExecuted++;
                ghost_totalCallsExecuted++;
                ghost_total2dNonceTxs++;
            }
            ghost_totalTxReverted++;
        }
    }

    /// @notice Handler: Use P256 access key with Tempo transaction
    /// @dev Tests P256 access keys with Tempo transactions
    function handler_tempoUseP256AccessKey(uint256 actorSeed, uint256 keySeed, uint256 recipientSeed, uint256 amount, uint256 nonceKeySeed) external {
        uint256 actorIdx = actorSeed % actors.length;
        address owner = actors[actorIdx];
        uint256 recipientIdx = recipientSeed % actors.length;
        if (actorIdx == recipientIdx) {
            recipientIdx = (recipientIdx + 1) % actors.length;
        }
        address recipient = actors[recipientIdx];

        // Get a P256 access key
        (address keyId, uint256 keyPk, bytes32 pubKeyX, bytes32 pubKeyY) = _getActorP256AccessKey(actorIdx, keySeed);

        // Only use if authorized
        if (!ghost_keyAuthorized[owner][keyId]) {
            return;
        }

        // Check if key is expired
        if (ghost_keyExpiry[owner][keyId] <= block.timestamp) {
            return;
        }

        amount = bound(amount, 1e6, 50e6);

        // Check balance
        uint256 balance = feeToken.balanceOf(owner);
        if (balance < amount) {
            return;
        }

        // Check spending limit if enforced
        if (ghost_keyEnforceLimits[owner][keyId]) {
            uint256 limit = ghost_keySpendingLimit[owner][keyId][address(feeToken)];
            uint256 spent = ghost_keySpentAmount[owner][keyId][address(feeToken)];
            if (spent + amount > limit) {
                return; // Would exceed limit
            }
        }

        uint64 nonceKey = uint64(bound(nonceKeySeed, 1, 100));
        ghost_previous2dNonce[owner][nonceKey] = ghost_2dNonce[owner][nonceKey];
        uint64 currentNonce = uint64(ghost_2dNonce[owner][nonceKey]);

        // Build Tempo transaction signed by P256 access key
        bytes memory signedTx = TxBuilder.buildTempoCallKeychainP256(
            vmRlp,
            vm,
            address(feeToken),
            abi.encodeCall(ITIP20.transfer, (recipient, amount)),
            nonceKey,
            currentNonce,
            keyPk,
            pubKeyX,
            pubKeyY,
            owner
        );

        vm.coinbase(validator);

        // Nonce is consumed when tx is included, regardless of execution success/revert
        try vmExec.executeTransaction(signedTx) {
            ghost_2dNonce[owner][nonceKey]++;
            ghost_2dNonceUsed[owner][nonceKey] = true;
            ghost_totalTxExecuted++;
            ghost_totalCallsExecuted++;
            ghost_total2dNonceTxs++;

            // Track spending for K9 invariant
            if (ghost_keyEnforceLimits[owner][keyId]) {
                _recordKeySpending(owner, keyId, address(feeToken), amount);
            }
        } catch {
            // Check if 2D nonce was actually incremented
            uint64 actualNonce = nonce.getNonce(owner, nonceKey);
            if (actualNonce > ghost_2dNonce[owner][nonceKey]) {
                ghost_2dNonce[owner][nonceKey]++;
                ghost_2dNonceUsed[owner][nonceKey] = true;
                ghost_totalTxExecuted++;
                ghost_totalCallsExecuted++;
                ghost_total2dNonceTxs++;
            }
            ghost_totalTxReverted++;
        }
    }

    /*//////////////////////////////////////////////////////////////
                    ACCESS KEY HANDLERS (K1-K12)
    //////////////////////////////////////////////////////////////*/

    /// @notice Handler: Authorize an access key with random key type (secp256k1 or P256)
    /// @dev Tests K1-K4 (key authorization rules) with multiple signature types
    function handler_authorizeKey(uint256 actorSeed, uint256 keySeed, uint256 expirySeed, uint256 limitSeed) external {
        uint256 actorIdx = actorSeed % actors.length;
        address owner = actors[actorIdx];

        // Randomly choose between secp256k1 and P256 access keys
        bool useP256 = keySeed % 2 == 0;
        address keyId;
        IAccountKeychain.SignatureType keyType;

        if (useP256) {
            (keyId,,,) = _getActorP256AccessKey(actorIdx, keySeed);
            keyType = IAccountKeychain.SignatureType.P256;
        } else {
            (keyId,) = _getActorAccessKey(actorIdx, keySeed);
            keyType = IAccountKeychain.SignatureType.Secp256k1;
        }

        // Skip if already authorized
        if (ghost_keyAuthorized[owner][keyId]) {
            return;
        }

        // Set expiry to future timestamp
        uint64 expiry = uint64(block.timestamp + bound(expirySeed, 1 hours, 365 days));

        // Set spending limit
        uint256 limit = bound(limitSeed, 1e6, 1000e6);

        // Simulate root key transaction (transactionKey = 0)
        vm.prank(owner);
        IAccountKeychain.TokenLimit[] memory limits = new IAccountKeychain.TokenLimit[](1);
        limits[0] = IAccountKeychain.TokenLimit({token: address(feeToken), amount: limit});

        try keychain.authorizeKey(keyId, keyType, expiry, true, limits) {
            // Update ghost state
            address[] memory tokens = new address[](1);
            tokens[0] = address(feeToken);
            uint256[] memory amounts = new uint256[](1);
            amounts[0] = limit;
            _authorizeKey(owner, keyId, expiry, true, tokens, amounts);
        } catch {
            // Authorization failed (maybe key already exists or was revoked)
        }
    }

    /// @notice Handler: Revoke an access key (secp256k1 or P256)
    /// @dev Tests K7-K8 (revoked keys rejected)
    function handler_revokeKey(uint256 actorSeed, uint256 keySeed) external {
        uint256 actorIdx = actorSeed % actors.length;
        address owner = actors[actorIdx];

        // Randomly choose between secp256k1 and P256 access keys
        bool useP256 = keySeed % 2 == 0;
        address keyId;

        if (useP256) {
            (keyId,,,) = _getActorP256AccessKey(actorIdx, keySeed);
        } else {
            (keyId,) = _getActorAccessKey(actorIdx, keySeed);
        }

        // Only revoke if authorized
        if (!ghost_keyAuthorized[owner][keyId]) {
            return;
        }

        vm.prank(owner);
        try keychain.revokeKey(keyId) {
            _revokeKey(owner, keyId);
        } catch {
            // Revocation failed
        }
    }

    /// @notice Handler: Use an authorized access key to transfer tokens
    /// @dev Tests K5 (key must exist), K9 (spending limits enforced)
    function handler_useAccessKey(uint256 actorSeed, uint256 keySeed, uint256 recipientSeed, uint256 amount) external {
        uint256 actorIdx = actorSeed % actors.length;
        address owner = actors[actorIdx];
        uint256 recipientIdx = recipientSeed % actors.length;
        if (actorIdx == recipientIdx) {
            recipientIdx = (recipientIdx + 1) % actors.length;
        }
        address recipient = actors[recipientIdx];

        // Get a secp256k1 access key
        (address keyId, uint256 keyPk) = _getActorAccessKey(actorIdx, keySeed);

        // Only use if authorized
        if (!ghost_keyAuthorized[owner][keyId]) {
            return;
        }

        // Check if key is expired
        if (ghost_keyExpiry[owner][keyId] <= block.timestamp) {
            return;
        }

        amount = bound(amount, 1e6, 50e6);

        // Check balance
        uint256 balance = feeToken.balanceOf(owner);
        if (balance < amount) {
            return;
        }

        // Check spending limit if enforced
        if (ghost_keyEnforceLimits[owner][keyId]) {
            uint256 limit = ghost_keySpendingLimit[owner][keyId][address(feeToken)];
            uint256 spent = ghost_keySpentAmount[owner][keyId][address(feeToken)];
            if (spent + amount > limit) {
                return; // Would exceed limit
            }
        }

        ghost_previousProtocolNonce[owner] = ghost_protocolNonce[owner];
        uint64 currentNonce = uint64(ghost_protocolNonce[owner]);

        // Build transaction signed by access key
        bytes memory signedTx = TxBuilder.buildLegacyCallKeychain(
            vmRlp,
            vm,
            address(feeToken),
            abi.encodeCall(ITIP20.transfer, (recipient, amount)),
            currentNonce,
            keyPk,
            owner
        );

        vm.coinbase(validator);

        // Nonce is consumed when tx is included, regardless of execution success/revert
        try vmExec.executeTransaction(signedTx) {
            ghost_protocolNonce[owner]++;
            ghost_totalTxExecuted++;
            ghost_totalCallsExecuted++;
            ghost_totalProtocolNonceTxs++;

            // Track spending for K9 invariant
            if (ghost_keyEnforceLimits[owner][keyId]) {
                _recordKeySpending(owner, keyId, address(feeToken), amount);
            }
        } catch {
            // Check if nonce was actually incremented on-chain
            uint256 actualNonce = vm.getNonce(owner);
            if (actualNonce > ghost_protocolNonce[owner]) {
                ghost_protocolNonce[owner]++;
                ghost_totalProtocolNonceTxs++;
            }
            ghost_totalTxReverted++;
        }
    }

    /// @notice Handler: Attempt transfer with insufficient balance
    /// @dev Tests F9 (insufficient balance rejected) - tx reverts but nonce is consumed
    function handler_insufficientBalanceTransfer(uint256 actorSeed, uint256 recipientSeed) external {
        uint256 senderIdx = actorSeed % actors.length;
        uint256 recipientIdx = recipientSeed % actors.length;
        if (senderIdx == recipientIdx) {
            recipientIdx = (recipientIdx + 1) % actors.length;
        }

        address sender = actors[senderIdx];
        address recipient = actors[recipientIdx];

        // Try to transfer more than balance
        uint256 balance = feeToken.balanceOf(sender);
        uint256 excessAmount = balance + 1e6;

        ghost_previousProtocolNonce[sender] = ghost_protocolNonce[sender];
        uint64 currentNonce = uint64(ghost_protocolNonce[sender]);

        bytes memory signedTx = _buildAndSignLegacyTransfer(senderIdx, recipient, excessAmount, currentNonce);

        vm.coinbase(validator);

        // On Tempo, vmExec.executeTransaction throws when the inner call reverts,
        // but the tx IS included and nonce IS consumed (same as Ethereum behavior)
        try vmExec.executeTransaction(signedTx) {
            // Tx succeeded (unexpected for insufficient balance)
            ghost_protocolNonce[sender]++;
            ghost_totalTxExecuted++;
            ghost_totalCallsExecuted++;
            ghost_totalProtocolNonceTxs++;
        } catch {
            // Tx reverted but was still included - nonce IS consumed
            // Check if nonce was actually incremented on-chain
            uint256 actualNonce = vm.getNonce(sender);
            if (actualNonce > ghost_protocolNonce[sender]) {
                // Tx was included, nonce consumed (call reverted internally)
                ghost_protocolNonce[sender]++;
                ghost_totalTxExecuted++;
                ghost_totalCallsExecuted++;
                ghost_totalProtocolNonceTxs++;
            } else {
                // Tx was truly rejected (not included)
                ghost_totalTxReverted++;
            }
        }
    }

    /*//////////////////////////////////////////////////////////////
                    NONCE INVARIANTS (N1-N5, N12-N15)
    //////////////////////////////////////////////////////////////*/

    /// @notice INVARIANT N1: Protocol nonce NEVER decreases
    function invariant_N1_protocolNonceMonotonic() public view {
        for (uint256 i = 0; i < actors.length; i++) {
            address actor = actors[i];
            uint256 currentNonce = ghost_protocolNonce[actor];
            uint256 previousNonce = ghost_previousProtocolNonce[actor];

            assertGe(currentNonce, previousNonce, "N1: Protocol nonce decreased");
        }
    }

    /// @notice INVARIANT N2: Protocol nonce matches ghost state after CALLs
    function invariant_N2_protocolNonceMatchesExpected() public view {
        for (uint256 i = 0; i < actors.length; i++) {
            address actor = actors[i];
            uint256 actualNonce = vm.getNonce(actor);
            uint256 expectedNonce = ghost_protocolNonce[actor];

            assertEq(actualNonce, expectedNonce, string(abi.encodePacked("N2: Nonce mismatch for actor ", vm.toString(i))));
        }
    }

    /// @notice INVARIANT N3: Protocol nonce transactions bump protocol nonce correctly
    /// @dev Sum of all protocol nonces == protocol nonce tx count
    /// Only Legacy txs and Tempo txs with nonceKey=0 increment protocol nonce
    function invariant_N3_protocolNonceTxsBumpNonce() public view {
        uint256 sumOfNonces = 0;
        // Sum secp256k1 actor nonces
        for (uint256 i = 0; i < actors.length; i++) {
            sumOfNonces += ghost_protocolNonce[actors[i]];
        }
        // Sum P256 address nonces
        for (uint256 i = 0; i < actors.length; i++) {
            sumOfNonces += ghost_protocolNonce[actorP256Addresses[i]];
        }
        // Protocol nonces only count Legacy + Tempo with nonceKey=0
        assertEq(sumOfNonces, ghost_totalProtocolNonceTxs, "N3: Protocol nonce sum doesn't match protocol tx count");
    }

    /// @notice INVARIANT N5: CREATE address uses protocol nonce correctly
    /// @dev Checks both secp256k1 and P256 addresses
    function invariant_N5_createAddressUsesProtocolNonce() public view {
        // Check secp256k1 actors
        for (uint256 i = 0; i < actors.length; i++) {
            _verifyCreateAddressNonce(actors[i]);
        }
        // Check P256 addresses
        for (uint256 i = 0; i < actors.length; i++) {
            _verifyCreateAddressNonce(actorP256Addresses[i]);
        }
    }

    /// @dev Helper to verify CREATE address derivation for a given account
    function _verifyCreateAddressNonce(address account) internal view {
        uint256 createCount = ghost_createCount[account];

        for (uint256 n = 0; n < createCount; n++) {
            bytes32 key = keccak256(abi.encodePacked(account, n));
            address recorded = ghost_createAddresses[key];

            if (recorded != address(0)) {
                address computed = TxBuilder.computeCreateAddress(account, n);
                assertEq(recorded, computed, "N5: CREATE address derivation mismatch");
            }
        }
    }

    /*//////////////////////////////////////////////////////////////
                    2D NONCE INVARIANTS (N6-N11)
    //////////////////////////////////////////////////////////////*/

    /// @notice INVARIANT N6: 2D nonce keys are independent
    /// @dev Each key's nonce matches its own ghost value, unaffected by other keys
    function invariant_N6_2dNonceIndependent() public view {
        for (uint256 i = 0; i < actors.length; i++) {
            address actor = actors[i];

            // Check that each used key matches its ghost value independently
            for (uint256 key = 1; key <= 10; key++) {
                if (ghost_2dNonceUsed[actor][key]) {
                    uint64 actual = nonce.getNonce(actor, key);
                    uint256 expected = ghost_2dNonce[actor][key];
                    assertEq(actual, expected, "N6: 2D nonce value mismatch - keys may not be independent");
                }
            }
        }
    }

    /// @notice INVARIANT N7: 2D nonces NEVER decrease
    function invariant_N7_2dNonceMonotonic() public view {
        for (uint256 i = 0; i < actors.length; i++) {
            address actor = actors[i];

            for (uint256 key = 1; key <= 100; key++) {
                if (ghost_2dNonceUsed[actor][key]) {
                    uint256 current = ghost_2dNonce[actor][key];
                    uint256 previous = ghost_previous2dNonce[actor][key];
                    assertGe(current, previous, "N7: 2D nonce decreased");
                }
            }
        }
    }

    /// @notice INVARIANT N8: 2D nonce doesn't affect protocol nonce
    function invariant_N8_2dNonceNoProtocolEffect() public view {
        for (uint256 i = 0; i < actors.length; i++) {
            address actor = actors[i];

            // Protocol nonce should only be affected by actual transactions
            // The sum of 2D nonce increments should NOT affect protocol nonce
            uint256 protocolNonce = vm.getNonce(actor);
            assertEq(protocolNonce, ghost_protocolNonce[actor], "N8: Protocol nonce affected by 2D nonce operations");
        }
    }

    /// @notice INVARIANT: 2D nonces match expected values
    function invariant_2dNonceMatchesExpected() public view {
        for (uint256 i = 0; i < actors.length; i++) {
            address actor = actors[i];

            for (uint256 key = 1; key <= 100; key++) {
                if (ghost_2dNonceUsed[actor][key]) {
                    uint64 actual = nonce.getNonce(actor, key);
                    uint256 expected = ghost_2dNonce[actor][key];
                    assertEq(actual, expected, string(abi.encodePacked("2D nonce mismatch for key ", vm.toString(key))));
                }
            }
        }
    }

    /*//////////////////////////////////////////////////////////////
                    CREATE INVARIANTS (C1-C9)
    //////////////////////////////////////////////////////////////*/

    /// @notice INVARIANT C5: CREATE address is deterministic
    /// @dev Verifies deployed contracts exist at computed addresses and have code
    function invariant_C5_createAddressDeterministic() public view {
        // Check secp256k1 actors
        for (uint256 i = 0; i < actors.length; i++) {
            _verifyCreateAddresses(actors[i]);
        }

        // Check P256 addresses
        for (uint256 i = 0; i < actors.length; i++) {
            _verifyCreateAddresses(actorP256Addresses[i]);
        }
    }

    /// @dev Helper to verify CREATE addresses for a given account
    function _verifyCreateAddresses(address account) internal view {
        uint256 createCount = ghost_createCount[account];

        for (uint256 n = 0; n < createCount; n++) {
            bytes32 key = keccak256(abi.encodePacked(account, n));
            address recorded = ghost_createAddresses[key];

            if (recorded != address(0)) {
                // Verify the recorded address matches the computed address
                address computed = TxBuilder.computeCreateAddress(account, n);
                assertEq(recorded, computed, "C5: Recorded address doesn't match computed");

                // Verify code exists at the address (CREATE succeeded)
                assertTrue(recorded.code.length > 0, "C5: No code at CREATE address");
            }
        }
    }

    /*//////////////////////////////////////////////////////////////
                    ACCESS KEY INVARIANTS (K1-K12)
    //////////////////////////////////////////////////////////////*/

    /// @notice INVARIANT K5: Authorized keys exist on-chain
    function invariant_K5_keyAuthorizationConsistent() public view {
        for (uint256 i = 0; i < actors.length; i++) {
            address owner = actors[i];

            for (uint256 j = 0; j < ACCESS_KEYS_PER_ACTOR; j++) {
                address keyId = actorAccessKeys[i][j];

                bool ghostAuth = ghost_keyAuthorized[owner][keyId];
                IAccountKeychain.KeyInfo memory info = keychain.getKey(owner, keyId);

                if (ghostAuth) {
                    // If ghost says authorized, chain should confirm (unless expired)
                    if (info.expiry > block.timestamp && !info.isRevoked) {
                        assertTrue(info.keyId != address(0), "K5: Authorized key not found on-chain");
                    }
                }
            }
        }
    }

    /// @notice INVARIANT K9: Spending limits are enforced
    function invariant_K9_spendingLimitEnforced() public view {
        for (uint256 i = 0; i < actors.length; i++) {
            address owner = actors[i];

            for (uint256 j = 0; j < ACCESS_KEYS_PER_ACTOR; j++) {
                address keyId = actorAccessKeys[i][j];

                if (ghost_keyAuthorized[owner][keyId] && ghost_keyEnforceLimits[owner][keyId]) {
                    uint256 limit = ghost_keySpendingLimit[owner][keyId][address(feeToken)];
                    uint256 spent = ghost_keySpentAmount[owner][keyId][address(feeToken)];

                    // Spent should never exceed limit
                    assertLe(spent, limit, "K9: Spending exceeded limit");
                }
            }
        }
    }

    /*//////////////////////////////////////////////////////////////
                    COUNTING INVARIANTS
    //////////////////////////////////////////////////////////////*/

    /// @notice INVARIANT: CREATE count matches deployed contracts
    function invariant_createCountConsistent() public view {
        uint256 totalCreates = 0;
        // Sum secp256k1 actor create counts
        for (uint256 i = 0; i < actors.length; i++) {
            totalCreates += ghost_createCount[actors[i]];
        }
        // Sum P256 address create counts
        for (uint256 i = 0; i < actors.length; i++) {
            totalCreates += ghost_createCount[actorP256Addresses[i]];
        }
        assertEq(totalCreates, ghost_totalCreatesExecuted, "CREATE count mismatch");
    }

    /// @notice INVARIANT: Calls + Creates = Total executed
    /// @dev Only successfully included transactions increment nonce and count as executed
    function invariant_callsAndCreatesEqualTotal() public view {
        assertEq(
            ghost_totalCallsExecuted + ghost_totalCreatesExecuted,
            ghost_totalTxExecuted,
            "Calls + Creates should equal total executed"
        );
    }

    /// @notice INVARIANT: Protocol nonce txs + 2D nonce txs = Total executed
    /// @dev Transactions are partitioned into protocol nonce (Legacy/Tempo with key=0) and 2D nonce (Tempo with key>0)
    function invariant_nonceTypePartition() public view {
        assertEq(
            ghost_totalProtocolNonceTxs + ghost_total2dNonceTxs,
            ghost_totalTxExecuted,
            "Nonce type partition: protocol + 2D should equal total"
        );
    }

    /*//////////////////////////////////////////////////////////////
                    BALANCE INVARIANTS (F1-F10)
    //////////////////////////////////////////////////////////////*/

    /// @notice INVARIANT F9: Sum of all actor balances is consistent
    /// @dev Total token supply minus contract holdings should equal sum of actor balances
    function invariant_F9_balanceSumConsistent() public view {
        uint256 sumOfActorBalances = 0;
        for (uint256 i = 0; i < actors.length; i++) {
            sumOfActorBalances += feeToken.balanceOf(actors[i]);
        }
        
        // Total supply should be >= sum of actor balances
        // (difference is held by contracts, fee manager, etc.)
        assertGe(
            feeToken.totalSupply(),
            sumOfActorBalances,
            "F9: Actor balances exceed total supply"
        );
    }

    /// @notice INVARIANT: Total tokens in circulation is conserved
    /// @dev Sum of all actor + P256 address balances should equal initial total minus fees/contracts
    function invariant_tokenConservation() public view {
        uint256 totalActorBalances = 0;

        // Sum secp256k1 actor balances
        for (uint256 i = 0; i < actors.length; i++) {
            totalActorBalances += feeToken.balanceOf(actors[i]);
        }

        // Sum P256 address balances
        for (uint256 i = 0; i < actors.length; i++) {
            totalActorBalances += feeToken.balanceOf(actorP256Addresses[i]);
        }

        // Total should not exceed what was originally minted to actors + P256 addresses
        // Initial: 5 actors * 100M + 5 P256 * 100M = 1000M = 1e15
        uint256 initialTotal = actors.length * 100_000_000e6 * 2;
        assertLe(totalActorBalances, initialTotal, "Token conservation violated");
    }

    /*//////////////////////////////////////////////////////////////
                    ADDITIONAL STATE CONSISTENCY
    //////////////////////////////////////////////////////////////*/

    /// @notice INVARIANT: P256 addresses track nonces independently from secp256k1
    /// @dev Verifies P256-derived addresses have correct nonce tracking
    function invariant_P256NoncesTracked() public view {
        for (uint256 i = 0; i < actors.length; i++) {
            address p256Addr = actorP256Addresses[i];
            uint256 actualNonce = vm.getNonce(p256Addr);

            // P256 address nonce should match ghost state
            assertEq(
                actualNonce,
                ghost_protocolNonce[p256Addr],
                "P256 address nonce mismatch"
            );
        }
    }

}
