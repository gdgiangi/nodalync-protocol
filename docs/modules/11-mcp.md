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

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

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

### `query_knowledge`

Query knowledge from the network and pay for access.

**Input:**
```json
{
  "query": "string (base58-encoded content hash)",
  "budget_hbar": "number (optional, max to spend on this query)"
}
```

> **Note:** Natural language queries are not yet supported. Use `list_sources` to discover available content hashes.

**Output:**
```json
{
  "content": "string (the knowledge content)",
  "hash": "string (content hash)",
  "sources": ["array of source hashes (L0 roots)"],
  "provenance": ["array of all contributing hashes"],
  "cost_hbar": "number (amount spent)",
  "remaining_budget_hbar": "number (session budget remaining)"
}
```

### `list_sources`

List available knowledge sources on the network.

**Input:**
```json
{
  "topic": "string (optional filter)",
  "limit": "number (optional, default 10)"
}
```

**Output:**
```json
{
  "sources": [
    {
      "hash": "string",
      "title": "string",
      "price_hbar": "number",
      "preview": "string",
      "topics": ["array of strings"]
    }
  ],
  "total_available": "number"
}
```

### `budget_status`

Check remaining session budget.

**Output:**
```json
{
  "total_hbar": 1.0,
  "spent_hbar": 0.05,
  "remaining_hbar": 0.95,
  "auto_approve_hbar": 0.01
}
```

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
