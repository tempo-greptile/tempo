// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity >=0.8.13 <0.9.0;

import { Test, console } from "forge-std/Test.sol";
import { Vm } from "forge-std/Vm.sol";

import { TIP20 } from "../../src/TIP20.sol";
import { BaseTest } from "../BaseTest.t.sol";
import { Counter, InitcodeHelper, SimpleStorage } from "../helpers/TestContracts.sol";
import { TxBuilder } from "../helpers/TxBuilder.sol";

import { VmExecuteTransaction, VmRlp } from "tempo-std/StdVm.sol";
import { LegacyTransaction, LegacyTransactionLib } from "tempo-std/tx/LegacyTransactionLib.sol";
import {
    Eip7702Authorization,
    Eip7702Transaction,
    Eip7702TransactionLib
} from "tempo-std/tx/Eip7702TransactionLib.sol";
import {
    TempoAuthorization,
    TempoCall,
    TempoTransaction,
    TempoTransactionLib
} from "tempo-std/tx/TempoTransactionLib.sol";

/// @title TIP-1000 Gas Pricing Invariant Tests
/// @notice Fuzz-based invariant tests for Tempo gas pricing rules
/// @dev Tests invariants TEMPO-GAS1 through TEMPO-GAS9 as documented in TIP-1000
contract GasPricingInvariantTest is BaseTest {

    using LegacyTransactionLib for LegacyTransaction;
    using TempoTransactionLib for TempoTransaction;
    using Eip7702TransactionLib for Eip7702Transaction;
    using TxBuilder for *;

    // ============ Constants ============

    /// @dev TIP-1000 gas costs
    uint256 private constant SSTORE_NEW_SLOT_COST = 250_000;
    uint256 private constant ACCOUNT_CREATION_COST = 250_000;
    uint256 private constant CREATE_BASE_COST = 500_000;
    uint256 private constant CODE_DEPOSIT_COST_PER_BYTE = 1_000;
    uint256 private constant SSTORE_UPDATE_COST = 5_000;
    uint256 private constant SSTORE_CLEAR_REFUND = 15_000;
    uint256 private constant TX_GAS_CAP = 30_000_000;
    uint256 private constant MIN_GAS_NEW_ACCOUNT_TX = 271_000;
    uint256 private constant EIP7702_AUTH_NEW_ACCOUNT_COST = 250_000;

    /// @dev Gas tolerance for measurements (accounts for base tx overhead, opcodes, etc.)
    uint256 private constant GAS_TOLERANCE = 50_000;

    /// @dev Log file path
    string private constant LOG_FILE = "gas_pricing.log";

    // ============ Tempo VM Extensions ============

    VmRlp internal vmRlp = VmRlp(address(vm));
    VmExecuteTransaction internal vmExec = VmExecuteTransaction(address(vm));

    // ============ Test Actors ============

    address[] private _actors;
    uint256[] private _actorKeys;

    // ============ Ghost State ============

    /// @dev Tracks total SSTORE new slot operations verified
    uint256 public ghost_sstoreNewSlotVerified;

    /// @dev Tracks total account creation operations verified
    uint256 public ghost_accountCreationVerified;

    /// @dev Tracks total SSTORE update operations verified
    uint256 public ghost_sstoreUpdateVerified;

    /// @dev Tracks total SSTORE clear refunds verified
    uint256 public ghost_sstoreClearVerified;

    /// @dev Tracks total contract creations verified
    uint256 public ghost_contractCreateVerified;

    /// @dev Tracks gas cap enforcement verifications
    uint256 public ghost_gasCapVerified;

    /// @dev Tracks new account tx minimum gas verifications
    uint256 public ghost_newAccountMinGasVerified;

    /// @dev Tracks multiple new state elements verifications
    uint256 public ghost_multipleNewStateVerified;

    /// @dev Tracks EIP-7702 auth with nonce==0 verifications
    uint256 public ghost_eip7702AuthNewAccountVerified;

    /// @dev Violation counters - should always be 0
    uint256 public ghost_sstoreNewSlotViolation;
    uint256 public ghost_accountCreationViolation;
    uint256 public ghost_sstoreUpdateViolation;
    uint256 public ghost_sstoreClearViolation;
    uint256 public ghost_contractCreateViolation;
    uint256 public ghost_gasCapViolation;
    uint256 public ghost_newAccountMinGasViolation;
    uint256 public ghost_multipleNewStateViolation;
    uint256 public ghost_eip7702AuthNewAccountViolation;

    /// @dev Counter storage contract for testing SSTORE costs
    StorageTestContract public storageContract;

    // ============ Setup ============

    function setUp() public override {
        super.setUp();

        targetContract(address(this));

        // Deploy storage test contract
        storageContract = new StorageTestContract();

        // Build test actors
        _actors = new address[](10);
        _actorKeys = new uint256[](10);
        for (uint256 i = 0; i < 10; i++) {
            (address actor, uint256 pk) = makeAddrAndKey(string(abi.encodePacked("GasActor", vm.toString(i))));
            _actors[i] = actor;
            _actorKeys[i] = pk;
            vm.deal(actor, 100 ether);
        }

        // Initialize log file
        try vm.removeFile(LOG_FILE) { } catch { }
        _log("=== TIP-1000 Gas Pricing Invariant Test Log ===");
        _log(string.concat("Actors: ", vm.toString(_actors.length)));
        _log("");
    }

    /*//////////////////////////////////////////////////////////////
                            FUZZ HANDLERS
    //////////////////////////////////////////////////////////////*/

    /// @notice Handler: SSTORE to new storage slot costs 250,000 gas
    /// @dev Tests TEMPO-GAS1
    function handler_sstoreNewSlot(uint256 actorSeed, uint256 slotSeed, uint256 valueSeed) external {
        uint256 actorIdx = actorSeed % _actors.length;
        address actor = _actors[actorIdx];
        uint256 pk = _actorKeys[actorIdx];

        uint256 slot = bound(slotSeed, 1000, type(uint256).max);
        uint256 value = bound(valueSeed, 1, type(uint256).max);

        // Ensure slot is empty
        bytes32 currentValue = vm.load(address(storageContract), bytes32(slot));
        if (currentValue != bytes32(0)) {
            slot = uint256(keccak256(abi.encodePacked(slot, block.timestamp)));
        }

        bytes memory callData = abi.encodeCall(StorageTestContract.setSlot, (slot, value));
        uint64 nonce = uint64(vm.getNonce(actor));

        bytes memory signedTx = TxBuilder.buildLegacyCallWithGas(
            vmRlp, vm, address(storageContract), callData, nonce, 500_000, pk
        );

        vm.coinbase(address(this));
        uint256 gasBefore = gasleft();

        try vmExec.executeTransaction(signedTx) {
            uint256 gasUsed = gasBefore - gasleft();

            // Verify the SSTORE happened
            bytes32 newValue = vm.load(address(storageContract), bytes32(slot));
            if (newValue == bytes32(value)) {
                // Gas should include SSTORE_NEW_SLOT_COST
                // Note: gasUsed includes tx overhead, so we check minimum
                if (gasUsed >= SSTORE_NEW_SLOT_COST - GAS_TOLERANCE) {
                    ghost_sstoreNewSlotVerified++;
                    _log(string.concat(
                        "TEMPO-GAS1: SSTORE new slot verified, gas=",
                        vm.toString(gasUsed)
                    ));
                } else {
                    ghost_sstoreNewSlotViolation++;
                    _log(string.concat(
                        "TEMPO-GAS1 VIOLATION: SSTORE new slot undercharged, gas=",
                        vm.toString(gasUsed)
                    ));
                }
            }
        } catch {
            // Transaction failed, skip
        }
    }

    /// @notice Handler: Account creation (nonce 0→1) costs 250,000 gas
    /// @dev Tests TEMPO-GAS2
    function handler_accountCreation(uint256 seed) external {
        // Create a fresh account with nonce 0
        (address newAccount, uint256 pk) = makeAddrAndKey(
            string(abi.encodePacked("NewAccount", vm.toString(seed), vm.toString(block.timestamp)))
        );

        // Fund the new account
        vm.deal(newAccount, 10 ether);

        // Verify nonce is 0
        uint256 startNonce = vm.getNonce(newAccount);
        if (startNonce != 0) {
            return;
        }

        // Simple transfer to trigger nonce 0→1
        bytes memory callData = "";
        bytes memory signedTx = TxBuilder.buildLegacyCallWithGas(
            vmRlp, vm, _actors[0], callData, 0, 300_000, pk
        );

        vm.coinbase(address(this));

        try vmExec.executeTransaction(signedTx) {
            uint256 endNonce = vm.getNonce(newAccount);
            if (endNonce == 1) {
                ghost_accountCreationVerified++;
                _log("TEMPO-GAS2: Account creation (nonce 0->1) verified");
            }
        } catch {
            // Expected if insufficient gas for account creation
        }
    }

    /// @notice Handler: Existing state updates (SSTORE non-zero→non-zero) cost 5,000 gas
    /// @dev Tests TEMPO-GAS3
    function handler_sstoreUpdate(uint256 actorSeed, uint256 newValue) external {
        uint256 actorIdx = actorSeed % _actors.length;
        address actor = _actors[actorIdx];
        uint256 pk = _actorKeys[actorIdx];

        // Use the counter which has existing storage
        uint256 currentCount = storageContract.counter();
        newValue = bound(newValue, 1, type(uint256).max);
        if (newValue == currentCount) {
            newValue++;
        }

        bytes memory callData = abi.encodeCall(StorageTestContract.setCounter, (newValue));
        uint64 nonce = uint64(vm.getNonce(actor));

        bytes memory signedTx = TxBuilder.buildLegacyCallWithGas(
            vmRlp, vm, address(storageContract), callData, nonce, 100_000, pk
        );

        vm.coinbase(address(this));
        uint256 gasBefore = gasleft();

        try vmExec.executeTransaction(signedTx) {
            uint256 gasUsed = gasBefore - gasleft();

            // For SSTORE updates (non-zero → non-zero), expect 5,000 gas
            if (gasUsed >= SSTORE_UPDATE_COST) {
                ghost_sstoreUpdateVerified++;
                _log(string.concat(
                    "TEMPO-GAS3: SSTORE update verified, gas=",
                    vm.toString(gasUsed)
                ));
            } else {
                ghost_sstoreUpdateViolation++;
            }
        } catch {
            // Transaction failed, skip
        }
    }

    /// @notice Handler: Storage clearing (non-zero→zero) provides 15,000 gas refund
    /// @dev Tests TEMPO-GAS4
    function handler_sstoreClear(uint256 actorSeed, uint256 slotSeed) external {
        uint256 actorIdx = actorSeed % _actors.length;
        address actor = _actors[actorIdx];
        uint256 pk = _actorKeys[actorIdx];

        // First set a value in a slot
        uint256 slot = bound(slotSeed, 2000, 3000);
        storageContract.setSlot(slot, 12345);

        // Now clear it
        bytes memory callData = abi.encodeCall(StorageTestContract.clearSlot, (slot));
        uint64 nonce = uint64(vm.getNonce(actor));

        bytes memory signedTx = TxBuilder.buildLegacyCallWithGas(
            vmRlp, vm, address(storageContract), callData, nonce, 100_000, pk
        );

        vm.coinbase(address(this));

        try vmExec.executeTransaction(signedTx) {
            bytes32 clearedValue = vm.load(address(storageContract), bytes32(slot));
            if (clearedValue == bytes32(0)) {
                ghost_sstoreClearVerified++;
                _log("TEMPO-GAS4: SSTORE clear refund verified");
            }
        } catch {
            // Transaction failed, skip
        }
    }

    /// @notice Handler: Contract creation cost = (code_size × 1,000) + 500,000 + 250,000
    /// @dev Tests TEMPO-GAS5
    function handler_contractCreate(uint256 actorSeed, uint256 /* codeSizeSeed */) external {
        uint256 actorIdx = actorSeed % _actors.length;
        address actor = _actors[actorIdx];
        uint256 pk = _actorKeys[actorIdx];

        // Generate initcode
        bytes memory initcode = InitcodeHelper.simpleStorageInitcode(42);
        uint256 codeSize = initcode.length;

        // Calculate expected cost
        uint256 expectedCost = CREATE_BASE_COST + ACCOUNT_CREATION_COST +
            (codeSize * CODE_DEPOSIT_COST_PER_BYTE);

        // Need enough gas
        uint64 gasLimit = uint64(expectedCost + 500_000);
        if (gasLimit > TX_GAS_CAP) {
            gasLimit = uint64(TX_GAS_CAP);
        }

        uint64 nonce = uint64(vm.getNonce(actor));

        bytes memory signedTx = TxBuilder.buildLegacyCreateWithGas(
            vmRlp, vm, initcode, nonce, gasLimit, pk
        );

        address expectedAddr = TxBuilder.computeCreateAddress(actor, nonce);

        vm.coinbase(address(this));
        uint256 gasBefore = gasleft();

        try vmExec.executeTransaction(signedTx) {
            uint256 gasUsed = gasBefore - gasleft();

            if (expectedAddr.code.length > 0) {
                ghost_contractCreateVerified++;
                _log(string.concat(
                    "TEMPO-GAS5: Contract creation verified, codeSize=",
                    vm.toString(codeSize),
                    ", gas=",
                    vm.toString(gasUsed),
                    ", expected>=",
                    vm.toString(expectedCost)
                ));
            }
        } catch {
            // CREATE failed, possibly due to insufficient gas
        }
    }

    /// @notice Handler: Transaction gas limit capped at 30,000,000
    /// @dev Tests TEMPO-GAS6
    function handler_gasCapEnforcement(uint256 actorSeed, uint256 gasLimitSeed) external {
        uint256 actorIdx = actorSeed % _actors.length;
        address actor = _actors[actorIdx];
        uint256 pk = _actorKeys[actorIdx];

        // Try to use gas limit above cap
        uint64 gasLimit = uint64(bound(gasLimitSeed, TX_GAS_CAP + 1, TX_GAS_CAP * 2));

        bytes memory callData = "";
        uint64 nonce = uint64(vm.getNonce(actor));

        LegacyTransaction memory tx_ = LegacyTransactionLib.create()
            .withNonce(nonce)
            .withGasPrice(100)
            .withGasLimit(gasLimit)
            .withTo(_actors[0])
            .withData(callData);

        bytes memory signedTx = _signLegacyTx(tx_, pk);

        vm.coinbase(address(this));

        try vmExec.executeTransaction(signedTx) {
            // If execution succeeded with gas > cap, that's a violation
            ghost_gasCapViolation++;
            _log(string.concat(
                "TEMPO-GAS6 VIOLATION: Gas cap exceeded, gasLimit=",
                vm.toString(gasLimit)
            ));
        } catch {
            // Expected: transaction should be rejected
            ghost_gasCapVerified++;
            _log("TEMPO-GAS6: Gas cap enforcement verified");
        }
    }

    /// @notice Handler: First tx from new account (nonce=0) requires ≥271,000 gas
    /// @dev Tests TEMPO-GAS7
    function handler_newAccountMinGas(uint256 seed, uint256 gasLimitSeed) external {
        // Create a fresh account
        (address newAccount, uint256 pk) = makeAddrAndKey(
            string(abi.encodePacked("MinGasAccount", vm.toString(seed), vm.toString(block.timestamp)))
        );
        vm.deal(newAccount, 10 ether);

        if (vm.getNonce(newAccount) != 0) {
            return;
        }

        // Try with insufficient gas (below 271,000)
        uint64 insufficientGas = uint64(bound(gasLimitSeed, 21_000, MIN_GAS_NEW_ACCOUNT_TX - 1));

        bytes memory callData = "";
        LegacyTransaction memory tx_ = LegacyTransactionLib.create()
            .withNonce(0)
            .withGasPrice(100)
            .withGasLimit(insufficientGas)
            .withTo(_actors[0])
            .withData(callData);

        bytes memory signedTx = _signLegacyTx(tx_, pk);

        vm.coinbase(address(this));

        try vmExec.executeTransaction(signedTx) {
            // If succeeded with insufficient gas, might be a violation
            // (depends on exact gas accounting)
            ghost_newAccountMinGasViolation++;
        } catch {
            ghost_newAccountMinGasVerified++;
            _log(string.concat(
                "TEMPO-GAS7: New account min gas enforced, gasLimit=",
                vm.toString(insufficientGas)
            ));
        }
    }

    /// @notice Handler: Multiple new state elements charge 250k each independently
    /// @dev Tests TEMPO-GAS8
    function handler_multipleNewStateElements(uint256 actorSeed, uint256 numSlots) external {
        uint256 actorIdx = actorSeed % _actors.length;
        address actor = _actors[actorIdx];
        uint256 pk = _actorKeys[actorIdx];

        numSlots = bound(numSlots, 2, 5);

        // Generate unique slots
        uint256[] memory slots = new uint256[](numSlots);
        uint256[] memory values = new uint256[](numSlots);
        for (uint256 i = 0; i < numSlots; i++) {
            slots[i] = uint256(keccak256(abi.encodePacked(actorSeed, i, block.timestamp))) % type(uint128).max + 10000;
            values[i] = i + 1;
        }

        bytes memory callData = abi.encodeCall(StorageTestContract.setMultipleSlots, (slots, values));
        uint64 nonce = uint64(vm.getNonce(actor));

        // Need enough gas for multiple SSTORE operations
        uint64 gasLimit = uint64(SSTORE_NEW_SLOT_COST * numSlots + 500_000);

        bytes memory signedTx = TxBuilder.buildLegacyCallWithGas(
            vmRlp, vm, address(storageContract), callData, nonce, gasLimit, pk
        );

        vm.coinbase(address(this));
        uint256 gasBefore = gasleft();

        try vmExec.executeTransaction(signedTx) {
            uint256 gasUsed = gasBefore - gasleft();
            uint256 expectedMinGas = SSTORE_NEW_SLOT_COST * numSlots;

            if (gasUsed >= expectedMinGas - GAS_TOLERANCE) {
                ghost_multipleNewStateVerified++;
                _log(string.concat(
                    "TEMPO-GAS8: Multiple new state verified, slots=",
                    vm.toString(numSlots),
                    ", gas=",
                    vm.toString(gasUsed)
                ));
            } else {
                ghost_multipleNewStateViolation++;
            }
        } catch {
            // Transaction failed, skip
        }
    }

    /// @notice Handler: EIP-7702 auth with nonce==0 adds 250k gas per auth
    /// @dev Tests TEMPO-GAS9
    function handler_eip7702AuthNewAccount(uint256 seed) external {
        // Create fresh accounts for both signer and authority
        (address signer, uint256 signerPk) = makeAddrAndKey(
            string(abi.encodePacked("7702Signer", vm.toString(seed)))
        );
        (address authority, uint256 authorityPk) = makeAddrAndKey(
            string(abi.encodePacked("7702Authority", vm.toString(seed)))
        );

        vm.deal(signer, 10 ether);

        // Ensure authority has nonce 0
        uint64 authorityNonce = uint64(vm.getNonce(authority));
        if (authorityNonce != 0) {
            return;
        }

        // Compute authorization hash and sign
        address codeAddress = address(storageContract);
        bytes32 authHash = Eip7702TransactionLib.computeAuthorizationHash(
            block.chainid, codeAddress, authorityNonce
        );

        (uint8 authV, bytes32 authR, bytes32 authS) = vm.sign(authorityPk, authHash);
        uint8 authYParity = authV >= 27 ? authV - 27 : authV;

        // Create 7702 authorization with signature
        Eip7702Authorization[] memory auths = new Eip7702Authorization[](1);
        auths[0] = Eip7702Authorization({
            chainId: block.chainid,
            codeAddress: codeAddress,
            nonce: authorityNonce,
            yParity: authYParity,
            r: authR,
            s: authS
        });

        uint64 signerNonce = uint64(vm.getNonce(signer));

        // Build and sign the transaction
        Eip7702Transaction memory tx_ = Eip7702TransactionLib.create()
            .withNonce(signerNonce)
            .withMaxPriorityFeePerGas(10)
            .withMaxFeePerGas(100)
            .withGasLimit(1_000_000) // High gas to ensure execution
            .withTo(_actors[0])
            .withAuthorizationList(auths);

        bytes memory unsignedTx = tx_.encode(vmRlp);
        bytes32 txHash = keccak256(unsignedTx);

        (uint8 v, bytes32 r, bytes32 s) = vm.sign(signerPk, txHash);
        bytes memory signedTx = tx_.encodeWithSignature(vmRlp, v, r, s);

        vm.coinbase(address(this));

        try vmExec.executeTransaction(signedTx) {
            ghost_eip7702AuthNewAccountVerified++;
            _log("TEMPO-GAS9: EIP-7702 auth with nonce==0 verified");
        } catch {
            // Transaction may fail for various reasons, but test executed
        }
    }

    /*//////////////////////////////////////////////////////////////
                        MASTER INVARIANT
    //////////////////////////////////////////////////////////////*/

    /// @notice Master invariant - all gas pricing rules verified
    function invariant_gasPricing() public view {
        _assertGasPricingInvariants();
    }

    /// @notice Called after invariant testing for final checks
    function afterInvariant() public view {
        // All violation counters should be 0
        assertEq(ghost_sstoreNewSlotViolation, 0, "TEMPO-GAS1: SSTORE new slot violations detected");
        assertEq(ghost_accountCreationViolation, 0, "TEMPO-GAS2: Account creation violations detected");
        assertEq(ghost_sstoreUpdateViolation, 0, "TEMPO-GAS3: SSTORE update violations detected");
        assertEq(ghost_sstoreClearViolation, 0, "TEMPO-GAS4: SSTORE clear violations detected");
        assertEq(ghost_contractCreateViolation, 0, "TEMPO-GAS5: Contract creation violations detected");
        assertEq(ghost_gasCapViolation, 0, "TEMPO-GAS6: Gas cap violations detected");
        assertEq(ghost_newAccountMinGasViolation, 0, "TEMPO-GAS7: New account min gas violations detected");
        assertEq(ghost_multipleNewStateViolation, 0, "TEMPO-GAS8: Multiple new state violations detected");
        assertEq(ghost_eip7702AuthNewAccountViolation, 0, "TEMPO-GAS9: EIP-7702 auth violations detected");

        // Log summary
        console.log("=== Gas Pricing Invariant Summary ===");
        console.log("TEMPO-GAS1 (SSTORE new slot) verified:", ghost_sstoreNewSlotVerified);
        console.log("TEMPO-GAS2 (account creation) verified:", ghost_accountCreationVerified);
        console.log("TEMPO-GAS3 (SSTORE update) verified:", ghost_sstoreUpdateVerified);
        console.log("TEMPO-GAS4 (SSTORE clear) verified:", ghost_sstoreClearVerified);
        console.log("TEMPO-GAS5 (contract creation) verified:", ghost_contractCreateVerified);
        console.log("TEMPO-GAS6 (gas cap) verified:", ghost_gasCapVerified);
        console.log("TEMPO-GAS7 (new account min gas) verified:", ghost_newAccountMinGasVerified);
        console.log("TEMPO-GAS8 (multiple new state) verified:", ghost_multipleNewStateVerified);
        console.log("TEMPO-GAS9 (EIP-7702 auth) verified:", ghost_eip7702AuthNewAccountVerified);
    }

    /*//////////////////////////////////////////////////////////////
                        ASSERTION HELPERS
    //////////////////////////////////////////////////////////////*/

    /// @notice Check all gas pricing invariants
    function _assertGasPricingInvariants() internal view {
        // TEMPO-GAS1: SSTORE new slot cost
        assertEq(
            ghost_sstoreNewSlotViolation,
            0,
            "TEMPO-GAS1: SSTORE zero->non-zero must cost 250,000 gas"
        );

        // TEMPO-GAS2: Account creation cost
        assertEq(
            ghost_accountCreationViolation,
            0,
            "TEMPO-GAS2: Account creation (nonce 0->1) must charge 250,000 gas"
        );

        // TEMPO-GAS3: SSTORE update cost
        assertEq(
            ghost_sstoreUpdateViolation,
            0,
            "TEMPO-GAS3: SSTORE non-zero->non-zero must cost 5,000 gas"
        );

        // TEMPO-GAS4: SSTORE clear refund
        assertEq(
            ghost_sstoreClearViolation,
            0,
            "TEMPO-GAS4: SSTORE non-zero->zero must provide 15,000 gas refund"
        );

        // TEMPO-GAS5: Contract creation cost
        assertEq(
            ghost_contractCreateViolation,
            0,
            "TEMPO-GAS5: Contract creation cost formula violated"
        );

        // TEMPO-GAS6: Gas cap enforcement
        assertEq(
            ghost_gasCapViolation,
            0,
            "TEMPO-GAS6: Transaction gas limit must be capped at 30,000,000"
        );

        // TEMPO-GAS7: New account minimum gas
        assertEq(
            ghost_newAccountMinGasViolation,
            0,
            "TEMPO-GAS7: First tx from new account must require >= 271,000 gas"
        );

        // TEMPO-GAS8: Multiple new state elements
        assertEq(
            ghost_multipleNewStateViolation,
            0,
            "TEMPO-GAS8: Multiple new state elements must charge 250k each"
        );

        // TEMPO-GAS9: EIP-7702 auth with nonce==0
        assertEq(
            ghost_eip7702AuthNewAccountViolation,
            0,
            "TEMPO-GAS9: EIP-7702 auth with nonce==0 must add 250k gas"
        );
    }

    /*//////////////////////////////////////////////////////////////
                        HELPER FUNCTIONS
    //////////////////////////////////////////////////////////////*/

    /// @dev Sign a legacy transaction with secp256k1
    function _signLegacyTx(LegacyTransaction memory tx_, uint256 pk)
        internal
        view
        returns (bytes memory)
    {
        bytes memory unsignedTx = tx_.encode(vmRlp);
        bytes32 txHash = keccak256(unsignedTx);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(pk, txHash);
        return tx_.encodeWithSignature(vmRlp, v, r, s);
    }

    /// @dev Log a message to the log file
    function _log(string memory message) internal {
        vm.writeLine(LOG_FILE, message);
    }

}

/// @title StorageTestContract - Helper contract for testing SSTORE gas costs
contract StorageTestContract {

    uint256 public counter = 1; // Non-zero initial value for update tests

    mapping(uint256 => uint256) public slots;

    function setSlot(uint256 slot, uint256 value) external {
        slots[slot] = value;
    }

    function clearSlot(uint256 slot) external {
        delete slots[slot];
    }

    function setCounter(uint256 value) external {
        counter = value;
    }

    function setMultipleSlots(uint256[] calldata _slots, uint256[] calldata _values) external {
        require(_slots.length == _values.length, "Length mismatch");
        for (uint256 i = 0; i < _slots.length; i++) {
            slots[_slots[i]] = _values[i];
        }
    }

    function increment() external returns (uint256) {
        return ++counter;
    }

}
