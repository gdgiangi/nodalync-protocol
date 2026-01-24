# Nodalync Architecture

This document defines the module structure, dependencies, and implementation order for the Nodalync protocol.

## Module Dependency Graph

```
                         ┌──────────────────┐
                         │  nodalync-cli    │
                         │  (binary crate)  │
                         └────────┬─────────┘
                                  │
          ┌───────────────────────┼───────────────────────┐
          │                       │                       │
          ▼                       ▼                       ▼
   ┌─────────────┐        ┌─────────────┐        ┌──────────────┐
   │ nodalync-net│        │ nodalync-ops│        │nodalync-settle│
   │  (P2P/DHT)  │        │ (operations)│        │   (chain)    │
   └──────┬──────┘        └──────┬──────┘        └──────┬───────┘
          │                      │                      │
          │               ┌──────┴──────┐               │
          │               │             │               │
          ▼               ▼             ▼               ▼
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

## Implementation Order

Implement in this order to minimize blocked dependencies:

### Phase 1: Foundation (No external dependencies)
1. **`nodalync-crypto`** — Pure functions, no I/O
   - Content hashing (SHA-256)
   - Ed25519 keypair generation
   - PeerId derivation
   - Message signing/verification

2. **`nodalync-types`** — Data structures only
   - All enums (ContentType, Visibility, etc.)
   - All structs (Manifest, Provenance, etc.)
   - No logic, just definitions

### Phase 2: Core Logic (Internal only)
3. **`nodalync-wire`** — Serialization
   - CBOR encoding/decoding
   - Message envelope format
   - Deterministic serialization

4. **`nodalync-store`** — Persistence
   - Content storage (filesystem)
   - Manifest storage (SQLite or similar)
   - Cache management

5. **`nodalync-valid`** — Validation rules
   - Content validation
   - Provenance validation
   - Payment validation
   - Message validation

6. **`nodalync-econ`** — Economics
   - Revenue distribution calculation
   - Weight handling
   - Settlement batching logic

### Phase 3: Operations (Combines everything)
7. **`nodalync-ops`** — Protocol operations
   - CREATE, PUBLISH, UPDATE
   - QUERY, PREVIEW
   - DERIVE, REFERENCE_L3_AS_L0
   - Channel operations

### Phase 4: External Integration
8. **`nodalync-net`** — Networking
   - libp2p integration
   - DHT (Kademlia)
   - Message routing
   - Peer discovery

9. **`nodalync-settle`** — Settlement
   - Hedera SDK integration
   - Batch settlement
   - Channel dispute resolution

### Phase 5: User Interface
10. **`nodalync-cli`** — CLI application
    - Command parsing
    - User interaction
    - Configuration management

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

Node configuration lives in `~/.nodalync/config.toml`:

```toml
[identity]
keyfile = "~/.nodalync/identity/keypair.key"

[storage]
content_dir = "~/.nodalync/content"
database = "~/.nodalync/nodalync.db"
cache_dir = "~/.nodalync/cache"

[network]
listen_addresses = ["/ip4/0.0.0.0/tcp/9000"]
bootstrap_nodes = [
    "/dns4/bootstrap1.nodalync.io/tcp/9000/p2p/...",
]

[settlement]
chain = "hedera-testnet"
account_id = "0.0.12345"

[economics]
default_price = 100000  # 0.001 NDL in smallest units
```

## Development Workflow

1. **Pick a module** from the implementation order
2. **Read the module spec** in `docs/modules/XX-{module}.md`
3. **Write tests first** from spec validation rules
4. **Implement** until tests pass
5. **Check off** items in `CHECKLIST.md`
6. **PR review** — verify spec compliance
7. **Move to next module**

## File Naming Conventions

```
crates/nodalync-{module}/
├── Cargo.toml
├── src/
│   ├── lib.rs          # Public API, re-exports
│   ├── types.rs        # Module-specific types (if any)
│   ├── traits.rs       # Trait definitions
│   ├── impl.rs         # Default implementations
│   └── error.rs        # Module-specific errors
└── tests/
    ├── unit.rs         # Unit tests
    ├── integration.rs  # Integration tests
    └── spec_compliance.rs  # Spec validation tests
```
