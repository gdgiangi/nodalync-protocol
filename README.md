<p align="center">
  <h1 align="center">Nodalync Protocol</h1>
</p>

<p align="center">
  <strong>Publish knowledge. AI agents query it. You get paid — forever.</strong>
</p>

<p align="center">
  <a href="https://github.com/gdgiangi/nodalync-protocol/actions"><img src="https://img.shields.io/github/actions/workflow/status/gdgiangi/nodalync-protocol/ci.yml?branch=main&style=flat-square&logo=github&label=CI" alt="CI"></a>
  <a href="https://github.com/gdgiangi/nodalync-protocol/releases"><img src="https://img.shields.io/badge/CLI-v0.10.1-blue?style=flat-square" alt="CLI Version"></a>
  <a href="docs/spec.md"><img src="https://img.shields.io/badge/protocol-v0.7.1-blue?style=flat-square" alt="Protocol Version"></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-orange?style=flat-square" alt="License"></a>
  <a href="https://hashscan.io/testnet/contract/0.0.7729011"><img src="https://img.shields.io/badge/Hedera-testnet%20live-8259EF?style=flat-square" alt="Hedera Testnet"></a>
  <a href="https://discord.gg/hYVrEAM6"><img src="https://img.shields.io/badge/Discord-join-5865F2?style=flat-square&logo=discord&logoColor=white" alt="Discord"></a>
</p>

<p align="center">
  <a href="https://gdgiangi.github.io/nodalync-protocol/">Docs</a> &middot;
  <a href="https://gdgiangi.github.io/nodalync-protocol/quickstart.html">Quick Start</a> &middot;
  <a href="https://gdgiangi.github.io/nodalync-protocol/papers/whitepaper.html">Whitepaper</a> &middot;
  <a href="https://discord.gg/hYVrEAM6">Discord</a>
</p>

---

## What is Nodalync?

Nodalync is an open protocol where knowledge creators earn perpetual royalties every time an AI agent queries their work. 95% of every payment flows back to original sources through cryptographic provenance chains — regardless of how many layers of synthesis stand between the query and the source.

You publish knowledge once. As others build on it, you earn from every downstream query. The protocol monetizes *access*, not ownership — buyers query your node, they never download your data.

Built with Rust. Peer-to-peer over libp2p. Settlement on Hedera. AI agents connect via MCP.

## Why Nodalync?

- **Perpetual royalties** — Publish once, earn from every downstream query forever
- **95/5 revenue distribution** — 95% flows to foundational sources, not intermediaries
- **AI-native** — MCP interface lets any AI agent query and pay automatically
- **Local-first** — Your data stays on your node; buyers get query access, not downloads
- **Cryptographic provenance** — Every insight links to its sources via content-addressed hashes
- **On-chain settlement** — Payment channels on Hedera with a live testnet smart contract
- **834+ tests** — Comprehensive test coverage across 11 protocol and application crates

## How It Works

<img width="633" height="224" alt="Screenshot 2026-02-03 at 10 46 20 PM" src="https://github.com/user-attachments/assets/300d3533-305f-4d4a-969b-fafb4a3f155d" />

Knowledge is structured in four layers with strict economic rules:

| Layer | What it contains | Queryable | Economics |
|-------|-----------------|-----------|-----------|
| **L0** | Raw documents, notes, transcripts | Yes | Original source — earns royalties |
| **L1** | Atomic facts extracted from L0 | Yes | Structured claims — earns royalties |
| **L2** | Your entity graph across L1s | No | Personal perspective — never shared |
| **L3** | Novel insights synthesizing sources | Yes | Original analysis — captures 5% |

L2 is intentionally private. It represents *your* unique interpretation of knowledge. Its value surfaces when you create L3 insights that others find valuable.

## Quick Start

```bash
# Build from source
git clone https://github.com/gdgiangi/nodalync-protocol.git
cd nodalync-protocol && cargo build --release -p nodalync-cli

# Initialize your identity (fails if already initialized — delete data dir to reset)
export NODALYNC_PASSWORD=your-secure-password
./target/release/nodalync init

# Create and publish content (price is in HBAR)
echo "# My Research" > my-research.md
./target/release/nodalync publish my-research.md --visibility shared --price 0.01

# Start your node
./target/release/nodalync start

# Check earnings
./target/release/nodalync earnings
```

For the full walkthrough, see the [Quick Start guide](https://gdgiangi.github.io/nodalync-protocol/quickstart.html).

## CLI

```bash
# Content operations
nodalync publish document.txt --visibility shared --price 0.01
nodalync synthesize --sources <hash1>,<hash2> --output insight.md
nodalync build-l2 <l1-hash-1> <l1-hash-2>
nodalync query <hash>

# Node management
nodalync start --daemon --health --health-port 8080
nodalync status
nodalync stop

# Payment channels
nodalync open-channel <peer-id> --deposit 100
nodalync list-channels
nodalync close-channel <peer-id>
```

**Health endpoints** (when `--health` enabled):
- `GET /health` — `{"status":"ok","connected_peers":N,"uptime_secs":M}`
- `GET /metrics` — Prometheus metrics

## Network

Three bootstrap nodes are deployed across regions:

| Region | Endpoint |
|--------|----------|
| US East | `nodalync-bootstrap.eastus.azurecontainer.io` |
| EU North | `nodalync-eu.northeurope.azurecontainer.io` |
| Asia SE | `nodalync-asia.southeastasia.azurecontainer.io` |

**Hedera testnet contract:** [`0.0.7729011`](https://hashscan.io/testnet/contract/0.0.7729011)

## Roadmap

Nodalync provides the provenance and economic layer. We are building toward a complete stack for autonomous knowledge commerce:

| Integration | What it adds | Status |
|-------------|-------------|--------|
| [**ERC-8004**](https://eips.ethereum.org/EIPS/eip-8004) | On-chain identity, reputation, and validation for AI agents — verifiable trust for every query | Planned |
| [**x402**](https://www.x402.org/) | HTTP-native micropayments (Coinbase/Cloudflare) — agents pay per request via standard HTTP | Planned |

**ERC-8004** gives agents verifiable identities and accumulated reputation. Nodalync tells you *what to pay and who to pay*. x402 handles *how to pay* at the HTTP layer. Together: trustless agents discover knowledge, pay with stablecoins, and 95% flows to original sources — no accounts, no subscriptions, no intermediaries.

<details>
<summary><strong>Crate structure</strong></summary>

| Layer | Crate | Version | Description |
|-------|-------|---------|-------------|
| Protocol | `nodalync-crypto` | 0.7.1 | SHA-256 hashing, Ed25519 signing, PeerId derivation |
| Protocol | `nodalync-types` | 0.7.1 | All data structures including L2 Entity Graph |
| Protocol | `nodalync-wire` | 0.7.1 | Deterministic CBOR serialization, 21 message types |
| Protocol | `nodalync-store` | 0.7.1 | SQLite manifests, filesystem content, settlement queue |
| Protocol | `nodalync-valid` | 0.7.1 | Content, provenance, payment, L2 validation |
| Protocol | `nodalync-econ` | 0.7.1 | 95/5 revenue distribution, Merkle batching |
| Protocol | `nodalync-ops` | 0.7.1 | CREATE, DERIVE, BUILD_L2, MERGE_L2, QUERY |
| Protocol | `nodalync-net` | 0.7.1 | libp2p networking, Kademlia DHT, GossipSub |
| Protocol | `nodalync-settle` | 0.7.1 | Hedera settlement, smart contract integration |
| App | `nodalync-cli` | 0.10.1 | Full CLI with daemon mode, health endpoints, alerting |
| App | `nodalync-mcp` | 0.10.1 | MCP server for AI agent integration |

</details>

<details>
<summary><strong>Building from source</strong></summary>

**Prerequisites:** Rust 1.85+, SQLite dev headers, (optional) `protoc` for Hedera SDK

```bash
git clone https://github.com/gdgiangi/nodalync-protocol.git
cd nodalync-protocol

cargo build --workspace
cargo test --workspace

# With Hedera settlement support
cargo build --release -p nodalync-cli --features hedera-sdk

# Smart contract tests
cd contracts && npm install && npm test
```

</details>

<details>
<summary><strong>Versioning</strong></summary>

This repository uses split versioning:

| Component | Version | Tag pattern |
|-----------|---------|-------------|
| Protocol crates | `0.7.x` (spec-driven) | `protocol-v*` |
| Application crates | `0.10.x` (feature releases) | `v*` |

**Users:** download releases tagged `v*` for CLI binaries.
**Developers:** protocol crate versions indicate wire compatibility.

</details>

## Documentation

- [Protocol Specification](https://gdgiangi.github.io/nodalync-protocol/spec.html) — Source of truth
- [Architecture](https://gdgiangi.github.io/nodalync-protocol/architecture.html) — System design
- [CLI Reference](https://gdgiangi.github.io/nodalync-protocol/modules/10-cli.html) — All commands
- [MCP Server](https://gdgiangi.github.io/nodalync-protocol/modules/11-mcp.html) — AI agent integration
- [FAQ](https://gdgiangi.github.io/nodalync-protocol/FAQ.html) — Common questions

## Community

- **Discord:** [discord.gg/hYVrEAM6](https://discord.gg/hYVrEAM6)
- **X:** [@GabrielGia29751](https://x.com/GabrielGia29751)
- **Email:** gabegiangi@gmail.com

## Contributors

Gabriel Giangi

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.

The protocol specification and documentation (`docs/`) are released under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
