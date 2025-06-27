# Reth-Malachite Test Network

This directory contains scripts and configurations for running a local test network of reth-malachite nodes.

## Prerequisites

The testnet scripts require the following tools:
- `openssl` - For generating validator keys
- `xxd` - For hex encoding/decoding
- `jq` or `python3` - For JSON processing in genesis generation

## Quick Start

```bash
# Launch a 3-node network (default)
./spawn.sh

# Launch a 4-node network
./spawn.sh 4

# Launch with custom block time
BLOCK_TIME=2s ./spawn.sh 4

# Clean up all nodes
./spawn.sh clean
```

## Network Configurations

### 3-Node Network (Default)
- Suitable for basic testing
- All validators have equal voting power
- Minimal resource usage

### 4-Node Network
- Tests Byzantine fault tolerance (can tolerate 1 faulty node)
- Better for consensus testing
- Recommended for development

### Custom Configurations
You can customize the network by setting environment variables:

```bash
# Set block time (default: 1s)
BLOCK_TIME=2s ./spawn.sh 3

# Example for slower block production
BLOCK_TIME=5s ./spawn.sh 4
```

## Directory Structure

```
testnet/
├── spawn.sh              # Main script to launch network
├── config/
│   └── template.toml    # Node configuration template
├── scripts/
│   ├── generate_keys.sh # Generate validator keys
│   ├── generate_genesis.sh # Create genesis file
│   └── monitor.sh       # Real-time network monitoring
└── nodes/               # Runtime directory (created on launch)
    ├── node0/
    ├── node1/
    └── ...
```

## Port Allocation

Each node uses the following ports (base + node_id):

| Service | Base Port | Node 0 | Node 1 | Node 2 | Node 3 |
|---------|-----------|---------|---------|---------|---------|
| Consensus P2P | 26656 | 26656 | 26657 | 26658 | 26659 |
| Metrics | 9000 | 9000 | 9001 | 9002 | 9003 |
| Reth P2P | 30303 | 30303 | 30304 | 30305 | 30306 |
| Reth HTTP RPC | 8545 | 8545 | 8546 | 8547 | 8548 |
| Engine API | 8551 | 8551 | 8552 | 8553 | 8554 |

## Monitoring

### Using the Monitor Script
The testnet includes a monitoring script that provides real-time network status:

```bash
# Start monitoring
./scripts/monitor.sh

# The monitor displays:
# - Node status (running/stopped)
# - Current block height
# - Peer connections
# - Consensus activity
# - Recent logs
```

### View Logs Manually
```bash
# Follow a specific node
tail -f nodes/node0/node.log

# Follow all nodes
tail -f nodes/node*/node.log

# Search for errors
grep ERROR nodes/node*/node.log

# Watch consensus messages
tail -f nodes/node0/node.log | grep consensus
```

### Metrics
Each node exposes Prometheus metrics:
- Node 0: http://localhost:9000/metrics
- Node 1: http://localhost:9001/metrics
- Node 2: http://localhost:9002/metrics
- Node 3: http://localhost:9003/metrics

### RPC Access
Connect to nodes via JSON-RPC:
```bash
# Check node 0 status
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'

# Get peer count
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"net_peerCount","params":[],"id":1}'
```

## Troubleshooting

### Nodes not connecting
- Check that all ports are available
- Verify peer addresses in node configs
- Look for connection errors in logs

### Consensus not progressing
- Ensure at least 2/3 of validators are online
- Check for time synchronization issues
- Verify genesis configuration matches across nodes

### High CPU usage
- Normal during initial sync
- Check metrics for excessive message rates
- Consider increasing block time

## Advanced Usage

### Custom Genesis
Modify `scripts/generate_genesis.sh` to:
- Change validator voting power
- Add initial accounts
- Set custom chain parameters

### Network Topology
Edit peer connections in `spawn.sh` to create:
- Star topology (all connect to node0)
- Ring topology (each connects to neighbors)
- Custom patterns for testing partitions