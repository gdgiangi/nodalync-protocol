# Nodalync Protocol Specification

**Version:** 0.2.0-draft  
**Author:** Gabriel Giangi  
**Date:** January 2026  
**Status:** Draft

---

## Table of Contents

1. [Overview](#1-overview)
2. [Notation and Conventions](#2-notation-and-conventions)
3. [Cryptographic Primitives](#3-cryptographic-primitives)
4. [Data Structures](#4-data-structures)
5. [Node State](#5-node-state)
6. [Message Types](#6-message-types)
7. [Protocol Operations](#7-protocol-operations)
8. [State Transitions](#8-state-transitions)
9. [Validation Rules](#9-validation-rules)
10. [Economic Rules](#10-economic-rules)
11. [Network Layer](#11-network-layer)
12. [Settlement Layer](#12-settlement-layer)
13. [Security Considerations](#13-security-considerations)

---

## 1. Overview

### 1.1 Purpose

The Nodalync Protocol enables decentralized knowledge exchange with cryptographic provenance and automatic compensation. Participants publish knowledge (L0), extract atomic facts (L1), build entity graphs (L2), and synthesize insights (L3). Every query triggers payment distributed through the complete provenance chain to foundational contributors.

### 1.2 Design Principles

1. **Local-first**: All content stored on owner's node
2. **Decentralized**: No central authority required
3. **Trustless**: Cryptographic verification, not social trust
4. **Fair**: 95% of value flows to foundational contributors
5. **Minimal**: Protocol specifies only what's necessary

### 1.3 Protocol Layers

```
┌─────────────────────────────────────────┐
│          Application Layer              │  (Out of scope)
├─────────────────────────────────────────┤
│          Protocol Layer                 │  ← This specification
│  ┌─────────────────────────────────┐   │
│  │  Content    Provenance  Payment │   │
│  └─────────────────────────────────┘   │
├─────────────────────────────────────────┤
│          Network Layer (libp2p)         │  (Referenced)
├─────────────────────────────────────────┤
│          Settlement Layer (Hedera)      │  (Referenced)
└─────────────────────────────────────────┘
```

---

## 2. Notation and Conventions

### 2.1 Data Types

```
uint8       Unsigned 8-bit integer
uint32      Unsigned 32-bit integer (big-endian)
uint64      Unsigned 64-bit integer (big-endian)
int64       Signed 64-bit integer (big-endian)
float64     IEEE 754 double-precision float
bytes       Variable-length byte array
string      UTF-8 encoded string
bool        Boolean (0x00 = false, 0x01 = true)
Hash        32 bytes (SHA-256 output)
Signature   64 bytes (Ed25519 signature)
PublicKey   32 bytes (Ed25519 public key)
PeerId      Derived from PublicKey (see 3.2)
Timestamp   uint64 (milliseconds since Unix epoch)
Amount      uint64 (smallest token unit, 10^-8 NDL)
```

### 2.2 Encoding

All multi-byte integers are big-endian. Structures are serialized using a deterministic CBOR encoding (RFC 8949) with the following rules:

1. Map keys sorted lexicographically
2. No indefinite-length arrays or maps
3. Minimal integer encoding
4. No floating-point for amounts (use uint64)

### 2.3 Notation

```
||          Concatenation
H(x)        SHA-256 hash of x
Sign(k, m)  Ed25519 signature of message m with private key k
Verify(p, m, s)  Verify signature s of message m with public key p
len(x)      Length of x in bytes
```

---

## 3. Cryptographic Primitives

### 3.1 Hash Function

**Algorithm:** SHA-256

Content hashes are computed as:

```
ContentHash(content) = H(
    0x00 ||                    # Domain separator (content)
    len(content) as uint64 ||
    content
)
```

### 3.2 Identity

**Algorithm:** Ed25519

Node identity is an Ed25519 keypair. PeerId is derived as:

```
PeerId = H(
    0x00 ||                    # Key type: Ed25519
    public_key                 # 32 bytes
)[0:20]                        # Truncate to 20 bytes
```

Human-readable format: `ndl1` + base32(PeerId)

Example: `ndl1qpzry9x8gf2tvdw0s3jn54khce6mua7l`

### 3.3 Signatures

All protocol messages requiring authentication are signed:

```
SignedMessage = {
    payload: bytes,
    signer: PeerId,
    signature: Sign(private_key, H(payload))
}
```

Verification:
```
Valid(msg) = Verify(
    lookup_public_key(msg.signer),
    H(msg.payload),
    msg.signature
)
```

### 3.4 Content Addressing

Content is referenced by its hash. The hash serves as a unique, verifiable identifier.

```
Given content C:
    hash = ContentHash(C)
    
Anyone receiving C can verify:
    ContentHash(C) == claimed_hash
```

---

## 4. Data Structures

### 4.1 Content Types

```
enum ContentType : uint8 {
    L0 = 0x00,      # Raw input (documents, notes, transcripts)
    L1 = 0x01,      # Mentions (extracted atomic facts)
    L2 = 0x02,      # Entity Graph (linked entities and relationships)
    L3 = 0x03       # Insights (emergent synthesis)
}
```

**Knowledge Layer Semantics:**

| Layer | Content | Typical Operation | Queryable | Value Added |
|-------|---------|-------------------|-----------|-------------|
| L0 | Raw documents, notes, transcripts | CREATE | Yes | Original source material |
| L1 | Atomic facts extracted from L0 | EXTRACT_L1 | Yes | Structured, quotable claims |
| L2 | Entities and relationships across L1s | BUILD_L2 | **No** (personal) | Cross-document linking, your perspective |
| L3 | Novel insights synthesizing sources | DERIVE | Yes | Original analysis and conclusions |

**L2 is Personal:** Your L2 represents your unique perspective — how you link entities, 
resolve ambiguities, and structure knowledge. It is never shared or queried directly. 
Its value surfaces when you create L3 insights that others find valuable enough to query.

### 4.2 Visibility

```
enum Visibility : uint8 {
    Private   = 0x00,   # Local only, not served
    Unlisted  = 0x01,   # Served if hash known, not announced
    Shared    = 0x02    # Announced to DHT, publicly queryable
}
```

### 4.3 Version

```
struct Version {
    number: uint32,         # Sequential version number (1-indexed)
    previous: Hash?,        # Hash of previous version (null if first)
    root: Hash,             # Hash of first version (stable identifier)
    timestamp: Timestamp    # Creation time
}

Constraints:
    - If number == 1: previous MUST be null, root MUST equal self hash
    - If number > 1: previous MUST NOT be null, root MUST equal previous.root
```

### 4.4 Mention (L1)

```
struct Mention {
    id: Hash,                       # H(content || source_location)
    content: string,                # The atomic fact (max 1000 chars)
    source_location: SourceLocation,
    classification: Classification,
    confidence: Confidence,
    entities: string[]              # Extracted entity names
}

struct SourceLocation {
    type: LocationType,
    reference: string,              # Location identifier
    quote: string?                  # Exact quote (max 500 chars)
}

enum LocationType : uint8 {
    Paragraph = 0x00,
    Page      = 0x01,
    Timestamp = 0x02,
    Line      = 0x03,
    Section   = 0x04
}

enum Classification : uint8 {
    Claim       = 0x00,
    Statistic   = 0x01,
    Definition  = 0x02,
    Observation = 0x03,
    Method      = 0x04,
    Result      = 0x05
}

enum Confidence : uint8 {
    Explicit = 0x00,    # Directly stated in source
    Inferred = 0x01     # Reasonably inferred
}
```

### 4.4a Entity Graph (L2)

L2 Entity Graphs are **personal knowledge structures**. They represent a node's 
interpretation and linking of entities across their queried L1 sources. L2 is 
never directly queried by others — its value surfaces when used to create L3 insights.

```
struct L2EntityGraph {
    # === Core Identity ===
    id: Hash,                           # H(serialized entities + relationships)
    
    # === Sources ===
    source_l1s: L1Reference[],          # L1 summaries this graph was built from
    source_l2s: Hash[],                 # Other L2 graphs merged/extended (optional)
    
    # === Namespace Prefixes (for compact URIs) ===
    prefixes: PrefixMap,                # Maps short prefixes to full URIs
    
    # === Graph Content ===
    entities: Entity[],                 # Resolved entities
    relationships: Relationship[],      # Relationships between entities
    
    # === Statistics ===
    entity_count: uint32,
    relationship_count: uint32,
    source_mention_count: uint32        # Total mentions linked
}

struct PrefixMap {
    entries: PrefixEntry[]              # Ordered list of prefix mappings
}

struct PrefixEntry {
    prefix: string,                     # Short form, e.g., "schema"
    uri: string                         # Full URI, e.g., "http://schema.org/"
}

# Default prefixes (always available, can be overridden):
#   "ndl"    -> "https://nodalync.io/ontology/"
#   "schema" -> "http://schema.org/"
#   "foaf"   -> "http://xmlns.com/foaf/0.1/"
#   "dc"     -> "http://purl.org/dc/elements/1.1/"
#   "rdf"    -> "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
#   "rdfs"   -> "http://www.w3.org/2000/01/rdf-schema#"
#   "xsd"    -> "http://www.w3.org/2001/XMLSchema#"
#   "owl"    -> "http://www.w3.org/2002/07/owl#"

struct L1Reference {
    l1_hash: Hash,                      # Hash of the L1Summary content
    l0_hash: Hash,                      # The original L0 this L1 came from
    mention_ids_used: Hash[]            # Which specific mentions were used
}

struct Entity {
    id: Hash,                           # Stable entity ID: H(canonical_uri || canonical_label)
    
    # === Identity ===
    canonical_label: string,            # Primary human-readable name (max 200 chars)
    canonical_uri: Uri?,                # Optional: canonical URI (e.g., "dbr:Albert_Einstein")
    aliases: string[],                  # Alternative names/spellings (max 50)
    
    # === Type (RDF-compatible) ===
    entity_types: Uri[],                # e.g., ["schema:Person", "foaf:Person"]
    
    # === Evidence ===
    source_mentions: MentionRef[],      # Which L1 mentions establish this entity
    
    # === Confidence ===
    confidence: float64,                # 0.0 - 1.0, resolution confidence
    resolution_method: ResolutionMethod,
    
    # === Optional Metadata ===
    description: string?,               # Summary description (max 500 chars)
    same_as: Uri[]?                     # Links to external entities (owl:sameAs)
}

# Uri can be:
#   - Full URI: "http://schema.org/Person"
#   - Compact URI (CURIE): "schema:Person" (expanded using prefixes)
#   - Protocol-defined: "ndl:Person" (Nodalync ontology)
type Uri = string

struct MentionRef {
    l1_hash: Hash,                      # Which L1 contains this mention
    mention_id: Hash                    # Specific mention ID within that L1
}

struct Relationship {
    id: Hash,                           # H(subject || predicate || object)
    
    # === Triple ===
    subject: Hash,                      # Entity ID
    predicate: Uri,                     # RDF predicate, e.g., "schema:worksFor"
    object: RelationshipObject,         # Entity ID or literal
    
    # === Evidence ===
    source_mentions: MentionRef[],      # Mentions that support this relationship
    confidence: float64,                # 0.0 - 1.0
    
    # === Temporal (optional) ===
    valid_from: Timestamp?,
    valid_to: Timestamp?
}

enum RelationshipObject {
    EntityRef(Hash),                    # Reference to another entity in this graph
    ExternalRef(Uri),                   # Reference to external entity
    Literal(LiteralValue)               # A typed value
}

struct LiteralValue {
    value: string,                      # The value as string
    datatype: Uri?,                     # XSD datatype, e.g., "xsd:date" (null = plain string)
    language: string?                   # Language tag, e.g., "en" (for strings only)
}

# Standard XSD datatypes (use "xsd:" prefix):
#   xsd:string, xsd:integer, xsd:decimal, xsd:boolean,
#   xsd:date, xsd:dateTime, xsd:anyURI

enum ResolutionMethod : uint8 {
    ExactMatch    = 0x00,               # Same string
    Normalized    = 0x01,               # Case/punctuation normalized
    Alias         = 0x02,               # Known alias matched
    Coreference   = 0x03,               # Pronoun/reference resolved
    ExternalLink  = 0x04,               # Matched via external KB
    Manual        = 0x05,               # Human-verified
    AIAssisted    = 0x06                # ML model assisted
}

Constraints:
    1. len(source_l1s) >= 1              # Must derive from at least one L1
    2. len(entities) >= 1                 # Must have at least one entity
    3. Each entity.id is unique within the graph
    4. Each relationship references valid entity IDs (or external URIs)
    5. All MentionRefs point to valid L1s in source_l1s
    6. 0.0 <= confidence <= 1.0
    7. len(canonical_label) <= 200
    8. len(aliases) <= 50
    9. All URIs are valid (full URI or valid CURIE with known prefix)
    10. entity_count == len(entities)
    11. relationship_count == len(relationships)
    
L2 Visibility:
    - L2 content is ALWAYS Private (never Unlisted or Shared)
    - L2 is never announced to DHT
    - L2 has no price (cannot be queried for payment)
    - L2's value is realized through L3 insights derived from it
```

### 4.4b Nodalync Ontology (ndl:)

The protocol defines a minimal ontology at `https://nodalync.io/ontology/`:

```
# Entity Types
ndl:Person
ndl:Organization  
ndl:Location
ndl:Concept
ndl:Event
ndl:Work              # Paper, book, article
ndl:Product
ndl:Technology
ndl:Metric            # Quantitative measure
ndl:TimePoint

# Relationship Predicates
ndl:mentions          # L1 mention references entity
ndl:relatedTo         # Generic relationship
ndl:partOf
ndl:createdBy
ndl:locatedIn
ndl:occurredAt
ndl:hasValue
ndl:sameAs            # Equivalent to owl:sameAs

# Provenance Predicates
ndl:derivedFrom       # Content derivation
ndl:extractedFrom     # L1 extracted from L0
ndl:builtFrom         # L2 built from L1s
```

Nodes are free to use any ontology. The `ndl:` namespace provides defaults
for nodes that don't need external ontology integration.

### 4.5 Provenance

```
struct Provenance {
    root_L0L1: ProvenanceEntry[],   # All foundational sources
    derived_from: Hash[],            # Direct parent hashes
    depth: uint32                    # Max derivation depth from any L0
}

struct ProvenanceEntry {
    hash: Hash,                 # Content hash
    owner: PeerId,              # Owner's node ID
    visibility: Visibility,     # Visibility at time of derivation
    weight: uint32              # Number of times this source appears (for duplicates)
}

Constraints:
    - root_L0L1 contains entries of type L0 or L1 only (never L2 or L3)
    - L0 content: root_L0L1 = [self], derived_from = [], depth = 0
    - L1 content: root_L0L1 = [parent L0], derived_from = [L0 hash], depth = 1
    - L2 content: root_L0L1 = merged roots from source L1s, 
                  derived_from = source L1/L2 hashes, depth = max(source.depth) + 1
    - L3 content: root_L0L1 = merged roots from all sources,
                  derived_from = source hashes, depth = max(source.depth) + 1
    - All entries in derived_from MUST have been queried by creator
    
Provenance Chain Examples:
    Simple chain:
        L0(doc) → L1(mentions) → L2(entities) → L3(insight)
        depth:  0       1            2              3
    
    L3 deriving directly from L1 (valid, skipping L2):
        L0(doc) → L1(mentions) → L3(insight)
        depth:  0       1            2
    
    L3 from mix of L1 and L2:
        L0(doc1) → L1(m1) → L2(graph) ─┐
                                        ├→ L3(insight)
        L0(doc2) → L1(m2) ─────────────┘
        
        L3.provenance = {
            root_L0L1: [doc1, doc2],  # Merged from both paths
            derived_from: [L2.hash, m2],
            depth: 4  # max(3, 2) + 1
        }
```

### 4.6 Access Control

```
struct AccessControl {
    allowlist: PeerId[]?,       # If set, only these peers can query
    denylist: PeerId[]?,        # These peers are blocked
    require_bond: bool,         # Require payment bond
    bond_amount: Amount?,       # Bond amount if required
    max_queries_per_peer: uint32?   # Rate limit (null = unlimited)
}

Access granted if:
    (allowlist is null OR peer in allowlist) AND
    (denylist is null OR peer NOT in denylist) AND
    (require_bond is false OR peer has posted bond)
```

### 4.7 Economics

```
struct Economics {
    price: Amount,              # Price per query (in smallest unit)
    currency: Currency,         # Currency identifier
    total_queries: uint64,      # Total queries served
    total_revenue: Amount       # Total revenue generated
}

enum Currency : uint8 {
    NDL = 0x00                  # Native Nodalync token
}
```

### 4.8 Manifest

The manifest is the complete metadata for a content item:

```
struct Manifest {
    # Identity
    hash: Hash,                 # Content hash
    content_type: ContentType,
    owner: PeerId,              # Content owner (serves content, receives synthesis fee)
    
    # Versioning
    version: Version,
    
    # Visibility & Access
    visibility: Visibility,
    access: AccessControl,
    
    # Metadata
    metadata: Metadata,
    
    # Economics
    economics: Economics,
    
    # Provenance
    provenance: Provenance,
    
    # Timestamps
    created_at: Timestamp,
    updated_at: Timestamp
}

struct Metadata {
    title: string,              # Max 200 chars
    description: string?,       # Max 2000 chars
    tags: string[],             # Max 20 tags, each max 50 chars
    content_size: uint64,       # Size in bytes
    mime_type: string?          # MIME type if applicable
}
```

### 4.9 L1 Summary (Preview)

```
struct L1Summary {
    l0_hash: Hash,              # Source L0 hash
    mention_count: uint32,      # Total mentions extracted
    preview_mentions: Mention[], # First N mentions (max 5)
    primary_topics: string[],   # Main topics (max 5)
    summary: string             # 2-3 sentence summary (max 500 chars)
}
```

---

## 5. Node State

### 5.1 State Components

A node maintains the following state:

```
struct NodeState {
    # Identity
    identity: Identity,
    
    # Content storage
    content: Map<Hash, ContentRecord>,
    
    # Provenance graph
    provenance_graph: ProvenanceGraph,
    
    # Payment channels
    channels: Map<PeerId, Channel>,
    
    # Peer information
    peers: Map<PeerId, PeerInfo>,
    
    # Query cache (content from others)
    cache: Map<Hash, CachedContent>,
    
    # Settlement queue
    settlement_queue: SettlementEntry[]
}

struct Identity {
    private_key: bytes,         # Ed25519 private key (encrypted at rest)
    public_key: PublicKey,
    peer_id: PeerId
}

struct ContentRecord {
    manifest: Manifest,
    content: bytes,             # Encrypted at rest
    l1_data: L1Summary?,        # Null if L1 not extracted
    local_path: string          # Filesystem path
}

struct PeerInfo {
    peer_id: PeerId,
    public_key: PublicKey,
    addresses: MultiAddr[],     # libp2p multiaddresses
    last_seen: Timestamp,
    reputation: int64           # Reputation score
}

struct CachedContent {
    hash: Hash,
    content: bytes,
    source_peer: PeerId,
    queried_at: Timestamp,
    payment_proof: PaymentProof
}
```

### 5.2 Provenance Graph

```
struct ProvenanceGraph {
    # Forward edges: what does this content derive from?
    derived_from: Map<Hash, Hash[]>,
    
    # Backward edges: what derives from this content?
    derivations: Map<Hash, Hash[]>,
    
    # Flattened roots cache
    roots_cache: Map<Hash, ProvenanceEntry[]>
}

Operations:
    add_content(hash, derived_from[]) → updates both directions
    get_roots(hash) → returns flattened root_L0L1
    get_derivations(hash) → returns all downstream content
```

### 5.3 Payment Channels

```
struct Channel {
    peer_id: PeerId,
    state: ChannelState,
    my_balance: Amount,
    their_balance: Amount,
    nonce: uint64,
    last_update: Timestamp,
    pending_payments: Payment[]
}

enum ChannelState : uint8 {
    Opening   = 0x00,
    Open      = 0x01,
    Closing   = 0x02,
    Closed    = 0x03,
    Disputed  = 0x04
}

struct Payment {
    id: Hash,                   # H(channel_id || nonce || amount || recipient)
    amount: Amount,
    recipient: PeerId,
    query_hash: Hash,           # Content that was queried
    provenance: ProvenanceEntry[], # For distribution
    timestamp: Timestamp,
    signature: Signature        # Signed by payer
}
```

---

## 6. Message Types

### 6.1 Message Envelope

All protocol messages use a common envelope:

```
struct Message {
    version: uint8,             # Protocol version (0x01)
    type: MessageType,
    id: Hash,                   # Unique message ID
    timestamp: Timestamp,
    sender: PeerId,
    payload: bytes,             # Type-specific payload
    signature: Signature        # Signs H(version || type || id || timestamp || sender || payload)
}

enum MessageType : uint16 {
    # Discovery (0x01xx)
    ANNOUNCE         = 0x0100,
    ANNOUNCE_UPDATE  = 0x0101,
    SEARCH           = 0x0110,
    SEARCH_RESPONSE  = 0x0111,
    
    # Preview (0x02xx)
    PREVIEW_REQUEST  = 0x0200,
    PREVIEW_RESPONSE = 0x0201,
    
    # Query (0x03xx)
    QUERY_REQUEST    = 0x0300,
    QUERY_RESPONSE   = 0x0301,
    QUERY_ERROR      = 0x0302,
    
    # Version (0x04xx)
    VERSION_REQUEST  = 0x0400,
    VERSION_RESPONSE = 0x0401,
    
    # Channel (0x05xx)
    CHANNEL_OPEN     = 0x0500,
    CHANNEL_ACCEPT   = 0x0501,
    CHANNEL_UPDATE   = 0x0502,
    CHANNEL_CLOSE    = 0x0503,
    CHANNEL_DISPUTE  = 0x0504,
    
    # Settlement (0x06xx)
    SETTLE_BATCH     = 0x0600,
    SETTLE_CONFIRM   = 0x0601,
    
    # Peer (0x07xx)
    PING             = 0x0700,
    PONG             = 0x0701,
    PEER_INFO        = 0x0710
}
```

### 6.2 Discovery Messages

```
# ANNOUNCE - Publish content availability to DHT
struct AnnouncePayload {
    hash: Hash,
    content_type: ContentType,
    title: string,
    l1_summary: L1Summary,
    price: Amount,
    addresses: MultiAddr[]
}

# ANNOUNCE_UPDATE - Announce new version
struct AnnounceUpdatePayload {
    version_root: Hash,         # Stable identifier
    new_hash: Hash,             # New version hash
    version_number: uint32,
    title: string,
    l1_summary: L1Summary,
    price: Amount
}

# SEARCH - Query DHT for content
struct SearchPayload {
    query: string,              # Natural language query
    filters: SearchFilters?,
    limit: uint32,              # Max results (1-100)
    offset: uint32              # For pagination
}

struct SearchFilters {
    content_types: ContentType[]?,
    max_price: Amount?,
    min_reputation: int64?,
    created_after: Timestamp?,
    created_before: Timestamp?,
    tags: string[]?
}

# SEARCH_RESPONSE - Search results
struct SearchResponsePayload {
    results: SearchResult[],
    total_count: uint64,
    offset: uint32
}

struct SearchResult {
    hash: Hash,
    content_type: ContentType,
    title: string,
    owner: PeerId,
    l1_summary: L1Summary,
    price: Amount,
    total_queries: uint64,
    relevance_score: float64    # 0.0 - 1.0
}
```

### 6.3 Preview Messages

```
# PREVIEW_REQUEST - Request L1 preview (free)
struct PreviewRequestPayload {
    hash: Hash
}

# PREVIEW_RESPONSE - Return L1 preview
struct PreviewResponsePayload {
    hash: Hash,
    manifest: Manifest,         # Full manifest (no content)
    l1_summary: L1Summary
}
```

### 6.4 Query Messages

```
# QUERY_REQUEST - Request content (paid)
struct QueryRequestPayload {
    hash: Hash,
    query: string?,             # Optional: specific question about content
    payment: Payment,
    version: VersionSpec?       # Optional: specific version
}

enum VersionSpec : uint8 {
    Latest = 0x00,
    Number = 0x01,              # Followed by uint32 version number
    Hash   = 0x02               # Followed by Hash
}

# QUERY_RESPONSE - Return content
struct QueryResponsePayload {
    hash: Hash,
    content: bytes,
    manifest: Manifest,           # Contains full provenance chain
    payment_receipt: PaymentReceipt
}

# Whitepaper simplified response fields map to:
#   response.content    → content
#   response.sources[]  → manifest.provenance.root_L0L1[].hash
#   response.provenance → manifest.provenance
#   response.cost       → payment_receipt.amount

struct PaymentReceipt {
    payment_id: Hash,
    amount: Amount,
    timestamp: Timestamp,
    channel_nonce: uint64,
    distributor_signature: Signature    # Owner signs receipt
}

# QUERY_ERROR - Error response
struct QueryErrorPayload {
    hash: Hash,
    error_code: QueryError,
    message: string?
}

enum QueryError : uint16 {
    NOT_FOUND        = 0x0001,
    ACCESS_DENIED    = 0x0002,
    PAYMENT_REQUIRED = 0x0003,
    PAYMENT_INVALID  = 0x0004,
    RATE_LIMITED     = 0x0005,
    VERSION_NOT_FOUND= 0x0006,
    INTERNAL_ERROR   = 0xFFFF
}
```

### 6.5 Version Messages

```
# VERSION_REQUEST - Get version info
struct VersionRequestPayload {
    version_root: Hash          # Stable identifier
}

# VERSION_RESPONSE - Version history
struct VersionResponsePayload {
    version_root: Hash,
    versions: VersionInfo[],
    latest: Hash
}

struct VersionInfo {
    hash: Hash,
    number: uint32,
    timestamp: Timestamp,
    visibility: Visibility,
    price: Amount
}
```

### 6.6 Channel Messages

```
# CHANNEL_OPEN - Request to open payment channel
struct ChannelOpenPayload {
    channel_id: Hash,           # H(initiator || responder || nonce)
    initial_balance: Amount,    # Initiator's deposit
    funding_tx: bytes?          # On-chain funding proof (if required)
}

# CHANNEL_ACCEPT - Accept channel opening
struct ChannelAcceptPayload {
    channel_id: Hash,
    initial_balance: Amount,    # Responder's deposit
    funding_tx: bytes?
}

# CHANNEL_UPDATE - Update channel state (payment)
struct ChannelUpdatePayload {
    channel_id: Hash,
    nonce: uint64,
    balances: ChannelBalances,
    payments: Payment[],        # Payments in this update
    signature: Signature        # Signs the new state
}

struct ChannelBalances {
    initiator: Amount,
    responder: Amount
}

# CHANNEL_CLOSE - Initiate cooperative close
struct ChannelClosePayload {
    channel_id: Hash,
    final_balances: ChannelBalances,
    settlement_tx: bytes        # Proposed on-chain settlement
}

# CHANNEL_DISPUTE - Dispute channel state
struct ChannelDisputePayload {
    channel_id: Hash,
    claimed_state: ChannelUpdatePayload,    # Highest known state
    evidence: bytes[]           # Supporting evidence
}
```

### 6.7 Settlement Messages

```
# SETTLE_BATCH - Batch settlement request
struct SettleBatchPayload {
    batch_id: Hash,
    entries: SettlementEntry[],
    merkle_root: Hash           # Root of entries merkle tree
}

struct SettlementEntry {
    recipient: PeerId,
    amount: Amount,
    provenance_hashes: Hash[],  # Content hashes for audit
    payment_ids: Hash[]         # Payment IDs included
}

# SETTLE_CONFIRM - Confirm settlement on-chain
struct SettleConfirmPayload {
    batch_id: Hash,
    transaction_id: string,     # On-chain transaction ID
    block_number: uint64,
    timestamp: Timestamp
}
```

### 6.8 Peer Messages

```
# PING
struct PingPayload {
    nonce: uint64
}

# PONG
struct PongPayload {
    nonce: uint64               # Echo back
}

# PEER_INFO - Exchange peer information
struct PeerInfoPayload {
    peer_id: PeerId,
    public_key: PublicKey,
    addresses: MultiAddr[],
    capabilities: Capability[],
    content_count: uint64,
    uptime: uint64              # Seconds since node start
}

enum Capability : uint8 {
    QUERY    = 0x01,            # Can serve queries
    CHANNEL  = 0x02,            # Supports payment channels
    SETTLE   = 0x04,            # Can initiate settlement
    INDEX    = 0x08             # Participates in DHT indexing
}
```

---

## 7. Protocol Operations

### 7.1 Content Operations

#### 7.1.1 Create

Create new content locally (not yet published).

```
CREATE(content: bytes, content_type: ContentType, metadata: Metadata) → Hash

Procedure:
    1. hash = ContentHash(content)
    2. version = Version {
           number: 1,
           previous: null,
           root: hash,
           timestamp: now()
       }
    3. provenance = compute_provenance(content_type, sources=[])
    4. manifest = Manifest {
           hash: hash,
           content_type: content_type,
           version: version,
           visibility: Private,
           access: default_access(),
           metadata: metadata,
           economics: Economics { price: 0, currency: NDL, ... },
           provenance: provenance,
           created_at: now(),
           updated_at: now()
       }
    5. Store content and manifest locally
    6. Return hash
```

#### 7.1.2 Extract L1

Extract mentions from L0 content.

```
EXTRACT_L1(hash: Hash) → L1Summary

Preconditions:
    - Content exists locally
    - content_type == L0
    
Procedure:
    1. content = load_content(hash)
    2. mentions = extract_mentions(content)  # AI or rule-based
    3. summary = L1Summary {
           l0_hash: hash,
           mention_count: len(mentions),
           preview_mentions: mentions[0:5],
           primary_topics: extract_topics(mentions),
           summary: generate_summary(content)
       }
    4. Store L1 data with content record
    5. Return summary
```

#### 7.1.2a Build L2 (Entity Graph)

Build an L2 Entity Graph from one or more L1 sources. L2 is your personal 
knowledge structure — it is never published or queried by others.

```
BUILD_L2(source_l1s: Hash[], config: L2BuildConfig?) → Hash

Preconditions:
    - All source L1s have been queried (payment proof exists) OR are your own
    - len(source_l1s) >= 1

Procedure:
    1. Verify all L1 sources are accessible:
       For each l1_hash in source_l1s:
           assert cache.has(l1_hash) OR content.has(l1_hash)
           l1 = load_l1(l1_hash)
           assert l1.content_type == L1
           
    2. Extract entities from mentions:
       raw_entities = []
       For each l1 in source_l1s:
           For each mention in l1.mentions:
               extracted = extract_entities(mention, config.prefixes)
               raw_entities.extend(extracted)
               
    3. Resolve entities (merge duplicates):
       resolved_entities = resolve_entities(raw_entities, config)
       # Handles: exact match, alias resolution, coreference, external KB linking
       # Assigns URIs from configured ontologies
       
    4. Extract relationships:
       relationships = extract_relationships(resolved_entities, source_l1s, config)
       # Uses predicates from configured ontologies (default: ndl:)
       
    5. Build L2 structure:
       l2_graph = L2EntityGraph {
           id: computed after serialization,
           source_l1s: [L1Reference for each l1],
           source_l2s: [],
           prefixes: config.prefixes ?? default_prefixes(),
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
           visibility: Private,           # L2 is ALWAYS private
           economics: Economics { price: 0, ... },  # L2 has no price
           provenance: provenance,
           ...
       }
       
    9. Store content and manifest locally
    10. Return hash

struct L2BuildConfig {
    # Ontology configuration
    prefixes: PrefixMap?,                # Custom prefix mappings
    default_entity_type: Uri?,           # Default: "ndl:Concept"
    
    # Entity resolution settings
    resolution_threshold: float64?,      # Minimum confidence to merge (default: 0.8)
    use_external_kb: bool?,              # Link to external knowledge bases
    external_kb_list: Uri[]?,            # Which KBs: ["http://www.wikidata.org/", ...]
    
    # Relationship extraction
    extract_implicit: bool?,             # Infer implicit relationships
    relationship_predicates: Uri[]?      # Limit to specific predicates
}
```

#### 7.1.2b Merge L2

Combine multiple of your own L2 Entity Graphs into a unified graph. This is useful
when you have built separate graphs for different domains and want to unify them.

```
MERGE_L2(source_l2s: Hash[], config: L2MergeConfig?) → Hash

Preconditions:
    - All source L2s are your own (stored locally)
    - len(source_l2s) >= 2

Procedure:
    1. Load all L2 sources (must be local, L2 is never queried)
    2. Unify prefix mappings (merge, detect conflicts)
    3. Collect all entities and relationships from sources
    4. Cross-graph entity resolution (find same entities in different graphs)
       # Match by: canonical_uri, same_as links, label similarity
    5. Merge relationships (update entity references to merged IDs)
    6. Build new L2 with:
       source_l1s: union of all source L1 references
       source_l2s: the input source_l2s
       prefixes: merged prefix map
    7. Compute provenance:
       root_entries = merge roots from all source_l2s
       provenance = Provenance {
           root_L0L1: root_entries,
           derived_from: source_l2s,
           depth: max(l2.provenance.depth for l2 in source_l2s) + 1
       }
    8. Create manifest with visibility = Private
    9. Store and return hash

struct L2MergeConfig {
    prefixes: PrefixMap?,                # Override prefix mappings
    entity_merge_threshold: float64?,    # Confidence threshold for merging entities
    prefer_source: uint32?               # Index of source to prefer on conflicts
}
```

#### 7.1.3 Publish

Make content available on the network. Note: L2 content cannot be published.

```
PUBLISH(hash: Hash, visibility: Visibility, price: Amount, access: AccessControl?) → bool

Preconditions:
    - Content exists locally
    - content_type != L2  # L2 is always private
    - visibility != Private OR no-op
    
Procedure:
    1. manifest = load_manifest(hash)
    2. If manifest.content_type == L2:
           Return error("L2 content cannot be published")
    3. manifest.visibility = visibility
    4. manifest.economics.price = price
    5. manifest.access = access ?? default_access()
    6. manifest.updated_at = now()
    7. Save manifest
    
    8. If visibility == Shared:
           l1_summary = get_or_extract_l1(hash)
           announce = AnnouncePayload {
               hash: hash,
               content_type: manifest.content_type,
               title: manifest.metadata.title,
               l1_summary: l1_summary,
               price: price,
               addresses: my_addresses()
           }
           DHT.announce(hash, announce)
           
    9. Return true
```

#### 7.1.4 Update

Create a new version of existing content.

```
UPDATE(old_hash: Hash, new_content: bytes) → Hash

Preconditions:
    - Old content exists locally
    - Caller owns old content
    
Procedure:
    1. old_manifest = load_manifest(old_hash)
    2. new_hash = ContentHash(new_content)
    3. new_version = Version {
           number: old_manifest.version.number + 1,
           previous: old_hash,
           root: old_manifest.version.root,
           timestamp: now()
       }
    4. new_manifest = copy(old_manifest)
       new_manifest.hash = new_hash
       new_manifest.version = new_version
       new_manifest.updated_at = now()
    5. Store new content and manifest
    
    6. If old_manifest.visibility == Shared:
           update_announce = AnnounceUpdatePayload {
               version_root: new_manifest.version.root,
               new_hash: new_hash,
               version_number: new_version.number,
               ...
           }
           DHT.announce_update(new_manifest.version.root, update_announce)
           
    7. Return new_hash
```

#### 7.1.5 Derive (Create L3)

Create an L3 insight from multiple sources.

```
DERIVE(sources: Hash[], insight_content: bytes, metadata: Metadata) → Hash

Sources may include any combination of:
    - L0 content (raw documents)
    - L1 content (mention collections)
    - L2 content (entity graphs)
    - L3 content (other insights)

Preconditions:
    - All sources have been queried (payment proof exists)
    - At least one source
    
Procedure:
    1. Verify all sources were queried:
       For each source in sources:
           assert cache.has(source) OR content.has(source)
           
    2. Compute provenance:
       root_entries = []
       For each source in sources:
           source_prov = get_provenance(source)
           For each entry in source_prov.root_L0L1:
               merge_or_increment(root_entries, entry)
       
       # Note: For L0/L1 sources, merge their root_L0L1 directly
       #       For L2 sources, merge the L2's root_L0L1 (traces back to L0/L1)
       #       For L3 sources, merge the L3's root_L0L1 (recursive)
           
       provenance = Provenance {
           root_L0L1: root_entries,
           derived_from: sources,
           depth: max(source.provenance.depth for source in sources) + 1
       }
       
    3. hash = ContentHash(insight_content)
    4. Create manifest with content_type = L3, provenance
    5. Store locally
    6. Return hash

Helper merge_or_increment(entries, new_entry):
    existing = find(entries, e => e.hash == new_entry.hash)
    If existing:
        existing.weight += new_entry.weight
    Else:
        entries.append(new_entry with weight=1)
```

#### 7.1.6 Reference L3 as L0 (Import)

Reference an external L3 as foundational input for your own derivations.

```
REFERENCE_L3_AS_L0(source_l3_hash: Hash) → Reference

Preconditions:
    - L3 has been queried at least once (payment proof exists)
    - Source content_type == L3
    
Procedure:
    1. Verify L3 was queried:
           assert cache.has(source_l3_hash)
           source_manifest = cache[source_l3_hash].manifest
           assert source_manifest.content_type == L3
           
    2. Create reference in local graph:
           reference = Reference {
               hash: source_l3_hash,
               owner: source_manifest.owner,
               treat_as: L0,  # Treat this L3 as foundational for derivations
               imported_at: now()
           }
           
    3. Store reference locally
    4. Return reference

IMPORTANT: This is a reference operation, not data transfer. The actual 
content remains on the original owner's node. "Import" means treating an 
external L3 as foundational input (L0) in your own derivation chains.

When deriving from this reference:
    - The reference is included in derived_from[]
    - The L3's root_L0L1 is merged into the new content's root_L0L1
    - The L3 itself is added to root_L0L1 (the creator becomes a root)
    - Each query to your derived content triggers payments to:
      - You (5% synthesis fee)
      - The L3 creator (as a root contributor)
      - All upstream contributors in the L3's provenance chain
```

### 7.2 Query Operations

#### 7.2.1 Discover

Search for content on the network.

```
DISCOVER(query: string, filters: SearchFilters?) → SearchResult[]

Procedure:
    1. search_payload = SearchPayload {
           query: query,
           filters: filters,
           limit: 50,
           offset: 0
       }
    2. results = DHT.search(search_payload)
    3. Return results sorted by relevance_score
```

#### 7.2.2 Preview

Get L1 preview for content (free).

```
PREVIEW(peer: PeerId, hash: Hash) → (Manifest, L1Summary)

Procedure:
    1. Send PREVIEW_REQUEST { hash } to peer
    2. Await PREVIEW_RESPONSE
    3. Verify response.hash == hash
    4. Return (response.manifest, response.l1_summary)

Handler (receiving node):
    1. manifest = load_manifest(request.hash)
    2. If manifest is null:
           Return QUERY_ERROR { NOT_FOUND }
    3. If manifest.visibility == Private:
           Return QUERY_ERROR { NOT_FOUND }  # Don't reveal existence
    4. If manifest.visibility == Unlisted:
           If not check_access(sender, manifest.access):
               Return QUERY_ERROR { ACCESS_DENIED }
    5. l1_summary = load_l1_summary(request.hash)
    6. Return PREVIEW_RESPONSE { hash, manifest, l1_summary }
```

#### 7.2.3 Query

Request content with payment.

```
QUERY(peer: PeerId, hash: Hash, query_text: string?) → (bytes, Manifest, PaymentReceipt)

Procedure:
    1. Ensure channel exists with peer:
           If not channels.has(peer):
               CHANNEL_OPEN(peer)
               
    2. Preview first to get price:
           (manifest, _) = PREVIEW(peer, hash)
           price = manifest.economics.price
           
    3. Create payment:
           payment = Payment {
               id: H(channel_id || nonce || price || peer),
               amount: price,
               recipient: peer,
               query_hash: hash,
               provenance: manifest.provenance.root_L0L1,
               timestamp: now(),
               signature: Sign(my_key, payment_data)
           }
           
    4. Send QUERY_REQUEST { hash, query_text, payment }
    5. Await QUERY_RESPONSE
    
    6. Verify response:
           assert ContentHash(response.content) == hash
           assert response.payment_receipt.amount == price
           
    7. Update channel state:
           channel.my_balance -= price
           channel.nonce += 1
           channel.pending_payments.append(payment)
           
    8. Cache content:
           cache[hash] = CachedContent {
               hash, content, peer, now(), response.payment_receipt
           }
           
    9. Return (response.content, response.manifest, response.payment_receipt)

Handler (receiving node):
    1. manifest = load_manifest(request.hash)
    2. Validate visibility and access (same as PREVIEW)
    
    3. Validate payment:
           assert request.payment.amount >= manifest.economics.price
           assert request.payment.recipient == my_peer_id
           assert Verify(sender_pubkey, payment_data, request.payment.signature)
           assert channel_has_balance(sender, request.payment.amount)
           
    4. Update channel state:
           channel.their_balance -= request.payment.amount
           channel.my_balance += (request.payment.amount * 0.05)  # Synthesis fee
           channel.nonce = max(channel.nonce, request.payment.nonce) + 1
           
    5. Queue distribution:
           For each entry in manifest.provenance.root_L0L1:
               share = (request.payment.amount * 0.95) / total_weight
               queue_settlement(entry.owner, share * entry.weight, hash)
               
    6. Update economics:
           manifest.economics.total_queries += 1
           manifest.economics.total_revenue += request.payment.amount
           
    7. content = load_content(request.hash)
    8. receipt = PaymentReceipt { ... }
    9. Return QUERY_RESPONSE { hash, content, manifest, receipt }
```

### 7.3 Channel Operations

#### 7.3.1 Open Channel

```
CHANNEL_OPEN(peer: PeerId, initial_balance: Amount) → Channel

Procedure:
    1. channel_id = H(my_peer_id || peer || random_nonce())
    2. Send CHANNEL_OPEN { channel_id, initial_balance, funding_tx }
    3. Await CHANNEL_ACCEPT
    4. channel = Channel {
           peer_id: peer,
           state: Open,
           my_balance: initial_balance,
           their_balance: response.initial_balance,
           nonce: 0,
           last_update: now(),
           pending_payments: []
       }
    5. channels[peer] = channel
    6. Return channel
```

#### 7.3.2 Close Channel

```
CHANNEL_CLOSE(peer: PeerId) → SettlementEntry[]

Procedure:
    1. channel = channels[peer]
    2. Assert channel.state == Open
    
    3. Create settlement entries from pending payments:
           entries = aggregate_payments(channel.pending_payments)
           
    4. Send CHANNEL_CLOSE { channel_id, final_balances, settlement_tx }
    5. Await acknowledgment or timeout
    
    6. If cooperative:
           Submit settlement to chain
           channel.state = Closed
       Else:
           Initiate dispute resolution
           
    7. Return entries
```

### 7.4 Settlement Operations

```
SETTLE_BATCH(entries: SettlementEntry[]) → TransactionId

Procedure:
    1. batch_id = H(entries || now())
    2. merkle_root = compute_merkle_root(entries)
    
    3. Build on-chain transaction:
           For each entry in entries:
               Add transfer: entry.recipient receives entry.amount
               
    4. Submit transaction to Hedera
    5. Await confirmation
    
    6. Broadcast SETTLE_CONFIRM { batch_id, tx_id, block, timestamp }
    7. Clear settled payments from channels
    
    8. Return tx_id
```

---

## 8. State Transitions

### 8.1 Content State Machine

```
                    ┌──────────────────────────────────────────┐
                    │                                          │
                    ▼                                          │
┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐  │
│ (none)  │────▶│ Private │────▶│Unlisted │────▶│ Shared  │──┘
└─────────┘     └─────────┘     └─────────┘     └─────────┘
    │               │               │               │
    │               │               │               │
    │  CREATE       │  PUBLISH      │  PUBLISH      │
    │               │  (unlisted)   │  (shared)     │
    │               │               │               │
    │               │◀──────────────│◀──────────────│
    │               │   UNPUBLISH   │   UNPUBLISH   │
    │               │               │               │
    │               │               │               │
    └───────────────┴───────────────┴───────────────┘
                            │
                            │ DELETE
                            ▼
                      ┌─────────┐
                      │ Deleted │
                      └─────────┘

Valid transitions:
    (none) → Private:    CREATE
    Private → Unlisted:  PUBLISH(visibility=Unlisted)
    Private → Shared:    PUBLISH(visibility=Shared)
    Unlisted → Shared:   PUBLISH(visibility=Shared)
    Unlisted → Private:  UNPUBLISH
    Shared → Unlisted:   UNPUBLISH(keep_unlisted=true)
    Shared → Private:    UNPUBLISH
    Any → Deleted:       DELETE (local only, provenance persists)
```

### 8.2 Channel State Machine

```
┌─────────┐     ┌─────────┐     ┌─────────┐
│ (none)  │────▶│ Opening │────▶│  Open   │
└─────────┘     └─────────┘     └─────────┘
                    │               │   │
                    │ timeout       │   │ UPDATE
                    │               │   └────┐
                    ▼               │        │
              ┌─────────┐          │        │
              │ Failed  │          │◀───────┘
              └─────────┘          │
                                   │
                    ┌──────────────┴──────────────┐
                    │                             │
                    ▼ cooperative                 ▼ unilateral/dispute
              ┌─────────┐                   ┌──────────┐
              │ Closing │                   │ Disputed │
              └─────────┘                   └──────────┘
                    │                             │
                    │ settled                     │ resolved
                    ▼                             ▼
              ┌─────────────────────────────────────┐
              │              Closed                 │
              └─────────────────────────────────────┘

Valid transitions:
    (none) → Opening:    CHANNEL_OPEN sent
    Opening → Open:      CHANNEL_ACCEPT received
    Opening → Failed:    Timeout or rejection
    Open → Open:         CHANNEL_UPDATE (payment)
    Open → Closing:      CHANNEL_CLOSE (cooperative)
    Open → Disputed:     CHANNEL_DISPUTE
    Closing → Closed:    Settlement confirmed
    Disputed → Closed:   Dispute resolved on-chain
```

### 8.3 Query State Machine (per request)

```
┌─────────┐     ┌─────────┐     ┌──────────┐     ┌─────────┐
│Initiate │────▶│ Preview │────▶│ Payment  │────▶│Complete │
└─────────┘     └─────────┘     └──────────┘     └─────────┘
                    │               │
                    │ error         │ error
                    ▼               ▼
              ┌───────────────────────┐
              │        Failed         │
              └───────────────────────┘

States:
    Initiate:   Query started
    Preview:    L1 preview received, evaluating
    Payment:    Payment sent, awaiting content
    Complete:   Content received and verified
    Failed:     Error at any stage
```

---

## 9. Validation Rules

### 9.1 Content Validation

```
VALIDATE_CONTENT(content: bytes, manifest: Manifest) → bool

Rules:
    1. ContentHash(content) == manifest.hash
    2. len(content) == manifest.metadata.content_size
    3. len(manifest.metadata.title) <= 200
    4. len(manifest.metadata.description) <= 2000
    5. len(manifest.metadata.tags) <= 20
    6. For each tag: len(tag) <= 50
    7. manifest.content_type in {L0, L1, L2, L3}
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

### 9.2 Version Validation

```
VALIDATE_VERSION(manifest: Manifest, previous: Manifest?) → bool

Rules:
    1. If manifest.version.number == 1:
           manifest.version.previous == null
           manifest.version.root == manifest.hash
           
    2. If manifest.version.number > 1:
           previous != null
           manifest.version.previous == previous.hash
           manifest.version.root == previous.version.root
           manifest.version.number == previous.version.number + 1
           manifest.version.timestamp > previous.version.timestamp
```

### 9.3 Provenance Validation

```
VALIDATE_PROVENANCE(manifest: Manifest, sources: Manifest[]) → bool

Rules:
    1. If manifest.content_type == L0:
           manifest.provenance.root_L0L1 == [self_entry]
           manifest.provenance.derived_from == []
           manifest.provenance.depth == 0
           
    2. If manifest.content_type == L1:
           len(manifest.provenance.root_L0L1) >= 1
           len(manifest.provenance.derived_from) == 1
           derived_from[0] is an L0 hash
           All root_L0L1 entries are type L0
           manifest.provenance.depth == 1
           
    3. If manifest.content_type == L2:
           len(manifest.provenance.root_L0L1) >= 1
           len(manifest.provenance.derived_from) >= 1
           All derived_from are L1 or L2 hashes
           All root_L0L1 entries are type L0 or L1
           manifest.provenance.depth >= 2
           
    4. If manifest.content_type == L3:
           len(manifest.provenance.root_L0L1) >= 1
           len(manifest.provenance.derived_from) >= 1
           All derived_from hashes exist in sources
           All root_L0L1 entries are type L0 or L1
           
    5. root_L0L1 computation is correct:
           computed = compute_root_L0L1(sources)
           manifest.provenance.root_L0L1 == computed
           
    6. Depth is correct:
           manifest.provenance.depth == max(s.provenance.depth for s in sources) + 1
           
    7. No self-reference:
           manifest.hash not in manifest.provenance.derived_from
           manifest.hash not in [e.hash for e in manifest.provenance.root_L0L1]
           
    8. No cycles in provenance graph
```

### 9.4 Payment Validation

```
VALIDATE_PAYMENT(payment: Payment, channel: Channel, manifest: Manifest) → bool

Rules:
    1. payment.amount >= manifest.economics.price
    2. payment.recipient == manifest_owner
    3. payment.query_hash == manifest.hash
    4. channel.state == Open
    5. channel.their_balance >= payment.amount  # Payer has funds
    6. payment.nonce > channel.nonce  # No replay
    7. Verify(payer_pubkey, payment_data, payment.signature)
    8. payment.provenance == manifest.provenance.root_L0L1
```

### 9.5 Message Validation

```
VALIDATE_MESSAGE(msg: Message) → bool

Rules:
    1. msg.version == PROTOCOL_VERSION
    2. msg.type is valid MessageType
    3. msg.timestamp within acceptable skew (±5 minutes)
    4. msg.sender is valid PeerId
    5. Verify(lookup_pubkey(msg.sender), H(msg without signature), msg.signature)
    6. msg.payload decodes correctly for msg.type
    7. Payload-specific validation passes
```

### 9.6 Access Validation

```
VALIDATE_ACCESS(requester: PeerId, manifest: Manifest) → bool

Rules:
    1. If manifest.visibility == Private:
           Return false  # No external access
           
    2. If manifest.visibility == Unlisted:
           If manifest.access.allowlist != null:
               requester in manifest.access.allowlist
           If manifest.access.denylist != null:
               requester not in manifest.access.denylist
               
    3. If manifest.visibility == Shared:
           If manifest.access.denylist != null:
               requester not in manifest.access.denylist
           # Allowlist ignored for Shared (open to all)
           
    4. If manifest.access.require_bond:
           has_bond(requester, manifest.access.bond_amount)
```

---

## 10. Economic Rules

### 10.1 Revenue Distribution

```
DISTRIBUTE_REVENUE(payment: Payment) → Distribution[]

Constants:
    SYNTHESIS_FEE = 0.05  # 5%
    ROOT_POOL = 0.95      # 95%

Procedure:
    1. total = payment.amount
    2. owner_share = total * SYNTHESIS_FEE
    3. root_pool = total * ROOT_POOL
    
    4. total_weight = sum(e.weight for e in payment.provenance)
    5. per_weight = root_pool / total_weight
    
    6. distributions = []
    7. For each entry in payment.provenance:
           amount = per_weight * entry.weight
           
           # Owner also gets share if they have roots
           If entry.owner == content_owner:
               owner_share += amount
           Else:
               distributions.append(Distribution {
                   recipient: entry.owner,
                   amount: amount,
                   source_hash: entry.hash
               })
               
    8. distributions.append(Distribution {
           recipient: content_owner,
           amount: owner_share,
           source_hash: payment.query_hash
       })
       
    9. Return distributions
```

### 10.2 Distribution Example

```
Scenario:
    Bob's L3 derives from:
        - Alice's L0 (2 documents)
        - Carol's L0 (1 document)
        - Bob's L0 (2 documents)
    
    Query payment: 100 NDL

Provenance:
    root_L0L1 = [
        { hash: alice_1, owner: Alice, weight: 1 },
        { hash: alice_2, owner: Alice, weight: 1 },
        { hash: carol_1, owner: Carol, weight: 1 },
        { hash: bob_1, owner: Bob, weight: 1 },
        { hash: bob_2, owner: Bob, weight: 1 }
    ]
    total_weight = 5

Distribution:
    owner_share = 100 * 0.05 = 5 NDL (Bob's synthesis fee)
    root_pool = 100 * 0.95 = 95 NDL
    per_weight = 95 / 5 = 19 NDL

    Alice: 2 * 19 = 38 NDL
    Carol: 1 * 19 = 19 NDL
    Bob (roots): 2 * 19 = 38 NDL
    Bob (synthesis): 5 NDL
    Bob total: 43 NDL (5 + 38)
    
Final:
    Alice: 38 NDL (38%)
    Carol: 19 NDL (19%)
    Bob: 43 NDL (43%)
```

### 10.3 Price Setting

```
Constraints:
    MIN_PRICE = 1  # 1 smallest unit (10^-8 NDL)
    MAX_PRICE = 10^16  # Practical maximum
    
Rules:
    1. price >= MIN_PRICE
    2. price <= MAX_PRICE
    3. price is uint64 (no floating point)
    4. Owner can change price at any time (takes effect immediately)
```

### 10.4 Settlement Batching

```
BATCH_THRESHOLD = 100 NDL  # Minimum to trigger auto-settlement
BATCH_INTERVAL = 3600      # Maximum seconds between settlements

Rules:
    1. Settlement triggered when:
           sum(pending_payments) >= BATCH_THRESHOLD
           OR time_since_last_settlement >= BATCH_INTERVAL
           OR channel_closing
           
    2. Batch includes all pending payments across all channels
    3. Payments aggregated by recipient (one entry per recipient)
    4. Merkle root allows any recipient to verify inclusion
```

---

## 11. Network Layer

### 11.1 Transport

The protocol uses libp2p for peer-to-peer communication:

```
Transports:
    - TCP (primary)
    - QUIC (preferred when available)
    - WebSocket (browser compatibility)
    
Multiplexing:
    - yamux
    - mplex (fallback)
    
Security:
    - Noise protocol (XX handshake pattern)
    - TLS 1.3 (fallback)
```

### 11.2 Discovery

```
DHT: Kademlia
    - Key space: 256-bit (SHA-256)
    - Bucket size: 20
    - Alpha (parallelism): 3
    - Replication factor: 20

Content records stored at:
    key = H(content_hash)
    value = AnnouncePayload (signed)
    
Version updates stored at:
    key = H("version:" || version_root)
    value = AnnounceUpdatePayload (signed)
    
Search index:
    - Local inverted index per node
    - Gossip-based index synchronization
    - Semantic embeddings for similarity search
```

### 11.3 Peer Discovery

```
Bootstrap nodes:
    - Hardcoded list of well-known nodes
    - DNS-based discovery (TXT records)
    
Peer exchange:
    - Nodes share peer lists periodically
    - Prefer peers with high uptime and low latency
    
NAT traversal:
    - STUN for address discovery
    - Relay nodes for symmetric NAT
    - Hole punching via DCUtR
```

### 11.4 Message Routing

```
Direct messages:
    - Point-to-point when peer is known
    - DHT lookup to find peer addresses
    
Broadcast messages:
    - GossipSub for protocol announcements
    - Topic: /nodalync/announce/1.0.0
    
Request-response:
    - Dedicated protocol streams
    - Timeout: 30 seconds default
    - Retry: 3 attempts with exponential backoff
```

---

## 12. Settlement Layer

### 12.1 Chain Selection

Primary: Hedera Hashgraph

Rationale:
    - Fast finality (3-5 seconds)
    - Low cost (~$0.0001/tx)
    - High throughput (10,000+ TPS)
    - Suitable for micropayment batching

### 12.2 On-Chain Data

```
Settlement Contract State:
    balances: Map<AccountId, Amount>        # Token balances
    channels: Map<ChannelId, ChannelState>  # Channel states
    attestations: Map<Hash, Attestation>    # Content attestations

struct Attestation {
    content_hash: Hash,
    owner: AccountId,
    timestamp: Timestamp,
    provenance_root: Hash  # Merkle root of root_L0L1
}

struct ChannelState {
    participants: [AccountId, AccountId],
    balances: [Amount, Amount],
    nonce: uint64,
    status: ChannelStatus
}
```

### 12.3 Contract Operations

```
// Deposit tokens to protocol
deposit(amount: Amount)
    Requires: sender has sufficient tokens
    Effects: balances[sender] += amount

// Withdraw tokens from protocol
withdraw(amount: Amount)
    Requires: balances[sender] >= amount
    Effects: balances[sender] -= amount, transfer to sender

// Attest content publication
attest(content_hash: Hash, provenance_root: Hash)
    Requires: caller is content owner
    Effects: attestations[content_hash] = Attestation { ... }

// Open payment channel
openChannel(peer: AccountId, myDeposit: Amount, peerDeposit: Amount)
    Requires: both parties sign, sufficient balances
    Effects: Create channel, lock deposits

// Update channel state (cooperative)
updateChannel(channelId: ChannelId, newState: ChannelState, signatures: [Sig, Sig])
    Requires: Both signatures valid, nonce > current nonce
    Effects: Update channel state

// Close channel (cooperative)
closeChannel(channelId: ChannelId, finalState: ChannelState, signatures: [Sig, Sig])
    Requires: Both signatures valid
    Effects: Distribute balances, delete channel

// Dispute channel (unilateral)
disputeChannel(channelId: ChannelId, claimedState: ChannelState, signature: Sig)
    Requires: Valid signature from one party
    Effects: Start dispute period (24 hours)

// Resolve dispute
resolveDispute(channelId: ChannelId)
    Requires: Dispute period elapsed
    Effects: Apply highest-nonce state, close channel

// Batch settlement
settleBatch(entries: SettlementEntry[], merkleProofs: MerkleProof[])
    Requires: Valid merkle proofs, sufficient channel balances
    Effects: Transfer amounts to recipients
```

### 12.4 Token Economics

```
Token: NDL (Nodalync Token)
    Decimals: 8
    Total supply: Fixed at genesis (TBD)
    
Initial distribution:
    - Protocol development: X%
    - Early contributors: Y%
    - Network incentives: Z%
    - Reserve: W%
    
No inflation. Fees are redistributed, not burned.
```

---

## 13. Security Considerations

### 13.1 Threat Model

```
Assumptions:
    - Network is asynchronous and unreliable
    - Adversaries can delay or drop messages
    - Adversaries can create unlimited identities (Sybil)
    - Adversaries cannot break cryptographic primitives
    - Majority of economic stake is honest

Threats addressed:
    1. Content theft (copying after query)
    2. Payment fraud (fake payments, double-spending)
    3. Provenance manipulation (false attribution)
    4. Eclipse attacks (isolating nodes)
    5. Denial of service
    
Threats NOT addressed (out of scope):
    1. Content quality/accuracy
    2. Legal disputes over IP
    3. Privacy of query patterns
    4. Nation-state level attacks
```

### 13.2 Mitigations

```
Content theft:
    - Mitigation: Audit trail, timestamps, legal recourse
    - Note: Cannot prevent, only detect and prove
    
Payment fraud:
    - Mitigation: Cryptographic signatures, channel states
    - Settlement disputes resolve on-chain with evidence
    
Provenance manipulation:
    - Mitigation: Content-addressed hashing
    - Cannot claim derivation without querying (payment proof)
    
Eclipse attacks:
    - Mitigation: Multiple bootstrap nodes, peer diversity requirements
    - Monitor for unusual peer behavior
    
Denial of service:
    - Mitigation: Rate limiting, require payment bonds
    - Reputation system penalizes bad actors
```

### 13.3 Key Management

```
Private key storage:
    - Encrypted at rest (AES-256-GCM)
    - Key derived from user password (Argon2id)
    - Optional hardware security module support
    
Key rotation:
    - Supported via identity update message
    - Old key signs authorization for new key
    - Grace period for transition
    
Recovery:
    - Optional mnemonic backup (BIP-39)
    - Social recovery (threshold signatures) - future
```

### 13.4 Privacy Considerations

```
Visible to network:
    - Content hashes (not content)
    - L1 previews (for shared content)
    - Provenance chains
    - Payment amounts (in settlement batches)
    
Hidden from network:
    - Private content (entirely local)
    - Query text (between querier and node)
    - Unlisted content (unless you have hash)
    
Future improvements:
    - ZK proofs for provenance verification
    - Private settlement channels
    - Onion routing for query privacy
```

---

## Appendix A: Wire Formats

### A.1 Message Encoding

All messages use deterministic CBOR encoding:

```
Message wire format:
    [0x00]                  # Protocol magic byte
    [version: uint8]        # Protocol version
    [type: uint16]          # Message type
    [length: uint32]        # Payload length
    [payload: bytes]        # CBOR-encoded payload
    [signature: 64 bytes]   # Ed25519 signature
```

### A.2 Hash Computation

```
ContentHash:
    H(
        [0x00]              # Domain separator for content
        [length: uint64]    # Content length
        [content: bytes]    # Raw content
    )

MessageHash (for signing):
    H(
        [0x01]              # Domain separator for messages
        [version: uint8]
        [type: uint16]
        [id: 32 bytes]
        [timestamp: uint64]
        [sender: 20 bytes]
        [payload_hash: 32 bytes]  # H(payload)
    )

ChannelStateHash:
    H(
        [0x02]              # Domain separator for channels
        [channel_id: 32 bytes]
        [nonce: uint64]
        [initiator_balance: uint64]
        [responder_balance: uint64]
    )
```

---

## Appendix B: Constants

```
PROTOCOL_VERSION = 0x01
PROTOCOL_MAGIC = 0x00

# Timing
MESSAGE_TIMEOUT_MS = 30000
CHANNEL_DISPUTE_PERIOD_MS = 86400000  # 24 hours
MAX_CLOCK_SKEW_MS = 300000  # 5 minutes

# Limits
MAX_CONTENT_SIZE = 104857600  # 100 MB
MAX_MESSAGE_SIZE = 10485760   # 10 MB
MAX_MENTIONS_PER_L0 = 1000
MAX_SOURCES_PER_L3 = 100
MAX_PROVENANCE_DEPTH = 100
MAX_TAGS = 20
MAX_TAG_LENGTH = 50
MAX_TITLE_LENGTH = 200
MAX_DESCRIPTION_LENGTH = 2000

# L2 Entity Graph limits
MAX_ENTITIES_PER_L2 = 10000
MAX_RELATIONSHIPS_PER_L2 = 50000
MAX_ALIASES_PER_ENTITY = 50
MAX_CANONICAL_LABEL_LENGTH = 200
MAX_PREDICATE_LENGTH = 100
MAX_ENTITY_DESCRIPTION_LENGTH = 500
MAX_SOURCE_L1S_PER_L2 = 100
MAX_SOURCE_L2S_PER_MERGE = 20

# Economics
MIN_PRICE = 1  # Smallest unit
SYNTHESIS_FEE_NUMERATOR = 5
SYNTHESIS_FEE_DENOMINATOR = 100  # 5%
SETTLEMENT_BATCH_THRESHOLD = 10000000000  # 100 NDL (10^8 units)
SETTLEMENT_BATCH_INTERVAL_MS = 3600000  # 1 hour

# DHT
DHT_BUCKET_SIZE = 20
DHT_ALPHA = 3
DHT_REPLICATION = 20
```

---

## Appendix C: Error Codes

```
# Query Errors (0x0001 - 0x00FF)
NOT_FOUND        = 0x0001  # Content does not exist
ACCESS_DENIED    = 0x0002  # Not authorized
PAYMENT_REQUIRED = 0x0003  # No payment provided
PAYMENT_INVALID  = 0x0004  # Payment validation failed
RATE_LIMITED     = 0x0005  # Too many requests
VERSION_NOT_FOUND= 0x0006  # Specific version not found

# Channel Errors (0x0100 - 0x01FF)
CHANNEL_NOT_FOUND    = 0x0100
CHANNEL_CLOSED       = 0x0101
INSUFFICIENT_BALANCE = 0x0102
INVALID_NONCE        = 0x0103
INVALID_SIGNATURE    = 0x0104

# Validation Errors (0x0200 - 0x02FF)
INVALID_HASH        = 0x0200
INVALID_PROVENANCE  = 0x0201
INVALID_VERSION     = 0x0202
INVALID_MANIFEST    = 0x0203
CONTENT_TOO_LARGE   = 0x0204

# L2 Entity Graph Errors (0x0210 - 0x021F)
L2_INVALID_STRUCTURE    = 0x0210  # Malformed L2EntityGraph
L2_MISSING_SOURCE       = 0x0211  # Source L1 not found
L2_ENTITY_LIMIT         = 0x0212  # Too many entities
L2_RELATIONSHIP_LIMIT   = 0x0213  # Too many relationships
L2_INVALID_ENTITY_REF   = 0x0214  # Relationship references invalid entity
L2_CYCLE_DETECTED       = 0x0215  # Circular entity reference
L2_INVALID_URI          = 0x0216  # Invalid URI or CURIE format
L2_CANNOT_PUBLISH       = 0x0217  # L2 content cannot be published

# Network Errors (0x0300 - 0x03FF)
PEER_NOT_FOUND      = 0x0300
CONNECTION_FAILED   = 0x0301
TIMEOUT             = 0x0302

# Internal Errors (0xFF00 - 0xFFFF)
INTERNAL_ERROR      = 0xFFFF
```

---

## Appendix D: Reference Implementation Notes

The reference implementation SHOULD:

1. Use Rust for memory safety and performance
2. Use libp2p-rs for networking
3. Use SQLite for local storage
4. Use RocksDB for high-performance caching
5. Provide both CLI and library interfaces
6. Support WASM compilation for browser nodes (future)

Directory structure:
```
nodalync/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Library root
│   ├── main.rs          # CLI entry point
│   ├── types/           # Data structures
│   ├── crypto/          # Cryptographic operations
│   ├── storage/         # Local storage
│   ├── network/         # P2P networking
│   ├── protocol/        # Protocol operations
│   ├── channels/        # Payment channels
│   └── settlement/      # Chain settlement
├── tests/
└── docs/
```

---

*End of Protocol Specification*

**Version History:**
- 0.2.0-draft (January 2026): Added L2 Entity Graph as protocol-level content type
- 0.1.0-draft (January 2025): Initial draft
