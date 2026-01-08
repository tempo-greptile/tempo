// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import { BaseTest } from "../BaseTest.t.sol";
import { FeeAMMHandler } from "./FeeAMMHandler.sol";
import { TIP20 } from "../../src/TIP20.sol";
import { IFeeAMM } from "../../src/interfaces/IFeeAMM.sol";
import { StdInvariant } from "forge-std/StdInvariant.sol";
import { console } from "forge-std/console.sol";

/// @title FeeAMMInvariantTest
/// @notice Invariant tests for the FeeAMM contract
/// @dev Tests run against both Solidity reference (forge test) and Rust precompiles (tempo-forge test)
contract FeeAMMInvariantTest is StdInvariant, BaseTest {

    FeeAMMHandler public handler;
    TIP20 public userToken;
    TIP20 public validatorToken;
    bytes32 public poolId;

    function setUp() public override {
        super.setUp();

        // Create tokens for testing
        userToken =
            TIP20(factory.createToken("UserToken", "UTK", "USD", pathUSD, admin, bytes32("user")));
        validatorToken = TIP20(
            factory.createToken("ValidatorToken", "VTK", "USD", pathUSD, admin, bytes32("validator"))
        );

        // Create handler
        handler = new FeeAMMHandler(amm, userToken, validatorToken, admin);
        poolId = amm.getPoolId(address(userToken), address(validatorToken));

        // Grant issuer role to handler so it can mint tokens directly
        userToken.grantRole(_ISSUER_ROLE, address(handler));
        validatorToken.grantRole(_ISSUER_ROLE, address(handler));

        // Target only the handler
        targetContract(address(handler));
    }

    /*//////////////////////////////////////////////////////////////
                    INVARIANT A1: LP TOKEN ACCOUNTING
    //////////////////////////////////////////////////////////////*/

    /// @notice Total supply must equal MIN_LIQUIDITY + sum of all user balances
    function invariant_lpTokenAccounting() public view {
        uint256 totalSupply = amm.totalSupply(poolId);

        if (totalSupply == 0) return; // Pool not initialized

        uint256 sumBalances = handler.sumLPBalances();
        uint256 minLiquidity = amm.MIN_LIQUIDITY();

        // totalSupply = MIN_LIQUIDITY (locked) + sum of all user balances
        assertEq(totalSupply, minLiquidity + sumBalances, "LP token accounting mismatch");
    }

    /*//////////////////////////////////////////////////////////////
                    INVARIANT A2: RESERVES NEVER NEGATIVE
    //////////////////////////////////////////////////////////////*/

    /// @notice Pool reserves must never underflow (always >= 0)
    function invariant_reservesNonNegative() public view {
        IFeeAMM.Pool memory pool = amm.getPool(address(userToken), address(validatorToken));

        // uint128 can't be negative, but check they're valid
        assertTrue(pool.reserveUserToken >= 0, "User reserve negative");
        assertTrue(pool.reserveValidatorToken >= 0, "Validator reserve negative");
    }

    /*//////////////////////////////////////////////////////////////
                INVARIANT A3: NO VALUE CREATION FROM ROUNDING
    //////////////////////////////////////////////////////////////*/

    /// @notice Users cannot extract more LP tokens than minted through rounding
    function invariant_noFreeValue() public view {
        // After any sequence of operations:
        // ghost_totalBurned <= ghost_totalMinted (can't burn more LP than minted)
        assertLe(
            handler.ghost_totalBurned(),
            handler.ghost_totalMinted(),
            "Burned more LP than minted"
        );
    }

    /*//////////////////////////////////////////////////////////////
                INVARIANT A4: REBALANCE SWAP RATE CORRECTNESS
    //////////////////////////////////////////////////////////////*/

    /// @notice Rebalance swap input must be >= (output * N) / SCALE + 1
    function invariant_rebalanceSwapRateCorrect() public view {
        uint256 totalIn = handler.ghost_rebalanceIn();
        uint256 totalOut = handler.ghost_rebalanceOut();

        if (totalOut == 0) return;

        // Minimum expected input: totalOut * 9985 / 10000 + roundUp (1 per swap)
        uint256 minExpectedIn = (totalOut * 9985) / 10_000 + handler.rebalanceCalls();

        assertGe(totalIn, minExpectedIn, "Rebalance swap: insufficient input collected");
    }

    /*//////////////////////////////////////////////////////////////
                INVARIANT A5: POOL SOLVENCY
    //////////////////////////////////////////////////////////////*/

    /// @notice Pool must always have enough tokens to cover reserves
    function invariant_poolSolvency() public view {
        uint256 totalSupply = amm.totalSupply(poolId);
        if (totalSupply == 0) return;

        IFeeAMM.Pool memory pool = amm.getPool(address(userToken), address(validatorToken));

        // FeeManager must hold at least the reserve amounts
        uint256 userBalance = userToken.balanceOf(address(amm));
        uint256 validatorBalance = validatorToken.balanceOf(address(amm));

        assertGe(userBalance, pool.reserveUserToken, "Insufficient userToken balance");
        assertGe(validatorBalance, pool.reserveValidatorToken, "Insufficient validatorToken balance");
    }

    /*//////////////////////////////////////////////////////////////
                INVARIANT A6: MIN_LIQUIDITY PERMANENTLY LOCKED
    //////////////////////////////////////////////////////////////*/

    /// @notice MIN_LIQUIDITY tokens are permanently locked on first mint
    /// @dev After first mint, totalSupply >= MIN_LIQUIDITY always
    function invariant_minLiquidityLocked() public view {
        uint256 ts = amm.totalSupply(poolId);
        if (ts == 0) return; // Pool not initialized

        uint256 minLiquidity = amm.MIN_LIQUIDITY();
        assertGe(ts, minLiquidity, "Total supply below MIN_LIQUIDITY");
    }

    /*//////////////////////////////////////////////////////////////
                INVARIANT A7: FEE SWAP RATE CORRECTNESS
    //////////////////////////////////////////////////////////////*/

    /// @notice Fee swap output must be exactly (input * M) / SCALE
    /// @dev M = 9970, SCALE = 10000, so output = input * 0.997
    function invariant_feeSwapRateCorrect() public view {
        uint256 totalIn = handler.ghost_feeSwapIn();
        uint256 totalOut = handler.ghost_feeSwapOut();

        if (totalIn == 0) return;

        // Expected output with rounding down per swap
        uint256 expectedOut = (totalIn * 9970) / 10_000;

        // Each swap rounds down, so actual can be at most expected
        assertLe(totalOut, expectedOut, "Fee swap output too high");

        // Actual should be close to expected (within one unit per swap due to rounding)
        uint256 maxRoundingError = handler.feeSwapCalls();
        assertGe(totalOut + maxRoundingError, expectedOut, "Fee swap output too low");
    }

    /*//////////////////////////////////////////////////////////////
                INVARIANT A8: RESERVES BOUNDED BY UINT128
    //////////////////////////////////////////////////////////////*/

    /// @notice Pool reserves must always fit in uint128
    function invariant_reservesBounded() public view {
        IFeeAMM.Pool memory pool = amm.getPool(address(userToken), address(validatorToken));
        assertLe(pool.reserveUserToken, type(uint128).max, "User reserve overflow");
        assertLe(pool.reserveValidatorToken, type(uint128).max, "Validator reserve overflow");
    }

    /*//////////////////////////////////////////////////////////////
                        CALL SUMMARY
    //////////////////////////////////////////////////////////////*/

    /// @notice Log call statistics for debugging
    function invariant_callSummary() public view {
        console.log("=== FeeAMM Invariant Call Summary ===");
        console.log("Mint calls:", handler.mintCalls());
        console.log("Burn calls:", handler.burnCalls());
        console.log("Rebalance calls:", handler.rebalanceCalls());
        console.log("Fee swap calls:", handler.feeSwapCalls());
        console.log("Total LP minted:", handler.ghost_totalMinted());
        console.log("Total LP burned:", handler.ghost_totalBurned());
        console.log("Total rebalance in:", handler.ghost_rebalanceIn());
        console.log("Total rebalance out:", handler.ghost_rebalanceOut());
        console.log("Total fee swap in:", handler.ghost_feeSwapIn());
        console.log("Total fee swap out:", handler.ghost_feeSwapOut());
    }

}
