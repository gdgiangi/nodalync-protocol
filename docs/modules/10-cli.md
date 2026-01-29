# Module: nodalync-cli

**Source:** Not in spec (application layer)

## Overview

Command-line interface for interacting with a Nodalync node. User-facing binary.

## Dependencies

- All `nodalync-*` crates
- `clap` — Argument parsing
- `indicatif` — Progress bars
- `colored` — Terminal colors

---

## Commands

### Identity

```bash
# Initialize new identity
nodalync init
> Identity created: ndl1abc123...
> Configuration saved to ~/.nodalync/config.toml

# Show identity
nodalync whoami
> PeerId: ndl1abc123...
> Public Key: 0x...
> Addresses: /ip4/0.0.0.0/tcp/9000
```

### Content Management

```bash
# Publish content
nodalync publish <file> [--price <amount>] [--visibility <private|unlisted|shared>]
> Hashing content...
> Extracting L1 mentions... (23 found)
> Published: QmXyz...
> Price: 0.10 HBAR
> Visibility: shared

# List local content
nodalync list [--visibility <filter>]
> SHARED (3)
>   QmXyz... "Research Paper" v3, 0.10 HBAR, 847 queries
>   QmAbc... "Analysis" v1, 0.05 HBAR, 234 queries
>
> PRIVATE (2)
>   QmJkl... "Draft Ideas" v4
>   QmMno... "Personal Notes" v1

# Update content (new version)
nodalync update <hash> <new-file>
> Previous: QmXyz... (v1)
> New: QmAbc... (v2)
> Version root: QmXyz...

# Show versions
nodalync versions <hash>
> Version root: QmXyz...
> v1: QmXyz... (2025-01-15) - shared
> v2: QmAbc... (2025-01-20) - shared [latest]

# Change visibility
nodalync visibility <hash> <private|unlisted|shared>
> Visibility updated: QmXyz... → shared

# Delete (local only)
nodalync delete <hash>
> Deleted: QmXyz... (local copy only, provenance preserved)
```

### Discovery & Querying

```bash
# Search network
nodalync search "climate change mitigation" [--max-price <amount>] [--limit <n>]
> Found 47 results
> [1] QmAbc... "IPCC Report Summary" by ndl1def... (0.05/query, 847 queries)
>     Preview: Global temperatures have risen 1.1°C since pre-industrial...
> [2] QmDef... "Carbon Capture Analysis" by ndl1ghi... (0.12/query, 234 queries)
>     Preview: Current carbon capture technology can sequester...

# Preview content (free)
nodalync preview <hash>
> Title: "IPCC Report Summary"
> Owner: ndl1def...
> Price: 0.05 HBAR
> Queries: 847
> 
> L1 Mentions (5 of 23):
> - Global temperatures have risen 1.1°C since pre-industrial
> - Net-zero by 2050 requires 45% emission reduction by 2030
> - ...

# Query content (paid)
nodalync query <hash>
> Querying QmAbc...
> Payment: 0.05 HBAR
> Content saved to ./cache/QmAbc...
```

### Synthesis

```bash
# Create L3 insight from sources
nodalync synthesize --sources <hash1>,<hash2>,... --output <file>
> Verifying sources queried... ✓
> Computing provenance (12 roots)...
> L3 hash: QmNew...
> 
> Publish now? [y/n/set price]: 0.15
> Published: QmNew... (0.15 HBAR, shared)

# Reference external L3 as L0
nodalync reference <l3-hash>
> Referencing QmXyz... as L0 for future derivations
```

### Economics

```bash
# Check balance
nodalync balance
> Protocol Balance: 127.50 HBAR
> Pending Earnings: 4.23 HBAR
> Pending Settlement: 12 payments
>
> Breakdown:
>   Direct queries: 89.20 HBAR
>   Root contributions: 38.30 HBAR

# Earnings by content
nodalync earnings [--content <hash>]
> Top earning content:
>   QmXyz... "Research Paper": 45.30 HBAR (234 queries)
>   QmAbc... "Analysis": 23.10 HBAR (462 queries, as root)

# Deposit tokens
nodalync deposit <amount>
> Depositing 50.00 HBAR...
> Transaction: 0x...
> New balance: 177.50 HBAR

# Withdraw tokens
nodalync withdraw <amount>
> Withdrawing 100.00 HBAR...
> Transaction: 0x...
> New balance: 77.50 HBAR

# Force settlement
nodalync settle
> Settling 12 pending payments...
> Batch ID: QmBatch...
> Transaction: 0x...
> Settled: 4.23 HBAR to 5 recipients
```

### Payment Channels

```bash
# Open payment channel with peer
nodalync open-channel <peer-id> --deposit 1.0
> Channel opened: QmChan...
> Peer: ndl1abc123...
> State: Open
> My Balance: 1.00 HBAR
> Their Balance: 1.00 HBAR

# List all payment channels
nodalync list-channels
> Payment Channels: 3 channels (2 open)
>   QmChan1... ndl1abc... [Open] my: 0.85 HBAR / their: 1.15 HBAR
>   QmChan2... ndl1def... [Open] my: 2.30 HBAR / their: 0.70 HBAR (5 pending)
>   QmChan3... ndl1ghi... [Closed] my: 0.00 HBAR / their: 0.00 HBAR

# Close payment channel
nodalync close-channel <peer-id>
> Channel closed: QmChan...
> Peer: ndl1abc123...
> Final Balance: my: 0.85 HBAR / their: 1.15 HBAR
```

### Node Management

```bash
# Start node (foreground)
nodalync start
> Starting Nodalync node...
> PeerId: 12D3KooW...
> Listening on /ip4/0.0.0.0/tcp/9000
> Connected to 12 peers
> DHT bootstrapped

# Start with health endpoint (for containers/monitoring)
nodalync start --health --health-port 8080
> Starting Nodalync node...
> PeerId: 12D3KooW...
> Health endpoint: http://0.0.0.0:8080/health
> Metrics endpoint: http://0.0.0.0:8080/metrics

# Start as daemon (background)
nodalync start --daemon
> Nodalync daemon started (PID: 12345)
> PeerId: 12D3KooW...

# Node status
nodalync status
> Node: running (PID: 12345)
> PeerId: 12D3KooW...
> Uptime: 4h 23m
> Peers: 12 connected
> Content: 5 shared, 2 private
> Pending: 12 payments (4.23 HBAR)

# Stop daemon
nodalync stop
> Shutting down gracefully...
> Flushing pending operations...
> Node stopped
```

**Health Endpoints** (when `--health` flag is used):

| Endpoint | Content-Type | Description |
|----------|--------------|-------------|
| `GET /health` | `application/json` | `{"status":"ok","connected_peers":N,"uptime_secs":M}` |
| `GET /metrics` | `text/plain` | Prometheus metrics format |

**Prometheus Metrics:**
- `nodalync_connected_peers` — Current peer count
- `nodalync_peer_events_total{event}` — Connect/disconnect events
- `nodalync_dht_operations_total{op,result}` — DHT put/get operations
- `nodalync_gossipsub_messages_total` — Broadcast messages received
- `nodalync_settlement_batches_total{status}` — Settlement batches
- `nodalync_settlement_latency_seconds` — Settlement operation latency
- `nodalync_queries_total` — Total queries processed
- `nodalync_query_latency_seconds` — Query latency histogram
- `nodalync_uptime_seconds` — Node uptime
- `nodalync_node_info{version,peer_id}` — Node metadata

---

## CLI Structure

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "nodalync")]
#[command(about = "Nodalync Protocol CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    
    /// Path to config file
    #[arg(short, long, default_value = "~/.nodalync/config.toml")]
    pub config: PathBuf,
    
    /// Output format
    #[arg(short, long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize new identity
    Init,
    
    /// Show identity info
    Whoami,
    
    /// Publish content
    Publish {
        file: PathBuf,
        #[arg(short, long)]
        price: Option<f64>,
        #[arg(short, long, default_value = "shared")]
        visibility: Visibility,
    },
    
    /// List local content
    List {
        #[arg(short, long)]
        visibility: Option<Visibility>,
    },
    
    /// Search network
    Search {
        query: String,
        #[arg(long)]
        max_price: Option<f64>,
        #[arg(short, long, default_value = "10")]
        limit: u32,
    },
    
    /// Preview content (free)
    Preview { hash: String },
    
    /// Query content (paid)
    Query { hash: String },
    
    /// Create L3 synthesis
    Synthesize {
        #[arg(short, long, value_delimiter = ',')]
        sources: Vec<String>,
        #[arg(short, long)]
        output: PathBuf,
    },
    
    /// Check balance
    Balance,
    
    /// Start node
    Start {
        #[arg(short, long)]
        daemon: bool,

        /// Enable HTTP health endpoint
        #[arg(long)]
        health: bool,

        /// Port for health endpoint (default: 8080)
        #[arg(long, default_value = "8080")]
        health_port: u16,
    },
    
    /// Node status
    Status,
    
    /// Stop node
    Stop,

    /// Open payment channel
    OpenChannel {
        peer_id: String,
        #[arg(short, long)]
        deposit: f64,
    },

    /// Close payment channel
    CloseChannel { peer_id: String },

    /// List payment channels
    ListChannels,

    // ... more commands
}

#[derive(Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
}
```

---

## Output Formatting

```rust
pub trait Render {
    fn render_human(&self) -> String;
    fn render_json(&self) -> String;
}

impl Render for SearchResult {
    fn render_human(&self) -> String {
        format!(
            "{} \"{}\" by {} ({}/query, {} queries)\n    Preview: {}",
            self.hash.short(),
            self.title,
            self.owner.short(),
            format_amount(self.price),
            self.total_queries,
            self.l1_summary.summary.truncate(80),
        )
    }
    
    fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }
}
```

---

## Error Handling

```rust
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Publish { file, price, visibility } => {
            let result = publish(&file, price, visibility)?;
            println!("{}", result.render(cli.format));
        }
        // ...
    }
    
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{}: {}", "Error".red().bold(), e);
        std::process::exit(1);
    }
}
```

---

## Configuration

```toml
# ~/.nodalync/config.toml

[identity]
keyfile = "~/.nodalync/identity/keypair.key"

[storage]
content_dir = "~/.nodalync/content"
database = "~/.nodalync/nodalync.db"
cache_dir = "~/.nodalync/cache"
cache_max_size_mb = 1000

[network]
listen_addresses = ["/ip4/0.0.0.0/tcp/9000"]
bootstrap_nodes = [
    "/dns4/bootstrap1.nodalync.io/tcp/9000/p2p/...",
]

[settlement]
network = "hedera-testnet"
account_id = "0.0.12345"

[economics]
default_price = 0.10  # In HBAR
auto_settle_threshold = 100.0  # In HBAR

[display]
default_format = "human"
show_previews = true
max_search_results = 20
```

---

## Test Cases

1. **init**: Creates identity and config
2. **publish**: File hashed, L1 extracted, announced
3. **search**: Returns results from network
4. **query**: Pays and retrieves content
5. **synthesize**: Creates L3 with correct provenance
6. **balance**: Shows correct amounts
7. **JSON output**: Valid JSON for all commands
8. **Error messages**: Helpful, actionable errors
9. **open-channel**: Opens channel, both sides have state
10. **list-channels**: Shows all channels with states
11. **close-channel**: Cooperative close, settles on-chain
