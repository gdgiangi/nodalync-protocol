# Nodalync Protocol: Frequently Asked Questions

This document addresses common questions and concerns about the Nodalync protocol design.

**Status Legend:**
- **Designed** — Addressed in protocol/implementation
- **Gap** — Known limitation, not yet addressed
- **Deferred** — Planned for future work
- **Out of Scope** — Intentionally not part of the protocol
- **Known Limitation** — Acknowledged tradeoff

---

# Economic & Incentive Questions

## 1. Can People Game the System with Low-Effort Contributions?

| Status | Designed |
|--------|----------|

**Concern:** Users might add useless or trivial content just to insert themselves into provenance chains and collect unearned payments.

**Answer:** The protocol's economic design makes this strategy unprofitable.

**Revenue only flows when content is queried.** Creating thousands of low-value nodes generates zero income because no one will query them. The market determines value through actual usage, not mere existence in the system.

From the whitepaper (Section 10.2 - Attribution Gaming):
> "Revenue distributes only when content is queried. Creating thousands of unused nodes generates no income. The market determines value through actual queries."

**You cannot insert yourself into someone else's provenance chain.** Provenance chains are cryptographically computed when content is created. To be in someone's `root_L0L1[]` array, your content must have been:
1. Queried and paid for by the creator
2. Used as a source in their derivation

The spec (Section 9.3) enforces that:
> "All entries in derived_from MUST have been queried by creator"

This means you can't retroactively attach yourself to successful content. The only way to earn is to create content valuable enough that others choose to query it and build upon it.

**Synthesizers who don't contribute foundational work earn only 5%.** The protocol intentionally rewards original contribution over mere reorganization. A "pure synthesizer" using entirely others' sources receives only the 5% synthesis fee—this is by design.

---

## 2. Will the Platform Get Flooded with Low-Quality Content?

| Status | Designed |
|--------|----------|

**Concern:** Since uploading L0 content can lead to long-term payouts, people might spam the network with low-quality or copied content, burying valuable material.

**Answer:** Several mechanisms prevent spam from being profitable or visible.

**Protocol-level mechanisms:**
1. **Pricing as filter** — spam is unprofitable if nobody queries it
2. **Rate limiting** — configurable per peer/content hash via `AccessControl`
3. **Payment bonds** — `require_bond: bool` in AccessControl can require deposits
4. **Reputation** — `PeerInfo.reputation: int64` tracked per peer
5. **Allowlist/denylist** — per-content access control

**Discovery is application-layer:** The protocol itself doesn't include search. Discovery occurs through application-layer indexes (search engines, directories, AI agents) that can implement their own quality filtering, reputation systems, and relevance ranking. From the spec (Section 1.4):
> "Content discovery/search... Applications index L1 previews and build search UX"
> "Content moderation — policy decisions for specific communities/jurisdictions" [Out of scope]

**L1 previews enable informed decisions:** Before paying for content, users see the L1 summary (extracted mentions, topics, preview). This free preview layer helps users evaluate relevance without payment, making it easy to skip low-quality content.

**Philosophy:** Bad content doesn't get queried, therefore doesn't earn. Applications build quality filters on top.

---

## 3. Can Someone Game L3 Provenance by Citing Sources They Don't Actually Use?

| Status | Known Limitation |
|--------|------------------|

**Concern:** Someone could query prestigious sources, claim them in their L3 provenance, but write completely unrelated content—essentially "name-dropping" for credibility.

**Answer:** This is a real concern. The protocol guarantees cryptographic provenance but not intellectual honesty.

**What the protocol guarantees:**
- Cryptographic provenance chain exists
- You can only claim sources you actually queried and paid for
- Payment proof exists for every claimed source

**What it does NOT guarantee:**
- That the L3 content actually uses the claimed sources intellectually
- That the L3 is "good" or "honest" synthesis

**Why the attack is limited:**
1. They still have to **pay for every source** they claim
2. If their L3 is garbage, nobody queries it—no revenue
3. The sources don't "endorse" the L3—provenance just means "this L3 paid for access to these sources"

**Potential future mitigations:**
- Semantic similarity checking between L3 and claimed sources (application layer)
- Reputation for L3 creators based on downstream utility
- ZK proofs for content derivation (mentioned in spec §13.4)

---

## 4. Does the Protocol Pay Equal Amounts for Unequal Work?

| Status | Designed |
|--------|----------|

**Concern:** The system pays everyone the same share per root entry, whether someone contributed a single sentence or comprehensive research. This seems unfair.

**Answer:** This is a deliberate design choice, not an oversight.

**Each root entry represents a discrete contribution that was valuable enough to be used.** If your single sentence was included in someone's L3, that means they queried it, paid for it, and found it valuable enough to derive from. The protocol doesn't judge contribution size—the market does.

**The weighting system handles contribution frequency:**
From the spec (Section 4.5):
> "When the same source appears multiple times in a provenance chain (through different derivation paths), it receives proportionally more: a source contributing twice receives twice the share."

**Quality is priced at the source:** Content owners set their own prices. Comprehensive, high-quality research can be priced higher than trivial observations.

**The alternative (contribution-weighted shares) creates worse problems:**
- Who decides what contribution is "worth more"? This requires subjective judgment.
- Gaming becomes easier if you can inflate perceived contribution size.
- Equal weighting is objective and trustless—a hash is either in the provenance chain or it isn't.

---

## 5. Will Early Users Lock In Permanent Advantages?

| Status | Designed (Intentional) |
|--------|------------------------|

**Concern:** Those who publish first in any topic could lock in lifelong royalties, making it hard for newcomers to compete.

**Answer:** You're correct that this is largely unavoidable—and it's intentional.

**This is feature, not bug.** The protocol's explicit goal is to reward foundational contributors perpetually. From the whitepaper abstract:
> "A researcher can publish valuable findings once and receive perpetual royalties as the ecosystem builds upon their work."

**Factors that prevent an impenetrable moat:**

- **New contributions create new chains:** If you publish novel research, you create new provenance chains. Later contributors building on YOUR work include YOU in their chains.
- **Quality and relevance matter:** Early publication doesn't guarantee usage. Superior later work will be preferred by synthesizers.
- **Versioning supports improvement:** The spec supports content versioning (Section 4.3). Updated versions can be published.

**The alternative is worse:** Systems that DON'T reward early contributors (like current academic publishing) create no economic incentive for foundational research at all.

---

## 6. Can Content Be Reused Forever After a Single Payment?

| Status | Designed |
|--------|----------|

**Concern:** Once someone pays for content, they can cache and reuse it infinitely without paying again. Creators only get paid once.

**Answer:** This is accurate and intentional.

**You're paying for access, not per-read:** Like buying a book, the initial query gives you the content. Rereading your own copy doesn't generate new payments.

**New queries DO trigger new payments:**
From the whitepaper (Section 5.1):
> "Subsequent queries to the same node (for updated information or different query parameters) trigger new payments."

**Derivation requires payment:** Creating new L3 content that derives from cached sources still requires having queried (and paid for) each source at least once. From the spec (Section 7.1.5):
> "All sources have been queried (payment proof exists)"

**The value is in the provenance chain:** When you use cached content to create an L3, and others query YOUR L3, revenue flows back through the entire provenance chain to original creators.

**Unlimited re-reads would break usability:** If every re-read required payment, the system would be unusable for research or synthesis work.

---

## 7. How Do Creators Know What to Charge?

| Status | Gap |
|--------|-----|

**Concern:** Without pricing guidance, creators may set inefficient prices.

**Answer:** The spec explicitly treats pricing as a market function:
> "Pricing recommendations — market dynamics emerge from application-layer analytics"

**What exists:**
- `Economics` struct tracks `total_queries` and `total_revenue` per content
- This data is visible to anyone indexing the DHT

**What's missing:**
- No pricing suggestions in the protocol
- Could be built as an application: "content similar to yours earns X HBAR/query on average"
- Initial testing uses tiny prices (0.001 HBAR per query) to prove the flow

Unlike prior data marketplaces that failed attempting to solve pricing algorithmically, Nodalync treats price discovery as a market function rather than a protocol function.

---

# Technical Questions

## 8. How Does Discovery Work Without Knowing the Hash?

| Status | Designed |
|--------|----------|

**Concern:** If content is addressed by hash, how do users find content they don't already know about?

**Answer:** Discovery is an application-layer concern, with protocol primitives to support it.

```
Application developers can:
┌─────────────────────────────────────────────────────────────┐
│  SEARCH ENGINES                                             │
│  - Subscribe to ANNOUNCE broadcasts on DHT                  │
│  - Fetch free PREVIEW for all shared content                │
│  - Index L1 summaries, tags, content types                  │
│  - Build relevance ranking from total_queries, reputation   │
│  - Return content hashes → users query through protocol     │
└─────────────────────────────────────────────────────────────┘
```

**The MCP server has a `list_sources` tool** that shows available content with title, price, preview text, and topics.

**Current gap:** MCP doesn't support natural language search yet—use `list_sources` to discover hashes. Full-text search would be an application-layer index.

---

## 9. How Is L1 Extraction Done — Manual or AI?

| Status | Designed |
|--------|----------|

**Concern:** How are atomic facts (L1 mentions) extracted from L0 documents?

**Answer:** Currently rule-based, with plugin architecture for AI extractors.

**Current implementation:** Rule-based NLP
```rust
pub trait L1Extractor {
    fn extract(&self, content: &[u8], mime_type: Option<&str>) -> Result<Vec<Mention>>;
}

/// Rule-based extractor for MVP
pub struct RuleBasedExtractor;
```

It splits text into sentences, does basic classification (Claim, Statistic, Definition, etc.), and extracts entities (capitalized words).

**Future design:** Plugin architecture for AI-powered extractors:
```rust
pub trait L1ExtractorPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn supported_mime_types(&self) -> Vec<&str>;
    fn extract(&self, content: &[u8], mime_type: &str) -> Result<Vec<Mention>>;
}
```

**Quality enforcement:** The spec says "AI extraction quality — pluggable extractors; quality is a market signal." If your L1s are garbage, nobody queries them, you earn nothing.

---

## 10. What About the Cold Start / Chicken-and-Egg Problem?

| Status | Designed |
|--------|----------|

**Concern:** The network needs content to be valuable, but creators won't publish until there's demand.

**Answer:** The spec explicitly acknowledges this is an application-layer concern, not a protocol concern. The protocol provides primitives; bootstrap is left to implementations.

**Practical solutions in the design:**
- **L1 previews are free** — anyone can browse without paying
- **Discovery through DHT ANNOUNCE broadcasts** — search engines can subscribe and index
- **Initial plan:** Seed with own content first (spec, whitepaper, technical docs), then dogfood with Claude
- **The MCP server lets AI agents query immediately** — if even 1 person has good content, an AI can use it

**Gap:** No automated discovery UX yet. Intentional—prove the economics first, then build the search layer.

---

## 11. How Does the Protocol Scale to Millions of Nodes?

| Status | Designed (Untested at Scale) |
|--------|------------------------------|

**Concern:** Can the system handle large-scale adoption?

**Answer:** DHT design from spec §11:
```
DHT: Kademlia
- Key space: 256-bit (SHA-256)
- Bucket size: 20
- Alpha (parallelism): 3
- Replication factor: 20
```

Kademlia scales logarithmically—lookups are O(log n). IPFS uses the same approach and handles millions of nodes.

**Potential bottlenecks:**
- Settlement batching—currently batches at 100 HBAR or 1 hour intervals
- GossipSub for announcements—needs tuning at scale
- Bootstrap node capacity

**Current testing:** Single node. Multi-node testing is a future priority.

---

## 12. What Are the Privacy Implications?

| Status | Known Concern |
|--------|---------------|

**Concern:** Can others monitor what I'm querying through the DHT?

**Answer:** From spec §13.4:

| Visible to Network | Hidden from Network |
|--------------------|--------------------|
| Content hashes (not content) | Private content (entirely local) |
| L1 previews (for shared content) | Query text (between querier and node) |
| Provenance chains | Unlisted content (unless you have hash) |
| Payment amounts (in settlement batches) | |

**Current state:** Your query goes directly to the content owner—not routed through random peers. But DHT lookups (finding where content lives) are visible.

**Future improvements (from spec):**
- ZK proofs for provenance verification
- Private settlement channels
- Onion routing for query privacy

---

## 13. What If Hedera Fails? Is Multi-Chain Supported?

| Status | Abstracted |
|--------|------------|

**Concern:** The protocol is tied to Hedera. What happens if Hedera has issues?

**Answer:** Currently Hedera-specific, but abstracted behind a trait:

```rust
pub trait Settlement {
    fn submit_batch(&self, batch: SettlementBatch) -> Result<TransactionId>;
    fn verify_settlement(&self, tx_id: &TransactionId) -> Result<SettlementStatus>;
    fn open_channel(&self, peer: &PeerId, deposit: Amount) -> Result<ChannelId>;
    fn close_channel(&self, channel_id: &ChannelId) -> Result<TransactionId>;
}
```

**Why Hedera was chosen:**
- Fast finality (3-5 seconds)
- Low cost (~$0.0001/tx)
- High throughput (10,000+ TPS)
- Good for micropayment batching

**Multi-chain possibility:** The `Settlement` trait could have implementations for Solana, Arbitrum/Optimism (L2s), or even Bitcoin Lightning.

**Current priority:** Prove the model works on one chain first, then generalize.

---

## 14. What Token Does Nodalync Use?

| Status | Designed |
|--------|----------|

**Concern:** Is there an NDL or DNL token?

**Answer:** **Neither.** The protocol uses **HBAR directly** (Hedera's native token)—no native token.

From spec §12.4:
> - Eliminates token bootstrapping complexity
> - Leverages existing HBAR liquidity and exchanges
> - Avoids securities/regulatory concerns
> - Allows focus on proving the knowledge economics model

All amounts are denominated in **tinybars** (10⁻⁸ HBAR).

---

# Practical & UX Questions

## 15. Could AI Tools Accidentally Spend Large Amounts?

| Status | Designed |
|--------|----------|

**Concern:** AI agents might fire off many queries rapidly, leading to unexpected bills.

**Answer:** This is addressed at the application layer.

**Budget controls are application-layer responsibility:**
From the whitepaper (Section 7.2):
> "Application-level concerns—budget controls, cost previews, spending limits, auto-approve settings—are outside protocol scope."

**The MCP server implementation includes budget tracking:**
```rust
struct QueryInput {
    query: String,
    budget_hbar: f64,
}
```

Agents are configured with a session budget and cannot exceed it. When budget is exhausted, queries are rejected.

**Cost preview before execution:** The PREVIEW operation is free. Agents can check content price before querying.

---

## 16. Can Stolen Content Enter the System?

| Status | Known Limitation |
|--------|------------------|

**Concern:** People can upload material they don't own, and the system has no built-in way to prevent profiting from stolen work.

**Answer:** The protocol cannot prevent unauthorized uploads at the entry point, but it provides strong deterrence and evidence mechanisms.

**Timestamps provide priority evidence:**
From the whitepaper (Section 10.5):
> "Timestamps record when content was published in-system. Earnings are fully visible and auditable. Evidence for legal recourse is built-in, not forensic."

**Audit trails document everything:** Every query, every payment, every derivation is logged with cryptographic proof.

**Republished content lacks provenance benefits:**
> "Republished content lacks provenance linkage to the original; the original has earlier timestamps providing evidence of priority."

**Future enhancement:** Embedding similarity detection can flag potential copies at the application layer.

**Practical advice for creators:**
- Publish to Nodalync first to establish timestamped priority
- Also establish external prior art (arXiv, journal publication, etc.)
- The protocol itself can serve as a proof-of-creation layer

---

## 17. Is There a GUI or Only CLI?

| Status | CLI Only |
|--------|----------|

**Concern:** How do non-technical users interact with the protocol?

**Answer:** Currently CLI only.

**What exists:**
- `nodalync init` — setup
- `nodalync publish` — publish content
- `nodalync query` — query content
- `nodalync mcp-server` — for Claude Desktop integration

**Planned CLI polish:**
- `nodalync search <query>`
- `nodalync wallet`
- `nodalync earnings`

**GUI/Web:** Not in the current roadmap. Focus is proving economics work first. A web interface would be an application built on top of the protocol.

---

## 18. Can I Bulk Import Existing Content?

| Status | Deferred |
|--------|----------|

**Concern:** How do I migrate an existing knowledge base to Nodalync?

**Answer:** Not built yet. Current workflow is:
1. `nodalync init`
2. `nodalync publish <file>` one at a time
3. Extract L1 manually or with rule-based extractor

**What would help:**
- Directory scanner that publishes all files
- Watch mode for auto-publishing new content
- Integration with existing knowledge bases

Explicitly deferred—after core experience works.

---

## 19. Is There a Takedown Mechanism for Copyright?

| Status | Out of Scope |
|--------|--------------|

**Concern:** How do copyright holders request content removal?

**Answer:** The spec explicitly says this is out of scope:
> "Content moderation — policy decisions for specific communities/jurisdictions."
> "Takedown mechanisms — legal/policy layer above protocol."

The protocol is infrastructure—like IPFS doesn't have takedowns, but Pinata (an application) can.

**Practical implications:**
- Content is stored on the owner's node (local-first)
- Removing your node removes your content
- No global "delete" because there's no central storage
- DMCA-type requests would go to node operators, not the protocol

---

# Summary Table

| Question | Status | Notes |
|----------|--------|-------|
| Gaming with low-effort content | Designed | Revenue only flows on queries; can't insert into others' chains |
| Content flooding/spam | Designed | Market incentives + rate limits + app-layer filtering |
| L3 provenance gaming | Known Limitation | Payment proof required; content quality not verified |
| Equal pay for unequal work | Designed | Market prices quality; weighting handles frequency |
| Early mover advantage | Designed (Intentional) | Feature, not bug; quality still matters |
| Single payment caching | Designed | Standard for information goods; derivation still pays |
| Pricing guidance | Gap | Market signals exist, no recommendations yet |
| Discovery without hash | Designed | `list_sources` MCP tool; full search is app-layer |
| L1 extraction | Designed | Rule-based MVP, plugin architecture for AI |
| Cold start | Designed | Seed with own content first; prove economics |
| Scalability | Designed (Untested) | Kademlia DHT scales logarithmically |
| Privacy | Known Concern | DHT lookups visible; onion routing planned |
| Multi-chain | Abstracted | Trait exists; Hedera-only for now |
| Token name | Designed | HBAR (no native token) |
| AI runaway spending | Designed | Application-layer budgets; free previews |
| Stolen content | Known Limitation | Timestamps + audit trails for legal recourse |
| GUI | Gap | CLI only; GUI would be app-layer |
| Bulk import | Deferred | Not built yet |
| Takedowns | Out of Scope | Legal/policy layer above protocol |

---

*Document Version: 2.0*
*Last Updated: January 2026*
*References: Nodalync Whitepaper, Protocol Specification v0.2.1-draft*
*Contract: 0.0.7729011 (Hedera Testnet)*
