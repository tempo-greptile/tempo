// SPDX-License-Identifier: MIT
pragma solidity ^0.8.13;

import { FeeManager } from "../../src/FeeManager.sol";
import { TIP20 } from "../../src/TIP20.sol";
import { IFeeAMM } from "../../src/interfaces/IFeeAMM.sol";
import { CommonBase } from "forge-std/Base.sol";
import { StdCheats } from "forge-std/StdCheats.sol";
import { StdUtils } from "forge-std/StdUtils.sol";

/// @title FeeAMMHandler
/// @notice Handler contract for FeeAMM invariant testing
/// @dev Wraps FeeAMM operations with bounded inputs and tracks ghost state
contract FeeAMMHandler is CommonBase, StdCheats, StdUtils {

    FeeManager public amm;
    TIP20 public userToken;
    TIP20 public validatorToken;
    bytes32 public poolId;
    address public admin;

    // Ghost variables for invariant tracking
    uint256 public ghost_totalMinted;
    uint256 public ghost_totalBurned;
    uint256 public ghost_rebalanceIn;
    uint256 public ghost_rebalanceOut;
    uint256 public ghost_feeSwapIn;
    uint256 public ghost_feeSwapOut;

    // Track LP balances per actor
    mapping(address => uint256) public ghost_lpBalances;
    address[] public actors;

    // Call counters for debugging
    uint256 public mintCalls;
    uint256 public burnCalls;
    uint256 public rebalanceCalls;
    uint256 public feeSwapCalls;

    constructor(FeeManager _amm, TIP20 _userToken, TIP20 _validatorToken, address _admin) {
        amm = _amm;
        userToken = _userToken;
        validatorToken = _validatorToken;
        poolId = amm.getPoolId(address(userToken), address(validatorToken));
        admin = _admin;

        // Setup actors
        actors.push(address(0x1001));
        actors.push(address(0x1002));
        actors.push(address(0x1003));
    }

    /// @notice Bounded mint operation
    /// @param actorSeed Seed to select actor
    /// @param amount Amount of validatorToken to deposit (will be bounded)
    function mint(uint256 actorSeed, uint256 amount) external {
        // Select actor
        address actor = actors[actorSeed % actors.length];

        // Bound amount: must be > 2000 (MIN_LIQUIDITY * 2) for first mint
        // Use 2002 to 10_000_000e18 range
        amount = bound(amount, 2002, 10_000_000e18);

        // Mint tokens to actor (handler has ISSUER_ROLE)
        validatorToken.mint(actor, amount);

        vm.startPrank(actor);
        validatorToken.approve(address(amm), amount);

        try amm.mint(address(userToken), address(validatorToken), amount, actor) returns (
            uint256 liquidity
        ) {
            ghost_totalMinted += liquidity;
            ghost_lpBalances[actor] += liquidity;
            mintCalls++;
        } catch {
            // Expected to fail sometimes (e.g., insufficient amount)
        }
        vm.stopPrank();
    }

    /// @notice Bounded burn operation
    /// @param actorSeed Seed to select actor
    /// @param pct Percentage of balance to burn (1-100)
    function burn(uint256 actorSeed, uint256 pct) external {
        address actor = actors[actorSeed % actors.length];

        uint256 balance = amm.liquidityBalances(poolId, actor);
        if (balance == 0) return;

        // Burn 1-100% of balance
        pct = bound(pct, 1, 100);
        uint256 amount = (balance * pct) / 100;
        if (amount == 0) return;

        vm.startPrank(actor);
        try amm.burn(address(userToken), address(validatorToken), amount, actor) returns (
            uint256,
            uint256
        ) {
            ghost_totalBurned += amount;
            ghost_lpBalances[actor] -= amount;
            burnCalls++;
        } catch {
            // Expected to fail sometimes
        }
        vm.stopPrank();
    }

    /// @notice Bounded rebalance swap operation
    /// @param actorSeed Seed to select actor
    /// @param amountOut Amount of userToken to receive (will be bounded)
    function rebalanceSwap(uint256 actorSeed, uint256 amountOut) external {
        address actor = actors[actorSeed % actors.length];

        IFeeAMM.Pool memory pool = amm.getPool(address(userToken), address(validatorToken));
        if (pool.reserveUserToken == 0) return;

        // Bound to available userToken reserve
        amountOut = bound(amountOut, 1, pool.reserveUserToken);

        // Calculate required input: (amountOut * N) / SCALE + 1
        uint256 amountIn = (amountOut * 9985) / 10_000 + 1;

        // Mint tokens to actor (handler has ISSUER_ROLE)
        validatorToken.mint(actor, amountIn);

        vm.startPrank(actor);
        validatorToken.approve(address(amm), amountIn);

        try amm.rebalanceSwap(address(userToken), address(validatorToken), amountOut, actor)
        returns (uint256 actualIn) {
            ghost_rebalanceIn += actualIn;
            ghost_rebalanceOut += amountOut;
            rebalanceCalls++;
        } catch {
            // May fail if reserves depleted
        }
        vm.stopPrank();
    }

    /// @notice Simulate fee swap operation
    /// @dev Fee swaps are internal and only called by protocol, so we simulate the math
    /// @param amountIn Amount of userToken to swap
    function simulateFeeSwap(uint256 amountIn) external {
        IFeeAMM.Pool memory pool = amm.getPool(address(userToken), address(validatorToken));

        // Bound amount to ensure we have liquidity
        amountIn = bound(amountIn, 1, 1_000_000e18);

        // Calculate expected output
        uint256 amountOut = (amountIn * 9970) / 10_000;

        // Skip if insufficient liquidity
        if (pool.reserveValidatorToken < amountOut) return;

        // Track the simulated swap (we can't actually call executeFeeSwap as it's internal)
        ghost_feeSwapIn += amountIn;
        ghost_feeSwapOut += amountOut;
        feeSwapCalls++;
    }

    /// @notice Helper to get sum of all LP balances
    function sumLPBalances() external view returns (uint256 total) {
        for (uint256 i = 0; i < actors.length; i++) {
            total += ghost_lpBalances[actors[i]];
        }
    }

    /// @notice Get number of actors
    function actorCount() external view returns (uint256) {
        return actors.length;
    }

    /// @notice Get actor by index
    function getActor(uint256 index) external view returns (address) {
        return actors[index];
    }

}
