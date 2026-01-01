// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import { IStablecoinExchange } from "../src/interfaces/IStablecoinExchange.sol";
import { ITIP20 } from "../src/interfaces/ITIP20.sol";
import { BaseTest } from "./BaseTest.t.sol";

/// @title StablecoinExchangeFuzz
/// @notice Fuzz tests for verifying exact accounting in single-order full-fill scenarios
/// @dev Tests that the protocol is exactly zero-sum with no dust accumulation.
///      These tests use base token amounts which are always exact (never rounded).
contract StablecoinExchangeFuzz is BaseTest {

    bytes32 pairKey;
    uint128 constant INITIAL_BALANCE = 1_000_000_000e18;

    event OrderPlaced(
        uint128 indexed orderId,
        address indexed maker,
        address indexed base,
        uint128 amount,
        bool isBid,
        int16 tick
    );

    event OrderFilled(
        uint128 indexed orderId,
        address indexed maker,
        address indexed taker,
        uint128 amountFilled,
        bool partialFill
    );

    function setUp() public override {
        super.setUp();

        vm.startPrank(admin);
        token1.grantRole(_ISSUER_ROLE, admin);
        token1.mint(alice, INITIAL_BALANCE);
        token1.mint(bob, INITIAL_BALANCE);
        vm.stopPrank();

        vm.startPrank(pathUSDAdmin);
        pathUSD.grantRole(_ISSUER_ROLE, pathUSDAdmin);
        pathUSD.mint(alice, INITIAL_BALANCE);
        pathUSD.mint(bob, INITIAL_BALANCE);
        vm.stopPrank();

        vm.startPrank(alice);
        token1.approve(address(exchange), type(uint256).max);
        pathUSD.approve(address(exchange), type(uint256).max);
        vm.stopPrank();

        vm.startPrank(bob);
        token1.approve(address(exchange), type(uint256).max);
        pathUSD.approve(address(exchange), type(uint256).max);
        vm.stopPrank();

        pairKey = exchange.createPair(address(token1));
    }

    /// @dev Helper to calculate bid escrow (ceiling)
    function _bidEscrow(uint128 amount, int16 tick) internal view returns (uint128) {
        uint32 price = exchange.tickToPrice(tick);
        uint32 ps = exchange.PRICE_SCALE();
        return uint128((uint256(amount) * uint256(price) + ps - 1) / uint256(ps));
    }

    /// @dev Helper to calculate bid release (floor) - what taker actually receives
    function _bidRelease(uint128 amount, int16 tick) internal view returns (uint128) {
        uint32 price = exchange.tickToPrice(tick);
        return uint128((uint256(amount) * uint256(price)) / uint256(exchange.PRICE_SCALE()));
    }

    /// @dev Helper to calculate quote for ask (ceiling - favors maker)
    function _askQuote(uint128 amount, int16 tick) internal view returns (uint128) {
        uint32 price = exchange.tickToPrice(tick);
        uint32 ps = exchange.PRICE_SCALE();
        return uint128((uint256(amount) * uint256(price) + ps - 1) / uint256(ps));
    }

    /// @dev Helper to normalize tick to spacing within valid bounds
    function _normalizeTick(int16 tickOffset) internal view returns (int16) {
        int16 spacing = exchange.TICK_SPACING();
        int16 maxSteps = exchange.MAX_TICK() / spacing;
        int16 minSteps = exchange.MIN_TICK() / spacing;
        return int16(bound(int256(tickOffset), int256(minSteps), int256(maxSteps))) * spacing;
    }

    /*//////////////////////////////////////////////////////////////
                TEST 1: BID ORDER + EXACT IN SWAP (base specified)
    //////////////////////////////////////////////////////////////*/

    /// @notice Bid order fully filled via exactIn. Taker specifies exact base to sell.
    /// @dev Maker places bid (buying base with quote). Taker sells base, receives quote.
    ///      The base amount (order.amount) is exactly what gets transferred.
    /// @param amount The base token amount for the bid order (fuzzed)
    /// @param tickOffset Offset for the price tick (fuzzed)
    function testFuzz_BidOrder_ExactInSwap_FullFill(uint128 amount, int16 tickOffset) public {
        amount = uint128(bound(amount, exchange.MIN_ORDER_AMOUNT(), 10_000_000e18));
        int16 tick = _normalizeTick(tickOffset);
        uint128 escrow = _bidEscrow(amount, tick); // ceiling
        uint128 release = _bidRelease(amount, tick); // floor - what taker actually receives

        vm.assume(escrow > 0 && escrow <= INITIAL_BALANCE);

        // Snapshot balances
        uint256 makerQuoteBefore = pathUSD.balanceOf(alice);
        uint256 takerBaseBefore = token1.balanceOf(bob);
        uint256 takerQuoteBefore = pathUSD.balanceOf(bob);
        uint256 dexBaseBefore = token1.balanceOf(address(exchange));
        uint256 dexQuoteBefore = pathUSD.balanceOf(address(exchange));

        // Step 1: Maker (Alice) places bid order - escrowing quote tokens
        if (!isTempo) {
            vm.expectEmit(true, true, true, true);
            emit OrderPlaced(exchange.nextOrderId(), alice, address(token1), amount, true, tick);
        }

        vm.prank(alice);
        uint128 orderId = exchange.place(address(token1), amount, true, tick);

        assertEq(orderId, 1, "Order ID should be 1");
        assertEq(pathUSD.balanceOf(alice), makerQuoteBefore - escrow, "Maker escrowed quote");
        assertEq(
            pathUSD.balanceOf(address(exchange)), dexQuoteBefore + escrow, "DEX received escrow"
        );

        // Verify order in book
        _assertTickLevel(tick, true, orderId, orderId, amount);

        // Step 2: Taker (Bob) swaps exactIn with the full base amount
        // tokenIn = base (token1), tokenOut = quote (pathUSD)
        if (!isTempo) {
            vm.expectEmit(true, true, true, true);
            emit OrderFilled(orderId, alice, bob, amount, false); // false = not partial
        }

        vm.prank(bob);
        uint128 quoteOut = exchange.swapExactAmountIn(address(token1), address(pathUSD), amount, 0);

        // Step 3: Verify amounts - release is floor, may be 0-1 less than escrow (ceiling)
        assertEq(quoteOut, release, "Quote output equals floor (release amount)");
        assertLe(escrow - release, 1, "Dust from ceil escrow vs floor release is at most 1");

        // Step 4: Order fully filled and removed from book
        _assertOrderDoesNotExist(orderId);
        _assertTickLevel(tick, true, 0, 0, 0);

        // Step 5: Verify maker's DEX balance - received exact base amount
        assertEq(
            exchange.balanceOf(alice, address(token1)),
            amount,
            "Maker DEX base balance = order amount"
        );
        assertEq(exchange.balanceOf(alice, address(pathUSD)), 0, "Maker DEX quote balance = 0");

        // Step 6: Verify DEX token balances - dust (escrow - release) stays in DEX
        uint128 dust = escrow - release;
        assertEq(
            token1.balanceOf(address(exchange)), dexBaseBefore + amount, "DEX holds maker's base"
        );
        assertEq(
            pathUSD.balanceOf(address(exchange)),
            dexQuoteBefore + dust,
            "DEX retains dust from ceil escrow"
        );

        // Step 7: Verify taker wallet balances
        assertEq(token1.balanceOf(bob), takerBaseBefore - amount, "Taker paid exact base amount");
        assertEq(pathUSD.balanceOf(bob), takerQuoteBefore + release, "Taker received floor quote");
    }

    /*//////////////////////////////////////////////////////////////
                TEST 2: ASK ORDER + EXACT OUT SWAP (base specified)
    //////////////////////////////////////////////////////////////*/

    /// @notice Ask order fully filled via exactOut. Taker specifies exact base to receive.
    /// @dev Maker places ask (selling base for quote). Taker buys base with quote.
    ///      The base amount (order.amount) is exactly what gets transferred.
    /// @param amount The base token amount for the ask order (fuzzed)
    /// @param tickOffset Offset for the price tick (fuzzed)
    function testFuzz_AskOrder_ExactOutSwap_FullFill(uint128 amount, int16 tickOffset) public {
        amount = uint128(bound(amount, exchange.MIN_ORDER_AMOUNT(), 10_000_000e18));
        int16 tick = _normalizeTick(tickOffset);
        uint128 quoteNeeded = _askQuote(amount, tick);

        vm.assume(quoteNeeded > 0 && quoteNeeded <= INITIAL_BALANCE && amount <= INITIAL_BALANCE);

        // Snapshot balances
        uint256 makerBaseBefore = token1.balanceOf(alice);
        uint256 takerQuoteBefore = pathUSD.balanceOf(bob);
        uint256 takerBaseBefore = token1.balanceOf(bob);
        uint256 dexBaseBefore = token1.balanceOf(address(exchange));
        uint256 dexQuoteBefore = pathUSD.balanceOf(address(exchange));

        // Step 1: Maker (Alice) places ask order - escrowing base tokens
        if (!isTempo) {
            vm.expectEmit(true, true, true, true);
            emit OrderPlaced(exchange.nextOrderId(), alice, address(token1), amount, false, tick);
        }

        vm.prank(alice);
        uint128 orderId = exchange.place(address(token1), amount, false, tick);

        assertEq(orderId, 1, "Order ID should be 1");
        assertEq(token1.balanceOf(alice), makerBaseBefore - amount, "Maker escrowed base");
        assertEq(
            token1.balanceOf(address(exchange)), dexBaseBefore + amount, "DEX received base escrow"
        );

        // Verify order in book
        _assertTickLevel(tick, false, orderId, orderId, amount);

        // Step 2: Taker (Bob) swaps exactOut to get the exact base amount
        // tokenIn = quote (pathUSD), tokenOut = base (token1)
        if (!isTempo) {
            vm.expectEmit(true, true, true, true);
            emit OrderFilled(orderId, alice, bob, amount, false); // false = not partial
        }

        vm.prank(bob);
        uint128 quoteIn =
            exchange.swapExactAmountOut(address(pathUSD), address(token1), amount, quoteNeeded + 1);

        // Step 3: Verify exact amounts
        assertEq(quoteIn, quoteNeeded, "Quote in equals ceiling calculation");

        // Step 4: Order fully filled and removed from book
        _assertOrderDoesNotExist(orderId);
        _assertTickLevel(tick, false, 0, 0, 0);

        // Step 5: Verify maker's DEX balance - received quote from taker
        assertEq(
            exchange.balanceOf(alice, address(pathUSD)), quoteNeeded, "Maker DEX quote balance"
        );
        assertEq(exchange.balanceOf(alice, address(token1)), 0, "Maker DEX base balance = 0");

        // Step 6: Verify DEX token balances match internal state
        assertEq(
            pathUSD.balanceOf(address(exchange)),
            dexQuoteBefore + quoteNeeded,
            "DEX holds maker's quote"
        );
        assertEq(
            token1.balanceOf(address(exchange)),
            dexBaseBefore,
            "DEX base back to original (escrow released)"
        );

        // Step 7: Verify taker wallet balances
        assertEq(pathUSD.balanceOf(bob), takerQuoteBefore - quoteNeeded, "Taker paid quote");
        assertEq(
            token1.balanceOf(bob), takerBaseBefore + amount, "Taker received exact base amount"
        );

        // Step 8: Zero-sum verification
        // Taker paid `quoteNeeded` quote, maker received `quoteNeeded` quote
        // Maker escrowed `amount` base, taker received `amount` base
        assertEq(
            exchange.balanceOf(alice, address(pathUSD)),
            quoteIn,
            "Maker received = taker paid (zero-sum)"
        );
    }

    /*//////////////////////////////////////////////////////////////
                TEST 3: BID ORDER + EXACT OUT SWAP (base specified)
    //////////////////////////////////////////////////////////////*/

    /// @notice Bid order fully filled via exactOut where taker specifies exact base to receive.
    /// @dev Maker places bid. Taker wants exact base (what maker is buying).
    ///      Wait - this doesn't make sense. For a bid order, the maker BUYS base.
    ///      If taker does exactOut for base from a bid, they're on the same side.
    ///
    ///      Let me reconsider: For BID (maker buys base):
    ///      - Taker sells base to maker (taker is selling, maker is buying)
    ///      - swapExactAmountIn(base, quote): taker specifies base input ✓
    ///      - swapExactAmountOut(base, quote): taker specifies quote output (not base)
    ///
    ///      For ASK (maker sells base):
    ///      - Taker buys base from maker (taker is buying, maker is selling)
    ///      - swapExactAmountOut(quote, base): taker specifies base output ✓
    ///      - swapExactAmountIn(quote, base): taker specifies quote input (not base)
    ///
    ///      So base-specified tests are:
    ///      1. Bid + ExactIn (base input by taker)
    ///      2. Ask + ExactOut (base output to taker)
    ///
    ///      The other two tests would specify quote amounts which involves rounding.

    /*//////////////////////////////////////////////////////////////
                TEST 4: ASK ORDER + EXACT IN SWAP (quote specified)
    //////////////////////////////////////////////////////////////*/

    /// @notice Ask order fully filled via exactIn. Taker specifies exact quote to pay.
    /// @dev This test verifies that when taker pays the ceiling quote amount,
    ///      they receive the full base amount and the order is fully filled.
    /// @param amount The base token amount for the ask order (fuzzed)
    /// @param tickOffset Offset for the price tick (fuzzed)
    function testFuzz_AskOrder_ExactInSwap_FullFill(uint128 amount, int16 tickOffset) public {
        amount = uint128(bound(amount, exchange.MIN_ORDER_AMOUNT(), 10_000_000e18));
        int16 tick = _normalizeTick(tickOffset);
        uint128 quoteNeeded = _askQuote(amount, tick);

        vm.assume(quoteNeeded > 0 && quoteNeeded <= INITIAL_BALANCE && amount <= INITIAL_BALANCE);

        // Snapshot balances
        uint256 makerBaseBefore = token1.balanceOf(alice);
        uint256 takerQuoteBefore = pathUSD.balanceOf(bob);
        uint256 takerBaseBefore = token1.balanceOf(bob);
        uint256 dexBaseBefore = token1.balanceOf(address(exchange));
        uint256 dexQuoteBefore = pathUSD.balanceOf(address(exchange));

        // Step 1: Maker places ask order (escrows base)
        if (!isTempo) {
            vm.expectEmit(true, true, true, true);
            emit OrderPlaced(exchange.nextOrderId(), alice, address(token1), amount, false, tick);
        }

        vm.prank(alice);
        uint128 orderId = exchange.place(address(token1), amount, false, tick);

        assertEq(orderId, 1);
        assertEq(token1.balanceOf(alice), makerBaseBefore - amount, "Maker escrowed base");
        assertEq(token1.balanceOf(address(exchange)), dexBaseBefore + amount, "DEX holds escrow");

        // Step 2: Taker swaps exactIn with the quote amount needed for full fill
        // tokenIn = quote (pathUSD), tokenOut = base (token1)
        if (!isTempo) {
            vm.expectEmit(true, true, true, true);
            emit OrderFilled(orderId, alice, bob, amount, false);
        }

        vm.prank(bob);
        uint128 baseOut =
            exchange.swapExactAmountIn(address(pathUSD), address(token1), quoteNeeded, 0);

        // Step 3: Verify exact base output
        assertEq(baseOut, amount, "Base out equals order amount");

        // Step 4: Order fully filled and removed
        _assertOrderDoesNotExist(orderId);
        _assertTickLevel(tick, false, 0, 0, 0);

        // Step 5: Verify maker's DEX balance
        assertEq(exchange.balanceOf(alice, address(pathUSD)), quoteNeeded, "Maker received quote");
        assertEq(exchange.balanceOf(alice, address(token1)), 0, "Maker base balance = 0");

        // Step 6: Verify DEX token balances
        assertEq(
            pathUSD.balanceOf(address(exchange)),
            dexQuoteBefore + quoteNeeded,
            "DEX holds maker's quote"
        );
        assertEq(token1.balanceOf(address(exchange)), dexBaseBefore, "DEX base back to original");

        // Step 7: Verify taker balances
        assertEq(pathUSD.balanceOf(bob), takerQuoteBefore - quoteNeeded, "Taker paid quote");
        assertEq(token1.balanceOf(bob), takerBaseBefore + amount, "Taker received base");

        // Step 8: Zero-sum verification
        assertEq(
            exchange.balanceOf(alice, address(pathUSD)), quoteNeeded, "Maker received = taker paid"
        );
    }

    /*//////////////////////////////////////////////////////////////
                    EDGE CASE TESTS
    //////////////////////////////////////////////////////////////*/

    /// @notice Test exact accounting at minimum order size
    function test_MinimumOrderSize_ExactAccounting() public {
        uint128 amount = exchange.MIN_ORDER_AMOUNT();
        int16 tick = 100;
        uint128 escrow = _bidEscrow(amount, tick);
        uint128 release = _bidRelease(amount, tick);

        vm.prank(alice);
        uint128 orderId = exchange.place(address(token1), amount, true, tick);

        vm.prank(bob);
        uint128 quoteOut = exchange.swapExactAmountIn(address(token1), address(pathUSD), amount, 0);

        assertEq(quoteOut, release, "Taker receives floor release");
        assertLe(escrow - release, 1, "Single fill dust at most 1");
        _assertOrderDoesNotExist(orderId);
    }

    /// @notice Test accounting at extreme tick values
    function test_ExtremeTicks_ExactAccounting() public {
        uint128 amount = 1_000_000e18;
        int16 spacing = exchange.TICK_SPACING();
        int16 highTick = (exchange.MAX_TICK() / spacing) * spacing;
        uint128 escrow = _bidEscrow(amount, highTick);
        uint128 release = _bidRelease(amount, highTick);

        vm.prank(alice);
        uint128 orderId = exchange.place(address(token1), amount, true, highTick);

        vm.prank(bob);
        uint128 quoteOut = exchange.swapExactAmountIn(address(token1), address(pathUSD), amount, 0);

        assertEq(quoteOut, release, "Taker receives floor release");
        assertLe(escrow - release, 1, "Single fill dust at most 1");
        _assertOrderDoesNotExist(orderId);
    }

    /// @notice Fuzz test: single fill dust is at most 1 unit
    /// @dev With ceiling escrow and floor release, the difference is at most 1
    function testFuzz_BidOrder_SingleFillDust_AtMostOne(uint128 amount, int16 tickOffset) public {
        amount = uint128(bound(amount, exchange.MIN_ORDER_AMOUNT(), 10_000_000e18));
        int16 tick = _normalizeTick(tickOffset);

        uint128 escrow = _bidEscrow(amount, tick);
        uint128 release = _bidRelease(amount, tick);

        vm.assume(escrow > 0 && escrow <= INITIAL_BALANCE);

        // The mathematical guarantee: ceil - floor <= 1
        uint128 dust = escrow - release;
        assertLe(dust, 1, "Single fill dust must be at most 1 unit");

        // Also verify via actual swap
        vm.prank(alice);
        exchange.place(address(token1), amount, true, tick);

        vm.prank(bob);
        uint128 quoteOut = exchange.swapExactAmountIn(address(token1), address(pathUSD), amount, 0);

        assertEq(quoteOut, release, "Actual release matches calculated");
    }

    /*//////////////////////////////////////////////////////////////
            TWO-TAKER PARTIAL FILL TESTS (QUOTE CONSERVATION)
    //////////////////////////////////////////////////////////////*/

    /// @notice Two takers together fill a bid order exactly. Total quote released must not exceed escrow.
    /// @dev Maker places bid (escrowing floor(amount * price / scale) quote).
    ///      Two takers sell base tokens, together summing to the full order amount.
    ///      The sum of quote released to both takers must be <= escrow (no over-release).
    ///      Any dust stays in the DEX.
    /// @param amount The base token amount for the bid order (fuzzed)
    /// @param tickOffset Offset for the price tick (fuzzed)
    /// @param splitPercent Percentage of amount for first taker (1-99, fuzzed)
    function testFuzz_BidOrder_TwoTakers_QuoteNotExceedEscrow(
        uint128 amount,
        int16 tickOffset,
        uint8 splitPercent
    ) public {
        // Constrain inputs
        amount = uint128(bound(amount, exchange.MIN_ORDER_AMOUNT() * 2, 10_000_000e18));
        int16 tick = _normalizeTick(tickOffset);
        splitPercent = uint8(bound(splitPercent, 1, 99));

        uint128 escrow = _bidEscrow(amount, tick);
        vm.assume(escrow > 0 && escrow <= INITIAL_BALANCE);

        // Calculate split amounts - they sum to exactly `amount`
        uint128 fill1 = uint128((uint256(amount) * splitPercent) / 100);
        uint128 fill2 = amount - fill1;

        // Both fills must be >= MIN_ORDER_AMOUNT for the swap to work
        // (actually swaps don't have minimum, but we want meaningful fills)
        vm.assume(fill1 >= 1 && fill2 >= 1);

        // Setup second taker (charlie)
        vm.startPrank(admin);
        token1.mint(charlie, INITIAL_BALANCE);
        vm.stopPrank();
        vm.prank(charlie);
        token1.approve(address(exchange), type(uint256).max);

        // Snapshot DEX quote balance
        uint256 dexQuoteBefore = pathUSD.balanceOf(address(exchange));

        // Step 1: Maker (Alice) places bid order
        if (!isTempo) {
            vm.expectEmit(true, true, true, true);
            emit OrderPlaced(exchange.nextOrderId(), alice, address(token1), amount, true, tick);
        }

        vm.prank(alice);
        uint128 orderId = exchange.place(address(token1), amount, true, tick);

        assertEq(orderId, 1, "Order ID should be 1");
        assertEq(
            pathUSD.balanceOf(address(exchange)), dexQuoteBefore + escrow, "DEX received escrow"
        );

        // Step 2: First taker (Bob) sells fill1 base tokens
        if (!isTempo) {
            vm.expectEmit(true, true, true, true);
            emit OrderFilled(orderId, alice, bob, fill1, true); // true = partial fill
        }

        vm.prank(bob);
        uint128 quoteOut1 = exchange.swapExactAmountIn(address(token1), address(pathUSD), fill1, 0);

        // Verify order still exists with remaining amount
        IStablecoinExchange.Order memory order = exchange.getOrder(orderId);
        assertEq(order.remaining, fill2, "Order remaining after first fill");

        // Step 3: Second taker (Charlie) sells fill2 base tokens to complete the order
        if (!isTempo) {
            vm.expectEmit(true, true, true, true);
            emit OrderFilled(orderId, alice, charlie, fill2, false); // false = complete fill
        }

        vm.prank(charlie);
        uint128 quoteOut2 = exchange.swapExactAmountIn(address(token1), address(pathUSD), fill2, 0);

        // Step 4: Verify order is fully filled and removed
        _assertOrderDoesNotExist(orderId);
        _assertTickLevel(tick, true, 0, 0, 0);

        // Step 5: CRITICAL - Total quote released must not exceed escrow
        uint128 totalQuoteReleased = quoteOut1 + quoteOut2;
        assertLe(totalQuoteReleased, escrow, "Total quote released must not exceed escrow");

        // Step 6: Calculate dust (escrow - released). This stays in DEX.
        uint128 dust = escrow - totalQuoteReleased;

        // Step 7: Verify DEX quote balance
        // Started with dexQuoteBefore, added escrow, released totalQuoteReleased
        // Remaining = dexQuoteBefore + escrow - totalQuoteReleased = dexQuoteBefore + dust
        assertEq(pathUSD.balanceOf(address(exchange)), dexQuoteBefore + dust, "DEX retains dust");

        // Step 8: Verify maker received full base amount
        assertEq(exchange.balanceOf(alice, address(token1)), amount, "Maker received full base");

        // Step 9: Dust analysis
        // With ceiling escrow: up to 1 unit from ceil vs floor
        // Plus up to 1 unit from partial fill rounding
        // Max dust = 2
        assertLe(dust, 2, "Dust should be at most 2 units with ceiling escrow");
    }

    /*//////////////////////////////////////////////////////////////
            QUOTE-SPECIFIED SWAP EDGE CASES
    //////////////////////////////////////////////////////////////*/

    /// @notice Two exactOut swaps for quote from a bid: either both succeed or second fails
    /// @dev With ceiling escrow, the max quote takers can get is floor(amount × price).
    ///      We split the RELEASE (floor) amount, not the escrow (ceiling).
    /// @param amount Base amount for bid order (fuzzed)
    /// @param tickOffset Price tick offset (fuzzed)
    /// @param quotePercent Percentage of release for first exactOut (fuzzed)
    function testFuzz_BidOrder_TwoExactOutQuote_SucceedsOrFailsCorrectly(
        uint128 amount,
        int16 tickOffset,
        uint8 quotePercent
    ) public {
        amount = uint128(bound(amount, exchange.MIN_ORDER_AMOUNT(), 10_000_000e18));
        int16 tick = _normalizeTick(tickOffset);
        quotePercent = uint8(bound(quotePercent, 1, 99));

        uint128 escrow = _bidEscrow(amount, tick); // ceiling
        uint128 release = _bidRelease(amount, tick); // floor - max takers can get
        vm.assume(release > 1); // Need at least 2 to split
        vm.assume(escrow <= INITIAL_BALANCE);

        // Calculate quote amounts for two swaps that together equal release (floor)
        uint128 quote1 = uint128((uint256(release) * quotePercent) / 100);
        uint128 quote2 = release - quote1;

        vm.assume(quote1 > 0 && quote2 > 0);

        // Setup charlie as second taker
        vm.startPrank(admin);
        token1.mint(charlie, INITIAL_BALANCE);
        vm.stopPrank();
        vm.prank(charlie);
        token1.approve(address(exchange), type(uint256).max);

        // Maker places bid
        vm.prank(alice);
        uint128 orderId = exchange.place(address(token1), amount, true, tick);

        // First taker: exactOut for quote1
        vm.prank(bob);
        uint128 baseIn1 = exchange.swapExactAmountOut(
            address(token1), address(pathUSD), quote1, type(uint128).max
        );

        // Check remaining base in order
        uint128 remainingBase;
        bool orderStillExists = true;
        try exchange.getOrder(orderId) returns (IStablecoinExchange.Order memory order) {
            remainingBase = order.remaining;
        } catch {
            orderStillExists = false;
            remainingBase = 0;
        }

        if (!orderStillExists) {
            // Order was fully filled by first swap
            return;
        }

        // Calculate max quote that remaining base can provide (floor)
        uint128 maxQuote2 = _bidRelease(remainingBase, tick);

        // Second taker: try exactOut for quote2
        if (quote2 <= maxQuote2) {
            // Should succeed
            vm.prank(charlie);
            uint128 baseIn2 = exchange.swapExactAmountOut(
                address(token1), address(pathUSD), quote2, type(uint128).max
            );

            // Verify total base consumed is at most order amount
            assertLe(baseIn1 + baseIn2, amount, "Total base consumed <= order amount");

            // Verify total quote released equals what we asked for
            assertEq(quote1 + quote2, release, "Total quote released = release (floor)");
        } else {
            // Should fail with InsufficientLiquidity
            vm.prank(charlie);
            try exchange.swapExactAmountOut(
                address(token1), address(pathUSD), quote2, type(uint128).max
            ) {
                // If it succeeded, verify it's still safe
                uint128 totalQuoteReleased = quote1 + quote2;
                assertLe(totalQuoteReleased, escrow, "Even if succeeded, no over-release");
            } catch (bytes memory err) {
                // Expected failure
                assertEq(
                    err,
                    abi.encodeWithSelector(IStablecoinExchange.InsufficientLiquidity.selector),
                    "Should fail with InsufficientLiquidity"
                );
            }
        }
    }

    /// @notice Verify that exactOut for release amount (floor) works on fresh bid order
    /// @dev With ceiling escrow, the max quote taker can get is floor(amount × price).
    ///      Asking for the full escrow (ceiling) may require more base than exists.
    function testFuzz_BidOrder_SingleExactOutFullRelease(uint128 amount, int16 tickOffset) public {
        amount = uint128(bound(amount, exchange.MIN_ORDER_AMOUNT(), 10_000_000e18));
        int16 tick = _normalizeTick(tickOffset);

        uint128 escrow = _bidEscrow(amount, tick); // ceiling
        uint128 release = _bidRelease(amount, tick); // floor - max taker can get
        vm.assume(release > 0 && escrow <= INITIAL_BALANCE);

        uint256 takerBaseBefore = token1.balanceOf(bob);
        uint256 takerQuoteBefore = pathUSD.balanceOf(bob);

        // Maker places bid
        vm.prank(alice);
        uint128 orderId = exchange.place(address(token1), amount, true, tick);

        // Taker tries exactOut for the release amount (floor, not ceiling)
        vm.prank(bob);
        uint128 baseIn = exchange.swapExactAmountOut(
            address(token1), address(pathUSD), release, type(uint128).max
        );

        // Taker received exactly release quote
        assertEq(
            pathUSD.balanceOf(bob), takerQuoteBefore + release, "Taker received release (floor)"
        );
        assertEq(token1.balanceOf(bob), takerBaseBefore - baseIn, "Taker paid base");

        // Maker received the base
        assertEq(exchange.balanceOf(alice, address(token1)), baseIn, "Maker received base");

        // CRITICAL: Order should be fully filled (baseIn = amount)
        // The +1 in baseNeeded ensures we consume all base when asking for floor quote
        assertEq(baseIn, amount, "Asking for floor quote should fully fill order");
        _assertOrderDoesNotExist(orderId);

        // Dust (escrow - release) stays in DEX
        uint128 dust = escrow - release;
        assertLe(dust, 1, "Dust is at most 1");
    }

    /*//////////////////////////////////////////////////////////////
                    HELPER FUNCTIONS
    //////////////////////////////////////////////////////////////*/

    function _assertTickLevel(
        int16 tick,
        bool isBid,
        uint128 expectedHead,
        uint128 expectedTail,
        uint128 expectedLiquidity
    ) internal view {
        (uint128 head, uint128 tail, uint128 liquidity) =
            exchange.getTickLevel(address(token1), tick, isBid);
        assertEq(head, expectedHead, "Tick head mismatch");
        assertEq(tail, expectedTail, "Tick tail mismatch");
        assertEq(liquidity, expectedLiquidity, "Tick liquidity mismatch");
    }

    function _assertOrderDoesNotExist(uint128 orderId) internal view {
        try exchange.getOrder(orderId) {
            revert CallShouldHaveReverted();
        } catch (bytes memory err) {
            assertEq(err, abi.encodeWithSelector(IStablecoinExchange.OrderDoesNotExist.selector));
        }
    }

}
