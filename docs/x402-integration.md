# x402 Payment Integration — Technical Design Document

**Author:** Daedalus  
**Date:** 2026-02-13  
**Status:** Implemented (Phase 1)  
**Sprint:** D2 (Application-Level Fee)  
**Crate:** `nodalync-x402` v0.4.0

---

## Overview

x402 is an open payment protocol (by Coinbase) that enables HTTP-native micropayments using the HTTP 402 "Payment Required" status code. Nodalync integrates x402 to allow AI agents and applications to pay for knowledge access programmatically — no accounts, no sessions, no checkout flows.

This is our **revenue mechanism** for the Apex hackathon and the core of Sprint D2.

## Architecture

```
┌─────────────┐     GET /knowledge      ┌──────────────────┐
│  AI Agent   │ ──────────────────────→  │  Nodalync Node   │
│  (Client)   │ ←─── 402 + requirements  │  (Server)        │
│             │                          │                  │
│             │  GET + X-PAYMENT header  │  ┌────────────┐  │
│             │ ──────────────────────→  │  │PaymentGate │  │
│             │                          │  └──────┬─────┘  │
│             │                          │         │        │
│             │                          │  ┌──────▼─────┐  │
│             │                          │  │ Blocky402  │  │
│             │                          │  │ Facilitator│  │
│             │  200 OK + content        │  └──────┬─────┘  │
│             │ ←──────────────────────  │         │        │
│             │  + provenance receipt    │  ┌──────▼─────┐  │
└─────────────┘                          │  │  Hedera    │  │
                                         │  │  (settle)  │  │
                                         │  └────────────┘  │
                                         └──────────────────┘
```

## Payment Flow

### 1. Client Requests Knowledge (No Payment)
```http
GET /knowledge?hash=abc123 HTTP/1.1
Host: node.nodalync.com
```

### 2. Server Responds with 402
```http
HTTP/1.1 402 Payment Required
X-PAYMENT-REQUIRED: {
  "x402Version": 1,
  "resource": {
    "url": "nodalync://query/abc123",
    "description": "Knowledge about Rust programming",
    "contentHash": "abc123"
  },
  "accepts": [{
    "scheme": "exact",
    "network": "hedera:testnet",
    "amount": "105",
    "asset": "HBAR",
    "payTo": "0.0.12345",
    "maxTimeoutSeconds": 300,
    "extra": {
      "protocol": "Nodalync",
      "protocolVersion": "0.7.1",
      "contentHash": "abc123",
      "appFeePercent": "5"
    }
  }]
}
```

### 3. Client Pays (Hedera Exact Scheme)
```http
GET /knowledge?hash=abc123 HTTP/1.1
Host: node.nodalync.com
X-PAYMENT: <base64-encoded PaymentPayload>
```

The PaymentPayload contains:
- Partially-signed Hedera CryptoTransfer transaction
- Client's signature (payer authorization)
- Validity window (validAfter/validBefore)
- Nonce for replay prevention

### 4. Server Verifies & Settles
1. **Local validation**: scheme, network, amount, timing, nonce, recipient
2. **Facilitator verification**: POST to Blocky402 `/verify`
3. **Settlement**: POST to Blocky402 `/settle` (facilitator pays gas)
4. **Record transaction**: App fee + creator payment tracked
5. **Deliver content**: 200 OK + knowledge + provenance receipt

### 5. Client Receives Content + Receipt
```http
HTTP/1.1 200 OK
X-PAYMENT-RESPONSE: {
  "success": true,
  "txHash": "0.0.12345@1700000000.123456789",
  "network": "hedera:testnet",
  "provenance": {
    "contentHash": "abc123",
    "owner": "0.0.12345",
    "contributors": [...],
    "appFee": 5
  }
}
Content-Type: application/json

{ "content": "...", "provenance": [...] }
```

## Components

### `nodalync-x402` Crate (New)

| Module | Purpose |
|--------|---------|
| `types.rs` | x402 protocol types: PaymentRequired, PaymentPayload, PaymentResponse, X402Config |
| `facilitator.rs` | HTTP client for Blocky402-compatible facilitators (verify, settle, supported) |
| `gate.rs` | PaymentGate middleware: validation, nonce tracking, transaction recording |
| `error.rs` | Error types with HTTP status codes and recovery suggestions |

### Desktop App IPC Commands (New)

| Command | Purpose |
|---------|---------|
| `get_x402_status` | Current x402 config + aggregate transaction stats |
| `configure_x402` | Enable/update x402 settings (account, fee %, network) |
| `get_x402_transactions` | Transaction history with settlement details |
| `check_x402_facilitator` | Verify facilitator connectivity and Hedera support |

### MCP Server Integration (Planned)

The MCP server (`nodalync-mcp`) already has 16 tools for knowledge access. x402 integration will gate paid queries:

- `query_knowledge` → Checks if content has a price → Returns 402 or processes payment
- `search_network` → Free (discovery should be free to encourage usage)
- `preview_content` → Free (previews encourage purchases)

## Fee Structure

```
Content Price (set by creator):  100 tinybars
App Fee (5% default):            + 5 tinybars
                                 ─────────────
Total Paid by Querier:           105 tinybars

Distribution:
  Creator (synthesis fee):       5 tinybars (5% protocol fee)
  Root Contributors:             95 tinybars (weighted by provenance)
  App Fee → Studio:              5 tinybars (Nodalync Studio revenue)
```

The app fee is **application-level** — other apps building on Nodalync can set their own rates. The protocol's 5% synthesis fee is separate and goes to creators.

## Compatibility

### Facilitators
- **Blocky402** (primary): Open source, supports Hedera testnet V1
- **Coinbase CDP**: Supports Base + Solana (future multi-chain support)
- **Self-hosted**: Any x402-compliant facilitator

### Hedera Payment Scheme
Nodalync uses the "exact" scheme on Hedera:
1. Client creates CryptoTransfer → resource server
2. Client partially signs (leaves fee-payer slot)
3. Facilitator verifies → adds gas signature → submits to Hedera
4. Settlement confirmed on-chain in seconds

### Network Identifiers (CAIP-2)
- `hedera:testnet` — Hedera testnet (development/hackathon)
- `hedera:mainnet` — Hedera mainnet (production)

## Comparison: x402 vs. Nodalync Payment Channels

| Feature | x402 | Payment Channels |
|---------|------|-----------------|
| Setup | Zero — HTTP-native | Requires channel open + deposit |
| Gas fees | Facilitator pays | User pays |
| Latency | One HTTP roundtrip | Pre-funded, instant |
| Best for | One-off queries, AI agents | Repeated access, streaming |
| Settlement | Per-request on-chain | Batched, periodic |

**Both systems coexist**: x402 for first-time/casual access, payment channels for high-frequency users.

## Hackathon Impact

### Integration Score (15% of judging)
- x402 is **Hedera's promoted payment standard** for AI agents
- Deep integration: Hedera settlement + Blocky402 facilitator + HBAR payments
- Demonstrates real revenue mechanism, not just a prototype

### Demo Script
1. AI agent discovers knowledge via MCP `search_network`
2. Agent requests content → receives 402 with price
3. Agent pays via x402 (HBAR on testnet)
4. Content delivered with full provenance receipt
5. Dashboard shows: transaction, fee split, settlement tx hash

### FedEx Narrative
"FedEx joined Hedera to verify supply chain data. Nodalync verifies knowledge data — and uses x402 so AI agents can pay creators when they use that knowledge."

## Implementation Status

### Phase 1 (Complete ✅)
- [x] `nodalync-x402` crate: types, facilitator client, payment gate
- [x] 30 tests passing (28 unit + 2 doc)
- [x] Desktop app IPC: 4 new commands
- [x] Configuration persistence
- [x] Transaction tracking with fee breakdown
- [x] Nonce-based replay prevention

### Phase 2 (Next)
- [ ] MCP server x402 middleware (gate query_knowledge)
- [ ] End-to-end testnet demo with Blocky402
- [ ] Desktop app UI: x402 status panel (Hephaestus ticket)
- [ ] CLI x402 commands (configure, status)

### Phase 3 (Post-hackathon)
- [ ] Multi-asset support (USDC on Hedera)
- [ ] Self-hosted facilitator option
- [ ] Payment channel ↔ x402 bridge (auto-open channel after N x402 payments)
- [ ] Rate limiting by payer address
- [ ] Analytics dashboard

## Test Coverage

```
nodalync-x402: 30 tests
  error.rs:        3 tests (suggestions, transient, http_status)
  types.rs:       10 tests (serialization, config, header roundtrip, fees)
  facilitator.rs:  4 tests (creation, URL normalization, config, debug)
  gate.rs:        11 tests (enabled/disabled, validation, nonce, tracking, status)
  lib.rs:          2 doc-tests
```

## References

- [x402 Specification](https://github.com/coinbase/x402)
- [x402 Protocol Docs](https://docs.cdp.coinbase.com/x402/welcome)
- [Blocky402 Facilitator](https://blocky402.com/)
- [Hedera x402 Blog](https://hedera.com/blog/hedera-and-the-x402-payment-standard/)
- [Nodalync Fee Commands](../apps/desktop/rust-src/fee_commands.rs)
- [Nodalync Settlement](../crates/protocol/nodalync-settle/)
