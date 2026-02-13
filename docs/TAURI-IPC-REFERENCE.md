# Tauri IPC Command Reference

All commands are invoked via `invoke("command_name", { args })` from the React frontend.

## Identity & Node

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `check_identity` | — | `bool` | Check if a node identity exists (no password needed) |
| `init_node` | `name?, password` | `IdentityInfo` | Create a new node identity |
| `unlock_node` | `password` | `IdentityInfo` | Unlock existing node with password |
| `get_identity` | — | `IdentityInfo` | Get current identity info |
| `get_node_status` | — | `NodeStatus` | Get node status (initialized, peers, content count) |

## Content Import (L0 — Local Only)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `add_content` | `file_path, title?, description?` | `ImportResult` | Import a file as L0 content. Extracts L1 mentions. Does NOT publish to network. |
| `add_text_content` | `text, title, description?` | `ImportResult` | Import text as L0 content. Same as above but for text input. |

### ImportResult
```json
{
  "hash": "abc123...",
  "title": "My Document",
  "size": 4096,
  "content_type": "text/markdown",
  "mentions": 12
}
```

## Content Publishing (L0 → Network)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `publish_file` | `file_path, title?, description?, price?, visibility?` | `PublishResult` | Publish a file to the network (creates L0 + announces) |
| `publish_text` | `text, title, description?, price?, visibility?` | `PublishResult` | Publish text to the network |
| `list_content` | — | `ContentItem[]` | List all local content |
| `get_content_details` | `hash` | `ContentItem` | Get details for specific content |
| `delete_content` | `hash` | `string` | Delete content from local store |

### Visibility values
- `"private"` — local only
- `"unlisted"` — accessible but not discoverable
- `"shared"` / `"public"` — discoverable on network (default for publish)

## L1 Extraction

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `extract_mentions` | `content_hash` | `ExtractionResult` | Run L1 mention extraction on content, match to L2 entities |

### ExtractionResult
```json
{
  "content_hash": "abc123...",
  "total_mentions": 15,
  "entities_found": 8,
  "new_entities": 3,
  "entities": [
    {
      "entity_id": "e_42",
      "label": "Hedera",
      "entity_type": "Technology",
      "existing": true,
      "confidence": 0.95,
      "source_mention": "...Hedera consensus service..."
    }
  ]
}
```

## L2 Graph

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `get_graph_data` | — | `{ nodes, links }` | Full graph data for D3 force simulation |
| `get_subgraph` | `entity_id, depth?` | `{ nodes, links }` | Subgraph around a specific entity |
| `search_entities` | `query` | `GraphNode[]` | Search entities by name/type |
| `get_graph_stats` | — | `GraphStats` | Graph statistics (entity count, relationship count, etc.) |
| `get_context` | `entity_id` | `ContextResult` | Get full context for an entity (relationships, sources) |
| `get_entity_content_links` | `entity_id` | `ContentLink[]` | Get content linked to an entity |

## L3 Synthesis

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `create_l3_summary` | `entity_ids, title, summary_text` | `L3Summary` | Create an L3 summary linking multiple L2 entities |
| `get_l3_summaries` | — | `L3Summary[]` | List all L3 summaries |

## Content Discovery

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `search_network` | `query, max_results?` | `SearchResult[]` | Search connected peers for content (concurrent, relevance-scored) |
| `preview_content` | `hash` | `PreviewResult` | Get metadata preview of remote content |
| `query_content` | `hash, payment_amount?` | `QueryResult` | Retrieve full content with payment. Records transaction + app fee. |
| `unpublish_content` | `hash` | `string` | Remove content from network (keeps local copy) |
| `get_content_versions` | `hash` | `VersionInfo[]` | Get version history for content |

## Network

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `start_network` | `listen_port?` | `NetworkInfo` | Start network with default config |
| `start_network_configured` | `listen_port?, bootstrap_nodes?` | `NetworkInfo` | Start with custom config |
| `auto_start_network` | `listen_port?, identity_secret?` | `NetworkInfo` | Smart start: loads known peers, enables mDNS, starts health monitor |
| `stop_network` | — | `string` | Stop network (saves peers first) |
| `get_peers` | — | `string[]` | List connected peer IDs |
| `get_network_info` | — | `NetworkInfo` | Get network info (listen addrs, peer count) |
| `dial_peer` | `multiaddr` | `string` | Connect to a specific peer |
| `get_nat_status` | — | `NatStatus` | Get NAT traversal status (type, UPnP, relay) |
| `get_network_health` | — | `HealthStatus` | Get network health (healthy/degraded/connecting/disconnected/offline) |

## Peer Management

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `save_known_peers` | — | `string` | Save current peers to disk |
| `get_known_peers` | — | `PeerInfo[]` | List known peers from store |
| `add_known_peer` | `peer_id, address` | `string` | Manually add a peer |
| `reannounce_content` | — | `string` | Re-announce all content to network (useful after reconnect) |

## Fees & Transactions (D2)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `get_fee_config` | — | `FeeConfig` | Get current fee config (rate, min fee, max fee) |
| `set_fee_rate` | `rate` | `FeeConfig` | Set application fee rate (0.0–1.0) |
| `get_transaction_history` | `limit?, offset?` | `TransactionRecord[]` | Get transaction history with fee breakdown |
| `get_fee_quote` | `content_price` | `FeeQuote` | Get fee quote for a content price |

### FeeConfig
```json
{
  "rate": 0.05,
  "min_fee": 0,
  "max_fee": null,
  "fee_recipient": "nodalync-app"
}
```

### TransactionRecord
```json
{
  "id": "uuid",
  "content_hash": "abc123...",
  "content_title": "Research Paper",
  "content_cost": 50000000,
  "app_fee": 2500000,
  "total_cost": 52500000,
  "counterparty": "peer_id",
  "status": "Pending",
  "timestamp": "2026-02-13T12:00:00Z"
}
```

---

*Auto-generated from rust-src/ IPC commands. Last updated: 2026-02-13.*
