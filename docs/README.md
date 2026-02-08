# Nodalync Protocol

**A protocol for fair knowledge economics in the age of AI.**

---

## Abstract

We propose a protocol for knowledge economics that ensures original contributors receive
perpetual, proportional compensation from all downstream value creation. A researcher can
publish valuable findings once and receive perpetual royalties as the ecosystem builds upon
their work. A writer's insights compound in value as others synthesize and extend them.

The protocol enables humans to benefit from knowledge compounding—earning from what they
know, not just what they continuously produce.

## Key Features

- **Cryptographic Provenance** — Every piece of knowledge carries its complete derivation history
- **Fair Economics** — 95% of transaction value flows to foundational contributors
- **Local-First** — Your data stays on your machine, under your control
- **AI-Native** — MCP integration for seamless AI agent consumption

## Quick Navigation

### Getting Started
- [Quick Start](./quickstart.md) — Get your node running in under 5 minutes
- [FAQ](./FAQ.md) — Common questions answered

### Protocol
- [Specification](./spec.md) — Complete protocol specification
- [Architecture](./architecture.md) — Module structure and dependencies
- [Whitepaper](./papers/whitepaper.md) — Protocol design and economics
- [L2 Addendum](./l2_addendum.md) — Entity graph details

### Module Documentation
- [Crypto](./modules/01-crypto.md) — Hashing, signing, identity
- [Types](./modules/02-types.md) — Data structures
- [Wire](./modules/03-wire.md) — Serialization
- [Store](./modules/04-store.md) — Storage layer
- [Validation](./modules/05-valid.md) — Validation rules
- [Economics](./modules/06-econ.md) — Revenue distribution
- [Operations](./modules/07-ops.md) — Protocol operations
- [Networking](./modules/08-net.md) — P2P layer
- [Settlement](./modules/09-settle.md) — Hedera integration

### Applications
- [CLI](./modules/10-cli.md) — Command-line interface
- [MCP Server](./modules/11-mcp.md) — AI agent integration

## Protocol Layers

| Layer | Name | Contents | Properties |
|-------|------|----------|------------|
| L0 | Raw Inputs | Documents, transcripts, notes | Immutable, publishable, queryable |
| L1 | Mentions | Atomic facts with L0 pointers | Extracted, visible as preview |
| L2 | Entity Graph | Entities + RDF relations | Internal only, never shared |
| L3 | Insights | Emergent patterns and conclusions | Shareable, importable as L0 |

## Current Status

**Protocol v0.7.1** · **CLI v0.10.1**

| Layer | Crate | Description |
|-------|-------|-------------|
| Protocol | `nodalync-crypto` | Hashing (SHA-256), Ed25519 signing, PeerId derivation |
| Protocol | `nodalync-types` | All data structures including L2 Entity Graph |
| Protocol | `nodalync-wire` | Deterministic CBOR serialization, 21 message types |
| Protocol | `nodalync-store` | SQLite manifests, filesystem content, settlement queue |
| Protocol | `nodalync-valid` | Content, provenance, payment, L2 validation |
| Protocol | `nodalync-econ` | 95/5 revenue distribution, Merkle batching |
| Protocol | `nodalync-ops` | CREATE, DERIVE, BUILD_L2, MERGE_L2, QUERY |
| Protocol | `nodalync-net` | libp2p (TCP/Noise/yamux), Kademlia DHT, GossipSub |
| Protocol | `nodalync-settle` | Hedera settlement, smart contract deployed to testnet |
| App | `nodalync-cli` | Full CLI with daemon mode, health endpoints, alerting |
| App | `nodalync-mcp` | MCP server for AI agent integration |

## Hedera Testnet

| Resource | Value |
|----------|-------|
| Contract ID | `0.0.7729011` |
| EVM Address | `0xc6b4bFD28AF2F6999B32510557380497487A60dD` |
| HashScan | [View Contract](https://hashscan.io/testnet/contract/0.0.7729011) |

## Links

- [GitHub Repository](https://github.com/gdgiangi/nodalync-protocol)
- [Discord Community](https://discord.gg/hYVrEAM6)
