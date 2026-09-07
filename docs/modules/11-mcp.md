# Module 11: MCP Server

The `nodalync-mcp` crate provides an MCP (Model Context Protocol) server that enables AI assistants like Claude to query knowledge from a local Nodalync node.

## Quick Start

### 1. Build the CLI

```bash
cargo build --release -p nodalync-cli
```

### 2. Initialize a Node

```bash
./target/release/nodalync init
```

### 3. Configure Claude Desktop

Add to your Claude Desktop MCP config (typically `~/.config/claude/mcp.json` on macOS/Linux):

```json
{
  "mcpServers": {
    "nodalync": {
      "command": "/path/to/nodalync",
      "args": ["mcp-server", "--budget", "1.0", "--auto-approve", "0.01"]
    }
  }
}
```

### 4. Restart Claude Desktop

Quit and reopen Claude Desktop to load the MCP server.

## CLI Usage

```bash
# Start MCP server with defaults (1 HBAR budget, 0.01 auto-approve)
nodalync mcp-server

# Custom budget and auto-approve threshold
nodalync mcp-server --budget 5.0 --auto-approve 0.1
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--budget`, `-b` | 1.0 | Total session budget in HBAR |
| `--auto-approve`, `-a` | 0.01 | Auto-approve queries under this HBAR amount |

## MCP Tools

When the MCP server is running, AI agents have access to these tools:

| Tool | Description |
|------|-------------|
| `query_knowledge` | Query content by hash or natural language (paid) |
| `list_sources` | Browse available content with metadata |
| `search_network` | Search connected peers for content (requires `--enable-network`) |
| `preview_content` | View content metadata without paying |
| `publish_content` | Publish new content from the agent |
| `synthesize_content` | Create L3 synthesis from multiple sources |
| `update_content` | Create a new version of existing content |
| `delete_content` | Delete content and set visibility to offline |
| `set_visibility` | Change content visibility |
| `list_versions` | List all versions of a content item |
| `get_earnings` | View earnings breakdown by content |
| `status` | Node health, budget, channels, and Hedera status |
| `deposit_hbar` | Deposit HBAR to the settlement contract |
| `open_channel` | Open a payment channel with a peer |
| `close_channel` | Close a payment channel |
| `close_all_channels` | Close all open payment channels |

> **Note:** Natural language queries are not yet supported for `query_knowledge`. Use `list_sources` or `search_network` to discover content hashes first.

## MCP Resources

### `knowledge://{hash}`

Direct content access by hash. Use `list_sources` to discover available hashes.

**URI Format:** `knowledge://<base58-encoded-hash>`

**Example:**
```
knowledge://5dY7Kx9mT2...
```

Returns the content directly. Payment is handled automatically from session budget.

## Architecture

```
┌──────────────┐     stdio      ┌─────────────────┐
│ Claude       │ ◄────────────► │ nodalync        │
│ Desktop      │     MCP        │ mcp-server      │
└──────────────┘                └────────┬────────┘
                                         │
                        ┌────────────────┼────────────────┐
                        │                │                │
                        ▼                ▼                ▼
                ┌─────────────┐  ┌─────────────┐  ┌─────────────┐
                │ nodalync-   │  │ nodalync-   │  │ Event Loop  │
                │ store       │  │ net         │  │ (background)│
                │ (local)     │  │ (P2P)       │  │             │
                └─────────────┘  └─────────────┘  └─────────────┘
```

### Event Processing

When `--enable-network` is used, the MCP server spawns a background event loop that processes incoming network events (e.g., `ChannelAccept` messages). This enables full payment channel lifecycle support:

1. **Channel Open**: Server sends `ChannelOpen` to peer
2. **Event Loop**: Receives `ChannelAccept` from peer
3. **State Transition**: Channel moves from `Opening` → `Open`
4. **Payments**: Channel is ready for micropayments

## Budget System

The budget system prevents runaway spending:

1. **Session Budget**: Total HBAR available for the session
2. **Auto-Approve Threshold**: Queries below this cost are approved automatically
3. **Atomic Tracking**: Thread-safe spending with `compare_exchange`

```rust
// Budget is tracked atomically
pub fn try_spend(&self, amount: Amount) -> Result<Amount, McpError> {
    // Atomic compare-and-swap ensures thread safety
}
```

## Error Handling

| Error | Cause | Resolution |
|-------|-------|------------|
| `BudgetExceeded` | Query cost > remaining budget | Increase budget or use smaller queries |
| `ContentNotFound` | Hash doesn't exist locally | Ensure content is published |
| `StorageError` | Database issues | Check permissions, disk space |

## Testing

```bash
# Run MCP crate tests
cargo test -p nodalync-mcp

# Test server manually
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | ./target/release/nodalync mcp-server
```
