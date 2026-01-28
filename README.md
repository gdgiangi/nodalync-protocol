# Nodalync Protocol

**A protocol for fair knowledge economics in the age of AI.**

---

### Abstract

We propose a protocol for knowledge economics that ensures original contributors receive
perpetual, proportional compensation from all downstream value creation. A researcher can
publish valuable findings once and receive perpetual royalties as the ecosystem builds upon
their work. A writer's insights compound in value as others synthesize and extend them. The
protocol enables humans to benefit from knowledge compounding—earning from what they
know, not just what they continuously produce. The protocol structures knowledge into four
layers where source material (L0) forms an immutable foundation from which all derivative
value flows. Cryptographic provenance chains link every insight back to its roots. A revenue
distribution mechanism routes 95% of transaction value to foundational contributors
regardless of derivation depth. Unlike prior approaches that attempted to price and transfer
data, the protocol monetizes query access—buyers gain the right to query a node, not
ownership of transferable assets. This eliminates secondary markets that have historically
enabled royalty bypass. The protocol implements Model Context Protocol (MCP) as the
standard interface for AI agent consumption, creating immediate demand from agentic
systems. The result is infrastructure where contributing valuable foundational knowledge once
creates perpetual economic participation in all derivative work.

---

### Read the Paper

📄 **[Nodalync Protocol v0.8 (PDF)](./docs/whitepaper.pdf)** — Draft, January 2026

---

### Quick Start

Get your node running in under 5 minutes: **[QUICKSTART.md](./QUICKSTART.md)**

---

### Status

**Protocol v0.3.0** · **CLI v0.6.0**

| Layer | Crate | Version | Description |
|-------|-------|---------|-------------|
| Protocol | `nodalync-crypto` | 0.3.0 | Hashing (SHA-256), Ed25519 signing, PeerId derivation |
| Protocol | `nodalync-types` | 0.3.0 | All data structures including L2 Entity Graph |
| Protocol | `nodalync-wire` | 0.3.0 | Deterministic CBOR serialization, 21 message types |
| Protocol | `nodalync-store` | 0.3.0 | SQLite manifests, filesystem content, settlement queue |
| Protocol | `nodalync-valid` | 0.3.0 | Content, provenance, payment, L2 validation |
| Protocol | `nodalync-econ` | 0.3.0 | 95/5 revenue distribution, Merkle batching |
| Protocol | `nodalync-ops` | 0.3.0 | CREATE, DERIVE, BUILD_L2, MERGE_L2, QUERY |
| Protocol | `nodalync-net` | 0.3.0 | libp2p (TCP/Noise/yamux), Kademlia DHT, GossipSub |
| Protocol | `nodalync-settle` | 0.3.0 | Hedera settlement, smart contract deployed to testnet |
| App | `nodalync-cli` | 0.6.0 | Full CLI with daemon mode, health endpoints |
| App | `nodalync-mcp` | 0.6.0 | MCP server for AI agent integration |

**Hedera Testnet:**

| Resource | Value |
|----------|-------|
| Contract ID | `0.0.7729011` |
| EVM Address | `0xc6b4bFD28AF2F6999B32510557380497487A60dD` |
| HashScan | [View Contract](https://hashscan.io/testnet/contract/0.0.7729011) |

---

### Building

**Prerequisites:**
- Rust 1.75+ (with cargo)
- SQLite development headers
- (Optional) `protoc` for Hedera SDK feature

```bash
# Clone the repository
git clone https://github.com/gdgiangi/nodalync-protocol.git
cd nodalync-protocol

# Build all crates
cargo build --workspace

# Run tests (776+ tests)
cargo test --workspace

# Build with Hedera support (requires protoc)
cargo build --workspace --features hedera-sdk

# Build documentation
cargo doc --workspace --no-deps --open

# Run smart contract tests
cd contracts && npm install && npm test
```

### CLI Usage

```bash
# Initialize identity
nodalync init

# Create and publish content
nodalync publish my-document.txt --visibility shared --price 100

# Build L2 entity graph from L1 sources
nodalync build-l2 <l1-hash-1> <l1-hash-2>

# Create L3 insight from sources
nodalync synthesize --sources <hash1>,<hash2> --file insight.md

# Query remote content
nodalync query <hash>

# Check earnings
nodalync earnings
```

### Running a Node

```bash
# Start node (foreground)
nodalync start

# Start node with health endpoint (for containers/monitoring)
nodalync start --health --health-port 8080

# Start as daemon (background)
nodalync start --daemon

# Check status
nodalync status

# Stop daemon
nodalync stop
```

**Health Endpoints** (when `--health` enabled):
- `GET /health` — JSON status: `{"status":"ok","connected_peers":N,"uptime_secs":M}`
- `GET /metrics` — Prometheus metrics (peers, DHT ops, settlements, queries)

**Bootstrap Node:**
```
/dns4/nodalync-bootstrap.eastus.azurecontainer.io/tcp/9000/p2p/12D3KooWMqrUmZm4e1BJTRMWqKHCe1TSX9Vu83uJLEyCGr2dUjYm
```

---

### Knowledge Layers

| Layer | Content | Operation | Queryable | Economics |
|-------|---------|-----------|-----------|-----------|
| **L0** | Raw documents, notes, transcripts | `CREATE` | Yes | Original source material |
| **L1** | Atomic facts extracted from L0 | `EXTRACT_L1` | Yes | Structured, quotable claims |
| **L2** | Entities and relationships across L1s | `BUILD_L2` | **No** (personal) | Your perspective, never monetized directly |
| **L3** | Novel insights synthesizing sources | `DERIVE` | Yes | Original analysis and conclusions |

**L2 is Personal:** Your L2 represents your unique interpretation — how you link entities, resolve ambiguities, and structure knowledge. It is never shared or queried. Its value surfaces when you create L3 insights that others find valuable.

---

### Core Ideas

1. **Four-layer knowledge model** — L0 (raw sources) → L1 (facts) → L2 (entity graphs) → L3 (insights), with strict rules about what can be queried at each layer

2. **Cryptographic provenance** — Every derived insight links back to its foundational sources via content-addressed hashes

3. **Fair revenue distribution** — 95% of query payment flows to L0/L1 contributors; synthesis captures only 5%

4. **L2 as personal perspective** — Your entity graph represents your unique understanding; value surfaces through L3 insights

5. **URI-based ontology** — L2 uses RDF-compatible URIs (Schema.org, FOAF, custom) for semantic interoperability

6. **AI-native interface** — MCP integration enables any agent to query knowledge bases with automatic compensation

7. **Local-first sovereignty** — Your data stays on your node; buyers get query access, not downloads

---

### Versioning

This repository uses **split versioning** to distinguish protocol stability from application features:

| Component | Version | Stability | Tag Pattern | Release Contents |
|-----------|---------|-----------|-------------|------------------|
| **Protocol crates** | `0.3.x` | Stable, spec-driven | `protocol-v*` | GitHub release only |
| **Application crates** | `0.6.x` | Feature releases | `v*` | Binaries + Docker |

**Protocol crates** (`nodalync-crypto`, `nodalync-types`, `nodalync-wire`, `nodalync-store`, `nodalync-valid`, `nodalync-econ`, `nodalync-ops`, `nodalync-net`, `nodalync-settle`):
- Version tracks the [protocol specification](./docs/spec.md) (currently v0.3.0)
- Changes are rare and require spec updates
- Breaking changes require major version bump
- Tag `protocol-v0.3.0` → creates GitHub release (libraries, no binaries)

**Application crates** (`nodalync-cli`, `nodalync-mcp`):
- Version tracks CLI/MCP features
- Independent release cadence
- Tag `v0.6.0` → builds binaries for all platforms + Docker images

**For users:** Download releases tagged `v*` (e.g., `v0.6.0`). This is the CLI version.

**For developers:** Protocol crate versions indicate wire compatibility. Same `0.3.x` = compatible.

---

### Contact

**Gabriel Giangi**  
gabegiangi@gmail.com  
DMs open on X: @GabrielGia29751

---

### Contributors

Gabriel Giangi, Thomas Blanc Bolelli

### License

The protocol specification is released under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
