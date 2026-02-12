# Nodalync Studio - Desktop App Specification

> **Note:** Nodalync Studio is a planned desktop application. It has not been built yet.
> This document is a design specification for future development. The current way
> to use Nodalync is via the CLI (`nodalync-cli`) or the MCP server (`nodalync-mcp`).

## Status: Draft v4 - Final

---

## 1. Executive Summary

**Product**: Nodalync Studio - a cross-platform Tauri desktop app for knowledge producers.

**Vision**: A "second brain" application where users add knowledge (journals, notes, research, insights) as L0 sources, which automatically extract L1 mentions and build a living L2 entity graph that evolves with every new piece of content.

**Design Philosophy**: Network-up development - core protocol integration first, polished UI second.

---

## 2. Design Decisions Summary

### Core Technology
| Decision | Choice |
|----------|--------|
| App Name | **Nodalync Studio** |
| Frontend | **React** + **shadcn/ui** (Radix + Tailwind) |
| Backend | **Tauri 2.0** (embedded Rust) |
| Graph Viz | **D3 force simulation** on **Canvas** (not SVG - DOM can't handle 1K+ nodes at 60fps) |
| State Management | **Zustand** |
| Database | **SQLite** via rusqlite (existing nodalync-store pattern) |

### AI Integration
| Decision | Choice |
|----------|--------|
| Processing Model | **Single-pass async** (Sonnet-tier) with progress events |
| L1 Extraction | **Rule-based NLP** (fallback), **AI-enhanced** (if key configured) |
| L2 Graph Building | **AI-powered** (required) |
| Providers | **OpenAI + Anthropic** (user configures one) |
| Entity Resolution | **AI-assisted auto-merge** (confidence > 0.9 auto-applied, others queued for review) |
| Ontology | **Fixed predicates** (worksFor, locatedIn, mentions, relatedTo, etc.) |
| API Key | **Required** for L2, **optional** for L1, stored locally encrypted |

### Node Runtime
| Decision | Choice |
|----------|--------|
| Lifecycle | **Embedded in app process** |
| Window Close | **Minimize to system tray** (node keeps running) |
| Startup | **Auto-connect to bootstrap** on launch |
| NAT Traversal | **Relay-only** through bootstrap nodes |
| Sleep/Wake | **Background health check** every 30s, reconnect if needed |

### UI/UX
| Decision | Choice |
|----------|--------|
| Navigation | **Collapsible Sidebar** |
| Theme | **System Preference** (dark/light) |
| Graph View | **Focus Mode** (click to explore connections) |
| Graph Animation | **Physics-based** (organic feel) |
| Node Freshness | **Size + Opacity** (fresh = larger, more opaque) |
| Graph Virtualization | **Enable at 1,000 entities** |
| Offline Features | **Disabled + Tooltip** explaining why |

### Security
| Decision | Choice |
|----------|--------|
| Key Derivation | **Argon2id** (256MB memory, 4 iterations, 1 parallelism) |
| Session Model | **Password at app launch only** (persists until app closes) |
| Encryption Scope | **Secrets only** (identity key, API keys). Content stored plaintext. |

### Operations
| Decision | Choice |
|----------|--------|
| Auto-Update | **Prompt + Force for breaking** changes |
| Telemetry | **Opt-in** analytics + crash reports |
| Backup/Export | **ZIP archive only** (no live folder sync - SQLite corruption risk) |
| Content Limit | **50 MB** per L0 document (with cost estimate for large files) |

---

## 3. Embedded Node Runtime

### 3.1 Startup Sequence

```
App Launch
    │
    ▼
┌─────────────────────────────────────┐
│ 1. Initialize Tauri + React        │
│ 2. Check for existing identity     │
│    ├─ None → Onboarding wizard     │
│    └─ Exists → Password prompt     │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│ 3. Show "Decrypting..." indicator  │  ← Argon2id takes 2-3s on lower-end hardware
│ 4. Decrypt identity with password  │
│ 5. Initialize NodeContext          │
│    ├─ Open SQLite database         │
│    ├─ Load content store           │
│    └─ Create NetworkNode           │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│ 5. Bootstrap to network            │
│    ├─ Connect to bootstrap nodes   │
│    ├─ Enable circuit relay (NAT)   │
│    └─ Subscribe to announcements   │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│ 6. Start background services       │
│    ├─ Health check loop (30s)      │
│    ├─ Event listener (network)     │
│    └─ Emit 'node:ready' event      │
└─────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────┐
│ 7. Load graph into memory           │
│    ├─ Query entities + aliases      │  ← Single query, <10ms for 1K entities
│    ├─ Query relationships           │
│    ├─ Build in-memory graph         │
│    └─ Emit 'graph:loaded' event     │
└─────────────────────────────────────┘
    │
    ▼
  Ready for user interaction
```

**Initial Graph Load Query** (optimized for startup performance):
```sql
-- Single query to load all entities with their aliases (<10ms for 1,000 entities)
SELECT
    e.id,
    e.canonical_label,
    e.entity_type,
    e.description,
    e.confidence,
    e.first_seen,
    e.last_updated,
    e.source_count,
    e.metadata_json,
    GROUP_CONCAT(a.alias, '|') as aliases
FROM entities e
LEFT JOIN entity_aliases a ON e.id = a.entity_id
GROUP BY e.id;

-- Separate query for relationships (can be parallelized)
SELECT * FROM relationships;
```

### 3.2 Window Close Behavior

- **Close button (X)**: Minimize to system tray, node keeps running
- **Quit from tray**: Full shutdown, disconnect from network, cleanup
- **Tray icon**: Click to restore window, right-click for menu (Quit, Status)

### 3.3 NAT Traversal

Home users behind routers use **circuit relay** through bootstrap nodes:

```rust
// libp2p relay configuration
SwarmBuilder::with_existing_identity(keypair)
    .with_tokio()
    .with_tcp(...)
    .with_relay_client(noise, yamux)  // Enable relay client
    .with_behaviour(|key, relay| {
        // Kademlia + GossipSub + RequestResponse + Relay
    })
```

No UPnP or hole-punching in MVP. All connections route through bootstrap relays.

### 3.4 Sleep/Wake Handling

```rust
// Background health check task
async fn health_check_loop(node: Arc<NetworkNode>, interval: Duration) {
    loop {
        tokio::time::sleep(interval).await;

        if node.connected_peers() == 0 {
            // Lost all connections (likely woke from sleep)
            emit_event("node:reconnecting");
            node.bootstrap().await?;
            emit_event("node:connected");
        }
    }
}
```

Interval: 30 seconds. On reconnect failure, exponential backoff up to 5 minutes.

### 3.5 Shutdown Sequence

```
Quit Signal (tray menu or Cmd+Q)
    │
    ▼
┌─────────────────────────────────────┐
│ 1. Cancel pending operations        │
│ 2. Flush pending writes to SQLite   │
│ 3. Close payment channels (if any)  │
│ 4. Disconnect from peers            │
│ 5. Emit 'node:shutdown' event       │
│ 6. Exit process                     │
└─────────────────────────────────────┘
```

---

## 4. Local Persistence

### 4.1 Data Directory Structure

```
$APP_DATA/nodalync/                    # Platform-specific app data
├── identity/
│   └── keypair.enc                    # Encrypted Ed25519 keypair
├── config.toml                        # User configuration
├── nodalync.db                        # SQLite database
├── content/                           # Raw L0 content files
│   └── {hash[0:2]}/{hash}.bin         # Sharded by first 2 chars
├── cache/                             # Downloaded content (LRU evicted)
│   └── {hash[0:2]}/{hash}.bin
├── logs/                              # Application logs
│   ├── app.log                        # Current session
│   └── app.log.{date}                 # Rotated logs
└── exports/                           # User exports
```

Platform paths:
- **macOS**: `~/Library/Application Support/nodalync/`
- **Windows**: `%APPDATA%\nodalync\`
- **Linux**: `~/.local/share/nodalync/`

### 4.2 SQLite Schema

Using existing `nodalync-store` schema (v2) plus new tables for desktop.

**Design Decisions**:
1. Relational tables are the source of truth for the graph (no JSON blob)
2. **Stable content IDs**: Each L0 gets a stable `content_id` (UUID) that persists across edits. The hash changes on edit, but the ID doesn't. Entity sources reference the stable ID, not the hash.
3. Graph objects built in memory from SQL queries on app unlock

```sql
-- Existing tables (from nodalync-store/schema.rs)
-- manifests, derived_from, root_cache, channels, payments,
-- peers, cache, settlement_queue, settlement_meta,
-- l1_summaries, announcements

-- =============================================================
-- CONTENT REGISTRY (Stable IDs for content that may be edited)
-- Solves: hash changes on edit, but entity_sources needs stable FK
-- =============================================================
CREATE TABLE content_registry (
    content_id TEXT PRIMARY KEY,        -- UUID, stable across edits
    current_hash BLOB NOT NULL,         -- Current version's hash (FK to manifests)
    content_type TEXT NOT NULL,         -- 'L0', 'L1', 'L2', 'L3'
    created_at INTEGER NOT NULL,
    deleted_at INTEGER                  -- Soft delete timestamp, NULL if active
);

CREATE INDEX idx_content_current_hash ON content_registry(current_hash);

-- =============================================================
-- CONTENT VERSIONS (Track edit history)
-- =============================================================
CREATE TABLE content_versions (
    hash BLOB PRIMARY KEY,              -- This version's hash
    content_id TEXT NOT NULL REFERENCES content_registry(content_id),
    previous_hash BLOB,                 -- Previous version, NULL for v1
    version_number INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_versions_content_id ON content_versions(content_id);

-- =============================================================
-- ENTITIES TABLE (Source of Truth)
-- =============================================================
CREATE TABLE entities (
    id TEXT PRIMARY KEY,                -- e.g., "e42" - auto-incrementing counter
    canonical_label TEXT NOT NULL,      -- Primary display name, max 200 chars
    entity_type TEXT NOT NULL,          -- Person, Organization, Concept, etc.
    description TEXT,                   -- AI-generated summary, max 500 chars
    confidence REAL NOT NULL DEFAULT 0.8,
    first_seen INTEGER NOT NULL,        -- Timestamp
    last_updated INTEGER NOT NULL,      -- Timestamp
    source_count INTEGER NOT NULL DEFAULT 1,
    metadata_json TEXT                  -- Optional additional metadata
);

CREATE INDEX idx_entities_label ON entities(canonical_label);
CREATE INDEX idx_entities_type ON entities(entity_type);
CREATE INDEX idx_entities_updated ON entities(last_updated);
CREATE INDEX idx_entities_source_count ON entities(source_count);

-- =============================================================
-- ENTITY ALIASES TABLE
-- =============================================================
CREATE TABLE entity_aliases (
    entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,
    PRIMARY KEY (entity_id, alias)
);

CREATE INDEX idx_aliases_alias ON entity_aliases(alias);

-- =============================================================
-- RELATIONSHIPS TABLE (Source of Truth)
-- =============================================================
CREATE TABLE relationships (
    id TEXT PRIMARY KEY,                -- e.g., "r17"
    subject_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    predicate TEXT NOT NULL,            -- Fixed ontology predicate
    object_type TEXT NOT NULL,          -- 'entity', 'literal', 'uri'
    object_value TEXT NOT NULL,         -- Entity ID, literal value, or URI
    confidence REAL NOT NULL DEFAULT 0.8,
    extracted_at INTEGER NOT NULL,
    metadata_json TEXT
);

CREATE INDEX idx_rel_subject ON relationships(subject_id);
CREATE INDEX idx_rel_predicate ON relationships(predicate);
CREATE INDEX idx_rel_object ON relationships(object_type, object_value);

-- =============================================================
-- ENTITY-SOURCE LINKS (Uses stable content_id, not hash)
-- =============================================================
CREATE TABLE entity_sources (
    entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    content_id TEXT NOT NULL REFERENCES content_registry(content_id),
    l1_mention_id TEXT,                 -- Specific mention within L1
    added_at INTEGER NOT NULL,
    PRIMARY KEY (entity_id, content_id)
);

CREATE INDEX idx_entity_sources_content ON entity_sources(content_id);

-- =============================================================
-- RELATIONSHIP-SOURCE LINKS (Uses stable content_id)
-- =============================================================
CREATE TABLE relationship_sources (
    relationship_id TEXT NOT NULL REFERENCES relationships(id) ON DELETE CASCADE,
    content_id TEXT NOT NULL REFERENCES content_registry(content_id),
    l1_mention_id TEXT,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (relationship_id, content_id)
);

CREATE INDEX idx_rel_sources_content ON relationship_sources(content_id);

-- =============================================================
-- REVIEW QUEUE
-- =============================================================
CREATE TABLE review_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    item_type TEXT NOT NULL,            -- 'entity_merge', 'conflict', 'ambiguity', 'stale'
    priority INTEGER NOT NULL DEFAULT 0,-- Higher = more urgent
    entity_ids TEXT,                    -- JSON array of affected entity IDs
    content_id TEXT,                    -- L0 that triggered this (stable ID)
    reason TEXT NOT NULL,               -- Human-readable explanation
    suggested_action TEXT,              -- JSON: what AI recommends
    status TEXT NOT NULL DEFAULT 'pending', -- pending, resolved, dismissed
    created_at INTEGER NOT NULL,
    resolved_at INTEGER
);

CREATE INDEX idx_review_status ON review_queue(status);
CREATE INDEX idx_review_priority ON review_queue(priority DESC);

-- =============================================================
-- AI PROCESSING JOBS (split types for partial failure recovery)
-- =============================================================
CREATE TABLE ai_jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_type TEXT NOT NULL,             -- 'extract_l1', 'build_l2' (split for retry granularity)
    content_id TEXT NOT NULL,           -- Stable content ID
    status TEXT NOT NULL DEFAULT 'queued', -- queued, running, completed, failed
    progress INTEGER DEFAULT 0,         -- 0-100
    result_json TEXT,                   -- Output on completion
    error_message TEXT,                 -- Error on failure
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    completed_at INTEGER,
    depends_on INTEGER,                 -- FK to another job that must complete first
    FOREIGN KEY (depends_on) REFERENCES ai_jobs(id)
);

CREATE INDEX idx_jobs_status ON ai_jobs(status, created_at);
CREATE INDEX idx_jobs_content ON ai_jobs(content_id);

-- =============================================================
-- ID COUNTERS (for entity/relationship ID generation)
-- =============================================================
CREATE TABLE id_counters (
    counter_name TEXT PRIMARY KEY,      -- 'entity', 'relationship'
    next_value INTEGER NOT NULL DEFAULT 1
);

-- Initialize counters on schema creation
INSERT OR IGNORE INTO id_counters (counter_name, next_value) VALUES ('entity', 1);
INSERT OR IGNORE INTO id_counters (counter_name, next_value) VALUES ('relationship', 1);

-- Entity/Relationship ID Generation:
-- IDs are prefixed strings: "e42" for entities, "r17" for relationships.
-- This makes them human-readable and type-distinguishable in logs/debug.
--
-- Usage pattern (atomic increment):
--   UPDATE id_counters SET next_value = next_value + 1 WHERE counter_name = 'entity' RETURNING next_value - 1;
--   → Returns 42, use "e42" as the entity ID

-- =============================================================
-- USER SETTINGS
-- =============================================================
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    encrypted INTEGER NOT NULL DEFAULT 0
);
```

### 4.3 AI Job Queue Concurrency Model

Jobs are processed **sequentially** (one at a time) to avoid concurrent graph updates:

```rust
// Job queue processor - runs as background task
async fn job_queue_processor(db: Arc<Database>, ai: Arc<AIClient>) {
    loop {
        // Get next queued job (oldest first, dependencies satisfied)
        let job = db.query_row(
            "SELECT * FROM ai_jobs WHERE status = 'queued'
               AND (depends_on IS NULL OR depends_on IN (SELECT id FROM ai_jobs WHERE status = 'completed'))
             ORDER BY created_at LIMIT 1",
            [], |row| Job::from_row(row)
        );

        match job {
            Some(job) => {
                // Mark as running
                db.execute("UPDATE ai_jobs SET status = 'running', started_at = ?1 WHERE id = ?2",
                    [now(), job.id]);

                // Process job
                let result = process_job(&job, &ai, &db).await;

                // Update status
                match result {
                    Ok(result_json) => {
                        db.execute("UPDATE ai_jobs SET status = 'completed', completed_at = ?1, result_json = ?2 WHERE id = ?3",
                            [now(), result_json, job.id]);
                        emit_event("job:completed", job.id);
                    }
                    Err(e) => {
                        db.execute("UPDATE ai_jobs SET status = 'failed', error_message = ?1 WHERE id = ?2",
                            [e.to_string(), job.id]);
                        emit_event("job:failed", job.id);
                    }
                }
            }
            None => {
                // No jobs queued, sleep before checking again
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}
```

**Behavior when user adds 5 L0s quickly:**
1. First L0: job starts immediately (status: running)
2. L0s 2-5: jobs queued (status: queued)
3. Frontend shows "Processing 1 of 5..." based on queue count
4. Jobs complete sequentially, graph updates after each

### 4.4 Content Edit/Delete Cascade Logic

When L0 content is **updated** (UpdateL0):
```
1. Get content_id from content_registry for this hash
2. Create new hash for updated content
3. Create new manifest version (link to previous via content_versions)
4. Update content_registry: SET current_hash = new_hash
5. Re-run L1 extraction on new content
6. Diff new mentions vs old mentions:
   - New mentions → process normally
   - Removed mentions → check if entity still has other sources
7. For each entity whose source_count would become 0:
   - DELETE entity (cascade deletes relationships via FK)
8. Queue L2 update job
```

When L0 content is **deleted** (DeleteL0):
```
1. Get content_id from content_registry for this hash
2. Get all entities linked to this content (via entity_sources.content_id)
3. For each entity:
   - DELETE FROM entity_sources WHERE content_id = ?
   - UPDATE entities SET source_count = source_count - 1 WHERE id = ?
   - If source_count = 0: DELETE FROM entities WHERE id = ?
     (CASCADE will clean up aliases, relationships, relationship_sources)
4. Delete L1 summary (l1_summaries WHERE l0_hash = ?)
5. Soft-delete: UPDATE content_registry SET deleted_at = NOW() WHERE content_id = ?
   (keeps manifest + versions for history)
6. Optionally delete content file (or keep for recovery)
7. Emit 'graph:updated' event with removed entities
```

**Orphan cleanup is automatic** via `ON DELETE CASCADE` foreign keys.

### 4.5 Encryption Specification

**Key Derivation (Argon2id):**
```rust
use argon2::{Argon2, Params, Version};

const MEMORY_COST: u32 = 262_144;  // 256 MB
const TIME_COST: u32 = 4;          // 4 iterations
const PARALLELISM: u32 = 1;        // Single-threaded
const OUTPUT_LEN: usize = 32;      // 256-bit key

fn derive_key(password: &[u8], salt: &[u8]) -> [u8; 32] {
    let params = Params::new(MEMORY_COST, TIME_COST, PARALLELISM, Some(OUTPUT_LEN))
        .expect("valid params");
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, Version::V0x13, params);

    let mut key = [0u8; 32];
    argon2.hash_password_into(password, salt, &mut key).expect("hash");
    key
}
```

**Encryption (AES-256-GCM):**
```rust
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};

struct EncryptedData {
    salt: [u8; 16],      // Random salt for key derivation
    nonce: [u8; 12],     // Random nonce for AES-GCM
    ciphertext: Vec<u8>, // Encrypted data + 16-byte auth tag
}

fn encrypt(plaintext: &[u8], password: &str) -> EncryptedData {
    let salt = rand::random::<[u8; 16]>();
    let nonce = rand::random::<[u8; 12]>();
    let key = derive_key(password.as_bytes(), &salt);

    let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
    let ciphertext = cipher.encrypt(Nonce::from_slice(&nonce), plaintext).unwrap();

    EncryptedData { salt, nonce, ciphertext }
}
```

**What's encrypted:**
- `identity/keypair.enc` - Ed25519 private key
- `settings` table rows where `encrypted = 1` (API keys)

**What's NOT encrypted (plaintext):**
- L0/L1/L2 content (performance, searchability)
- SQLite database structure
- Configuration (non-sensitive)

### 4.6 Migration Strategy

```rust
// Schema version tracking
const DESKTOP_SCHEMA_VERSION: u32 = 1;

fn migrate_desktop_schema(conn: &Connection) -> Result<()> {
    let current: Option<u32> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'desktop_schema_version'",
            [], |row| row.get(0)
        ).ok().and_then(|s: String| s.parse().ok());

    match current {
        None => {
            // Fresh install - create all tables
            create_desktop_tables(conn)?;
            conn.execute(
                "INSERT INTO settings (key, value, encrypted) VALUES ('desktop_schema_version', ?1, 0)",
                [DESKTOP_SCHEMA_VERSION.to_string()]
            )?;
        }
        Some(v) if v < DESKTOP_SCHEMA_VERSION => {
            // Apply migrations sequentially
            for version in (v + 1)..=DESKTOP_SCHEMA_VERSION {
                apply_migration(conn, version)?;
            }
        }
        _ => {} // Up to date
    }
    Ok(())
}
```

---

## 5. AI Integration Architecture

### 5.1 Processing Flow

Single-pass async processing with progress events:

```
User creates L0
    │
    ▼
┌─────────────────────────────────────┐
│ 1. Save L0 to content store         │ ← Immediate
│ 2. Create AI job (status: pending)  │
│ 3. Return success to frontend       │
│ 4. Emit 'l0:created' event          │
└─────────────────────────────────────┘
    │
    ▼ (async, ~3-10s)
┌─────────────────────────────────────┐
│ 5. Run L1 extraction                │
│    ├─ Rule-based (immediate)        │
│    └─ AI-enhanced (if configured)   │
│ 6. Emit 'l1:progress' events        │
│ 7. Store L1 mentions                │
│ 8. Emit 'l1:complete' event         │
└─────────────────────────────────────┘
    │
    ▼ (async, ~5-15s)
┌─────────────────────────────────────┐
│ 9. Run L2 graph update              │
│    ├─ Extract entities from L1      │
│    ├─ Match against existing graph  │
│    ├─ Create/update entities        │
│    ├─ Extract relationships         │
│    └─ Flag conflicts → review queue │
│ 10. Emit 'l2:progress' events       │
│ 11. Store updated graph             │
│ 12. Emit 'l2:complete' event        │
│ 13. Emit 'graph:updated' event      │
└─────────────────────────────────────┘
    │
    ▼
  Graph view re-renders with new nodes
```

### 5.2 AI Prompt Architecture

#### L1 Extraction Prompt (if AI-enhanced)

```
System: You are an expert at extracting atomic facts from documents.
Extract mentions (claims, statistics, definitions, observations, methods, results)
from the following content.

For each mention, provide:
- content: The exact text or paraphrase (max 1000 chars)
- classification: One of [claim, statistic, definition, observation, method, result]
- confidence: "explicit" (directly stated) or "inferred" (reasonably implied)
- entities: List of proper nouns, people, organizations, concepts mentioned
- source_location: { type: "paragraph", reference: "3", quote: "exact quote" }

Output as JSON array.

User: <document content, chunked if > 8000 tokens>

Expected output:
{
  "mentions": [
    {
      "content": "Machine learning models require large datasets for training",
      "classification": "claim",
      "confidence": "explicit",
      "entities": ["Machine learning"],
      "source_location": { "type": "paragraph", "reference": "2", "quote": "..." }
    }
  ],
  "primary_topics": ["machine learning", "data science"],
  "summary": "2-3 sentence summary of the document"
}
```

**Token budget**: 8,000 tokens input per chunk. Documents > 8K tokens are chunked with 500 token overlap.

#### L2 Entity Extraction Prompt

```
System: You are building a knowledge graph from extracted mentions.
Given the existing entities and new mentions, determine:
1. Which mentions refer to existing entities (match by label/alias)
2. Which mentions introduce new entities
3. What relationships exist between entities

Use ONLY these relationship predicates:
- worksFor, workedFor (employment)
- locatedIn, basedIn (geography)
- createdBy, authorOf (authorship)
- partOf, memberOf (membership)
- relatedTo (general association)
- mentions, discusses (reference)
- before, after, during (temporal)
- causes, enables, prevents (causal)
- isA, instanceOf (classification)
- hasProperty, hasAttribute (properties)
- uses, usedBy (tool/technology)
- fundedBy, investedIn, acquiredBy (business/financial)

Entity types: Person, Organization, Location, Concept, Event, Work, Product, Technology, Metric, TimePoint

For entity matching, consider aliases (ML = Machine Learning).
Confidence > 0.9 = auto-merge. Lower = flag for review.

User:
Existing entities: <JSON array of current entities with aliases>
New mentions: <JSON array of L1 mentions>

Expected output:
{
  "entity_updates": [
    { "action": "update", "entity_id": "e42", "add_aliases": ["ML"], "source_l1": "..." }
  ],
  "new_entities": [
    { "label": "GPT-4", "type": "Technology", "aliases": ["GPT4", "gpt-4"], "confidence": 0.95 }
  ],
  "relationships": [
    { "subject": "e42", "predicate": "relatedTo", "object": "e43", "confidence": 0.8 }
  ],
  "review_items": [
    { "type": "ambiguity", "reason": "Unclear if 'Marcus' is Person or Organization", "entities": ["new_1"] }
  ]
}
```

**Token budget**: 12,000 tokens (entities + mentions combined). If exceeded, process in batches.

#### Two-Step Entity Retrieval (Context Window Management)

At scale (1,000+ entities), sending the full entity list to the L2 prompt would exceed context limits. Instead, we use a two-step retrieval process:

**Step 1: Extract Candidate Names**
```
System: Extract all proper nouns, entity references, and named concepts from these mentions.
Return a JSON array of candidate names (lowercase, deduplicated).

User: <L1 mentions JSON>

Output: ["machine learning", "openai", "alice chen", "project atlas", ...]
```

**Step 2: Query Matching Entities Only**
```sql
-- Fetch only entities that might match the candidates
SELECT e.*, GROUP_CONCAT(a.alias) as aliases
FROM entities e
LEFT JOIN entity_aliases a ON e.id = a.entity_id
WHERE LOWER(e.canonical_label) IN (?1, ?2, ...)
   OR LOWER(a.alias) IN (?1, ?2, ...)
GROUP BY e.id
```

Then send the L2 prompt with only the matched entities (typically <100 even for large graphs).

### 5.3 Entity Resolution Strategy

```
When processing new L1 mentions:

1. Exact match: Label matches existing entity label → update existing
2. Alias match: Label matches existing alias → update existing
3. Fuzzy match (AI): Similar labels with confidence > 0.9 → auto-merge
4. Fuzzy match (AI): Confidence 0.7-0.9 → add to review queue
5. No match: Create new entity
```

Example conflict detection:
```
Monday: "Marcus prefers email for communication"
  → Entity: Marcus (Person), fact: prefers_email

Wednesday: "Just called Marcus on his cell"
  → Entity: Marcus (Person), fact: has_cell_phone
  → Potential conflict: communication preference
  → Add to review queue: "Conflicting communication preferences for Marcus"
```

### 5.4 Review Queue

Items added to review queue:

| Type | Trigger | Suggested Action |
|------|---------|------------------|
| `entity_merge` | Two entities might be same (0.7-0.9 confidence) | Merge with alias |
| `conflict` | New fact contradicts existing fact | Keep both / Replace / Dismiss |
| `ambiguity` | Entity type unclear | Classify as Person/Org/Concept |
| `stale` | Entity not updated in 30+ days **AND** source_count < 3 | Archive / Keep / Update |

**Note on stale items**: Only entities with low source counts (< 3) are flagged as stale. Well-established entities (like "Mom" with 10+ sources) are considered stable facts and don't trigger review just because they haven't been mentioned recently.

UI: Review queue accessible from sidebar. Shows count badge. User clicks to resolve.

---

## 6. Data Flow & IPC Contract

### 6.1 Complete Data Flow Sequence

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                            USER CREATES L0                                    │
└──────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ Frontend: invoke('create_l0', { content, type, tags, visibility })           │
└──────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ Backend: create_l0 command                                                    │
│   1. Validate input (size < 50MB, valid type)                                │
│   2. Generate content_id (UUID)                                              │
│   3. Hash content → L0 hash                                                  │
│   4. Store content to disk: content/{hash[0:2]}/{hash}.bin                   │
│   5. Create manifest (L0, owner, visibility, etc.)                           │
│   6. Create content_registry entry (content_id → hash)                       │
│   7. Create AI jobs: extract_l1 (depends_on: null), build_l2 (depends_on: 1) │
│   8. Return { content_id, hash, status: 'processing' }                       │
│   9. Job queue processor picks up extract_l1, then build_l2                  │
└──────────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┴───────────────┐
                    ▼                               ▼
            [Return to user]              [Background processing]
                    │                               │
                    ▼                               ▼
┌─────────────────────────────┐   ┌────────────────────────────────────────────┐
│ Frontend receives response  │   │ process_l0_background:                     │
│ Shows "Processing..." state │   │   1. Load L0 content                       │
│ Listens for events          │   │   2. Run rule-based L1 extraction          │
└─────────────────────────────┘   │   3. Emit event: 'ai:progress', 20%        │
                                  │   4. If AI key: run AI L1 extraction       │
                                  │   5. Emit event: 'ai:progress', 50%        │
                                  │   6. Store L1 summary                      │
                                  │   7. Emit event: 'l1:complete', { mentions }│
                                  │   8. Load current L2 graph                 │
                                  │   9. Run AI entity extraction              │
                                  │  10. Emit event: 'ai:progress', 80%        │
                                  │  11. Merge entities (auto if >0.9 conf)    │
                                  │  12. Add conflicts to review queue         │
                                  │  13. Store updated L2 graph                │
                                  │  14. Emit event: 'l2:complete'             │
                                  │  15. Emit event: 'graph:updated', { diff } │
                                  │  16. Update job status: 'completed'        │
                                  └────────────────────────────────────────────┘
                                                    │
                                                    ▼
                                  ┌────────────────────────────────────────────┐
                                  │ Frontend receives 'graph:updated' event    │
                                  │   1. Update Zustand graph store            │
                                  │   2. D3 force simulation adds new nodes    │
                                  │   3. Animate new entities appearing        │
                                  └────────────────────────────────────────────┘
```

### 6.2 IPC Command Types

```typescript
// ============ IDENTITY ============

type InitIdentity = {
  input: { password: string; display_name?: string };
  output: { peer_id: string; created_at: string };
};

type Unlock = {
  input: { password: string };
  output: { peer_id: string; connected_peers: number };
};

type GetIdentity = {
  input: {};
  output: { peer_id: string; public_key: string; display_name?: string } | null;
};

// ============ CONTENT ============

type ContentType = 'journal' | 'note' | 'article' | 'research' | 'insight' | 'question' | 'answer' | 'documentation' | 'custom';
type Visibility = 'private' | 'unlisted' | 'shared';

type CreateL0 = {
  input: {
    content: string;           // Raw text content (for small content, <1MB)
    content_type: ContentType;
    title: string;
    tags?: string[];
    visibility: Visibility;
  };
  output: {
    content_id: string;        // Stable UUID
    hash: string;              // Content hash (current version)
    created_at: string;
    processing_status: 'pending' | 'processing' | 'complete';
  };
};

// For large files: pass path instead of content through IPC
type ImportL0 = {
  input: {
    file_path: string;         // Absolute path to file on disk
    content_type: ContentType;
    title: string;
    tags?: string[];
    visibility: Visibility;
  };
  output: {
    content_id: string;        // Stable UUID
    hash: string;              // Content hash
    size_bytes: number;        // Actual file size
    created_at: string;
    processing_status: 'pending' | 'processing' | 'complete';
    estimated_cost?: {         // If large file, include cost estimate
      chunks: number;
      tokens: number;
      cost_usd: number;
    };
  };
};

type ListContent = {
  input: {
    content_type?: 'L0' | 'L1' | 'L2' | 'L3';
    visibility?: Visibility;
    include_deleted?: boolean;  // Default false
    limit?: number;
    offset?: number;
  };
  output: {
    items: ContentSummary[];  // Includes content_id, current_hash, title, etc.
    total: number;
  };
};

type ContentSummary = {
  content_id: string;
  current_hash: string;
  content_type: 'L0' | 'L1' | 'L2' | 'L3';
  title: string;
  visibility: Visibility;
  created_at: string;
  updated_at: string;
  processing_status: 'pending' | 'processing' | 'complete' | 'failed';
  deleted_at?: string;  // If soft-deleted
};

type GetContent = {
  input: { content_id: string };  // Stable content ID (not hash)
  output: {
    content_id: string;
    current_hash: string;
    version_count: number;
    manifest: Manifest;
    content?: string;          // For L0, the raw content
    l1_summary?: L1Summary;    // For L0, the extracted mentions
    l2_graph?: L2EntityGraph;  // For L2
  };
};

type PublishContent = {
  input: { content_id: string; visibility: Visibility; price_hbar?: number };
  output: { success: boolean; announced_to_peers: number; hash: string };
  // IMPORTANT: L2 content cannot be published (protocol spec §7.1.3)
  // Backend MUST reject publish requests where content_type == L2
};

type UpdateL0 = {
  input: {
    content_id: string;        // Stable content ID (not hash)
    content: string;           // New content
    title?: string;            // Optional new title
    tags?: string[];           // Optional new tags
  };
  output: {
    content_id: string;        // Same stable ID
    new_hash: string;          // New content hash (content changed)
    previous_hash: string;     // Link to old version
    version_number: number;    // New version number
    processing_status: 'pending' | 'processing' | 'complete';
    reprocessing: boolean;     // True if L1/L2 will be re-extracted
  };
};

type DeleteL0 = {
  input: {
    content_id: string;        // Stable content ID
    hard_delete?: boolean;     // Default false (soft delete, keep for recovery)
  };
  output: {
    success: boolean;
    entities_removed: number;      // Orphaned entities that were deleted
    relationships_removed: number; // Orphaned relationships deleted
  };
};

type EstimateProcessingCost = {
  input: { content_size_bytes: number };
  output: {
    estimated_chunks: number;        // Number of API calls needed
    estimated_tokens: number;        // Total tokens
    estimated_cost_usd: number;      // Rough cost estimate
    estimated_time_seconds: number;  // Processing time
    warning?: string;                // E.g., "Large file, consider splitting"
  };
};

// ============ AI PROCESSING ============

type SetApiKey = {
  input: { provider: 'openai' | 'anthropic'; api_key: string };
  output: { success: boolean; validated: boolean };
};

type GetProcessingStatus = {
  input: { content_id: string };
  output: {
    status: 'pending' | 'processing' | 'complete' | 'failed';
    progress: number;         // 0-100
    current_job: 'extract_l1' | 'build_l2' | null;
    error?: string;
  };
};

// ============ GRAPH ============

type GetGraph = {
  input: { include_stale?: boolean };
  output: {
    entities: Entity[];
    relationships: Relationship[];
    stats: { entity_count: number; relationship_count: number; last_updated: string };
  };
};

type GetEntityDetails = {
  input: { entity_id: string };
  output: {
    entity: Entity;
    sources: SourceReference[];     // L0s that mention this entity
    relationships: Relationship[];  // Edges involving this entity
    timeline: TimelineEvent[];      // When entity was mentioned/updated
  };
};

type FocusEntity = {
  input: { entity_id: string; depth?: number };  // depth: how many hops to include
  output: {
    center: Entity;
    connected_entities: Entity[];
    connecting_relationships: Relationship[];
    sources: SourceReference[];
  };
};

// ============ REVIEW QUEUE ============

type GetReviewQueue = {
  input: { status?: 'pending' | 'resolved' | 'dismissed' };
  output: { items: ReviewItem[]; pending_count: number };
};

type ResolveReviewItem = {
  input: {
    id: number;
    action: 'merge' | 'keep_both' | 'replace' | 'dismiss';
    merge_target?: string;    // If action is 'merge', which entity to merge into
  };
  output: { success: boolean; affected_entities: string[] };
};

// ============ NETWORK ============

type GetNodeStatus = {
  input: {};
  output: {
    connected: boolean;
    peer_id: string;
    connected_peers: number;
    bootstrap_status: 'connecting' | 'connected' | 'failed';
    relay_status: 'enabled' | 'disabled';
  };
};

type SearchNetwork = {
  input: { query: string; limit?: number };
  output: { results: SearchResult[]; total: number };
};

type PreviewContent = {
  input: { hash: string };
  output: {
    manifest: Manifest;
    l1_summary: L1Summary;
    provider_peer_id: string;
  };
};

// ============ SETTINGS ============

type GetSettings = {
  input: {};
  output: { [key: string]: any };
};

type UpdateSettings = {
  input: { [key: string]: any };
  output: { success: boolean };
};

// ============ BACKUP ============

type ExportBackup = {
  input: { destination_path: string };
  output: {
    backup_path: string;
    size_bytes: number;
    content_count: number;
    entity_count: number;
  };
};

type ImportBackup = {
  input: {
    backup_path: string;
    password: string;  // To decrypt identity from backup
    mode: 'replace' | 'merge';  // Replace all data or merge
  };
  output: {
    success: boolean;
    imported_content: number;
    imported_entities: number;
    conflicts?: number;  // If merge mode, count of conflicts to review
  };
};

// Note: ConfigureAutoBackup deferred to post-MVP (v0.2+)
// Scheduled backups will be configured through UpdateSettings:
//   { auto_backup_enabled: true, auto_backup_frequency: 'daily', auto_backup_path: '...' }
```

### 6.3 IPC Event Types

```typescript
// Events emitted from backend to frontend

type NodeEvent =
  | { type: 'node:ready' }
  | { type: 'node:reconnecting' }
  | { type: 'node:connected'; peers: number }
  | { type: 'node:disconnected'; reason: string }
  | { type: 'node:shutdown' };

type AIEvent =
  | { type: 'ai:progress'; content_id: string; percent: number; stage: 'extract_l1' | 'build_l2' }
  | { type: 'ai:error'; content_id: string; error: string };

type ContentEvent =
  | { type: 'l0:created'; content_id: string; hash: string }
  | { type: 'l1:complete'; content_id: string; mention_count: number }
  | { type: 'l2:complete'; content_id: string; entities_added: number; entities_updated: number };

type GraphEvent =
  | { type: 'graph:updated'; diff: GraphDiff }
  | { type: 'review:added'; item: ReviewItem };

// GraphDiff includes FULL entity/relationship objects for added/updated items
// so the frontend can render immediately without a re-fetch round-trip
type GraphDiff = {
  added_entities: Entity[];           // Full entity objects
  updated_entities: Entity[];         // Full entity objects (with new data)
  removed_entity_ids: string[];       // Just IDs for removals
  added_relationships: Relationship[];
  updated_relationships: Relationship[];
  removed_relationship_ids: string[];
};
```

---

## 7. Graph Data Model

### 7.1 Entity Schema

```typescript
interface Entity {
  id: string;                    // e.g., "e42" - unique within graph
  canonical_label: string;       // Primary display name, max 200 chars
  aliases: string[];             // Alternative names (ML, Machine Learning)
  entity_type: EntityType;       // Person, Organization, Concept, etc.
  description?: string;          // AI-generated summary, max 500 chars
  external_links: string[];      // URIs to external knowledge bases

  // Provenance
  first_seen: number;            // Timestamp of first mention
  last_updated: number;          // Timestamp of last update
  source_count: number;          // Number of L0s mentioning this entity
  mention_refs: MentionRef[];    // Links to specific L1 mentions

  // Confidence & Freshness
  confidence: number;            // 0.0-1.0, how certain we are this is correct
  freshness_score: number;       // Computed: f(last_updated, source_count)

  // Metadata
  metadata: Record<string, string>;
}

type EntityType =
  | 'Person'
  | 'Organization'
  | 'Location'
  | 'Concept'
  | 'Event'
  | 'Work'
  | 'Product'
  | 'Technology'
  | 'Metric'
  | 'TimePoint';  // Aligns with protocol spec §4.4b ndl:TimePoint
```

### 7.2 Relationship Schema

```typescript
interface Relationship {
  id: string;                    // e.g., "r17"
  subject: string;               // Entity ID
  predicate: Predicate;          // Fixed ontology
  object: RelationshipObject;    // Entity ID, literal, or URI

  // Provenance
  confidence: number;            // 0.0-1.0
  mention_refs: MentionRef[];    // Source L1 mentions
  extracted_at: number;          // Timestamp

  metadata: Record<string, string>;
}

type Predicate =
  // Employment
  | 'worksFor' | 'workedFor'
  // Geography
  | 'locatedIn' | 'basedIn'
  // Authorship
  | 'createdBy' | 'authorOf'
  // Membership
  | 'partOf' | 'memberOf'
  // Association
  | 'relatedTo'
  // Reference
  | 'mentions' | 'discusses'
  // Temporal
  | 'before' | 'after' | 'during'
  // Causal
  | 'causes' | 'enables' | 'prevents'
  // Classification
  | 'isA' | 'instanceOf'
  // Properties
  | 'hasProperty' | 'hasAttribute'
  // Tool/Technology
  | 'uses' | 'usedBy'
  // Business/Financial
  | 'fundedBy' | 'investedIn' | 'acquiredBy';

type RelationshipObject =
  | { type: 'entity'; entity_id: string }
  | { type: 'literal'; value: string; datatype?: string }
  | { type: 'uri'; uri: string };
```

### 7.3 Freshness Calculation

```typescript
function calculateFreshness(entity: Entity, now: number): number {
  const daysSinceUpdate = (now - entity.last_updated) / (1000 * 60 * 60 * 24);
  const recencyScore = Math.exp(-daysSinceUpdate / 30);  // Decay over 30 days

  const sourceScore = Math.min(entity.source_count / 10, 1);  // Cap at 10 sources

  // Weighted combination: 70% recency, 30% source density
  return 0.7 * recencyScore + 0.3 * sourceScore;
}

// Used for visualization:
// - Size: baseSize * (1 + 0.5 * freshness)
// - Opacity: 0.4 + 0.6 * freshness
```

### 7.4 Merge/Dedup Strategy

When merging entities:

```typescript
function mergeEntities(target: Entity, source: Entity): Entity {
  return {
    ...target,
    aliases: [...new Set([...target.aliases, source.canonical_label, ...source.aliases])],
    description: target.description || source.description,
    external_links: [...new Set([...target.external_links, ...source.external_links])],
    first_seen: Math.min(target.first_seen, source.first_seen),
    last_updated: Math.max(target.last_updated, source.last_updated),
    source_count: target.source_count + source.source_count,
    mention_refs: [...target.mention_refs, ...source.mention_refs],
    confidence: Math.max(target.confidence, source.confidence),
  };
}
```

### 7.5 Graph Virtualization at Scale

D3 force simulations don't have built-in virtualization. At 1,000+ entities, we implement:

**Viewport Culling:**
```typescript
// Only simulate/render nodes visible in current viewport
function getVisibleNodes(nodes: Entity[], viewport: Viewport): Entity[] {
  const margin = 100; // px buffer for smooth scrolling
  return nodes.filter(node => {
    const screenPos = worldToScreen(node.x, node.y, viewport);
    return screenPos.x >= -margin && screenPos.x <= viewport.width + margin
        && screenPos.y >= -margin && screenPos.y <= viewport.height + margin;
  });
}

// Render loop
function render() {
  const visible = getVisibleNodes(allNodes, currentViewport);
  // Only render visible nodes
  visible.forEach(renderNode);
  // Only render edges where both endpoints are visible (or one is)
  edges.filter(e => visible.includes(e.source) || visible.includes(e.target))
       .forEach(renderEdge);
}
```

**Level-of-Detail (LOD) Rendering:**
```typescript
// At different zoom levels, render differently
function renderNode(node: Entity, zoomLevel: number) {
  if (zoomLevel < 0.3) {
    // Very zoomed out: just dots, no labels
    renderDot(node, 3);
  } else if (zoomLevel < 0.6) {
    // Medium zoom: small circles, short labels for high-source-count entities
    renderCircle(node, 6);
    if (node.source_count >= 5) renderLabel(node, 10);
  } else {
    // Zoomed in: full rendering
    renderCircle(node, 8 + node.source_count);
    renderLabel(node, 12);
    renderTypeIcon(node);
  }
}
```

**Cluster Aggregation (future optimization):**
At very large scales (10K+ entities), consider clustering nearby entities into aggregate nodes that expand on zoom.

**Performance Targets:**
- 1,000 entities: 60fps, <100ms initial render
- 5,000 entities: 30fps acceptable, may show loading state
- 10,000+ entities: Requires clustering, not supported in MVP

---

## 8. Network Operations Protocol

### 8.1 DHT Search

```
User types query → invoke('search_network', { query })
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│ 1. Parse query into keywords                                     │
│ 2. For each keyword, compute hash: H(keyword.lowercase())        │
│ 3. DHT lookup: kademlia.get_record(hash)                         │
│ 4. Collect announcements from DHT responses                      │
│ 5. Filter by: price range, content type, date                    │
│ 6. Sort by: relevance (keyword matches), recency                 │
│ 7. Return top N results                                          │
└─────────────────────────────────────────────────────────────────┘
```

**Timeout**: 10 seconds for DHT query. Return partial results if timeout.

**When node hasn't synced**: Show warning "Still connecting to network..." but allow search. May return fewer results.

### 8.2 Preview Request

```
User clicks preview → invoke('preview_content', { hash })
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│ 1. DHT lookup for hash → get provider addresses                  │
│ 2. Connect to provider (via relay if needed)                     │
│ 3. Send PreviewRequest message                                   │
│ 4. Receive PreviewResponse:                                      │
│    - manifest (title, type, price, owner)                        │
│    - l1_summary (5 preview mentions, topics)                     │
│ 5. Cache response locally                                        │
│ 6. Return to frontend                                            │
└─────────────────────────────────────────────────────────────────┘
```

**Preview is FREE** - no payment required. Shows enough to decide if content is worth querying.

**Timeout**: 30 seconds. Show error if provider unreachable.

### 8.3 Query (Paid) - Post-MVP

```
User clicks query → invoke('query_content', { hash, budget })
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│ 1. Check if channel exists with provider                         │
│    ├─ Yes → use existing channel                                 │
│    └─ No → open channel (requires Hedera deposit)                │
│ 2. Send QueryRequest with payment signature                      │
│ 3. Receive QueryResponse with full content                       │
│ 4. Verify content hash matches                                   │
│ 5. Cache content locally                                         │
│ 6. Update channel state                                          │
│ 7. Return content to frontend                                    │
└─────────────────────────────────────────────────────────────────┘
```

---

## 9. Additional Specifications

### 9.1 Auto-Update

Using Tauri's built-in updater:

```rust
// tauri.conf.json
{
  "plugins": {
    "updater": {
      "active": true,
      "endpoints": ["https://updates.nodalync.io/desktop/{{target}}/{{arch}}/{{current_version}}"],
      "pubkey": "dW50...",  // Ed25519 public key for signature verification
      "windows": { "installMode": "passive" }
    }
  }
}
```

**Behavior**:
- Check for updates on launch
- If update available: show notification "Update available: v1.2.3"
- User clicks → download in background → prompt to restart
- **Force update** if server returns `"force": true` (security fixes, breaking protocol changes)

### 9.2 Logging & Telemetry

**Local logs** (always on):
```
$APP_DATA/nodalync/logs/
├── app.log           # Current session, rotated at 10MB
├── app.log.2024-01-15
└── app.log.2024-01-14
```

Log format: `[timestamp] [level] [module] message`

**Opt-in telemetry** (ask on first launch):
- Crash reports: Stack trace, OS version, app version (no content)
- Usage analytics: Feature usage counts, session duration (no content)
- Sent to: `https://telemetry.nodalync.io/`
- Can disable anytime in Settings

### 9.3 Backup & Export

**ZIP Export**:
```
nodalync-backup-2024-01-15.zip
├── manifest.json         # Backup metadata, version, schema version
├── identity/
│   └── keypair.enc       # Still encrypted, needs password to use
├── content/              # All L0 content files
├── nodalync.db           # Full database
└── config.toml           # Settings (non-sensitive)
```

**Commands**:
- Export: `invoke('export_backup', { path })` → returns ZIP file path
- Import: `invoke('import_backup', { path, password })` → validates, merges or replaces

**Scheduled Backup** (optional):
- Settings → Backup → Enable auto-backup
- Frequency: daily/weekly
- Location: user-specified folder
- Retention: keep last N backups

**Why no live folder sync?**

SQLite uses file-level locking and WAL mode, which cloud sync services (Dropbox, iCloud, OneDrive) don't handle correctly. Two instances of the app pointing at the same synced folder would corrupt the database. The ZIP backup approach is safe because it creates a point-in-time snapshot that can be synced without conflicts.

### 9.4 Keyboard Shortcuts

| Action | macOS | Windows/Linux |
|--------|-------|---------------|
| New content | ⌘N | Ctrl+N |
| Quick publish | ⌘⇧P | Ctrl+Shift+P |
| Search | ⌘K | Ctrl+K |
| Search network | ⌘⇧K | Ctrl+Shift+K |
| Focus graph | ⌘G | Ctrl+G |
| Review queue | ⌘R | Ctrl+R |
| Settings | ⌘, | Ctrl+, |
| Toggle sidebar | ⌘B | Ctrl+B |
| Zoom in | ⌘+ | Ctrl++ |
| Zoom out | ⌘- | Ctrl+- |
| Fit graph | ⌘0 | Ctrl+0 |
| Quit | ⌘Q | Alt+F4 |

### 9.5 Accessibility

- All interactive elements keyboard-focusable
- ARIA labels on graph nodes (entity name, type)
- High contrast mode support (follows system)
- Screen reader: announce graph changes ("3 new entities added")
- Reduce motion setting: disable physics animations

---

## 10. Testing Strategy

### 10.1 Test Matrix

| Layer | Tool | Coverage Target |
|-------|------|-----------------|
| Rust unit | `cargo test` | 80% command handlers |
| Rust integration | `tokio::test` + mocks | Core workflows |
| AI mocking | Recorded responses | Extraction + graph building |
| React components | Vitest + RTL | All interactive components |
| React integration | Vitest | Store updates, hooks |
| Graph rendering | Visual regression | Node layout, animations |
| E2E | Playwright + Tauri Driver | Critical user journeys |
| Performance | Criterion (Rust) | Graph operations at scale |

### 10.2 Critical Test Scenarios

1. **Onboarding**: Create identity → Set API key → First publish → Graph renders
2. **Content flow**: Create L0 → L1 extraction → L2 update → Review queue item
3. **Entity resolution**: Create two L0s with same entity → Auto-merge
4. **Conflict detection**: Create contradicting facts → Review queue populated
5. **Graph at scale**: Load 1000+ entities → Virtualization kicks in → 60fps maintained
6. **Offline mode**: Disconnect → Local operations work → Network features disabled
7. **Sleep/wake**: Sleep laptop → Wake → Auto-reconnect within 30s
8. **Backup/restore**: Export → Delete app data → Import → All content restored

---

## 11. Roadmap

### v0.1.0 - MVP
- Identity + onboarding
- L0 creation with content types
- L1 extraction (rule-based + AI)
- L2 graph building (AI-powered)
- Graph view with Focus Mode
- Review queue for conflicts/ambiguities
- Network search + preview
- Basic settings

### v0.2.0 - Background Agents
- Freshness Agent (flag stale entities)
- Merge Agent (suggest duplicates after each L0)
- Contradiction Agent (cross-reference facts)
- Completeness Agent (missing fields on entities)
- Agent dashboard in settings

### v0.3.0 - Economics
- Hedera account connection
- Payment channels
- Paid queries
- Earnings dashboard

### v0.4.0 - Synthesis
- L3 creation workspace
- Provenance visualization
- Publishing L3 content

### v1.0.0 - Polish
- Two-tier AI (if latency complaints)
- Animation refinements
- Accessibility audit
- Installer packages
- Documentation + tutorials

---

## 12. Appendix: Fixed Ontology

### Relationship Predicates

| Predicate | Domain | Range | Example |
|-----------|--------|-------|---------|
| **Employment** |||
| worksFor | Person | Organization | "Alice worksFor Acme Corp" |
| workedFor | Person | Organization | "Bob workedFor StartupX" |
| **Geography** |||
| locatedIn | Entity | Location | "Acme Corp locatedIn San Francisco" |
| basedIn | Organization | Location | "Nodalync basedIn Austin" |
| **Authorship** |||
| createdBy | Work | Person/Org | "Paper createdBy Alice" |
| authorOf | Person | Work | "Alice authorOf Paper" |
| **Membership** |||
| partOf | Entity | Entity | "Chapter partOf Book" |
| memberOf | Person | Organization | "Alice memberOf Board" |
| **Association** |||
| relatedTo | Entity | Entity | "ML relatedTo AI" |
| mentions | Work | Entity | "Paper mentions GPT-4" |
| discusses | Work | Concept | "Paper discusses transformers" |
| **Temporal** |||
| before | Event | Event | "Training before Deployment" |
| after | Event | Event | "Launch after Testing" |
| during | Event | Event | "Bug during Demo" |
| **Causal** |||
| causes | Entity | Entity | "Bug causes Outage" |
| enables | Entity | Entity | "API enables Integration" |
| prevents | Entity | Entity | "Firewall prevents Attack" |
| **Classification** |||
| isA | Entity | Concept | "GPT-4 isA LLM" |
| instanceOf | Entity | Concept | "Alice instanceOf Person" |
| **Properties** |||
| hasProperty | Entity | Literal | "Server hasProperty '16GB RAM'" |
| hasAttribute | Entity | Literal | "Alice hasAttribute 'prefers email'" |
| **Tool/Technology** |||
| uses | Entity | Technology | "Project uses React" |
| usedBy | Technology | Entity | "Python usedBy DataTeam" |
| **Business/Financial** |||
| fundedBy | Organization | Organization/Person | "Startup fundedBy VCFirm" |
| investedIn | Person/Org | Organization | "Alice investedIn Nodalync" |
| acquiredBy | Organization | Organization | "StartupX acquiredBy BigCorp" |

---

## Verification Checklist

### Core Functionality
- [ ] `cargo test --workspace` passes
- [ ] `npm test` in `ui/` passes
- [ ] Onboarding: Identity → API Key → First Publish → Graph animates
- [ ] Password unlock shows "Decrypting..." indicator (2-3s on low-end hardware)

### Content Lifecycle
- [ ] CreateL0: File imported → L1 extracted → L2 updated → Graph shows new entities
- [ ] UpdateL0: Edit content → Re-extraction runs → Graph updated with diff
- [ ] DeleteL0: Delete content → Orphaned entities removed → source_count decremented
- [ ] Large file (>10MB): Cost estimate shown before processing

### AI Processing
- [ ] AI job queue: Add 5 L0s quickly → processed sequentially → UI shows "1 of 5"
- [ ] AI processing: L0 → L1 → L2 completes in < 30s (typical document)
- [ ] Entity merge: AI detects "ML" and "Machine Learning" → auto-merged (conf > 0.9)
- [ ] Review queue: Conflict detected → Item appears → User resolves

### Graph Rendering
- [ ] Graph: 1000+ entities renders at 60fps (viewport culling active)
- [ ] LOD: Zoom out → nodes become dots, labels hidden
- [ ] LOD: Zoom in → full labels and type icons visible
- [ ] Focus Mode: Click node → sources/connections highlight
- [ ] graph:updated event includes full entity data (no extra fetch needed)

### Network & Resiltic
- [ ] Offline: Disconnect network → Local features work → Network features disabled
- [ ] Sleep/Wake: Reconnects within 30s
- [ ] Backup: Export ZIP → Delete data → Import → All content restored
- [ ] App launches on macOS, Windows, Linux
