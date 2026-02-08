# Nodalync Architecture

This document defines the module structure, dependencies, and implementation order for the Nodalync protocol.

## Module Dependency Graph

```
                  ┌──────────────────┐     ┌──────────────────┐
                  │  nodalync-cli    │     │  nodalync-mcp    │
                  │  (binary crate)  │     │  (MCP server)    │
                  └────────┬─────────┘     └────────┬─────────┘
                           │                        │
                           └───────────┬────────────┘
                                       │
          ┌────────────────────────────┼────────────────────────┐
          │                            │                        │
          ▼                            │                        ▼
   ┌─────────────┐                     │                 ┌──────────────┐
   │ nodalync-net│                     │                 │nodalync-settle│
   │  (P2P/DHT)  │                     │                 │   (chain)    │
   └──────┬──────┘                     │                 └──────┬───────┘
          │                            │                        │
          │        ┌───────────────────┘                        │
          │        │                                            │
          │        ▼                                            │
          │ ┌─────────────┐                                     │
          ├─│ nodalync-ops│                                     │
          │ │ (operations)│                                     │
          │ └──────┬──────┘                                     │
          │        │                                            │
          │ ┌──────┴──────┐                                     │
          │ │             │                                     │
          ▼ ▼             ▼                                     ▼
   ┌─────────────┐  ┌───────────┐ ┌───────────┐  ┌───────────┐
   │nodalync-wire│  │nodalync-  │ │nodalync-  │  │nodalync-  │
   │(serialization)│ │  store   │ │  valid    │  │   econ    │
   └──────┬──────┘  └─────┬─────┘ └─────┬─────┘  └─────┬─────┘
          │               │             │              │
          └───────────────┴──────┬──────┴──────────────┘
                                 │
                                 ▼
                         ┌─────────────┐
                         │nodalync-types│
                         │ (all structs)│
                         └──────┬──────┘
                                │
                                ▼
                         ┌─────────────┐
                         │nodalync-crypto│
                         │(hash, sign)  │
                         └─────────────┘
```

**Note:** `nodalync-net` depends on `nodalync-ops` to dispatch incoming messages to the appropriate handlers.

## Crates Overview

| Crate | Purpose | Spec Sections | Dependencies |
|-------|---------|---------------|--------------|
| `nodalync-crypto` | Hashing, signing, identity | §3 | None (external: sha2, ed25519-dalek) |
| `nodalync-types` | All data structures | §4 | crypto |
| `nodalync-wire` | Message serialization/deserialization | §6, Appendix A | types |
| `nodalync-store` | Local content & manifest storage | §5 | types |
| `nodalync-valid` | All validation rules | §9 | types |
| `nodalync-econ` | Revenue distribution math | §10 | types |
| `nodalync-ops` | Protocol operations (CREATE, QUERY, etc) | §7 | store, valid, econ, wire |
| `nodalync-net` | P2P networking, DHT | §11 | wire, ops |
| `nodalync-settle` | Blockchain settlement | §12 | econ, types |
| `nodalync-cli` | Command-line interface | — | all |
| `nodalync-mcp` | MCP server for AI agents | — | ops, store, net, settle |

## Key Interfaces (Traits)

Each crate exposes traits that define its contract. Implementations can vary (e.g., in-memory vs SQLite storage) but must satisfy the trait.

### `nodalync-crypto`
```rust
pub trait ContentHasher {
    fn hash(content: &[u8]) -> Hash;
    fn verify(content: &[u8], expected: &Hash) -> bool;
}

pub trait Signer {
    fn sign(&self, message: &[u8]) -> Signature;
    fn verify(public_key: &PublicKey, message: &[u8], signature: &Signature) -> bool;
}

pub trait Identity {
    fn generate() -> Self;
    fn public_key(&self) -> &PublicKey;
    fn peer_id(&self) -> PeerId;
    fn sign(&self, message: &[u8]) -> Signature;
}
```

### `nodalync-store`
```rust
pub trait ContentStore {
    fn store(&mut self, hash: &Hash, content: &[u8]) -> Result<()>;
    fn load(&self, hash: &Hash) -> Result<Option<Vec<u8>>>;
    fn exists(&self, hash: &Hash) -> bool;
    fn delete(&mut self, hash: &Hash) -> Result<()>;
}

pub trait ManifestStore {
    fn store(&mut self, manifest: &Manifest) -> Result<()>;
    fn load(&self, hash: &Hash) -> Result<Option<Manifest>>;
    fn list(&self, filter: ManifestFilter) -> Result<Vec<Manifest>>;
    fn update(&mut self, manifest: &Manifest) -> Result<()>;
}

pub trait ProvenanceGraph {
    fn add(&mut self, hash: &Hash, derived_from: &[Hash]) -> Result<()>;
    fn get_roots(&self, hash: &Hash) -> Result<Vec<ProvenanceEntry>>;
    fn get_derivations(&self, hash: &Hash) -> Result<Vec<Hash>>;
}
```

### `nodalync-valid`
```rust
pub trait Validator {
    fn validate_content(&self, content: &[u8], manifest: &Manifest) -> Result<()>;
    fn validate_version(&self, manifest: &Manifest, previous: Option<&Manifest>) -> Result<()>;
    fn validate_provenance(&self, manifest: &Manifest, sources: &[Manifest]) -> Result<()>;
    fn validate_payment(&self, payment: &Payment, channel: &Channel, manifest: &Manifest) -> Result<()>;
    fn validate_message(&self, message: &Message) -> Result<()>;
    fn validate_access(&self, requester: &PeerId, manifest: &Manifest) -> Result<()>;
}
```

### `nodalync-econ`
```rust
pub trait Distributor {
    fn distribute(&self, payment: &Payment, provenance: &[ProvenanceEntry]) -> Vec<Distribution>;
    fn calculate_batch(&self, payments: &[Payment]) -> SettlementBatch;
}
```

### `nodalync-ops`
```rust
pub trait Operations {
    // Content operations
    fn create(&mut self, content: &[u8], content_type: ContentType, metadata: Metadata) -> Result<Hash>;
    fn publish(&mut self, hash: &Hash, visibility: Visibility, price: Amount) -> Result<()>;
    fn update(&mut self, old_hash: &Hash, new_content: &[u8]) -> Result<Hash>;
    fn derive(&mut self, sources: &[Hash], insight: &[u8], metadata: Metadata) -> Result<Hash>;
    
    // Query operations
    fn preview(&self, hash: &Hash) -> Result<(Manifest, L1Summary)>;
    fn query(&mut self, hash: &Hash, payment: Payment) -> Result<QueryResponse>;
}
```

### `nodalync-net`
```rust
pub trait Network {
    fn announce(&self, hash: &Hash, manifest: &Manifest) -> Result<()>;
    fn search(&self, query: &str, filters: SearchFilters) -> Result<Vec<SearchResult>>;
    fn send(&self, peer: &PeerId, message: Message) -> Result<()>;
    fn receive(&mut self) -> Result<(PeerId, Message)>;
}
```

### `nodalync-settle`
```rust
pub trait Settlement {
    fn submit_batch(&self, batch: SettlementBatch) -> Result<TransactionId>;
    fn verify_settlement(&self, tx_id: &TransactionId) -> Result<SettlementStatus>;
    fn open_channel(&self, peer: &PeerId, deposit: Amount) -> Result<ChannelId>;
    fn close_channel(&self, channel_id: &ChannelId) -> Result<TransactionId>;
}
```

## Testing Strategy

Each crate has three test levels:

1. **Unit tests** — Test individual functions
   - Location: `src/*.rs` (inline `#[cfg(test)]` modules)
   - Run: `cargo test -p nodalync-{crate}`

2. **Integration tests** — Test crate as a whole
   - Location: `crates/nodalync-{crate}/tests/`
   - Run: `cargo test -p nodalync-{crate} --test '*'`

3. **Spec compliance tests** — Verify against spec validation rules
   - Location: `crates/nodalync-{crate}/tests/spec_compliance.rs`
   - These tests are derived directly from spec §9
   - Each test references the specific spec section it validates

## Error Handling

All crates use a common error type:

```rust
// In nodalync-types
#[derive(Debug, thiserror::Error)]
pub enum NodalyncError {
    #[error("Content validation failed: {0}")]
    ContentValidation(String),
    
    #[error("Provenance validation failed: {0}")]
    ProvenanceValidation(String),
    
    #[error("Payment validation failed: {0}")]
    PaymentValidation(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Settlement error: {0}")]
    Settlement(String),
    
    // Maps to spec Appendix C error codes
    #[error("Protocol error {code}: {message}")]
    Protocol { code: u16, message: String },
}
```

## Configuration

Node configuration lives in a platform-specific data directory (unless overridden by `NODALYNC_DATA_DIR`):

- **macOS**: `~/Library/Application Support/io.nodalync.nodalync/config.toml`
- **Linux**: `~/.local/share/nodalync/config.toml` (or `$XDG_DATA_HOME/nodalync/`)
- **Windows**: `%APPDATA%\nodalync\nodalync\config.toml`

Example `config.toml` (generated by `nodalync init`):

```toml
[identity]
keyfile = "<data_dir>/identity/keypair.key"

[storage]
content_dir = "<data_dir>/content"
database = "<data_dir>/nodalync.db"
cache_dir = "<data_dir>/cache"
cache_max_size_mb = 1000

[network]
enabled = true
listen_addresses = ["/ip4/0.0.0.0/tcp/9000"]
bootstrap_nodes = [
    "/dns4/nodalync-bootstrap.eastus.azurecontainer.io/tcp/9000/p2p/12D3KooWMqrUmZm4e1BJTRMWqKHCe1TSX9Vu83uJLEyCGr2dUjYm",
]

[settlement]
network = "hedera-testnet"
auto_deposit = false

[economics]
default_price = 0.10  # In HBAR
```

## File Layout

The codebase uses a workspace with two groups of crates:

```
crates/
├── protocol/                # Core protocol crates (v0.7.x)
│   ├── nodalync-crypto/
│   ├── nodalync-types/
│   ├── nodalync-wire/
│   ├── nodalync-store/
│   ├── nodalync-valid/
│   ├── nodalync-econ/
│   ├── nodalync-ops/
│   ├── nodalync-net/
│   └── nodalync-settle/
└── apps/                    # Application crates (v0.10.x)
    ├── nodalync-cli/
    └── nodalync-mcp/
```

Each crate typically contains:

```
crates/{group}/nodalync-{module}/
├── Cargo.toml
├── src/
│   ├── lib.rs          # Public API, re-exports
│   └── ...             # Module-specific files
└── tests/
    └── ...             # Integration and compliance tests
```
