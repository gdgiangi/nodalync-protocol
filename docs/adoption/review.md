# Nodalync: make useful knowledge worth maintaining

**Research and implementation review · 7 September 2026**  
**Baseline:** [`db60d90`](https://github.com/gdgiangi/nodalync-protocol/tree/db60d90805f705149d24fff2559a26a97b37ee2d). This is a proposal, not a statement that the launch gates below have passed.

Nodalync's most consequential opportunity is to make maintaining useful knowledge economically worthwhile as AI becomes its principal consumer. The test is whether better access to expert evidence improves a real task, and whether the people maintaining that evidence receive verifiable compensation at an acceptable total cost.

That is a stronger, testable proposition than guaranteed passive royalties. The protocol can account for participating transactions. It cannot observe every future use of information once a recipient has received it.

## Why this problem matters

An agent can generate a plausible answer without having the latest failure report, a maintainer's unpublished workaround, or the context behind an engineering decision. More inference does not necessarily supply that missing evidence. An expert may possess it, but publishing and maintaining it costs time. An application needs an economical way to find it, assess it, obtain permission to use it, pay for it, and retain an audit trail.

Nodalync connects those activities through content identifiers, declared derivation chains, discovery, and payment distribution. Its distinctive idea is that downstream synthesis can continue compensating upstream contributors. Successful implementation could support a recurring loop: useful evidence improves an agent's work; buyers pay for that improvement; revenue funds updates, corrections, and new evidence.

This is an inference about a potential market, not evidence of current demand. Nodalync does not improve model training or intelligence by itself. The first contribution to AI should be a demonstrated improvement in completed tasks using information agents otherwise lack, with a sustainable supply of that information.

The [roles essay](https://gabegiangi.com/2026/02/21/every-role-in-the-knowledge-economy-thats-coming/) identifies creators, consumers, agents, synthesizers, and infrastructure providers. The [application builders essay](https://gabegiangi.com/2026/02/23/app-builders-arent-dumb-pipes-theyre-the-most-profitable-actors-in-the-knowledge-economy/) correctly highlights that applications bring demand and can charge for service. Those are useful hypotheses. Neither publication volume nor adding a routing fee establishes profitability; buyers must still prefer the product and cover its full costs.

## What is actually built

The Rust workspace contains a substantive implementation, plus a placeholder umbrella crate. The architecture documents include illustrative interfaces; the executable code is the more reliable guide to current behavior.

| Component | Actual responsibility and useful entry point |
|---|---|
| Identity and addressing | SHA-256 content hashes, Ed25519 keys/signatures, and Nodalync peer identifiers in [`nodalync-crypto`](https://github.com/gdgiangi/nodalync-protocol/tree/db60d90805f705149d24fff2559a26a97b37ee2d/crates/protocol/nodalync-crypto/src). Network transport also uses libp2p peer IDs, which must be mapped correctly. |
| Knowledge model | L0 source bytes; L1 extracted mentions; private L2 entities and relationships; L3 supplied insights and their provenance. [`content.rs`](https://github.com/gdgiangi/nodalync-protocol/blob/db60d90805f705149d24fff2559a26a97b37ee2d/crates/protocol/nodalync-ops/src/content.rs) accepts insight bytes; DERIVE is attribution bookkeeping, not an autonomous reasoning engine. |
| Local state | SQLite stores manifests, channels, provenance edges, peers, and settlement state; filesystem storage holds content and cache data. See [`nodalync-store`](https://github.com/gdgiangi/nodalync-protocol/tree/db60d90805f705149d24fff2559a26a97b37ee2d/crates/protocol/nodalync-store/src). A hash identifies bytes, not all mutable metadata about those bytes. |
| Wire and network | CBOR protocol messages, libp2p TCP/Noise/Yamux, Kademlia discovery, GossipSub announcements, and request/response exchange. The network crate exposes events; operations dispatch handlers. The network crate's dependency on operations is for tests, not a production dependency cycle. |
| Query service | [`query.rs`](https://github.com/gdgiangi/nodalync-protocol/blob/db60d90805f705149d24fff2559a26a97b37ee2d/crates/protocol/nodalync-ops/src/query.rs) discovers a provider, submits payment, checks returned content bytes against a hash, and caches results. [`handlers.rs`](https://github.com/gdgiangi/nodalync-protocol/blob/db60d90805f705149d24fff2559a26a97b37ee2d/crates/protocol/nodalync-ops/src/handlers.rs) checks access/payment, updates channel state, and attempts immediate settlement before returning paid content. |
| Economics | [`distribution.rs`](https://github.com/gdgiangi/nodalync-protocol/blob/db60d90805f705149d24fff2559a26a97b37ee2d/crates/protocol/nodalync-econ/src/distribution.rs) allocates a 5% owner fee and the remaining pool by declared root weight, with rounding remainder to the owner. Roots owned by the synthesizer also earn from the pool. |
| Settlement | The paid-query handler builds a batch for immediate settlement; separate operations also process queued distributions. The Hedera adapter submits contract transactions. [`NodalyncSettlement.sol`](https://github.com/gdgiangi/nodalync-protocol/blob/db60d90805f705149d24fff2559a26a97b37ee2d/contracts/src/NodalyncSettlement.sol) implements balances, channels, disputes, attestations, and batches, but retains security placeholders described below. |
| Agent interface | The RMCP server exposes tools and resources over stdio. `search_network` discovers content; `query_knowledge` expects a content hash. It returns content and accounting metadata, not a source-grounded answer to a natural-language question. The application supplies retrieval selection and reasoning. |

A normal paid remote read follows: discovery/preview → source selection → payment channel and signed payment → provider validation/channel update → settlement attempt → content response → requester verification/accounting/cache. Each arrow needs a failure contract. A successful RPC, a signed receipt, an internal accounting entry, and an independently confirmed payout are different events.

## Separate four kinds of trust

1. **Byte integrity:** do the received bytes match the requested hash?
2. **Declared attribution:** are the source links, owners, weights, and versions authentic and internally consistent?
3. **Epistemic quality:** do the sources support the answer, and is the answer correct for this task?
4. **Economic completion:** did the authorized payment reach the intended contributors exactly once?

Passing one does not establish the others. A signed false statement remains false. A valid content hash does not prove its publisher authored the work. An omitted source cannot be recovered from an otherwise consistent provenance tree. Payment is not, by itself, evidence that the seller granted particular reuse rights; the product must record the agreed terms rather than describe the protocol as automatic licensing.

The current client explicitly retrieves full content, and the CLI saves it to disk. Publishing permits discovery and serving from the publisher's node; it does not make received information noncopyable. L2's private designation is a network access boundary, not a guarantee that an L3 synthesis cannot reveal sensitive information from it. Preview extraction also deserves publisher review because useful facts can be disclosed before purchase.

## Findings that change the implementation priorities

These observations describe the baseline. Companion fixes should be assessed by their regression tests; they do not constitute a complete security audit.

| Priority | Evidence at the baseline | Required result |
|---|---|---|
| P0 | Contract `closeChannel`, `disputeChannel`, and `counterDispute` check signature length instead of authenticating the signed state. | Before real-value operation, specify and implement account/key binding, both-party authorization or valid dispute evidence, nonce/replay handling, and conservation of channel funds. Test hostile participants. Do not substitute EVM signatures for the existing identity model without a versioned design. |
| P0 | Incoming paid-query handling permits missing payer keys to skip verification. The payment validator does not bind the supplied channel ID to the loaded channel. Nonzero payments attached to free content bypass the priced-content validation branch. | Reject unauthenticated nonzero payments before mutating channel, earnings, or settlement state. Preserve an explicitly free zero-payment path. |
| P0 | The Hedera adapter encodes batch entries as EVM address/amount data; the checked-in contract decoder reads a Hedera account-number layout. The contract debits the batch sender, whereas the provider submits the batch. | Verify adapter/contract/deployed-bytecode compatibility and prove that buyer collateral funds the intended recipients without duplicate liabilities. A successful mock or transaction status cannot resolve this source-level mismatch. No deployed-contract exploit is asserted by this review. |
| P1 | MCP's documented automatic allowance is not applied to omitted per-call limits or resource reads. Automatic deposits and channel funding are separate from reported query spending. | Enforce the documented default allowance; separately design operator-authorized funding limits. An agent-supplied override is not human approval. |
| P1 | `reference_l3_as_l0` builds a replacement L0 manifest, but the manifest store uses `INSERT OR IGNORE` for the already present hash. | Make the local reference effective and durable while retaining the authentic source manifest. A later derivation must include both upstream roots and the original L3 creator as §7.1.6 specifies. |
| P1 | Provenance validation compares root hashes/weights but ignores recipient identity in `roots_match`. Remote query paths store returned manifests after checking content bytes, without establishing all manifest/provenance assertions. | Authenticate source metadata and validate complete recipient-bearing provenance before relying on it for new derivations. Test unchanged bytes with substituted metadata. |
| P1 | The payment signature encoding omits fields including the request nonce. Contract batch settlement accepts an advertised Merkle root without recomputing a proof of provenance distribution. | Bind the complete payment intent and quote to authorization; specify how evidence links source ownership to payout. A transaction ID or emitted root alone is insufficient. |
| P1 | Provider channel mutation, delivery, requester accounting, and settlement are separate operations with failure windows. | Define idempotent retry, crash recovery, receipt verification, and uncertain-outcome reconciliation. Reconcile confirmed balances independently of internal counters. |
| P1 | The [CI security audit](https://github.com/gdgiangi/nodalync-protocol/actions/runs/34159817072/job/101859074382) reports 14 vulnerability findings against the unchanged baseline lockfile, including HTTP/DNS, QUIC, archive, MCP, and certificate-validation dependencies. | Review feature/path reachability, upgrade affected dependency families with integration tests, and rerun the audit. Some remedies cross major versions; avoid hiding failures with blanket advisory suppression. |

Failing closed on missing payer keys exposes another prerequisite: current production discovery does not populate authenticated Nodalync payer keys in the peer store. The libp2p transport identity is distinct from the Nodalync signing identity. A signed, verified binding and key-discovery path must precede rollout of the payment hardening; tests that manually register keys do not establish that integration.

The proposed x402/Base/ERC-8004 and organizational-node documents in the working checkout are useful future designs, not shipped behavior at the reviewed commit. Their examples also contemplate different economic splits. Resolve that explicitly in a versioned decision before implementing a transport adapter; retain the current 95/5 behavior in compatibility fixes.

## The economic question a larger simulation will not resolve

The 95/5 rule allocates revenue among declared roots. It does not allocate 95% specifically to humans, independent experts, or the most causally valuable evidence.

Consider a 10,000-unit query with one genuinely valuable external root and 99 equally weighted roots owned by the synthesizer. The owner fee is 500; the root pool is 9,500; each root earns 95. The external expert receives 95, or 0.95% of the payment. The synthesizer receives 9,905. The split remains exactly 95/5. No fake identities were required.

This is a conditional arithmetic example, not a measured attack rate. It shows why source count is not contribution value. Direct-L0 weight duplication in current construction does not rescue the example when it affects all roots equally. Repeated import wrappers and duplicate paths also need adversarial tests. Any fix must distinguish legitimate complementary sources from padding without making an untrusted publisher the judge of its own contribution.

The [simulation report](../papers/simulation.md) is useful for exploring its assumptions. It uses quality-weighted selection, externally supplied demand, fixed behaviors, and controller-aware selection dampening. The report itself notes that dampening is not protocol enforcement and shows profitable long-horizon adversaries. Its confidence intervals describe runs of that model; they do not establish willingness to pay or resistance to adaptive attackers. The reviewed tree contains the report and charts, but not the advertised simulator code, complete configurations, and raw results. Reproducibility remains unverified until those artifacts are linked and runnable.

Three experiments matter more than another favorable actor ranking:

- **Substitution:** hold answer quality constant while permitting cheap copies, caching, source omission, and alternative suppliers. Does buyers' repeated spending persist?
- **Strategic attribution:** reward externally valuable answers while allowing irrelevant owned roots and import wrappers. How much of external buyers' money reaches the evidence they actually needed?
- **Budget closure:** model finite buyer budgets, churn, author maintenance, hosting, acquisition, model inference, settlement fees, and application support. Separate external revenue from transfers among actors. Cumulative transferred value is not automatically capital locked or profit.

Changing the split is premature. First measure the failure and compare candidate attribution policies on the same workload. Policies such as curated supplier admission or reviewed citations are reasonable pilot controls, but should be labeled governance rather than cryptographic resistance.

## Win one workflow before opening a marketplace

Start with **agent-assisted software integration troubleshooting**: current maintainer notes, incident explanations, version-specific workarounds, and small reproducible examples that a public documentation search misses. This is a proposed starting market because tasks can often be checked with executable outcomes and corrections have repeat value. It is not a claim that this market has already been validated.

Recruitment targets for a first pilot: three to five consenting experts, one hosting operator, and two application teams that already encounter these tasks. People should be able to contribute selected documents, review previews and terms, set attribution and prices, and inspect statements without operating a node. Treat this assisted hosting as disclosed custodial/service responsibility with export and removal processes; it is a proposed product service, not existing multi-creator support.

Agents need a short path: discover → inspect relevance/version/terms/price → authorize within operator policy → retrieve → cite → inspect receipt and payout status. For every failure they need machine-readable state and a safe retry rule. Default to read-only consumption and explicit publishing tools. Source content must be treated as untrusted data, including any embedded instructions to spend or publish. MCP annotations are hints rather than an authorization boundary, as the [MCP tool specification](https://modelcontextprotocol.io/specification/2025-06-18/server/tools) explains.

Use the [pilot protocol](pilot.md) to test this with matched tasks and a conventional retrieval baseline. There must be a benefit beyond merely putting the same documents behind another interface.

## Build in this order

| Stage | Deliverable | Decision gate |
|---|---|---|
| Correctness | Focused payment and MCP fixes; effective import semantics; remaining security findings tracked. | Regression tests fail on the old behavior and pass on the fix. No claim of production settlement safety. |
| Evidence contract | A design for authenticated source manifests, quotes, permissions references, receipts, payout state, and retry identity. Keep task evidence separate from private reasoning. | Two independent implementations agree on canonical vectors and reject substituted recipients, replayed quotes, omitted required fields, and malformed proofs. |
| Narrow pilot | A maintained corpus, matched task set, cost/quality ledger, and consented human evaluation. Initially local/mock and testnet only. | Useful evidence measurably improves outcomes; people choose to return; operator costs are visible. |
| Distribution | Add an HTTP payment adapter only after core evidence and settlement contracts are stable. | A second application integrates with bounded effort; economic and provenance behavior stays identical across transports. |
| Open participation | Supplier onboarding, dispute policy, recovery, monitoring, and an independently reviewed real-value settlement design. | Repeat demand and actual payouts justify expanding the network. |

[x402](https://github.com/x402-foundation/x402/blob/main/specs/x402-specification-v2.md) can standardize payment negotiation; it does not supply Nodalync's attribution policy or client budget management. [ERC-8004](https://eips.ethereum.org/EIPS/eip-8004) offers identity/reputation/validation interfaces and explicitly acknowledges Sybil attacks and the limits of advertised capabilities. Both are potential distribution tools. Neither resolves source quality, authorship, or royalty omission.

For an application, calculate contribution margin per successful task as the user payment minus source purchases, model inference, hosting, payment costs, support, and refunds. A separately disclosed service fee can fund the application while preserving the source-query split. The fee is viable only if the user values the result enough to pay the total.

The next milestone should be small enough to disprove: two application teams repeatedly solve a real task using maintained evidence from independent contributors, with a complete reconciliation from authorized spend to the contributors' receipts. If that fails, change the corpus, task, or business model before increasing protocol complexity.

## Review artifacts and validation

The [issue index](https://github.com/gdgiangi/nodalync-protocol/pull/45/files) tracks the open design questions. Companion proposals implement [payment authentication](https://github.com/gdgiangi/nodalync-protocol/pull/46), [MCP default allowances](https://github.com/gdgiangi/nodalync-protocol/pull/47), [durable L3 references](https://github.com/gdgiangi/nodalync-protocol/pull/48), and [stable-Rust lint compatibility](https://github.com/gdgiangi/nodalync-protocol/pull/49). Payment authentication and L3 references remain drafts for the integration and compatibility reasons above.

The reviewed baseline passed 1,136 tests. Combining the four code changes in an isolated checkout passed **1,166 tests with all features** on Rust 1.98.0, with three existing ignored examples. Workspace Clippy with all targets/features and warnings denied, formatting, and the documentation build also passed. The article's exact mock-distribution command was executed successfully. Independent reviews led to additional refund, cache-eviction, and visibility-change regressions.

These checks validate local code behavior and compatibility among the proposed patches. They do not certify real-value settlement, prove live network adoption, or resolve the dependency audit. The 14 security-audit findings remain open; the small lint fix does not upgrade third-party packages.
