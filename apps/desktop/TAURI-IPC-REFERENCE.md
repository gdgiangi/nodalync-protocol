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

### `get_network_health`
Get a snapshot of network health from the background health monitor.
- **Args:** none
- **Returns:**
  ```typescript
  {
    active: boolean,
    connected_peers: number,
    known_peers: number,           // Peers in persistent store
    uptime_secs: number,           // Since network start
    reconnect_attempts: number,    // Total reconnection attempts
    reconnect_successes: number,   // Successful reconnections
    last_check: string | null,     // ISO-8601 timestamp of last health check
    last_peer_save: string | null, // ISO-8601 timestamp of last peer save
    status: "healthy" | "degraded" | "connecting" | "disconnected" | "offline",
    message: string                // Human-readable status
  }
  ```
- **`status`** meanings:
  - `"healthy"` — 3+ peers connected, network is well-connected
  - `"degraded"` — 1-2 peers or no listen addresses
  - `"connecting"` — 0 peers, uptime < 60s (still bootstrapping)
  - `"disconnected"` — 0 peers, uptime > 60s (auto-reconnect in progress)
  - `"offline"` — network not started
- **Poll interval:** 10-30s from frontend. This reads a pre-computed snapshot — very cheap.
- **Background behavior:** The health monitor runs every 30s:
  - If connected peers drop below threshold, it tries reconnecting to known peers
  - Known peers are auto-saved to disk every 5 minutes
  - On network stop, all connected peers are saved before shutdown

---

## Seed Node Commands

### `get_seed_nodes`
Get all configured seed nodes (builtin + user-added).
- **Args:** none
- **Returns:** `SeedNodeInfo[]`
  ```typescript
  {
    peer_id: string,     // libp2p PeerId
    address: string,     // Multiaddr
    label: string | null,
    source: "builtin" | "user" | "dns" | "peer_exchange",
    enabled: boolean,
    added_at: string     // ISO-8601
  }
  ```

### `add_seed_node`
Add a seed node for network discovery. Used on next network start.
- **Args:** `{ peer_id: string, address: string, label?: string }`
- **Validation:** peer_id must be valid libp2p PeerId, address must be valid Multiaddr
- **Returns:** `SeedNodeInfo[]` (updated full list)

### `remove_seed_node`
Remove or disable a seed node. Builtin seeds are disabled, user seeds are deleted.
- **Args:** `{ peer_id: string }`
- **Returns:** `SeedNodeInfo[]` (updated full list)

---

## Peer Persistence Commands

### `save_known_peers`
Save currently connected peers to disk for reconnection on restart.
- **Args:** none
- **Returns:** `number` — count of peers saved
- Called automatically on network stop and app shutdown.

### `get_known_peers`
Get the list of known peers from the persistent store.
- **Args:** none
- **Returns:** `KnownPeerInfo[]`
  ```typescript
  {
    peer_id: string,
    addresses: string[],
    nodalync_id: string | null,
    last_seen: string,       // ISO-8601
    connection_count: number,
    manual: boolean
  }
  ```

### `add_known_peer`
Add a peer manually to the known peers store.
- **Args:** `{ peer_id: string, address: string }`
- **Returns:** `void`
- The peer will be used as a bootstrap node on next network start.

---

## Network Diagnostics

### `diagnose_network`
Analyze why the node can't find peers. Returns issues and suggestions.
- **Args:** none
- **Returns:**
  ```typescript
  {
    overall_status: "healthy" | "degraded" | "disconnected",
    network_active: boolean,
    connected_peers: number,
    known_peers: number,
    seed_nodes: number,      // Enabled seed count
    nat_status: string,
    issues: string[],        // What's wrong
    suggestions: string[]    // What to do
  }
  ```
- **Use case:** Show in network settings when connectivity problems occur. The `suggestions` array contains actionable steps.

---

## Channel Management Commands

Payment channels are required for querying paid content from peers. Without a channel, paid queries fail with `ChannelRequired`.

### `open_channel`
Open a payment channel with a peer.
- **Args:** `{ peer_id: string, deposit_hbar: number }`
- **`peer_id`** accepts libp2p format (`12D3KooW...`) or Nodalync format (`ndl1...` or hex)
- **`deposit_hbar`** minimum 1.0 HBAR
- **Returns:**
  ```typescript
  {
    channel: ChannelInfo,
    nodalync_peer_id: string  // Resolved Nodalync peer ID
  }
  ```
- **`ChannelInfo`:**
  ```typescript
  {
    channel_id: string,
    peer_id: string,              // Nodalync peer ID (ndl1...)
    libp2p_peer_id: string | null,
    state: string,                // "Opening" | "Open" | "Closing" | "Closed" | "Disputed"
    my_balance: number,           // tinybars
    their_balance: number,        // tinybars
    my_balance_hbar: number,      // HBAR (display-friendly)
    their_balance_hbar: number,   // HBAR (display-friendly)
    nonce: number,
    pending_payments: number,
    has_pending_close: boolean,
    has_pending_dispute: boolean,
    funding_tx_id: string | null, // On-chain transaction ID
    last_update: number           // Unix timestamp ms
  }
  ```

### `close_channel`
Cooperatively close a payment channel.
- **Args:** `{ peer_id: string }` (Nodalync peer ID)
- **Returns:**
  ```typescript
  {
    status: "closed" | "closed_offchain" | "peer_unresponsive" | "on_chain_failed",
    channel_id: string,
    my_final_balance: number,
    their_final_balance: number,
    transaction_id: string | null,
    message: string | null
  }
  ```
- If `status` is `"peer_unresponsive"`, show the `message` (suggests dispute flow).

### `list_channels`
List all payment channels.
- **Args:** none
- **Returns:**
  ```typescript
  {
    channels: ChannelInfo[],
    total: number,
    open_count: number,
    total_deposited: number,       // tinybars
    total_deposited_hbar: number   // HBAR
  }
  ```

### `get_channel`
Get details for a specific channel.
- **Args:** `{ peer_id: string }` (Nodalync peer ID)
- **Returns:** `ChannelInfo | null`

### `check_channel`
Check if an open channel exists with a peer. Accepts both libp2p and Nodalync peer IDs.
- **Args:** `{ peer_id: string }`
- **Returns:** `ChannelInfo | null` (null if no open channel)
- **Use case:** Before querying paid content, check if a channel exists. If null, prompt user to open one.

### `auto_open_and_query`
**Recommended for D3.** Auto-opens a channel if needed, then queries content. One-click paid content retrieval.
- **Args:** `{ hash: string, payment_amount?: number, deposit_hbar?: number }`
  - `hash`: 64-char hex content hash
  - `payment_amount`: price in NDL (e.g. 0.001), default 0
  - `deposit_hbar`: channel deposit if one needs to be opened, default 100.0 HBAR
- **Returns:**
  ```typescript
  {
    hash: string,
    title: string,
    content_type: string,
    content_text: string | null,
    content_size: number,
    price_paid: number,        // tinybars
    app_fee: number,           // tinybars
    total_cost: number,        // tinybars
    receipt_id: string,
    transaction_id: string | null,
    channel_opened: boolean,   // true if a new channel was opened
    channel_id: string | null  // channel ID if newly opened
  }
  ```
- **Flow:** Tries query → if ChannelRequired → opens channel with provider → retries query
- App fee is auto-recorded.

---

## Notes for Frontend

1. **Startup flow:** `check_identity` → if false: show onboarding → `init_node(password, name)`; if true: show password → `unlock_node`
2. **After unlock:** `get_identity` for profile display, then `auto_start_network` (recommended — loads seeds + known peers + mDNS + stable identity + spawns health monitor)
3. **Stable PeerId:** The network now derives its libp2p PeerId from the node's Nodalync identity. PeerId persists across restarts. Display it in the profile as the node's network address.
4. **Publish flow:** `publish_file`/`publish_text` → `extract_mentions(hash)` to populate L2 graph
5. **Query flow (simple):** Use `auto_open_and_query(hash, price)` — handles channel management automatically.
6. **Query flow (manual):** `get_fee_quote(price)` → `check_channel(provider_peer_id)` → if null: `open_channel(provider_peer_id, deposit)` → `query_content(hash, amount)` — fee is auto-recorded
7. **Fee dashboard:** `get_fee_config` for summary, `get_transaction_history` for details, `set_fee_rate` to configure
8. **Channel management:** `list_channels` for overview, `get_channel(peer_id)` for details, `close_channel(peer_id)` when done with a peer
9. **L3 synthesis:** Select entities in graph → `create_l3_summary(title, text, ids)` → new L3 node appears with `synthesizes` edges. List all with `get_l3_summaries`.
10. **Entity drill-down:** `get_entity_content_links(entity_id)` → shows which L0 content contributed to this entity. Combined with `get_subgraph`, this powers the full L0→L1→L2→L3 hierarchy view.
11. **Price values:** Frontend sends NDL (e.g. 0.001), backend converts to tinybars internally
12. **Hash format:** Always 64-char lowercase hex strings
13. **Error handling:** All commands return `Result<T, String>` — errors are human-readable strings
14. **NAT traversal:** Enabled by default (AutoNAT + UPnP + Relay + DCUtR). Use `get_nat_status` to display connectivity status. Most desktop users are behind NAT — the protocol automatically handles relay fallback and hole-punching.
15. **Network health:** Poll `get_network_health` every 10-30s for the network status indicator. The background health monitor (spawned by `auto_start_network`) handles auto-reconnect, peer saves, and health classification. `stop_network` automatically saves peers and shuts down the monitor.
16. **Peer handshake:** When a peer connects, the event loop automatically exchanges PeerInfo messages (protocol version, public key, capabilities). After handshake completes, `PeerInfo.handshake_complete` becomes `true` and message signature verification is enabled for that peer. No frontend action needed — this is fully automatic.
17. **Seed nodes:** `auto_start_network` loads seeds first (highest priority), then known peers, then mDNS. For first-run users with no known peers, seeds are the only way to discover the network. Use `get_seed_nodes` to display seed config, `add_seed_node` to let users add custom seeds.
18. **Network diagnostics:** If `get_network_health` shows "disconnected", call `diagnose_network` for actionable suggestions. Display `issues` + `suggestions` in a troubleshooting panel.
