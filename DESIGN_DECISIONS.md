# Nodalync Protocol - Design Decisions Summary

This document captures the key design decisions made during implementation planning.

---

## P0 Decisions (Must Have for MVP)

### 1. L1 Extraction: Rule-Based for MVP

**Decision:** Use rule-based NLP extraction for MVP with plugin architecture for future AI integration.

**Implementation:**
- `RuleBasedExtractor` struct implements `L1Extractor` trait
- `classify_sentence()` uses keyword heuristics
- `extract_entities()` finds capitalized words
- Future: `L1ExtractorPlugin` trait for OpenAI, Claude, etc.

**Location:** `docs/modules/07-ops.md` §7.1.2

---

### 2. Search: Hash-Only Lookup for MVP

**Decision:** Protocol supports hash-based content discovery only. Keyword/semantic search is application layer.

**Rationale:**
- Kademlia DHT only supports exact key lookup
- Keeps protocol minimal and focused
- Discovery happens via external channels (social, links, curated directories)
- Future app-layer services can build search indexes on top

**Implementation:**
- `dht_announce(hash, AnnouncePayload)` — store content metadata
- `dht_get(hash)` — retrieve by hash
- No `dht_search()` in protocol

**Location:** `docs/modules/08-net.md`

---

### 3. Payment Distribution: Settlement Contract Distributes to All

**Decision:** The settlement contract pays ALL recipients directly. Content owners don't manually pay upstream contributors.

**Flow:**
```
Bob queries Alice's L3 (uses Carol's L0)
    ↓
Bob pays into settlement contract
    ↓
Contract distributes:
  - Alice: 5% synthesis fee + her root shares
  - Carol: her root shares (95% pool)
  - Other contributors: their shares
```

**Rationale:**
- Trustless: Alice can't withhold payment from Carol
- Simpler: No need for Alice→Carol channels
- Requirement: All recipients need Hedera accounts

**Implementation:**
- Query handler writes ALL distributions to settlement queue
- `nodalync-settle` reads queue and submits batch
- Contract's `settleBatch()` pays all recipients

**Location:** `docs/modules/06-econ.md`, `docs/modules/07-ops.md`, `docs/modules/09-settle.md`

---

### 4. Manifest Owner Field: Explicit

**Decision:** Add explicit `owner: PeerId` field to Manifest struct.

**Rationale:**
- Needed for payment routing
- Clearer than deriving from provenance
- Simplifies access control and economics

**Implementation:**
- Manifest struct includes `owner: PeerId`
- Set to creator's PeerId during CREATE/DERIVE

**Location:** `docs/modules/02-types.md` §4.8, `docs/spec.md` §4.8

---

## P1 Decisions (Should Have)

### 5. Channel Auto-Open

**Decision:** When querying a peer with no channel, auto-open with configurable minimum deposit. Return `PAYMENT_REQUIRED` if insufficient funds.

**Configuration:**
```rust
pub struct ChannelConfig {
    pub min_deposit: Amount,      // 100 NDL default
    pub default_deposit: Amount,  // 1000 NDL default
}
```

**Flow:**
1. Check if channel exists
2. If not, check available balance
3. If balance < min_deposit → `PAYMENT_REQUIRED` error
4. If balance >= min_deposit → auto-open with min(balance, default_deposit)
5. Proceed with query

**Location:** `docs/modules/07-ops.md` §7.2.3

---

### 6. Settlement Queue Ownership

**Decision:** Queue lives in `nodalync-store` (data storage). `nodalync-ops` writes to queue. `nodalync-settle` reads from queue.

**Flow:**
```
Query received
    ↓
nodalync-ops handles query
    ↓
Calculates distributions (nodalync-econ)
    ↓
Writes to settlement queue (nodalync-store)
    ↓
Checks trigger conditions
    ↓ (if triggered)
nodalync-settle reads queue
    ↓
Creates batch, submits to chain
    ↓
Marks distributions as settled in queue
```

**Location:** `docs/modules/04-store.md`, `docs/modules/07-ops.md`

---

## Spec Updates Made

1. **§3.1 ContentHash:** Added domain separator `0x00` (was missing, inconsistent with Appendix A)
2. **§4.8 Manifest:** Added `owner: PeerId` field
3. **§6.6 Channel messages:** Added all message types (Accept, Close, Dispute)
4. **§6.7 Settlement messages:** Added SettleBatchPayload, SettleConfirmPayload
5. **§6.8 Peer messages:** Added Ping, Pong, PeerInfo, Capability

---

## Module Documentation Updates

| Module | Key Changes |
|--------|-------------|
| 02-types | Added `owner` to Manifest, `channel_id` to Channel/Payment |
| 03-wire | Added Version, Settlement, Peer message types |
| 04-store | Added SettlementQueueStore trait |
| 06-econ | Clarified contract distributes to all |
| 07-ops | Added L1 extraction, channel auto-open, settlement trigger, version handler |
| 08-net | Clarified hash-only lookup, removed keyword search |
| 09-settle | Added resolve_dispute, counter_dispute, clarified distribution |

---

## Implementation Order (Unchanged)

1. `nodalync-crypto` — Pure functions, no deps
2. `nodalync-types` — Structs only
3. `nodalync-wire` — Serialization
4. `nodalync-store` — Persistence + settlement queue
5. `nodalync-valid` — Validation rules
6. `nodalync-econ` — Revenue math
7. `nodalync-ops` — Protocol logic
8. `nodalync-net` — P2P (hash-only DHT)
9. `nodalync-settle` — Hedera integration
10. `nodalync-cli` — User interface

---

## Out of Scope (Application Layer)

- Keyword/semantic search
- Content recommendation
- Reputation algorithms
- Tiered pricing (commercial/academic)
- Confidence scoring
- AI-powered L1 extraction (future plugin)
