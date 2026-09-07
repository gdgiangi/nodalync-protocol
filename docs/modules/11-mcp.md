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
| `--budget`, `-b` | 1.0 | Total session budget for query prices, in HBAR |
| `--auto-approve`, `-a` | 0.01 | Default maximum query price, in HBAR, including resource reads |

## MCP Tools

When the MCP server is running, AI agents have access to these tools:

| Tool | Description |
|------|-------------|
| `query_knowledge` | Query content by hash (paid) |
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

Returns the content directly. The price must be at or below the configured
auto-approve threshold and fit within the remaining session query budget.
Resource reads cannot provide an explicit per-query allowance; above the default
threshold, use `query_knowledge` with an authorized `budget_hbar` instead.

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

The server checks query prices before x402 payment processing, deposits, channel
funding, or retrieval:

1. **Default allowance**: If `query_knowledge` omits `budget_hbar`, the price must
   be at or below `--auto-approve`. The same limit applies to `knowledge://`
   resource reads.
2. **Explicit allowance**: `query_knowledge` can supply `budget_hbar` to replace
   that default for one call. The caller must already have authorization for
   this amount. A smaller explicit allowance is also enforced.
3. **Session ceiling**: Every query must fit within the remaining `--budget`,
   including calls with an explicit allowance. Budget reservation is atomic;
   a failed native query refunds the reserved query price. x402 queries reserve
   the content price before payment processing and release it only for a definite
   local or verification rejection. Uncertain settlement or network failures
   retain the reservation until the operator reconciles payment.
   A successful x402 payment remains counted if subsequent content delivery fails.
   Returning x402 payment requirements alone does not spend the budget.

With `--budget 1.0 --auto-approve 0.01`, content priced at 0.5 HBAR is rejected by
both an ordinary `query_knowledge` call and a resource read. After inspecting the
price with `preview_content` and obtaining authorization, a caller can request:

```json
{
  "query": "<content-hash>",
  "budget_hbar": 0.5
}
```

This succeeds only if at least 0.5 HBAR remains in the session query budget.
Startup limits and explicit query allowances must be finite, non-negative HBAR
amounts representable in tinybars. Setting both startup limits to zero allows
free content and rejects paid queries.

**These are query-price limits, not a wallet-wide spending policy.** The server
does not implement a human approval dialog: an agent can supply an explicit
allowance itself, so the host must enforce who may do that. Once a paid query is
allowed, existing automatic funding can deposit 10 HBAR into settlement and fund
a channel with 1 HBAR. Those funding amounts, x402 application fees, and network
fees are not deducted from the session query budget. The `deposit_hbar` and `open_channel` tools also
operate separately from it. Funding policy needs separate operator controls;
do not interpret `--budget` as a cap on all wallet movements.

## Error Handling

| Error | Cause | Resolution |
|-------|-------|------------|
| `QueryBudgetExceeded` | Query price > default or explicit allowance | Inspect the price; use cheaper content or an authorized explicit `budget_hbar` |
| `BudgetExceeded` | Query cost > remaining session budget | Use cheaper content or ask the operator to configure a larger session budget; a deposit does not change it |
| `ContentNotFound` | Hash doesn't exist locally | Ensure content is published |
| `StorageError` | Database issues | Check permissions, disk space |

## Testing

```bash
# Run MCP crate tests
cargo test -p nodalync-mcp

# Test server manually
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | ./target/release/nodalync mcp-server
```
