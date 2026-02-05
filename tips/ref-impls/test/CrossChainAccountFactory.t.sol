// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity >=0.8.28 <0.9.0;

import { CrossChainAccount } from "../src/CrossChainAccount.sol";
import { CrossChainAccountFactory } from "../src/CrossChainAccountFactory.sol";
import { Test, console } from "forge-std/Test.sol";

contract CrossChainAccountFactoryTest is Test {

    CrossChainAccountFactory factory;
    address accountKeychain = address(0xaAAAaaAA00000000000000000000000000000000);

    // Test passkey coordinates (would be real P-256 coords in production)
    bytes32 passkeyX =
        bytes32(uint256(0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef));
    bytes32 passkeyY =
        bytes32(uint256(0xfedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321));

    function setUp() public {
        factory = new CrossChainAccountFactory(accountKeychain);
    }

    function test_getAddress_isDeterministic() public view {
        address addr1 = factory.getAddress(passkeyX, passkeyY);
        address addr2 = factory.getAddress(passkeyX, passkeyY);
        assertEq(addr1, addr2, "Address should be deterministic");
    }

    function test_getAddress_differentForDifferentKeys() public view {
        address addr1 = factory.getAddress(passkeyX, passkeyY);
        address addr2 = factory.getAddress(passkeyY, passkeyX); // Swapped
        assertTrue(addr1 != addr2, "Different keys should produce different addresses");
    }

    function test_getAddress_differentForDifferentIndex() public view {
        address addr0 = factory.getAddress(passkeyX, passkeyY, 0);
        address addr1 = factory.getAddress(passkeyX, passkeyY, 1);
        assertTrue(addr0 != addr1, "Different indices should produce different addresses");
    }

    function test_createAccount_deploysAtPredictedAddress() public {
        address predicted = factory.getAddress(passkeyX, passkeyY);

        CrossChainAccount account = factory.createAccount(passkeyX, passkeyY);

        assertEq(address(account), predicted, "Account should be at predicted address");
        assertTrue(address(account).code.length > 0, "Account should have code");
    }

    function test_createAccount_initializesCorrectly() public {
        CrossChainAccount account = factory.createAccount(passkeyX, passkeyY);

        assertEq(account.ownerX(), passkeyX, "Owner X should be set");
        assertEq(account.ownerY(), passkeyY, "Owner Y should be set");
        assertEq(account.accountKeychain(), accountKeychain, "AccountKeychain should be set");
        assertTrue(account.isAuthorizedKey(passkeyX, passkeyY), "Owner key should be authorized");
        assertTrue(account.initialized(), "Account should be initialized");
    }

    function test_createAccount_returnsExistingIfAlreadyDeployed() public {
        CrossChainAccount account1 = factory.createAccount(passkeyX, passkeyY);
        CrossChainAccount account2 = factory.createAccount(passkeyX, passkeyY);

        assertEq(address(account1), address(account2), "Should return same account");
    }

    function test_createAccount_revertsOnInvalidPasskey() public {
        vm.expectRevert(CrossChainAccountFactory.InvalidPasskey.selector);
        factory.createAccount(bytes32(0), passkeyY);

        vm.expectRevert(CrossChainAccountFactory.InvalidPasskey.selector);
        factory.createAccount(passkeyX, bytes32(0));
    }

    function test_createAccount_withIndex() public {
        CrossChainAccount account0 = factory.createAccount(passkeyX, passkeyY, 0);
        CrossChainAccount account1 = factory.createAccount(passkeyX, passkeyY, 1);

        assertTrue(
            address(account0) != address(account1),
            "Different indices should create different accounts"
        );

        // Both should be properly initialized
        assertEq(account0.ownerX(), passkeyX);
        assertEq(account1.ownerX(), passkeyX);
    }

    function test_account_canReceiveETH() public {
        CrossChainAccount account = factory.createAccount(passkeyX, passkeyY);

        vm.deal(address(this), 1 ether);
        (bool success,) = address(account).call{ value: 0.5 ether }("");

        assertTrue(success, "Should receive ETH");
        assertEq(address(account).balance, 0.5 ether);
    }

    function test_crossChainAddressDeterminism() public {
        // Simulate two factories on different chains with different accountKeychains
        CrossChainAccountFactory factoryChain1 = new CrossChainAccountFactory(address(0x1111));
        CrossChainAccountFactory factoryChain2 = new CrossChainAccountFactory(address(0x2222));

        // Note: In this test, factory addresses differ, so addresses will differ.
        // In production, use deterministic deployment to ensure factory addresses match.
        address addr1 = factoryChain1.getAddress(passkeyX, passkeyY);
        address addr2 = factoryChain2.getAddress(passkeyX, passkeyY);

        // These will differ because factory addresses differ
        // This test documents that behavior - real cross-chain determinism
        // requires deterministic factory deployment at same address
        console.log("Chain 1 address:", addr1);
        console.log("Chain 2 address:", addr2);
    }

    function test_emitsAccountCreatedEvent() public {
        vm.expectEmit(true, true, true, true);
        emit CrossChainAccountFactory.AccountCreated(
            factory.getAddress(passkeyX, passkeyY), passkeyX, passkeyY, 0
        );

        factory.createAccount(passkeyX, passkeyY);
    }

}

contract CrossChainAccountTest is Test {

    CrossChainAccountFactory factory;
    CrossChainAccount account;
    address accountKeychain = address(0xaAAAaaAA00000000000000000000000000000000);

    bytes32 passkeyX = bytes32(uint256(0x1234));
    bytes32 passkeyY = bytes32(uint256(0x5678));
    bytes32 secondKeyX = bytes32(uint256(0xaaaa));
    bytes32 secondKeyY = bytes32(uint256(0xbbbb));

    function setUp() public {
        factory = new CrossChainAccountFactory(accountKeychain);
        account = factory.createAccount(passkeyX, passkeyY);
    }

    function test_addKey() public {
        vm.prank(accountKeychain);
        account.addKey(secondKeyX, secondKeyY);

        assertTrue(account.isAuthorizedKey(secondKeyX, secondKeyY), "New key should be authorized");
    }

    function test_addKey_revertsIfNotAuthorized() public {
        vm.expectRevert(CrossChainAccount.NotAuthorized.selector);
        account.addKey(secondKeyX, secondKeyY);
    }

    function test_addKey_revertsOnInvalidKey() public {
        vm.prank(accountKeychain);
        vm.expectRevert(CrossChainAccount.InvalidKey.selector);
        account.addKey(bytes32(0), secondKeyY);
    }

    function test_addKey_revertsIfKeyExists() public {
        vm.startPrank(accountKeychain);
        account.addKey(secondKeyX, secondKeyY);

        vm.expectRevert(CrossChainAccount.KeyAlreadyExists.selector);
        account.addKey(secondKeyX, secondKeyY);
        vm.stopPrank();
    }

    function test_removeKey() public {
        vm.startPrank(accountKeychain);
        account.addKey(secondKeyX, secondKeyY);
        account.removeKey(secondKeyX, secondKeyY);
        vm.stopPrank();

        assertFalse(
            account.isAuthorizedKey(secondKeyX, secondKeyY), "Removed key should not be authorized"
        );
    }

    function test_removeKey_cannotRemovePrimaryKey() public {
        vm.prank(accountKeychain);
        vm.expectRevert(CrossChainAccount.CannotRemovePrimaryKey.selector);
        account.removeKey(passkeyX, passkeyY);
    }

    function test_execute() public {
        // Fund the account
        vm.deal(address(account), 1 ether);

        address recipient = address(0xdead);

        vm.prank(accountKeychain);
        account.execute(recipient, 0.5 ether, "");

        assertEq(recipient.balance, 0.5 ether);
    }

    function test_execute_revertsIfNotAuthorized() public {
        vm.deal(address(account), 1 ether);

        vm.expectRevert(CrossChainAccount.NotAuthorized.selector);
        account.execute(address(0xdead), 0.5 ether, "");
    }

    function test_executeBatch() public {
        vm.deal(address(account), 2 ether);

        address[] memory targets = new address[](2);
        targets[0] = address(0xdead);
        targets[1] = address(0xbeef);

        uint256[] memory values = new uint256[](2);
        values[0] = 0.5 ether;
        values[1] = 0.3 ether;

        bytes[] memory callData = new bytes[](2);
        callData[0] = "";
        callData[1] = "";

        vm.prank(accountKeychain);
        account.executeBatch(targets, values, callData);

        assertEq(targets[0].balance, 0.5 ether);
        assertEq(targets[1].balance, 0.3 ether);
    }

    function test_cannotReinitialize() public {
        vm.expectRevert(CrossChainAccount.AlreadyInitialized.selector);
        account.initialize(bytes32(uint256(0x9999)), bytes32(uint256(0x8888)), address(0x1234));
    }

    function test_emitsKeyAddedEvent() public {
        vm.expectEmit(true, true, false, true);
        emit CrossChainAccount.KeyAdded(secondKeyX, secondKeyY);

        vm.prank(accountKeychain);
        account.addKey(secondKeyX, secondKeyY);
    }

    function test_emitsKeyRemovedEvent() public {
        vm.prank(accountKeychain);
        account.addKey(secondKeyX, secondKeyY);

        vm.expectEmit(true, true, false, true);
        emit CrossChainAccount.KeyRemoved(secondKeyX, secondKeyY);

        vm.prank(accountKeychain);
        account.removeKey(secondKeyX, secondKeyY);
    }

    function test_emitsExecutedEvent() public {
        vm.deal(address(account), 1 ether);

        vm.expectEmit(true, false, false, true);
        emit CrossChainAccount.Executed(address(0xdead), 0.5 ether, "");

        vm.prank(accountKeychain);
        account.execute(address(0xdead), 0.5 ether, "");
    }

}
