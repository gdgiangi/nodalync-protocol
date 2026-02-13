# Nodalync Studio — Tauri IPC Command Reference

> For Hephaestus (frontend) and any agent building the React UI.
> All commands are invoked via `window.__TAURI__.invoke("command_name", { args })`.

---

## L2 Graph Commands (Phase 1)

### `get_graph_data`
Returns the full L2 knowledge graph for visualization.
- **Args:** none
- **Returns:** `{ entities: Entity[], relationships: Relationship[], stats: GraphStats }`

### `get_subgraph`
Get entities within N hops of a root entity.
- **Args:** `{ entity_id: number, max_hops: number, max_results?: number }`
- **Returns:** `{ entities: Entity[], relationships: Relationship[] }`

### `search_entities`
Full-text search across entities.
- **Args:** `{ query: string, limit?: number }`
- **Returns:** `Entity[]`

### `get_graph_stats`
Get graph statistics (entity count, relationship count, etc).
- **Args:** none
- **Returns:** `GraphStats`

### `get_context`
Get focused context for an entity (compact, agent-friendly).
- **Args:** `{ entity_id: number, max_hops?: number }`
- **Returns:** `string` (formatted context text)

### `extract_mentions`
Run L1 extraction on content and bridge results to L2 graph.
- **Args:** `{ content_hash: string }` (64-char hex)
- **Returns:** `ExtractionResult`
  ```typescript
  {
    content_hash: string,
    content_id: string,       // Stable ID in graph DB
    mention_count: number,    // Total mentions found
    entities: ExtractedEntity[],
    topics: string[],
    summary: string
  }
  ```
- **`ExtractedEntity`:**
  ```typescript
  {
    entity_id: string,        // ID in L2 graph DB
    label: string,
    entity_type: string,      // "concept" | "technology" | "organization"
    existing: boolean,        // true if matched existing graph entity
    confidence: number,       // 1.0 for exact match, 0.6 for new stubs
    source_mention?: string   // The mention text that produced this entity
  }
  ```
- **Flow:** After `publish_file` or `publish_text`, call this with the returned hash to populate the graph.

---

## Identity Commands

### `check_identity`
Check if a node identity exists (no password needed).
- **Args:** none
- **Returns:** `boolean`

### `init_node`
Create a new node identity (Ed25519 keypair, encrypted).
- **Args:** `{ password: string, name?: string }`
- **Returns:** `{ name?: string, peer_id: string, public_key: string, data_dir: string, created_at?: string }`
- **`name`** is stored in a profile — defaults to "Anonymous" if omitted.
- **`public_key`** is hex-encoded Ed25519 public key.
- **`created_at`** is ISO-8601 timestamp.

### `unlock_node`
Decrypt an existing node identity.
- **Args:** `{ password: string }`
- **Returns:** `{ name?: string, peer_id: string, public_key: string, data_dir: string, created_at?: string }`

### `get_identity`
Get current identity info (node must be unlocked, no password needed).
- **Args:** none
- **Returns:** `{ name?: string, peer_id: string, public_key: string, data_dir: string, created_at?: string }`
- Use for dashboard/profile display after unlock.

---

## Publish Commands

### `publish_file`
Publish a file to the Nodalync network.
- **Args:** `{ file_path: string, title?: string, description?: string, price?: number, visibility?: "shared"|"private"|"unlisted" }`
- **Returns:** `{ hash: string, title: string, size: number, price: number, visibility: string, mentions?: number }`

### `publish_text`
Publish text content directly (quick publish).
- **Args:** `{ text: string, title: string, description?: string, price?: number, visibility?: "shared"|"private"|"unlisted" }`
- **Returns:** `{ hash: string, title: string, size: number, price: number, visibility: string, mentions?: number }`

### `list_content`
List all published content on this node.
- **Args:** none
- **Returns:** `ContentItem[]` — `{ hash, title, size, content_type, visibility, price, version, mention_count? }`

### `get_content_details`
Get details for a specific content item by hash.
- **Args:** `{ hash: string }` (64-char hex)
- **Returns:** `ContentItem`

### `delete_content`
Delete content from the local node.
- **Args:** `{ hash: string }` (64-char hex)
- **Returns:** `void`

### `unpublish_content`
Unpublish content (sets Private, removes from DHT).
- **Args:** `{ hash: string }` (64-char hex)
- **Returns:** `void`

---

## Discovery Commands

### `search_network`
Search local + cached + peer results, deduplicated.
- **Args:** `{ query: string, content_type?: "l0"|"l1"|"l2", limit?: number }`
- **Returns:** `SearchResult[]` — `{ hash, title, content_type, price, owner, mention_count, primary_topics, summary, total_queries, source }`
- **`source`** is one of: `"local"`, `"cached"`, `"peer"`

### `preview_content`
Preview metadata + L1 summary without retrieving full content.
- **Args:** `{ hash: string }` (64-char hex)
- **Returns:** `{ hash, title, content_type, size, price, visibility, owner, mention_count, primary_topics, summary, version, provider_peer_id? }`

### `query_content`
Retrieve full content with payment. Automatically applies app fee.
- **Args:** `{ hash: string, payment_amount?: number }` (price in NDL, e.g. 0.001)
- **Returns:** `{ hash, title, content_type, content_text?, content_size, price_paid, app_fee, total_cost, receipt_id, transaction_id? }`
- **`content_text`** is `null` for binary content.
- **`app_fee`** is the Studio application fee (tinybars). See `get_fee_config` to check rate.
- **`total_cost`** = `price_paid` + `app_fee`
- **`transaction_id`** is the UUID in the transaction log. Use with `get_transaction_history`.

### `get_content_versions`
Get version history for a content item.
- **Args:** `{ hash: string }` (64-char hex, root hash)
- **Returns:** `VersionItem[]` — `{ hash, number, timestamp, visibility, price }`

---

## Network Commands

### `get_node_status`
Get current node status (works whether or not initialized).
- **Args:** none
- **Returns:** `{ initialized, peer_id?, network_active, connected_peers, content_count, data_dir }`

### `start_network`
Start P2P network with default config (random port, no bootstrap).
- **Args:** none
- **Returns:** `void`

### `start_network_configured`
Start P2P network with custom port and bootstrap nodes.
- **Args:** `{ listen_port?: number, bootstrap_nodes?: string[] }`
- **Bootstrap format:** `"12D3KooW...@/ip4/192.168.1.5/tcp/9000"`
- **Returns:** `NetworkInfo` — `{ active, listen_addresses, connected_peers, peer_count }`

### `stop_network`
Stop the P2P network.
- **Args:** none
- **Returns:** `void`

### `get_peers`
Get list of connected peer IDs (simple).
- **Args:** none
- **Returns:** `string[]`

### `get_network_info`
Get detailed network info including listen addresses.
- **Args:** none
- **Returns:** `{ active, listen_addresses: string[], connected_peers: PeerInfo[], peer_count }`
- **PeerInfo:** `{ libp2p_id, nodalync_id? }`

### `dial_peer`
Manually connect to a peer by multiaddress.
- **Args:** `{ address: string }` (e.g. `/ip4/192.168.1.5/tcp/9000`)
- **Returns:** `{ success, address, error? }`

---

## Fee Commands (D2 — Application-Level Fee)

### `get_fee_config`
Get current fee configuration and summary stats.
- **Args:** none
- **Returns:**
  ```typescript
  {
    rate_percent: number,      // e.g. 5.0 for 5%
    rate: number,              // e.g. 0.05
    total_collected: number,   // tinybars
    total_collected_hbar: number, // HBAR (display-friendly)
    transaction_count: number,
    avg_fee_per_transaction: number, // tinybars
    updated_at: string         // ISO-8601
  }
  ```
- Works even before node unlock (reads from disk).

### `set_fee_rate`
Set the application fee rate.
- **Args:** `{ rate_percent: number }` (e.g. 5.0 for 5%)
- **Validation:** 0% to 50%
- **Returns:** `FeeConfigResponse` (same as `get_fee_config`)

### `get_transaction_history`
Get transaction history with fee breakdown.
- **Args:** `{ limit?: number, offset?: number }`
- **Default limit:** 50, **max:** 500
- **Returns:**
  ```typescript
  {
    transactions: TransactionRecord[],
    total_count: number,
    total_content_cost: number,   // tinybars
    total_app_fees: number,       // tinybars
    total_amount: number,         // tinybars
    total_amount_hbar: number,
    total_app_fees_hbar: number
  }
  ```
- **`TransactionRecord`:**
  ```typescript
  {
    id: string,                // UUID
    content_hash: string,
    content_title: string,
    content_cost: number,      // what the creator charges (tinybars)
    app_fee: number,           // Studio's cut (tinybars)
    total: number,             // content_cost + app_fee
    fee_rate: number,          // rate at time of transaction
    recipient: string,         // peer ID
    status: "Pending"|"Settled"|"Failed"|"Free",
    created_at: string         // ISO-8601
  }
  ```
- Transactions are returned newest-first.

### `get_fee_quote`
Preview the fee breakdown before querying content.
- **Args:** `{ content_price: number }` (tinybars)
- **Returns:**
  ```typescript
  {
    content_cost: number,
    app_fee: number,
    total: number,
    fee_rate_percent: number,
    content_cost_hbar: number,
    app_fee_hbar: number,
    total_hbar: number
  }
  ```
- Call this before `query_content` to show the user the full cost.

---

## L3 Synthesis Commands

### `create_l3_summary`
Create an L3 synthesis entity from selected L2 entities.
- **Args:** `{ title: string, summary_text: string, entity_ids: string[] }`
- **Validation:** 1–100 entities, non-empty title
- **Returns:**
  ```typescript
  {
    summary: {
      entity_id: string,           // Graph entity ID (e.g. "e42")
      title: string,
      summary_text: string,
      source_entity_ids: string[], // IDs of synthesized entities
      source_entity_labels: string[], // Labels for display
      created_at: string           // ISO-8601
    },
    relationships_created: number  // "synthesizes" edges created
  }
  ```
- **Flow:** User selects entities in graph → enters title + summary → this command creates the L3 entity with `synthesizes` relationships to each source.

### `get_l3_summaries`
List all L3 summaries in the graph.
- **Args:** `{ limit?: number }` (default 50, max 500)
- **Returns:** `L3Summary[]` — same structure as `create_l3_summary.summary`, sorted newest-first.

### `get_entity_content_links`
Get L0 content items linked to a specific entity.
- **Args:** `{ entity_id: string }`
- **Returns:**
  ```typescript
  EntityContentLink[] = {
    content_id: string,    // Registry content ID
    content_hash: string,  // 64-char hex hash
    content_type: string,  // e.g. "L0"
    linked_at: string      // ISO-8601
  }[]
  ```
- **Use case:** Powers the "L0 focus → L1 tendrils" interaction. When a user clicks an entity, show which raw content contributed to it.

---

## Network Maintenance Commands

### `reannounce_content`
Re-announce all published (Shared) content to the network.
- **Args:** none
- **Returns:** `number` — count of items re-announced
- **Use case:** After network start, or when peers may have lost track of our content. Called automatically by `auto_start_network`, but can be triggered manually.

### `get_nat_status`
Get the current NAT traversal status as detected by AutoNAT.
- **Args:** none
- **Returns:**
  ```typescript
  {
    status: "unknown" | "public" | "private",
    nat_traversal_enabled: boolean,
    relay_reservations: number
  }
  ```
- **`status`** meanings:
  - `"public"` — node is directly reachable from the internet (no NAT or UPnP succeeded)
  - `"private"` — node is behind NAT, using relay/hole-punching for inbound connections
  - `"unknown"` — AutoNAT probing in progress (normal on startup, resolves within ~30s)
- **Use case:** Show NAT status in the network view. If `"private"`, the node can still communicate but may have slower initial connections via relay before DCUtR hole-punching upgrades them to direct.

---

## Notes for Frontend

1. **Startup flow:** `check_identity` → if false: show onboarding → `init_node(password, name)`; if true: show password → `unlock_node`
2. **After unlock:** `get_identity` for profile display, then `start_network_configured` (or `start_network` for quick start)
3. **Publish flow:** `publish_file`/`publish_text` → `extract_mentions(hash)` to populate L2 graph
4. **Query flow:** `get_fee_quote(price)` to show breakdown → `query_content(hash, amount)` — fee is auto-recorded
5. **Fee dashboard:** `get_fee_config` for summary, `get_transaction_history` for details, `set_fee_rate` to configure
6. **L3 synthesis:** Select entities in graph → `create_l3_summary(title, text, ids)` → new L3 node appears with `synthesizes` edges. List all with `get_l3_summaries`.
7. **Entity drill-down:** `get_entity_content_links(entity_id)` → shows which L0 content contributed to this entity. Combined with `get_subgraph`, this powers the full L0→L1→L2→L3 hierarchy view.
8. **Price values:** Frontend sends NDL (e.g. 0.001), backend converts to tinybars internally
9. **Hash format:** Always 64-char lowercase hex strings
10. **Error handling:** All commands return `Result<T, String>` — errors are human-readable strings
11. **NAT traversal:** Enabled by default (AutoNAT + UPnP + Relay + DCUtR). Use `get_nat_status` to display connectivity status. Most desktop users are behind NAT — the protocol automatically handles relay fallback and hole-punching.
