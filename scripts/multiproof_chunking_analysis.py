#!/usr/bin/env python3
"""
Multiproof Chunking vs Gas Analysis for Tempo

Analyzes how multiproof chunk_size affects state root computation time
for different block sizes (especially <20M gas blocks on Tempo).

The relationship:
  gas → transactions → state changes → proof targets → chunks → parallel workers

Key reth defaults:
  - DEFAULT_MULTIPROOF_TASK_CHUNK_SIZE = 60
  - DEFAULT_MAX_TARGETS_FOR_CHUNKING = 300  (force-chunk above this)
  - Workers = 2 * available_parallelism (storage) + same for account

Usage:
  python3 multiproof_chunking_analysis.py [--rpc URL] [--blocks N] [--workers W]

Without --rpc, runs synthetic workload simulation.
With --rpc, collects real target data from a Tempo/reth node via debug_traceTransaction.
"""

import argparse
import json
import math
import os
import sys
from dataclasses import dataclass, field
from typing import Optional
import random
import csv

# ---------------------------------------------------------------------------
# 1. Transaction workload profiles (synthetic mode)
# ---------------------------------------------------------------------------

@dataclass
class TxProfile:
    name: str
    gas_mean: int
    gas_std: int
    accounts_mean: float
    accounts_std: float
    slots_mean: float
    slots_std: float
    weight: float  # relative frequency in block mix

PROFILES = {
    "simple_transfer": TxProfile(
        name="simple_transfer",
        gas_mean=21_000, gas_std=0,
        accounts_mean=2, accounts_std=0,
        slots_mean=0, slots_std=0,
        weight=0.40,
    ),
    "erc20_transfer": TxProfile(
        name="erc20_transfer",
        gas_mean=65_000, gas_std=10_000,
        accounts_mean=3, accounts_std=1,
        slots_mean=4, slots_std=2,
        weight=0.30,
    ),
    "dex_swap": TxProfile(
        name="dex_swap",
        gas_mean=180_000, gas_std=60_000,
        accounts_mean=5, accounts_std=2,
        slots_mean=20, slots_std=10,
        weight=0.15,
    ),
    "nft_mint": TxProfile(
        name="nft_mint",
        gas_mean=120_000, gas_std=30_000,
        accounts_mean=3, accounts_std=1,
        slots_mean=6, slots_std=3,
        weight=0.10,
    ),
    "complex_defi": TxProfile(
        name="complex_defi",
        gas_mean=350_000, gas_std=100_000,
        accounts_mean=8, accounts_std=3,
        slots_mean=40, slots_std=15,
        weight=0.05,
    ),
}


def sample_tx(profile: TxProfile, rng: random.Random) -> dict:
    gas = max(21_000, int(rng.gauss(profile.gas_mean, profile.gas_std)))
    accounts = max(1, int(rng.gauss(profile.accounts_mean, profile.accounts_std)))
    slots = max(0, int(rng.gauss(profile.slots_mean, profile.slots_std)))
    return {
        "type": profile.name,
        "gas": gas,
        "accounts": accounts,
        "slots": slots,
        "targets": accounts + slots,
    }


def generate_block(gas_limit: int, profiles: dict, rng: random.Random) -> dict:
    """Generate a synthetic block up to gas_limit."""
    txs = []
    gas_used = 0
    total_targets = 0
    total_accounts = 0
    total_slots = 0

    weights = [p.weight for p in profiles.values()]
    profile_list = list(profiles.values())

    while gas_used < gas_limit:
        profile = rng.choices(profile_list, weights=weights, k=1)[0]
        tx = sample_tx(profile, rng)
        if gas_used + tx["gas"] > gas_limit:
            break
        txs.append(tx)
        gas_used += tx["gas"]
        # Use unique targets (approximate dedup: ~80% unique across txs)
        total_accounts += tx["accounts"]
        total_slots += tx["slots"]

    # Approximate deduplication: overlapping accounts/slots across txs
    dedup_factor = max(0.5, 1.0 - len(txs) * 0.002)
    unique_accounts = int(total_accounts * dedup_factor)
    unique_slots = int(total_slots * dedup_factor)

    return {
        "gas_used": gas_used,
        "tx_count": len(txs),
        "total_accounts": unique_accounts,
        "total_slots": unique_slots,
        "total_targets": unique_accounts + unique_slots,
        "txs": txs,
    }


# ---------------------------------------------------------------------------
# 2. RPC-based target collection (empirical mode)
# ---------------------------------------------------------------------------

def collect_from_rpc(rpc_url: str, start_block: int, num_blocks: int) -> list[dict]:
    """Collect proof target data from a Tempo/reth node using debug_traceBlock."""
    try:
        import requests
    except ImportError:
        print("ERROR: 'requests' package required for RPC mode. Install with: pip install requests")
        sys.exit(1)

    results = []
    for block_num in range(start_block, start_block + num_blocks):
        # Get block
        resp = requests.post(rpc_url, json={
            "jsonrpc": "2.0", "method": "eth_getBlockByNumber",
            "params": [hex(block_num), True], "id": 1,
        })
        block = resp.json().get("result")
        if not block:
            continue

        gas_used = int(block["gasUsed"], 16)
        tx_count = len(block.get("transactions", []))

        # Use eth_createAccessList or debug_traceBlock to estimate targets
        total_accounts = set()
        total_slots = set()

        for tx in block.get("transactions", []):
            tx_hash = tx["hash"]
            try:
                trace_resp = requests.post(rpc_url, json={
                    "jsonrpc": "2.0", "method": "debug_traceTransaction",
                    "params": [tx_hash, {"tracer": "prestateTracer", "tracerConfig": {"diffMode": True}}],
                    "id": 2,
                })
                trace = trace_resp.json().get("result", {})
                # prestateTracer returns {address: {storage: {slot: ...}, ...}}
                for section in ["pre", "post"]:
                    if section in trace:
                        for addr, info in trace[section].items():
                            total_accounts.add(addr)
                            if "storage" in info:
                                for slot in info["storage"]:
                                    total_slots.add((addr, slot))
            except Exception as e:
                print(f"  Warning: trace failed for {tx_hash}: {e}")

        results.append({
            "block_number": block_num,
            "gas_used": gas_used,
            "tx_count": tx_count,
            "total_accounts": len(total_accounts),
            "total_slots": len(total_slots),
            "total_targets": len(total_accounts) + len(total_slots),
        })
        if block_num % 10 == 0:
            print(f"  Collected block {block_num}: gas={gas_used}, targets={len(total_accounts) + len(total_slots)}")

    return results


# ---------------------------------------------------------------------------
# 3. Multiproof chunk scheduling simulator
# ---------------------------------------------------------------------------

def simulate_block_time(
    targets: int,
    chunk_size: int,
    workers: int,
    max_targets_for_chunking: int = 300,
    t_per_target_ms: float = 0.02,
    t_chunk_overhead_ms: float = 0.15,
    t_fixed_ms: float = 0.5,
) -> dict:
    """
    Simulate state root computation time for a block.

    Returns dict with timing info.
    """
    # Determine chunking behavior
    # In reth: chunks if targets > max_targets OR if available_workers > 1
    # For simplicity, we chunk when targets > chunk_size (workers usually > 1)
    should_chunk = targets > chunk_size and workers > 1

    if should_chunk:
        n_chunks = math.ceil(targets / chunk_size)
        chunk_targets = [chunk_size] * (n_chunks - 1) + [targets - chunk_size * (n_chunks - 1)]
    else:
        n_chunks = 1
        chunk_targets = [targets]

    # Cost per chunk
    chunk_costs = [t_chunk_overhead_ms + t_per_target_ms * ct for ct in chunk_targets]

    # Greedy list scheduling (LPT)
    loads = [0.0] * workers
    for cost in sorted(chunk_costs, reverse=True):
        min_idx = min(range(workers), key=lambda j: loads[j])
        loads[min_idx] += cost

    wall_time = max(loads) + t_fixed_ms
    avg_load = sum(loads) / workers
    utilization = avg_load / max(loads) if max(loads) > 0 else 1.0

    return {
        "targets": targets,
        "chunk_size": chunk_size,
        "n_chunks": n_chunks,
        "wall_time_ms": wall_time,
        "utilization": utilization,
        "max_load_ms": max(loads),
        "overhead_ms": n_chunks * t_chunk_overhead_ms,
        "chunking_triggered": should_chunk,
    }


# ---------------------------------------------------------------------------
# 4. Analysis and reporting
# ---------------------------------------------------------------------------

def run_analysis(blocks: list[dict], workers: int, output_dir: str):
    """Run full chunk_size sweep and produce results."""
    os.makedirs(output_dir, exist_ok=True)

    chunk_sizes = [10, 15, 20, 30, 40, 50, 60, 80, 100, 120, 150, 200, 300]

    # --- Summary stats ---
    print("\n" + "=" * 70)
    print("BLOCK STATISTICS")
    print("=" * 70)
    gas_values = [b["gas_used"] for b in blocks]
    target_values = [b["total_targets"] for b in blocks]
    print(f"  Blocks analyzed: {len(blocks)}")
    print(f"  Gas range: {min(gas_values):,} - {max(gas_values):,}")
    print(f"  Gas mean: {sum(gas_values)/len(gas_values):,.0f}")
    print(f"  Target range: {min(target_values)} - {max(target_values)}")
    print(f"  Target mean: {sum(target_values)/len(target_values):.1f}")
    print(f"  Workers: {workers}")

    # --- Targets vs Gas relationship ---
    print("\n" + "=" * 70)
    print("TARGETS vs GAS (binned)")
    print("=" * 70)
    gas_bins = [0, 1_000_000, 2_000_000, 5_000_000, 10_000_000, 15_000_000, 20_000_000, 30_000_000]
    for i in range(len(gas_bins) - 1):
        lo, hi = gas_bins[i], gas_bins[i + 1]
        in_bin = [b for b in blocks if lo <= b["gas_used"] < hi]
        if in_bin:
            t = [b["total_targets"] for b in in_bin]
            targets_per_mgas = [b["total_targets"] / (b["gas_used"] / 1e6) for b in in_bin if b["gas_used"] > 0]
            print(f"  {lo/1e6:.0f}-{hi/1e6:.0f}M gas: n={len(in_bin)}, "
                  f"targets={min(t)}-{max(t)} (mean={sum(t)/len(t):.0f}), "
                  f"targets/Mgas={sum(targets_per_mgas)/len(targets_per_mgas):.1f}")

    # --- Chunk size sweep ---
    print("\n" + "=" * 70)
    print("CHUNK SIZE SWEEP")
    print("=" * 70)
    print(f"  {'chunk_size':>10} {'mean_time_ms':>12} {'p95_time_ms':>11} "
          f"{'mean_chunks':>11} {'%_chunked':>9} {'mean_util':>9}")
    print("  " + "-" * 64)

    sweep_results = []
    for cs in chunk_sizes:
        times = []
        chunks_list = []
        chunked_count = 0
        utils = []

        for block in blocks:
            result = simulate_block_time(
                targets=block["total_targets"],
                chunk_size=cs,
                workers=workers,
            )
            times.append(result["wall_time_ms"])
            chunks_list.append(result["n_chunks"])
            if result["chunking_triggered"]:
                chunked_count += 1
            utils.append(result["utilization"])

        times_sorted = sorted(times)
        p95 = times_sorted[int(len(times_sorted) * 0.95)]
        mean_time = sum(times) / len(times)
        mean_chunks = sum(chunks_list) / len(chunks_list)
        pct_chunked = chunked_count / len(blocks) * 100
        mean_util = sum(utils) / len(utils)

        print(f"  {cs:>10} {mean_time:>12.2f} {p95:>11.2f} "
              f"{mean_chunks:>11.1f} {pct_chunked:>8.1f}% {mean_util:>8.1f}%")

        sweep_results.append({
            "chunk_size": cs,
            "mean_time_ms": mean_time,
            "p95_time_ms": p95,
            "mean_chunks": mean_chunks,
            "pct_chunked": pct_chunked,
            "mean_utilization": mean_util,
        })

    # --- Find optimal ---
    best = min(sweep_results, key=lambda r: r["p95_time_ms"])
    print(f"\n  >> Optimal chunk_size (by p95): {best['chunk_size']} "
          f"(p95={best['p95_time_ms']:.2f}ms, mean={best['mean_time_ms']:.2f}ms)")

    # --- Also analyze max_targets_for_chunking threshold ---
    print("\n" + "=" * 70)
    print("MAX_TARGETS_FOR_CHUNKING SENSITIVITY (with chunk_size=60)")
    print("=" * 70)
    print(f"  {'threshold':>10} {'%_blocks_chunk':>14} {'mean_time_ms':>12} {'p95_time_ms':>11}")
    print("  " + "-" * 49)
    for threshold in [50, 100, 150, 200, 300, 500]:
        times = []
        chunked = 0
        for block in blocks:
            result = simulate_block_time(
                targets=block["total_targets"],
                chunk_size=60,
                workers=workers,
                max_targets_for_chunking=threshold,
            )
            times.append(result["wall_time_ms"])
            if result["chunking_triggered"]:
                chunked += 1
        times_sorted = sorted(times)
        p95 = times_sorted[int(len(times_sorted) * 0.95)]
        print(f"  {threshold:>10} {chunked/len(blocks)*100:>13.1f}% "
              f"{sum(times)/len(times):>12.2f} {p95:>11.2f}")

    # --- Write results CSV ---
    csv_path = os.path.join(output_dir, "chunk_sweep_results.csv")
    with open(csv_path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=sweep_results[0].keys())
        writer.writeheader()
        writer.writerows(sweep_results)

    blocks_csv_path = os.path.join(output_dir, "block_targets.csv")
    with open(blocks_csv_path, "w", newline="") as f:
        keys = ["gas_used", "tx_count", "total_accounts", "total_slots", "total_targets"]
        writer = csv.DictWriter(f, fieldnames=keys)
        writer.writeheader()
        for b in blocks:
            writer.writerow({k: b[k] for k in keys})

    print(f"\n  Results written to {csv_path}")
    print(f"  Block data written to {blocks_csv_path}")

    # --- Key insights for Tempo ---
    print("\n" + "=" * 70)
    print("KEY INSIGHTS FOR TEMPO (<20M GAS BLOCKS)")
    print("=" * 70)
    small_blocks = [b for b in blocks if b["gas_used"] < 20_000_000]
    if small_blocks:
        small_targets = [b["total_targets"] for b in small_blocks]
        mean_t = sum(small_targets) / len(small_targets)
        max_t = max(small_targets)
        pct_over_300 = sum(1 for t in small_targets if t > 300) / len(small_targets) * 100

        print(f"  Blocks <20M gas: {len(small_blocks)}/{len(blocks)}")
        print(f"  Mean targets: {mean_t:.0f}")
        print(f"  Max targets: {max_t}")
        print(f"  % exceeding DEFAULT_MAX_TARGETS (300): {pct_over_300:.1f}%")

        if mean_t < 100:
            print(f"  >> Most blocks have very few targets. Consider chunk_size=20-30")
            print(f"     or lowering max_targets_for_chunking to ~{int(mean_t * 1.5)}")
        elif mean_t < 300:
            print(f"  >> Moderate targets. chunk_size=30-60 is reasonable.")
            print(f"     Consider lowering max_targets_for_chunking to ~{int(mean_t)}")
        else:
            print(f"  >> High target count. Default chunk_size=60 likely fine.")

        if pct_over_300 < 10:
            print(f"  >> WARNING: <10% of blocks trigger chunking with threshold=300.")
            print(f"     This means multiproof runs single-threaded for most blocks.")
            print(f"     Strongly consider lowering max_targets_for_chunking.")


def main():
    parser = argparse.ArgumentParser(description="Multiproof chunking vs gas analysis")
    parser.add_argument("--rpc", type=str, help="RPC URL for empirical data collection")
    parser.add_argument("--start-block", type=int, default=0, help="Start block for RPC collection")
    parser.add_argument("--blocks", type=int, default=500, help="Number of blocks to analyze")
    parser.add_argument("--workers", type=int, default=8, help="Number of proof workers")
    parser.add_argument("--gas-limit", type=int, default=20_000_000, help="Block gas limit for synthetic mode")
    parser.add_argument("--output", type=str, default="multiproof_analysis", help="Output directory")
    parser.add_argument("--seed", type=int, default=42, help="Random seed for synthetic mode")
    args = parser.parse_args()

    if args.rpc:
        print(f"Collecting data from RPC: {args.rpc}")
        print(f"  Blocks: {args.start_block} to {args.start_block + args.blocks}")
        blocks = collect_from_rpc(args.rpc, args.start_block, args.blocks)
        if not blocks:
            print("ERROR: No blocks collected. Check RPC URL and block range.")
            sys.exit(1)
    else:
        print(f"Running synthetic simulation (no --rpc provided)")
        print(f"  Gas limit: {args.gas_limit:,}")
        print(f"  Blocks: {args.blocks}")
        rng = random.Random(args.seed)

        blocks = []
        # Generate blocks with varying gas usage (Tempo has smaller blocks)
        limit = args.gas_limit
        quartile = args.blocks // 4
        gas_targets = [rng.randint(100_000, min(2_000_000, limit)) for _ in range(quartile)]
        if limit > 2_000_000:
            gas_targets += [rng.randint(2_000_000, min(8_000_000, limit)) for _ in range(quartile)]
        else:
            gas_targets += [rng.randint(100_000, limit) for _ in range(quartile)]
        if limit > 8_000_000:
            gas_targets += [rng.randint(8_000_000, min(15_000_000, limit)) for _ in range(quartile)]
        else:
            gas_targets += [rng.randint(100_000, limit) for _ in range(quartile)]
        if limit > 15_000_000:
            gas_targets += [rng.randint(15_000_000, limit) for _ in range(quartile)]
        else:
            gas_targets += [rng.randint(100_000, limit) for _ in range(quartile)]
        rng.shuffle(gas_targets)

        for gas_target in gas_targets:
            block = generate_block(gas_target, PROFILES, rng)
            blocks.append(block)

    run_analysis(blocks, args.workers, args.output)


if __name__ == "__main__":
    main()
