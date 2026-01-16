# Local Network & Benchmarking

Run a local Tempo network and benchmark it using `tempo.nu` (requires [Nushell](https://www.nushell.sh/)).

## Quick Start

```bash
# Benchmark with TIP-20 transfers on a 3-node consensus network
./tempo.nu bench --reset --accounts 10 --mode consensus --tps 9000 --duration 30 --preset tip20

# Same thing but single dev node (no consensus)
./tempo.nu bench --reset --accounts 10 --mode dev --tps 9000 --duration 30 --preset tip20
```

The `bench` command:
1. Builds the node and benchmark binaries
2. Starts the observability stack (Grafana + Prometheus)
3. Spins up the local network
4. Runs the benchmark
5. Cleans up

Grafana is available at http://localhost:3000 with a Benchmarking dashboard showing TPS and gas/s metrics.

## Commands

```bash
./tempo.nu bench [flags]        # Full benchmark run
./tempo.nu localnet [flags]     # Run localnet only
./tempo.nu infra up             # Start Grafana + Prometheus
./tempo.nu infra down           # Stop observability stack
./tempo.nu kill                 # Kill running tempo processes
```

## Bench Presets

| Preset | Description |
|--------|-------------|
| `tip20` | 100% TIP-20 transfers |
| `erc20` | 100% ERC-20 transfers |
| `swap` | 100% swaps |
| `order` | 100% order placements |
| `tempo-mix` | 80% TIP-20, 19% swaps, 1% orders |

## Common Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--mode` | `consensus` | `dev` (single node) or `consensus` (multi-node) |
| `--nodes` | `3` | Number of consensus validators |
| `--tps` | `10000` | Target transactions per second |
| `--duration` | `30` | Benchmark duration in seconds |
| `--accounts` | `1000` | Number of genesis accounts |
| `--reset` | - | Wipe and regenerate localnet data |
| `--samply` | - | Profile with samply |
| `--loud` | - | Show all node logs (default: WARN/ERROR only) |

## Examples

```bash
# High TPS test with mixed workload
./tempo.nu bench --preset tempo-mix --tps 20000 --duration 60

# Dev node with profiling
./tempo.nu bench --preset tip20 --mode dev --samply --reset

# Just run localnet (no benchmark)
./tempo.nu localnet --mode dev --accounts 50000 --reset

# 5-node consensus network
./tempo.nu localnet --mode consensus --nodes 5
```

## Port Assignments (Consensus Mode)

Per node N (0, 1, 2...):

| Port | Service |
|------|---------|
| `8000 + N*100` | Consensus |
| `8001 + N*100` | P2P |
| `8002 + N*100` | Metrics |
| `8545 + N` | HTTP RPC |
| `9001 + N` | Reth Metrics |
