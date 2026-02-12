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

---

## Identity Commands

### `check_identity`
Check if a node identity exists (no password needed).
- **Args:** none
- **Returns:** `boolean`

### `init_node`
Create a new node identity (Ed25519 keypair, encrypted).
- **Args:** `{ password: string }`
- **Returns:** `{ peer_id: string, data_dir: string }`

### `unlock_node`
Decrypt an existing node identity.
- **Args:** `{ password: string }`
- **Returns:** `{ peer_id: string, data_dir: string }`

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
Retrieve full content with payment.
- **Args:** `{ hash: string, payment_amount?: number }` (price in NDL, e.g. 0.001)
- **Returns:** `{ hash, title, content_type, content_text?, content_size, price_paid, receipt_id }`
- **`content_text`** is `null` for binary content.

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

## Notes for Frontend

1. **Startup flow:** `check_identity` → if false: show onboarding → `init_node`; if true: show password → `unlock_node`
2. **After unlock:** `start_network_configured` (or `start_network` for quick start)
3. **Price values:** Frontend sends NDL (e.g. 0.001), backend converts to tinybars internally
4. **Hash format:** Always 64-char lowercase hex strings
5. **Error handling:** All commands return `Result<T, String>` — errors are human-readable strings
