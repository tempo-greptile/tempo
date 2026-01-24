# Bridge Sidecar Specification

This document specifies the bridge sidecar binary that validators run to observe cross-chain messages and produce BLS partial signatures.

## Overview

The bridge sidecar is a standalone binary that:
- Watches chains for `MessageSent` events from MessageBridge contracts
- Waits for finality on each chain
- Signs attestations with the validator's **BLS key share** (from DKG)
- Broadcasts partial signatures to other validators via P2P
- Aggregates partial signatures and submits threshold signature on-chain

Each validator runs their own sidecar instance. Validators gossip partial signatures, and any node can collect t-of-n partials, recover the threshold signature, and submit to the destination chain.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                           Bridge Sidecar Architecture                            │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│   ┌─────────────────┐           ┌─────────────────┐                             │
│   │   Chain Watcher │           │   Chain Watcher │                             │
│   │   (Ethereum)    │           │   (Tempo)       │                             │
│   │                 │           │                 │                             │
│   │  • Poll blocks  │           │  • Subscribe    │                             │
│   │  • Filter logs  │           │  • Filter logs  │                             │
│   │  • Wait final   │           │  • Wait final   │                             │
│   └────────┬────────┘           └────────┬────────┘                             │
│            │                             │                                       │
│            ▼                             ▼                                       │
│   ┌─────────────────────────────────────────────────────────────────┐           │
│   │                      Message Processor                          │           │
│   │                                                                 │           │
│   │  • Parse MessageSent events                                     │           │
│   │  • Compute attestation hash                                     │           │
│   │  • Sign with BLS key share (partial signature)                  │           │
│   │  • Broadcast partial via P2P                                    │           │
│   └─────────────────────────────────────────────────────────────────┘           │
│            │                                                                     │
│            ▼                                                                     │
│   ┌─────────────────────────────────────────────────────────────────┐           │
│   │                         Aggregator                              │           │
│   │                                                                 │           │
│   │  • Collect partial signatures from P2P                          │           │
│   │  • Wait for t-of-n threshold                                    │           │
│   │  • Recover threshold signature via Lagrange interpolation       │           │
│   └─────────────────────────────────────────────────────────────────┘           │
│            │                             │                                       │
│            ▼                             ▼                                       │
│   ┌─────────────────┐           ┌─────────────────┐                             │
│   │ Chain Submitter │           │ Chain Submitter │                             │
│   │   (Ethereum)    │           │   (Tempo)       │                             │
│   │                 │           │                 │                             │
│   │ receiveMessage()│           │ receiveMessage()│                             │
│   │ (with BLS sig)  │           │ (with BLS sig)  │                             │
│   └─────────────────┘           └─────────────────┘                             │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Configuration

```toml
# bridge-sidecar.toml

[general]
log_level = "info"
metrics_port = 9090
health_port = 8080

#=============================================================
#                      CHAIN CONFIGURATIONS
#=============================================================

[[chains]]
name = "ethereum"
chain_id = 1
rpc_url = "https://eth-mainnet.g.alchemy.com/v2/YOUR_KEY"
bridge_address = "0x..."
finality_mode = "finalized"
poll_interval_secs = 12

[[chains]]
name = "tempo"
chain_id = 12345
rpc_url = "http://localhost:8545"
bridge_address = "0x..."
finality_mode = "instant"
poll_interval_secs = 1

#=============================================================
#                         SIGNER
#=============================================================

[signer]
type = "file"
validator_index = 0
bls_key_share_file = "/path/to/validator.bls.key"

#=============================================================
#                         P2P
#=============================================================

[p2p]
listen_addr = "/ip4/0.0.0.0/tcp/9000"
bootstrap_peers = [
    "/ip4/10.0.0.1/tcp/9000/p2p/Qm...",
    "/ip4/10.0.0.2/tcp/9000/p2p/Qm..."
]

#=============================================================
#                       THRESHOLD
#=============================================================

[threshold]
threshold = 3
validator_count = 4

#=============================================================
#                      PERSISTENCE
#=============================================================

[persistence]
db_path = "/var/lib/bridge-sidecar/state.db"
```

## Core Components

### Chain Watcher

Monitors a chain for `MessageSent` events:

```rust
pub struct ChainWatcher {
    chain_id: u64,
    provider: Provider<Http>,
    bridge_address: Address,
    finality_mode: FinalityMode,
    poll_interval: Duration,
    last_block: u64,
}

impl ChainWatcher {
    pub async fn run(&mut self, tx: mpsc::Sender<MessageEvent>) -> Result<()> {
        loop {
            let finalized = self.get_finalized_block().await?;
            
            let logs = self.provider
                .get_logs(&Filter::new()
                    .address(self.bridge_address)
                    .topic0(MESSAGE_SENT_TOPIC)
                    .from_block(self.last_block + 1)
                    .to_block(finalized))
                .await?;
            
            for log in logs {
                let event = self.parse_message_sent(log)?;
                tx.send(event).await?;
            }
            
            self.last_block = finalized;
            tokio::time::sleep(self.poll_interval).await;
        }
    }
    
    async fn get_finalized_block(&self) -> Result<u64> {
        match self.finality_mode {
            FinalityMode::Finalized => {
                let block = self.provider.get_block(BlockId::finalized()).await?;
                Ok(block.number)
            }
            FinalityMode::Instant => {
                Ok(self.provider.get_block_number().await?)
            }
        }
    }
}
```

### BLS Signer

Signs attestation hashes with the validator's key share:

```rust
pub const BLS_DST: &[u8] = b"TEMPO_BRIDGE_BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_";

pub struct BLSSigner {
    share: Share,
    validator_index: u32,
}

impl BLSSigner {
    pub fn sign_partial(&self, attestation_hash: B256) -> PartialSignature {
        let partial = partial_sign_message::<MinSig>(
            &self.share,
            Some(BLS_DST),
            attestation_hash.as_slice(),
        );
        
        PartialSignature {
            index: self.validator_index,
            signature: partial.signature.compress(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialSignature {
    pub index: u32,
    pub signature: [u8; 96],
}
```

### Aggregator

Collects partial signatures and recovers threshold signature:

```rust
pub struct Aggregator {
    threshold: usize,
    sharing: Sharing<MinSig>,
    pending: HashMap<B256, PendingAttestation>,
}

struct PendingAttestation {
    message: OutboundMessage,
    partials: Vec<PartialSignature>,
    seen_indices: HashSet<u32>,
}

impl Aggregator {
    pub fn add_partial(
        &mut self,
        attestation_hash: B256,
        message: OutboundMessage,
        partial: PartialSignature,
    ) -> Option<AggregatedAttestation> {
        let pending = self.pending.entry(attestation_hash).or_default();
        
        if pending.seen_indices.contains(&partial.index) {
            return None;
        }
        pending.seen_indices.insert(partial.index);
        pending.partials.push(partial);
        
        if pending.partials.len() >= self.threshold {
            let signature = self.recover_threshold(&pending.partials)?;
            self.pending.remove(&attestation_hash);
            
            return Some(AggregatedAttestation {
                message,
                signature,
                contributors: pending.seen_indices.iter().copied().collect(),
            });
        }
        
        None
    }
    
    fn recover_threshold(&self, partials: &[PartialSignature]) -> Option<[u8; 96]> {
        let recovered = threshold_signature_recover(&self.sharing, partials).ok()?;
        Some(recovered.compress())
    }
}
```

### P2P Gossip

Broadcasts partial signatures between validators:

```rust
pub const PARTIALS_TOPIC: &str = "tempo-bridge-partials";

#[derive(Debug, Serialize, Deserialize)]
pub struct PartialGossipMessage {
    pub attestation_hash: B256,
    pub message: OutboundMessage,
    pub partial: PartialSignature,
}

pub struct P2PGossip {
    swarm: Swarm<gossipsub::Behaviour>,
    topic: gossipsub::IdentTopic,
}

impl P2PGossip {
    pub async fn broadcast_partial(
        &mut self,
        attestation_hash: B256,
        message: &OutboundMessage,
        partial: &PartialSignature,
    ) -> Result<()> {
        let msg = PartialGossipMessage {
            attestation_hash,
            message: message.clone(),
            partial: partial.clone(),
        };
        
        let encoded = bincode::serialize(&msg)?;
        self.swarm.behaviour_mut().publish(self.topic.clone(), encoded)?;
        Ok(())
    }
    
    pub async fn recv_partial(&mut self) -> Result<PartialGossipMessage> {
        loop {
            if let SwarmEvent::Behaviour(gossipsub::Event::Message { message, .. }) = 
                self.swarm.select_next_some().await 
            {
                return Ok(bincode::deserialize(&message.data)?);
            }
        }
    }
}
```

### Chain Submitter

Submits aggregated attestations to destination chain:

```rust
pub struct ChainSubmitter {
    provider: Provider<Http>,
    wallet: LocalWallet,
    bridge_address: Address,
}

impl ChainSubmitter {
    pub async fn submit_message(
        &self,
        attestation: &AggregatedAttestation,
    ) -> Result<TxHash> {
        let data = attestation.message.encode();
        
        let calldata = IMessageBridge::receiveMessageCall {
            data: data.into(),
            signature: attestation.signature.into(),
        }.abi_encode();
        
        let tx = TransactionRequest::default()
            .to(self.bridge_address)
            .data(calldata)
            .gas(300_000);
        
        let receipt = self.provider.send_transaction(tx, None).await?.await?;
        
        tracing::info!(
            tx_hash = %receipt.transaction_hash,
            message_hash = %attestation.message.message_hash,
            "Submitted message with threshold signature"
        );
        
        Ok(receipt.transaction_hash)
    }
}
```

## Main Loop

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    
    let signer = BLSSigner::from_file(&config.signer.bls_key_share_file)?;
    let aggregator = Arc::new(Mutex::new(Aggregator::new(&config)?));
    let mut gossip = P2PGossip::new(&config.p2p).await?;
    let db = StateDb::open(&config.persistence.db_path)?;
    
    let (event_tx, mut event_rx) = mpsc::channel(1000);
    
    // Spawn chain watchers
    for chain in &config.chains {
        let watcher = ChainWatcher::new(chain).await?;
        let tx = event_tx.clone();
        tokio::spawn(async move { watcher.run(tx).await });
    }
    
    // Main processing loop
    loop {
        tokio::select! {
            // Handle new events from chain watchers
            Some(event) = event_rx.recv() => {
                let attestation_hash = event.message.attestation_hash();
                
                // Sign with our key share
                let partial = signer.sign_partial(attestation_hash);
                
                // Broadcast to other validators
                gossip.broadcast_partial(attestation_hash, &event.message, &partial).await?;
                
                // Add to our aggregator
                if let Some(aggregated) = aggregator.lock().await.add_partial(
                    attestation_hash,
                    event.message.clone(),
                    partial,
                ) {
                    submit_to_destination(&aggregated).await?;
                }
            }
            
            // Handle partials from other validators
            Ok(msg) = gossip.recv_partial() => {
                // Verify partial signature before adding
                if verify_partial(&msg.partial, &msg.attestation_hash)? {
                    if let Some(aggregated) = aggregator.lock().await.add_partial(
                        msg.attestation_hash,
                        msg.message,
                        msg.partial,
                    ) {
                        submit_to_destination(&aggregated).await?;
                    }
                }
            }
        }
    }
}
```

## Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `bridge_messages_observed` | Counter | Total messages observed per chain |
| `bridge_partials_signed` | Counter | Partial signatures produced locally |
| `bridge_partials_received` | Counter | Partial signatures received via P2P |
| `bridge_aggregations_completed` | Counter | Threshold signatures recovered |
| `bridge_submissions_total` | Counter | Finalization transactions submitted |
| `bridge_submission_latency_seconds` | Histogram | Time from event to finalization |
| `bridge_last_block_processed` | Gauge | Last block processed per chain |
| `bridge_pending_aggregations` | Gauge | Messages awaiting threshold |

## Health Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Overall health status |
| `GET /ready` | Readiness (all chains connected) |
| `GET /live` | Liveness (process alive) |

## CLI Commands

```
bridge-sidecar

USAGE:
    bridge-sidecar [OPTIONS] <COMMAND>

COMMANDS:
    run           Run the sidecar (default)
    sync-once     Process pending events once and exit
    status        Show current status
    config        Validate and print config

OPTIONS:
    -c, --config <FILE>    Config file path [default: bridge-sidecar.toml]
    -v, --verbose          Increase verbosity
```

## Deployment

### Systemd Service

```ini
[Unit]
Description=Tempo Bridge Sidecar
After=network.target

[Service]
Type=simple
User=bridge
ExecStart=/usr/local/bin/bridge-sidecar run -c /etc/bridge-sidecar/config.toml
Restart=always
RestartSec=10
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```

### Docker

```dockerfile
FROM rust:1.75-slim as builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin bridge-sidecar

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/bridge-sidecar /usr/local/bin/
ENTRYPOINT ["bridge-sidecar"]
CMD ["run"]
```

## File Locations

| Component | Path |
|-----------|------|
| Main | `crates/bridge-sidecar/src/main.rs` |
| Config | `crates/bridge-sidecar/src/config.rs` |
| Chain Watcher | `crates/bridge-sidecar/src/watcher.rs` |
| BLS Signer | `crates/bridge-sidecar/src/signer.rs` |
| Aggregator | `crates/bridge-sidecar/src/aggregator.rs` |
| P2P | `crates/bridge-sidecar/src/p2p.rs` |
| Submitter | `crates/bridge-sidecar/src/submitter.rs` |
