# Nodalync Cognitive Architecture Spec
## Source: Gabe + Claude conversation, Feb 14 2026
## Captured: Feb 18 2026

## Core Thesis
An agent can ingest raw data, structure it against a schema, reason over it to produce novel insights, ground those insights against observed reality, and evolve its own schema when it encounters things it can't represent. If the loop runs and demonstrably improves over cycles, the architecture is validated.

## Knowledge Model (CORRECTED)
- **L0:** Raw source documents (ground truth, immutable, timestamped, provenance-tracked)
- **L1:** Invisible connective tissue (extracted mentions linking L0→L2)
- **L2-Reality:** World knowledge graph built strictly from verified L0. Conservative. Only represents what's been observed. Last-observed-state, not oracle truth.
- **L2-Model:** Agent's working worldview. Superset of Reality. Includes L3-synthesized entities tagged as speculative.
- **L3:** Synthesis layer — reasons over L2-Model (ideally in latent space), produces insights reified back into L2-Model. L3 outputs are themselves new L0s with provenance.
- **Divergence Layer:** Continuously compares L2-Model vs L2-Reality. Classifies divergences as: hallucination, stale data, or genuine novel insight.

## Dual Graph Architecture
- **Graph A (Model):** Agent's worldview — subjective, speculative, contains L3 inferences
- **Graph B (Reality):** Last observed state — conservative, only L0-sourced, timestamped
- **The delta is the signal:** Agreement = high confidence. Divergence = one of three cases:
  1. Hallucination (model wrong) → correction signal
  2. Stale reality (ground truth outdated) → model may be ahead
  3. Genuine insight (novel prediction) → most valuable case
- System learns to distinguish these three cases over time = calibrated confidence

## Schema Layer
- Schema = ontological vocabulary defining what can be perceived
- Starts hand-authored (seed), evolves via L3 pressure signals
- Schema IS the thing: defines what can be known
- Different agents develop different schemas = different expertise/specialization
- Schema fragments are tradeable assets via Nodalync

## Sleep Cycle (Consolidation)
During active operation: reactive processing. Schema modification during active ops = dangerous (changing lens while looking through it).

Sleep solves this:
1. **Collect pressure signals:** unrepresentable flags, low-confidence L3, high divergence
2. **Schema reflection:** LLM proposes modifications based on pressure signals
3. **Validation:** Check proposals against consistency constraints (OWL reasoning)
4. **Apply + reprocess:** Apply accepted changes, reprocess recent L0 under new schema
5. **Prune + compress:** Confidence decay on uncorroborated nodes, merge redundant schema elements

## Implementation Phases

### Phase 0: Infrastructure (Week 1)
- Neo4j: two named graphs (reality, model)
- L0 store: SQLite or flat files with hash, timestamp, source_uri
- Schema registry: versioned JSON-LD, git-tracked
- Embedding infra: sentence-transformer (all-MiniLM-L6-v2 → e5-large-v2)
- Local LLM via OpenClaw (70B+ preferred, 13B minimum)

### Phase 1: Symbolic Loop (Weeks 2-3)
- L0 ingestion: focused domain (AI/ML), 2-3 stable sources
- Extraction: LLM structured prompting against schema, require text span evidence
- Entity resolution: embed + cosine similarity (0.85 threshold)
- L3 synthesis: LLM-as-reasoner (subgraph extraction → structured prompting → parse to graph ops)
- Divergence detection: property diff, structural diff, embedding divergence, staleness score
- Sleep cycle v1: weekly, human-gated schema proposals
- **Validates:** knowledge model for continual learning

### Phase 2: Graph Embeddings (Weeks 4-5)
- Train RotatE/ComplEx on Reality graph via PyKEEN
- Link prediction as L3 supplement (structural inference, not just LLM prompting)
- Entity clustering via HDBSCAN on embeddings → non-obvious structural similarities
- Weekly embedding retraining during sleep
- **Validates:** sub-symbolic reasoning adds value beyond prompting

### Phase 3: Latent Reasoning Engine (Weeks 6-10) — THE EXPERIMENT
- **Graph Context Autoencoder (GCAE):**
  - Encoder: transformer, serialized subgraph → 1024-dim latent vector
  - Latent space: VAE with KL divergence (smooth, continuous, interpolatable)
  - Decoder: transformer, latent vector → reconstructed/enriched subgraph
- Training data from Phase 1-2 (subgraph pairs: input → enriched output)
- Loss: reconstruction + KL divergence + link prediction
- **Latent reasoning operations:**
  - Interpolation: blend two domain subgraphs → cross-domain insight
  - Analogy: latent arithmetic (B + (A'−A) = B')
  - Clustering/traversal: regions of latent space = knowledge pattern types

### Phase 4: Full Autonomous Loop (Weeks 10-12)
- Waking cycle (continuous): L0 ingest → extract → write → flag unrepresentable
- Reasoning cycle (every few hours): GCAE + LLM synthesis → validate → write to Model
- Sleep cycle (nightly): retrain embeddings, fine-tune GCAE, schema reflection, decay/prune
- Daily intelligence briefing output

## Risks (Self-Critical Assessment)
1. **Data scarcity for GCAE** — may need 10x more data than 5 weeks produces
2. **Decoded outputs may not be valid graphs** — need constrained decoding + schema validation
3. **Latent space may not be meaningfully structured** — interpolation could produce nonsense
4. **GCAE may just mimic LLM** — must track unique signal vs LLM-only baseline
5. **Schema evolution via LLM is fragile** — stage changes, validate, rollback capability essential
6. **LLM extraction contaminates reality graph** — need extractive (not generative) constraints

## Nodalync Protocol Implications
- L0 = data market (x402 payments for quality sources)
- Schema fragments = tradeable assets (selling "ways of seeing")
- L3 insights carry full provenance chain + divergence score
- Divergence score = trust primitive for knowledge market pricing
- GCAE latent space = agent's cognitive fingerprint
- Inter-agent knowledge exchange: latent vectors decoded through receiver's own decoder = conceptual translation
- Payment chain follows provenance: L0 creator compensated when their data contributes to sold L3

## Key Insight
"The knowledge graph is the accountability layer. The latent space is the reasoning layer."

The knowledge model doesn't depend on Phase 3 (latent reasoning) succeeding. Phases 1-2 validate the recursive loop. Phase 3 amplifies it.
