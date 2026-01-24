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

### Status

**v0.1.0 — Feature Complete**

| Phase | Crate | Description |
|-------|-------|-------------|
| 1 | `nodalync-crypto`  | Hashing (SHA-256), Ed25519 signing, PeerId derivation |
| 1 | `nodalync-types` | All data structures including L2 Entity Graph |
| 2 | `nodalync-wire` | Deterministic CBOR serialization, 17 message types |
| 2 | `nodalync-store` | SQLite manifests, filesystem content, settlement queue |
| 2 | `nodalync-valid` | Content, provenance, payment, L2 validation |
| 2 | `nodalync-econ` | 95/5 revenue distribution, Merkle batching |
| 3 | `nodalync-ops` | CREATE, DERIVE, BUILD_L2, MERGE_L2, QUERY |
| 4 | `nodalync-net` | libp2p (TCP/Noise/yamux), Kademlia DHT |
| 4 | `nodalync-settle` | Hedera settlement, smart contract deployed to testnet |
| 5 | `nodalync-cli` | Full CLI with interactive prompts, progress indicators |

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

# Run tests (738+ tests)
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

### Contact

**Gabriel Giangi**  
gabegiangi@gmail.com  
DMs open on X: @GabrielGia29751

---

### License

The protocol specification is released under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
