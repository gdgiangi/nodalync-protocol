# Nodalync Protocol Specification — L2 Entity Graph Addendum

**Version:** 0.2.0-draft  
**Date:** January 2026  
**Status:** Draft Addendum to v0.1.0

---

## Summary of Changes

This addendum elevates L2 (Entity Graph) from internal-only to a protocol-level content type, while keeping it as **personal/private** content. Key design decisions:

1. **Complete provenance chain:** L0 → L1 → L2 → L3
2. **L2 is personal:** Your L2 represents your unique perspective — it is never queried by others
3. **URI-based ontology:** Entity types and relationship predicates use URIs for RDF interoperability
4. **L3 derives from L2:** Your insights (L3) are built on your knowledge graph (L2)

---

## Design Philosophy

### L2 is Your Perspective

L2 represents how *you* understand and link entities across the documents you've studied. Two people reading the same papers might build very different L2 graphs based on:
- Which entities they consider important
- How they resolve ambiguous references
- What relationships they infer
- Which external ontologies they use

This is valuable intellectual work, but it's **personal**. Your L2 is never directly monetized — its value surfaces when you create L3 insights that others find valuable.

### Economic Model

```
Alice's L0 (document)
       ↓
Bob queries Alice's L0 → Alice gets paid
       ↓
Bob extracts L1 from Alice's L0
       ↓
You query Bob's L1 → Bob gets paid (Alice gets root share)
       ↓
You build L2 from Bob's L1 (YOUR perspective)
       ↓
You create L3 insight from YOUR L2
       ↓
Eve queries YOUR L3 → You get 5% synthesis fee
                    → Alice gets 95% (she's in root_L0L1)
```

Your L2 work is "invisible" economically — the compensation comes from your L3 insights.

### URI-Based Ontology

Instead of closed enums, L2 uses URIs for extensibility:

```
# Entity types (can be any ontology)
entity_types: ["schema:Person", "foaf:Person"]
entity_types: ["ndl:Concept"]
entity_types: ["http://example.org/ontology#CustomType"]

# Relationship predicates
predicate: "schema:worksFor"
predicate: "ndl:mentions"
predicate: "http://purl.org/dc/terms/creator"
```

This enables:
- Standard ontologies (Schema.org, FOAF, Dublin Core)
- Custom domain-specific ontologies
- Interoperability with semantic web tools
- No protocol changes needed for new types

---

## 1. Updated Data Structures

### §4.1 Content Types (REPLACE)

```
enum ContentType : uint8 {
    L0 = 0x00,      # Raw input (documents, notes, transcripts)
    L1 = 0x01,      # Mentions (extracted atomic facts)
    L2 = 0x02,      # Entity Graph (linked entities and relationships)
    L3 = 0x03       # Insights (emergent synthesis)
}
```

**Knowledge Layer Semantics:**

| Layer | Content | Typical Operation | Value Added |
|-------|---------|-------------------|-------------|
| L0 | Raw documents, notes, transcripts | CREATE | Original source material |
| L1 | Atomic facts extracted from L0 | EXTRACT_L1 | Structured, quotable claims |
| L2 | Entities and relationships across L1s | BUILD_L2 | Cross-document linking, entity resolution |
| L3 | Novel insights synthesizing sources | DERIVE | Original analysis and conclusions |

---

### §4.4a Entity Graph (L2) (NEW SECTION)

Insert after §4.4 Mention:

```
struct L2EntityGraph {
    # === Core Identity ===
    id: Hash,                           # H(serialized entities + relationships)
    
    # === Sources ===
    source_l1s: L1Reference[],          # L1 summaries this graph was built from
    source_l2s: Hash[],                 # Other L2 graphs merged/extended (optional)
    
    # === Graph Content ===
    entities: Entity[],                 # Resolved entities
    relationships: Relationship[],      # Relationships between entities
    
    # === Statistics ===
    entity_count: uint32,
    relationship_count: uint32,
    source_mention_count: uint32        # Total mentions linked
}

struct L1Reference {
    l1_hash: Hash,                      # Hash of the L1Summary content
    l0_hash: Hash,                      # The original L0 this L1 came from
    mention_ids_used: Hash[]            # Which specific mentions were used
}

struct Entity {
    id: Hash,                           # Stable entity ID: H(canonical_label || entity_type)
    canonical_label: string,            # Primary name (max 200 chars)
    aliases: string[],                  # Alternative names/spellings (max 50)
    entity_type: EntityType,
    
    # === Evidence ===
    source_mentions: MentionRef[],      # Which L1 mentions establish this entity
    
    # === Confidence ===
    confidence: float64,                # 0.0 - 1.0, resolution confidence
    resolution_method: ResolutionMethod,
    
    # === Optional Metadata ===
    description: string?,               # Summary description (max 500 chars)
    external_ids: ExternalId[]?         # Links to external knowledge bases
}

struct MentionRef {
    l1_hash: Hash,                      # Which L1 contains this mention
    mention_id: Hash                    # Specific mention ID within that L1
}

struct ExternalId {
    system: string,                     # e.g., "wikidata", "orcid", "doi"
    identifier: string                  # The ID in that system
}

struct Relationship {
    id: Hash,                           # H(subject || predicate || object)
    subject: Hash,                      # Entity ID
    predicate: string,                  # Relationship type (max 100 chars)
    object: RelationshipObject,         # Entity ID or literal
    
    # === Evidence ===
    source_mentions: MentionRef[],      # Mentions that support this relationship
    confidence: float64,                # 0.0 - 1.0
    
    # === Temporal (optional) ===
    valid_from: Timestamp?,
    valid_to: Timestamp?
}

enum RelationshipObject {
    EntityRef(Hash),                    # Reference to another entity
    Literal(LiteralValue)               # A value (string, number, date)
}

struct LiteralValue {
    value_type: LiteralType,
    value: string                       # Encoded value
}

enum LiteralType : uint8 {
    String    = 0x00,
    Integer   = 0x01,
    Float     = 0x02,
    Date      = 0x03,                   # ISO 8601
    DateTime  = 0x04,                   # ISO 8601
    Boolean   = 0x05,
    Uri       = 0x06
}

enum EntityType : uint8 {
    Person       = 0x00,
    Organization = 0x01,
    Location     = 0x02,
    Concept      = 0x03,
    Event        = 0x04,
    Work         = 0x05,                # Paper, book, article, etc.
    Product      = 0x06,
    Technology   = 0x07,
    Metric       = 0x08,                # Quantitative measure
    TimePoint    = 0x09,
    Other        = 0xFF
}

enum ResolutionMethod : uint8 {
    ExactMatch    = 0x00,               # Same string
    Normalized    = 0x01,               # Case/punctuation normalized
    Alias         = 0x02,               # Known alias matched
    Coreference   = 0x03,               # Pronoun/reference resolved
    ExternalLink  = 0x04,               # Matched via external KB
    Manual        = 0x05,               # Human-verified
    AIAssisted    = 0x06                # ML model assisted
}
```

**Constraints:**

```
L2 Entity Graph constraints:
    1. len(source_l1s) >= 1              # Must derive from at least one L1
    2. len(entities) >= 1                 # Must have at least one entity
    3. Each entity.id is unique within the graph
    4. Each relationship references valid entity IDs
    5. All MentionRefs point to valid L1s in source_l1s
    6. 0.0 <= confidence <= 1.0
    7. len(canonical_label) <= 200
    8. len(aliases) <= 50
    9. len(predicate) <= 100
    10. entity_count == len(entities)
    11. relationship_count == len(relationships)
```

---

### §4.4b L2 Summary (Preview) (NEW SECTION)

For previewing L2 content without revealing the full graph:

```
struct L2Summary {
    l2_hash: Hash,                      # Hash of the full L2EntityGraph
    entity_count: uint32,
    relationship_count: uint32,
    source_l1_count: uint32,
    
    # === Preview (free) ===
    top_entities: EntityPreview[],      # Top 10 entities by mention count
    entity_type_distribution: TypeCount[], # How many of each type
    relationship_types: string[],       # List of predicates used (max 20)
    
    # === Quality Indicators ===
    avg_confidence: float64,
    cross_document_links: uint32        # Entities appearing in multiple L1s
}

struct EntityPreview {
    id: Hash,
    canonical_label: string,
    entity_type: EntityType,
    mention_count: uint32,              # How many mentions support this entity
    relationship_count: uint32          # Relationships involving this entity
}

struct TypeCount {
    entity_type: EntityType,
    count: uint32
}
```

---

### §4.5 Provenance (UPDATED)

Update the constraints to include L2:

```
struct Provenance {
    root_L0L1: ProvenanceEntry[],       # All foundational L0/L1 sources
    derived_from: Hash[],                # Direct parent hashes (any content type)
    depth: uint32                        # Max derivation depth from any L0
}

Constraints:
    - root_L0L1 contains entries of type L0 or L1 only (never L2 or L3)
    - L0 content: root_L0L1 = [self], derived_from = [], depth = 0
    - L1 content: root_L0L1 = [parent L0], derived_from = [L0 hash], depth = 1
    - L2 content: root_L0L1 = merged roots from source L1s, 
                  derived_from = source L1 hashes, depth = max(source.depth) + 1
    - L3 content: root_L0L1 = merged roots from all sources,
                  derived_from = source hashes, depth = max(source.depth) + 1
    - All entries in derived_from MUST have been queried by creator
```

**Provenance Chain Examples:**

```
Simple chain:
    L0(doc) → L1(mentions) → L2(entities) → L3(insight)
    depth:  0       1            2              3

Branching:
    L0(doc1) → L1(m1) ─┐
                       ├→ L2(graph) → L3(insight)
    L0(doc2) → L1(m2) ─┘
    
    L2.provenance = {
        root_L0L1: [doc1, doc2],
        derived_from: [m1, m2],
        depth: 2
    }
    
    L3.provenance = {
        root_L0L1: [doc1, doc2],  # Inherited from L2
        derived_from: [L2.hash],
        depth: 3
    }

L3 deriving directly from L1 (skipping L2):
    L0(doc) → L1(mentions) → L3(insight)
    
    L3.provenance = {
        root_L0L1: [doc],
        derived_from: [mentions],
        depth: 2
    }
    
L3 deriving from mix of L1 and L2:
    L0(doc1) → L1(m1) → L2(graph) ─┐
                                    ├→ L3(insight)
    L0(doc2) → L1(m2) ─────────────┘
    
    L3.provenance = {
        root_L0L1: [doc1, doc2],  # Merged from both paths
        derived_from: [L2.hash, m2],
        depth: 4  # max(3, 2) + 1
    }
```

---

## 2. Updated Message Types

### §6.2 Discovery Messages (UPDATED)

L2 content can be announced and searched like any other content:

```
# AnnouncePayload for L2
When content_type == L2:
    l1_summary field is replaced with l2_summary: L2Summary
    
# SearchResult for L2  
struct SearchResult {
    hash: Hash,
    content_type: ContentType,
    title: string,
    owner: PeerId,
    # Type-specific preview:
    l1_summary: L1Summary?,      # If L0 or L1
    l2_summary: L2Summary?,      # If L2
    price: Amount,
    total_queries: uint64,
    relevance_score: float64
}
```

### §6.3a L2 Preview Messages (NEW)

```
# L2_PREVIEW_REQUEST = 0x0210
struct L2PreviewRequestPayload {
    hash: Hash
}

# L2_PREVIEW_RESPONSE = 0x0211
struct L2PreviewResponsePayload {
    hash: Hash,
    manifest: Manifest,
    l2_summary: L2Summary
}
```

### §6.1 MessageType (UPDATED)

Add new message types:

```
enum MessageType : uint16 {
    # ... existing types ...
    
    # L2 Preview (0x02xx range, after Preview)
    L2_PREVIEW_REQUEST   = 0x0210,
    L2_PREVIEW_RESPONSE  = 0x0211,
}
```

---

## 3. Updated Protocol Operations

### §7.1.2a Build L2 (Entity Graph) (NEW OPERATION)

Insert after §7.1.2 Extract L1:

```
BUILD_L2(source_l1s: Hash[], config: L2BuildConfig?) → Hash

Purpose:
    Build an L2 Entity Graph from one or more L1 sources.
    This operation performs entity extraction, resolution, and relationship inference.

Preconditions:
    - All source L1s have been queried (payment proof exists)
    - len(source_l1s) >= 1

Procedure:
    1. Verify all L1 sources were queried:
       For each l1_hash in source_l1s:
           assert cache.has(l1_hash) OR content.has(l1_hash)
           l1 = load_l1(l1_hash)
           assert l1.content_type == L1
           
    2. Extract entities from mentions:
       raw_entities = []
       For each l1 in source_l1s:
           For each mention in l1.mentions:
               extracted = extract_entities(mention)
               raw_entities.extend(extracted)
               
    3. Resolve entities (merge duplicates):
       resolved_entities = resolve_entities(raw_entities, config)
       # This handles:
       #   - Exact string matching
       #   - Alias resolution
       #   - Coreference resolution
       #   - External KB linking (optional)
       
    4. Extract relationships:
       relationships = extract_relationships(resolved_entities, source_l1s)
       
    5. Build L2 structure:
       l2_graph = L2EntityGraph {
           id: computed after serialization,
           source_l1s: [L1Reference for each l1],
           source_l2s: [],
           entities: resolved_entities,
           relationships: relationships,
           entity_count: len(resolved_entities),
           relationship_count: len(relationships),
           source_mention_count: total_mentions_linked
       }
       
    6. Compute hash:
       content = serialize(l2_graph)
       hash = ContentHash(content)
       l2_graph.id = hash
       
    7. Compute provenance:
       root_entries = []
       For each l1 in source_l1s:
           l1_prov = get_provenance(l1)
           For each entry in l1_prov.root_L0L1:
               merge_or_increment(root_entries, entry)
       
       provenance = Provenance {
           root_L0L1: root_entries,
           derived_from: source_l1s,
           depth: max(l1.provenance.depth for l1 in source_l1s) + 1
       }
       
    8. Create manifest:
       manifest = Manifest {
           hash: hash,
           content_type: L2,
           owner: my_peer_id,
           version: Version { number: 1, previous: null, root: hash, ... },
           visibility: Private,
           provenance: provenance,
           ...
       }
       
    9. Store content and manifest locally
    10. Return hash

struct L2BuildConfig {
    # Entity resolution settings
    resolution_threshold: float64?,     # Minimum confidence to merge (default: 0.8)
    use_external_kb: bool?,             # Link to external knowledge bases
    external_kb_list: string[]?,        # Which KBs to use: ["wikidata", "dbpedia"]
    
    # Relationship extraction
    extract_implicit: bool?,            # Infer relationships not explicitly stated
    relationship_types: string[]?       # Limit to specific predicates
}
```

### §7.1.2b Merge L2 (NEW OPERATION)

Merge multiple L2 graphs into one:

```
MERGE_L2(source_l2s: Hash[], config: L2MergeConfig?) → Hash

Purpose:
    Combine multiple L2 Entity Graphs, resolving entities across them.
    Creates a unified knowledge graph from multiple domain-specific graphs.

Preconditions:
    - All source L2s have been queried (payment proof exists)
    - len(source_l2s) >= 2

Procedure:
    1. Verify all L2 sources were queried
    
    2. Collect all entities and relationships from sources
    
    3. Cross-graph entity resolution:
       # Find same entities appearing in different graphs
       merged_entities = resolve_across_graphs(source_l2s, config)
       
    4. Merge relationships (update entity references)
    
    5. Build new L2 with:
       source_l1s: union of all source L1 references
       source_l2s: the input source_l2s
       
    6. Compute provenance:
       # Roots come from all underlying L1s (via source L2s)
       root_entries = merge roots from all source_l2s
       
       provenance = Provenance {
           root_L0L1: root_entries,
           derived_from: source_l2s,
           depth: max(l2.provenance.depth for l2 in source_l2s) + 1
       }
       
    7. Store and return hash
```

### §7.1.5 Derive (Create L3) (UPDATED)

L3 can now derive from L2 in addition to L0, L1, and other L3:

```
DERIVE(sources: Hash[], insight_content: bytes, metadata: Metadata) → Hash

Sources may include:
    - L0 content (raw documents)
    - L1 content (mention collections)
    - L2 content (entity graphs)
    - L3 content (other insights)
    
All sources must have been queried (payment proof exists).

Provenance computation:
    For L0/L1 sources: merge their root_L0L1 directly
    For L2 sources: merge the L2's root_L0L1 (which traces back to L0/L1)
    For L3 sources: merge the L3's root_L0L1 (recursive)
    
    derived_from = all source hashes
    depth = max(source.provenance.depth) + 1
```

### §7.2.2a L2 Preview (NEW)

```
L2_PREVIEW(hash: Hash) → (Manifest, L2Summary)

Procedure:
    1. Send L2_PREVIEW_REQUEST to content owner
    2. Receive L2_PREVIEW_RESPONSE
    3. Validate manifest
    4. Return (manifest, l2_summary)
    
Cost: Free (like L1 preview)
```

---

## 4. Updated Validation Rules

### §9.1 Content Validation (UPDATED)

```
VALIDATE_CONTENT(content: bytes, manifest: Manifest) → bool

Rules:
    # ... existing rules 1-6 ...
    7. manifest.content_type in {L0, L1, L2, L3}  # Updated
    8. manifest.visibility in {Private, Unlisted, Shared}
    
    # L2-specific validation
    9. If manifest.content_type == L2:
           l2 = deserialize(content) as L2EntityGraph
           assert l2.id == manifest.hash
           assert len(l2.source_l1s) >= 1
           assert len(l2.entities) >= 1
           assert all entity IDs are unique
           assert all relationship entity refs are valid
           assert all MentionRefs point to valid source L1s
           assert l2.entity_count == len(l2.entities)
           assert l2.relationship_count == len(l2.relationships)
```

### §9.3 Provenance Validation (UPDATED)

```
VALIDATE_PROVENANCE(manifest: Manifest, sources: Manifest[]) → bool

Rules:
    1. If manifest.content_type == L0:
           manifest.provenance.root_L0L1 == [self_entry]
           manifest.provenance.derived_from == []
           manifest.provenance.depth == 0
           
    2. If manifest.content_type == L1:
           len(manifest.provenance.root_L0L1) >= 1
           manifest.provenance.derived_from contains exactly one L0 hash
           manifest.provenance.depth == 1
           All root_L0L1 entries are type L0
           
    3. If manifest.content_type == L2:
           len(manifest.provenance.root_L0L1) >= 1
           len(manifest.provenance.derived_from) >= 1
           All derived_from are L1 or L2 hashes
           All root_L0L1 entries are type L0 or L1
           manifest.provenance.depth >= 2
           depth == max(source.depth) + 1
           
    4. If manifest.content_type == L3:
           len(manifest.provenance.root_L0L1) >= 1
           len(manifest.provenance.derived_from) >= 1
           All derived_from hashes exist in sources
           All root_L0L1 entries are type L0 or L1
           depth == max(source.depth) + 1
           
    5. For all types:
           Computed root_L0L1 matches declared root_L0L1
           No cycles in derived_from graph
           All weights > 0
```

---

## 5. Economic Rules (UPDATED)

### §10.1 Revenue Distribution (UPDATED)

The distribution formula remains unchanged. L2 creators receive payment when:

1. **Their L2 is queried directly** — They get the synthesis fee (5%) plus any roots they contributed
2. **Their L2 is used in an L3** — Their L2's root_L0L1 is merged, so underlying L0/L1 creators are paid

**Important:** L2 creators do NOT automatically get compensation when their L2 is derived from. Instead:
- The root_L0L1 (which traces back through the L2 to original L0/L1) gets paid
- If the L2 creator also created some of those L0/L1s, they get that share
- The L2 creator's work is compensated when someone **queries** the L2

This maintains the principle: **value flows to foundational contributors (L0/L1)**, while L2/L3 creators earn through synthesis fees when their content is queried.

### §10.2 Distribution Example (UPDATED)

```
Extended scenario with L2:

    Alice creates L0 (document)
    Bob extracts L1 from Alice's L0
    Carol builds L2 entity graph from Bob's L1
    Dave creates L3 insight from Carol's L2
    
    Eve queries Dave's L3 for 100 HBAR

Provenance chain:
    L0 (Alice) → L1 (Bob, depth=1) → L2 (Carol, depth=2) → L3 (Dave, depth=3)

    Dave's L3 provenance:
        root_L0L1 = [{ hash: alice_l0, owner: Alice, weight: 1 }]
        derived_from = [carol_l2]
        depth = 3

Distribution of 100 HBAR payment:
    Dave (L3 owner, synthesis fee): 5 HBAR
    Root pool: 95 HBAR

    Only root_L0L1 entries share the pool:
        Alice (L0 owner): 95 HBAR

    Carol receives nothing from THIS query.
    Carol earns when someone queries HER L2 directly.

What if Carol also contributed an L0?
    If Carol had created L0_carol that Bob also used:
        root_L0L1 = [
            { hash: alice_l0, owner: Alice, weight: 1 },
            { hash: carol_l0, owner: Carol, weight: 1 }
        ]

    Then distribution would be:
        Dave (synthesis): 5 HBAR
        Alice (1/2 root pool): 47.5 HBAR
        Carol (1/2 root pool): 47.5 HBAR
```

---

## 6. Appendix Updates

### Appendix B: Constants (ADD)

```
# L2 Entity Graph limits
MAX_ENTITIES_PER_L2 = 10000
MAX_RELATIONSHIPS_PER_L2 = 50000
MAX_ALIASES_PER_ENTITY = 50
MAX_CANONICAL_LABEL_LENGTH = 200
MAX_PREDICATE_LENGTH = 100
MAX_ENTITY_DESCRIPTION_LENGTH = 500
MAX_SOURCE_L1S_PER_L2 = 100
MAX_SOURCE_L2S_PER_MERGE = 20
```

### Appendix C: Error Codes (ADD)

```
# L2 specific errors
L2_INVALID_STRUCTURE    = 0x0210    # Malformed L2EntityGraph
L2_MISSING_SOURCE       = 0x0211    # Source L1 not found
L2_ENTITY_LIMIT         = 0x0212    # Too many entities
L2_RELATIONSHIP_LIMIT   = 0x0213    # Too many relationships
L2_INVALID_ENTITY_REF   = 0x0214    # Relationship references invalid entity
L2_CYCLE_DETECTED       = 0x0215    # Circular entity reference
```

---

## 7. Migration Notes

### Backward Compatibility

- Existing L0 → L1 → L3 chains remain valid
- L2 is optional; protocols can continue without it
- Nodes that don't understand L2 treat it as unknown content type
- Network upgrade is additive (no breaking changes)

### Recommended Upgrade Path

1. **Phase 1:** Add L2 data structures to types
2. **Phase 2:** Add L2 validation rules
3. **Phase 3:** Add BUILD_L2 operation
4. **Phase 4:** Update DERIVE to accept L2 sources
5. **Phase 5:** Add L2 preview messages
6. **Phase 6:** Update DHT announcements

---

## 8. Design Rationale

### Why L2 at Protocol Level?

1. **Complete Provenance:** Without L2, the provenance chain has a gap. Entity resolution work is invisible.

2. **Fair Compensation:** Building high-quality entity graphs requires significant effort (manual curation, ML models, external KB integration). This work deserves compensation.

3. **Reusability:** A well-built entity graph is valuable to many consumers. Making it a first-class content type enables this.

4. **Interoperability:** Protocol-level standardization ensures L2 graphs from different nodes are compatible.

### Why L0/L1 Remain the Roots?

The economic model preserves **foundational value**:

- L0/L1 represent irreducible source material
- L2/L3 are transformations that add value but depend on foundations
- Synthesis fees (5%) compensate L2/L3 creators for their work
- Root pool (95%) ensures original contributors are always paid

This prevents value extraction where intermediaries capture all revenue without compensating sources.

### L2 Implementation Flexibility

The spec defines structures but not algorithms:

- Entity extraction: Rule-based, NLP, or ML
- Entity resolution: String matching, embedding similarity, or external KB
- Relationship extraction: Dependency parsing, pattern matching, or LLM

Implementers choose appropriate methods for their use case.
