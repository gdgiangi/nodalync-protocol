# L2 Entity Graph — Implementation Planning

## Summary

L2 (Entity Graph) has been elevated from internal-only to a protocol-level content type in spec v0.2.0. Key design decisions:

1. **L2 is personal/private** — never queried by others, always `visibility = Private`
2. **URI-based ontology** — entity types and predicates are URIs, not closed enums
3. **L3 derives from L2** — your insights are built on your knowledge graph
4. **No L2 monetization** — L2 creators earn through their L3 insights

---

## Spec Changes Made

### Data Structures (§4)

| Section | Change |
|---------|--------|
| §4.1 ContentType | Added `L2 = 0x02`, noted L2 is personal |
| §4.4a L2EntityGraph | Full entity graph with URI support, PrefixMap |
| §4.4b Nodalync Ontology | Defined `ndl:` namespace for defaults |
| §4.5 Provenance | Updated constraints for L2 in chain |

### Key Design Changes from Initial Draft

| Aspect | Initial | Final |
|--------|---------|-------|
| L2 Queryable | Yes | **No** (always private) |
| L2 Price | Set by owner | **Always 0** |
| L2Summary/Preview | Defined | **Removed** (not needed) |
| Entity Types | Closed enum | **URIs** (open, RDF-compatible) |
| Predicates | Plain strings | **URIs** (ontology-based) |
| MERGE_L2 Sources | Queried L2s | **Own L2s only** |

### New Structures Defined

```
L2EntityGraph
├── id: Hash
├── source_l1s: L1Reference[]
├── source_l2s: Hash[]
├── prefixes: PrefixMap              # URI prefix mappings
├── entities: Entity[]
├── relationships: Relationship[]
└── statistics (counts)

Entity
├── id: Hash
├── canonical_label: string
├── canonical_uri: Uri?              # e.g., "dbr:Albert_Einstein"
├── aliases: string[]
├── entity_types: Uri[]              # e.g., ["schema:Person", "foaf:Person"]
├── source_mentions: MentionRef[]
├── confidence: float64
├── resolution_method: ResolutionMethod
├── description: string?
└── same_as: Uri[]?                  # owl:sameAs links

Relationship
├── id: Hash
├── subject: Hash (Entity ID)
├── predicate: Uri                   # e.g., "schema:worksFor"
├── object: EntityRef | ExternalRef | Literal
├── source_mentions: MentionRef[]
├── confidence: float64
└── temporal bounds (optional)

PrefixMap
└── entries: [{ prefix: "schema", uri: "http://schema.org/" }, ...]
```

### Removed from Initial Draft

- `L2Summary` — not needed since L2 is never previewed
- `EntityType` enum — replaced with `entity_types: Uri[]`
- `LiteralType` enum — replaced with `datatype: Uri` (XSD types)
- `L2_PREVIEW_REQUEST/RESPONSE` messages — L2 is never queried

### New URI Type

```rust
// Uri can be:
//   - Full: "http://schema.org/Person"
//   - Compact (CURIE): "schema:Person" (expanded using prefixes)
//   - Protocol-defined: "ndl:Person"
type Uri = String;
```

### Operations (§7)

| Operation | Status |
|-----------|--------|
| §7.1.2a BUILD_L2 | NEW |
| §7.1.2b MERGE_L2 | NEW |
| §7.1.5 DERIVE | Updated to accept L2 sources |

### Validation (§9)

| Section | Change |
|---------|--------|
| §9.1 Content | Added L2 to valid types, L2-specific validation |
| §9.3 Provenance | Added L2 provenance rules |

### Constants (Appendix B)

```
MAX_ENTITIES_PER_L2 = 10000
MAX_RELATIONSHIPS_PER_L2 = 50000
MAX_ALIASES_PER_ENTITY = 50
MAX_CANONICAL_LABEL_LENGTH = 200
MAX_PREDICATE_LENGTH = 100
MAX_ENTITY_DESCRIPTION_LENGTH = 500
MAX_SOURCE_L1S_PER_L2 = 100
MAX_SOURCE_L2S_PER_MERGE = 20
```

### Error Codes (Appendix C)

```
L2_INVALID_STRUCTURE    = 0x0210
L2_MISSING_SOURCE       = 0x0211
L2_ENTITY_LIMIT         = 0x0212
L2_RELATIONSHIP_LIMIT   = 0x0213
L2_INVALID_ENTITY_REF   = 0x0214
L2_CYCLE_DETECTED       = 0x0215
```

---

## Provenance Chain (Complete)

```
L0 (raw) ─────> L1 (mentions) ─────> L2 (entities) ─────> L3 (insight)
 depth=0         depth=1              depth=2              depth=3
 queryable       queryable            PRIVATE              queryable
 
 root_L0L1=[self]  root_L0L1=[L0]    root_L0L1=[L0s]     root_L0L1=[L0s]
 derived=[]        derived=[L0]       derived=[L1s]       derived=[L2]
```

**Economic Model:**

- L0/L1 creators are ALWAYS paid (they're in root_L0L1)
- L2 creators earn **nothing directly** (L2 is personal, never queried)
- L3 creators earn 5% synthesis fee when their L3 is queried
- L2's value is realized through the L3 insights built on top of it

**Example flow:**
```
Alice creates L0 (document)
Bob creates L1 from Alice's L0
You query Bob's L1 → Bob gets 5%, Alice gets 95%
You build L2 from Bob's L1 (your personal graph)
You create L3 from your L2
Eve queries your L3 → You get 5%, Alice gets 95%

Note: You never get paid for your L2 work directly.
The value of your L2 is captured in your L3's quality.
```

---

## Hedera Impact

**No smart contract changes needed:**

1. L2 hashes never appear in settlement (L2 is private, never queried)
2. Settlement contract only sees L3 queries
3. L3's provenance.root_L0L1 contains only L0/L1 entries (traced through L2)
4. Distribution formula unchanged (95% to roots, 5% synthesis)

**The L2 layer is invisible to Hedera** — it's local infrastructure that enables better L3 insights.

---

## URI/RDF Integration Notes

### Default Prefixes (built into protocol)

```
ndl:    https://nodalync.io/ontology/
schema: http://schema.org/
foaf:   http://xmlns.com/foaf/0.1/
dc:     http://purl.org/dc/elements/1.1/
rdf:    http://www.w3.org/1999/02/22-rdf-syntax-ns#
rdfs:   http://www.w3.org/2000/01/rdf-schema#
xsd:    http://www.w3.org/2001/XMLSchema#
owl:    http://www.w3.org/2002/07/owl#
```

### Nodalync Ontology (ndl:)

```turtle
# Entity Types
ndl:Person        rdfs:subClassOf schema:Person .
ndl:Organization  rdfs:subClassOf schema:Organization .
ndl:Location      rdfs:subClassOf schema:Place .
ndl:Concept       rdfs:subClassOf skos:Concept .
ndl:Event         rdfs:subClassOf schema:Event .
ndl:Work          rdfs:subClassOf schema:CreativeWork .

# Predicates
ndl:mentions      rdfs:domain ndl:Mention ; rdfs:range ndl:Entity .
ndl:derivedFrom   rdfs:comment "Content provenance relationship" .
ndl:extractedFrom rdfs:comment "L1 extracted from L0" .
ndl:builtFrom     rdfs:comment "L2 built from L1s" .
```

### CURIE Expansion

```rust
fn expand_curie(curie: &str, prefixes: &PrefixMap) -> Result<String> {
    if let Some((prefix, local)) = curie.split_once(':') {
        if let Some(base) = prefixes.get(prefix) {
            return Ok(format!("{}{}", base, local));
        }
        // Unknown prefix - could be full URI or error
    }
    // No colon - treat as full URI
    Ok(curie.to_string())
}
```

---

## Module Updates Required

### Priority Order

1. **nodalync-types** — Add L2 structures with URI support
2. **nodalync-valid** — Add L2 validation, URI validation, CURIE expansion
3. **nodalync-store** — L2 storage (similar to other content, always private)
4. **nodalync-ops** — Add BUILD_L2, MERGE_L2 operations; update PUBLISH to reject L2
5. **nodalync-wire** — No changes needed (no L2 messages)
6. **nodalync-settle** — No changes needed (L2 invisible to settlement)

### Detailed Changes by Module

#### nodalync-types

```rust
// Add to ContentType enum
pub enum ContentType {
    L0 = 0x00,
    L1 = 0x01,
    L2 = 0x02,  // NEW - always private
    L3 = 0x03,
}

// NEW: URI type (just a string, but semantically meaningful)
pub type Uri = String;

// NEW structures
pub struct PrefixMap {
    pub entries: Vec<PrefixEntry>,
}

pub struct PrefixEntry {
    pub prefix: String,  // e.g., "schema"
    pub uri: String,     // e.g., "http://schema.org/"
}

pub struct L2EntityGraph {
    pub id: Hash,
    pub source_l1s: Vec<L1Reference>,
    pub source_l2s: Vec<Hash>,
    pub prefixes: PrefixMap,
    pub entities: Vec<Entity>,
    pub relationships: Vec<Relationship>,
    pub entity_count: u32,
    pub relationship_count: u32,
    pub source_mention_count: u32,
}

pub struct Entity {
    pub id: Hash,
    pub canonical_label: String,
    pub canonical_uri: Option<Uri>,
    pub aliases: Vec<String>,
    pub entity_types: Vec<Uri>,           // URI-based, not enum
    pub source_mentions: Vec<MentionRef>,
    pub confidence: f64,
    pub resolution_method: ResolutionMethod,
    pub description: Option<String>,
    pub same_as: Option<Vec<Uri>>,
}

pub struct Relationship {
    pub id: Hash,
    pub subject: Hash,
    pub predicate: Uri,                    // URI-based predicate
    pub object: RelationshipObject,
    pub source_mentions: Vec<MentionRef>,
    pub confidence: f64,
    pub valid_from: Option<Timestamp>,
    pub valid_to: Option<Timestamp>,
}

pub enum RelationshipObject {
    EntityRef(Hash),
    ExternalRef(Uri),                      // NEW: external entity reference
    Literal(LiteralValue),
}

pub struct LiteralValue {
    pub value: String,
    pub datatype: Option<Uri>,             // XSD type URI
    pub language: Option<String>,
}

// Keep ResolutionMethod as enum (it's protocol-internal)
pub enum ResolutionMethod { ... }

pub struct L2BuildConfig { ... }
pub struct L2MergeConfig { ... }
```

#### nodalync-wire

```rust
// NO L2 preview messages needed (L2 is never queried)
// Message types unchanged from v0.1.0
```

#### nodalync-valid

```rust
// Add L2 validation
pub fn validate_l2_graph(graph: &L2EntityGraph) -> Result<()>;
pub fn validate_l2_provenance(manifest: &Manifest, sources: &[Manifest]) -> Result<()>;
pub fn validate_uri(uri: &Uri, prefixes: &PrefixMap) -> Result<()>;
pub fn expand_curie(curie: &str, prefixes: &PrefixMap) -> Result<String>;

// Update PUBLISH validation to reject L2
pub fn validate_publish(manifest: &Manifest, visibility: Visibility) -> Result<()> {
    if manifest.content_type == ContentType::L2 {
        return Err(Error::L2CannotBePublished);
    }
    // ...
}
```

#### nodalync-ops

```rust
// Add operations
pub fn build_l2(source_l1s: &[Hash], config: Option<L2BuildConfig>) -> Result<Hash>;
pub fn merge_l2(source_l2s: &[Hash], config: Option<L2MergeConfig>) -> Result<Hash>;

// Update DERIVE to accept L2 sources (already works, just document)
// Update PUBLISH to reject L2

// NO l2_preview needed
```

---

## Testing Strategy

### Unit Tests

1. **L2 Structure Tests**
   - Entity ID computation
   - Relationship ID computation  
   - L2EntityGraph serialization roundtrip
   - Constraint validation (limits, uniqueness)

2. **L2 Provenance Tests**
   - BUILD_L2 from single L1 → correct depth, roots
   - BUILD_L2 from multiple L1s → merged roots
   - MERGE_L2 from multiple L2s → correct depth
   - DERIVE from L2 → roots trace back to L0/L1

3. **L2 Validation Tests**
   - Valid L2 passes
   - Invalid entity references rejected
   - Duplicate entity IDs rejected
   - Confidence out of range rejected

### Integration Tests

1. **Full Chain Test**
   ```
   CREATE L0 → EXTRACT_L1 → BUILD_L2 → DERIVE L3 → QUERY
   Verify: All root_L0L1 entries paid correctly
   ```

2. **Cross-L2 Merge Test**
   ```
   L0_A → L1_A → L2_A ─┐
                       ├→ MERGE_L2 → L3
   L0_B → L1_B → L2_B ─┘
   Verify: roots = [L0_A, L0_B]
   ```

---

## Hedera Impact

**Minimal changes needed:**

1. L2 hashes are just hashes — settlement contract doesn't care about content type
2. Provenance root_L0L1 already tracks L0/L1 sources
3. Distribution formula unchanged (95% to roots, 5% synthesis)

**No smart contract changes required** — the existing settlement logic handles L2 automatically because:
- L2's provenance.root_L0L1 contains L0/L1 entries
- Settlement distributes to root_L0L1 entries
- L2 creator gets synthesis fee when L2 is queried

---

## Files to Update

### Spec (DONE)
- [x] /home/claude/nodalync-impl/docs/spec.md (v0.2.0)

### Module Docs (DONE)
- [x] /home/claude/nodalync-impl/docs/modules/02-types.md — L2 structures, Uri type, PrefixMap
- [x] /home/claude/nodalync-impl/docs/modules/05-valid.md — L2 validation, URI/CURIE validation
- [x] /home/claude/nodalync-impl/docs/modules/07-ops.md — BUILD_L2, MERGE_L2, PUBLISH rejection
- [x] /home/claude/nodalync-impl/docs/modules/03-wire.md — No changes needed (no L2 messages)

### Implementation (TODO)
- [ ] nodalync-types crate — L2 structures, Uri, PrefixMap
- [ ] nodalync-valid crate — L2 validation
- [ ] nodalync-ops crate — BUILD_L2, MERGE_L2
- [x] nodalync-wire crate — No changes needed
- [x] nodalync-settle crate — No changes needed (L2 invisible)

### Checklist (TODO)
- [ ] CHECKLIST.md — Add L2 items

### Checklist (TODO)
- [ ] CHECKLIST.md — Add L2 items

---

## Questions for Review

1. **Entity ID stability:** Current design uses `H(canonical_uri || canonical_label)`. 
   - If user corrects a typo in canonical_label, ID changes
   - Alternative: Use canonical_uri only when present, random UUID otherwise?

2. **Cross-L2 entity linking:** When MERGE_L2 finds same entity:
   - Match by canonical_uri first (most reliable)
   - Then by same_as links
   - Then by label similarity
   - On conflict: prefer higher confidence or manual resolution?

3. **Ontology validation:** Should URIs be validated?
   - MVP: No validation, trust the creator
   - Future: Optional validation against published ontologies

4. **L2 export:** Should there be a way to export L2 as RDF/Turtle/JSON-LD?
   - Not part of protocol, but useful for interop
   - Could be CLI feature: `nodalync export l2 <hash> --format turtle`

5. **Collaborative L2:** The current design is strictly personal. Future consideration:
   - Could L2 be shared with specific collaborators (Unlisted)?
   - Would need careful thought about economic implications

