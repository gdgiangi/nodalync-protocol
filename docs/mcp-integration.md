# Nodalync MCP Server — Technical Design & Integration Guide

**Version:** 0.10.1  
**Date:** 2026-02-13  
**Status:** Implemented & Tested (59 tests passing)

## Overview

The Nodalync MCP (Model Context Protocol) server enables AI agents to query knowledge from the Nodalync network with automatic attribution and payment. It implements the [MCP specification](https://modelcontextprotocol.io/) via the `rmcp` SDK, exposing Nodalync's content discovery, querying, publishing, and payment capabilities as standard MCP tools.

**Key value proposition:** AI agents currently use knowledge without attribution or compensation. Nodalync MCP bridges this gap — agents search, query, and pay for content through a standard protocol, and creators earn automatically.

## Architecture

```
┌──────────────────────────────────────────────────────┐
│  AI Agent (Claude Desktop / OpenClaw / LangChain)    │
│                                                       │
│  1. search_network("quantum computing")               │
│  2. query_knowledge(hash) → content + provenance      │
│  3. Automatic: channel open → payment → settlement    │
└────────────────────┬─────────────────────────────────┘
                     │ MCP (stdio/SSE)
┌────────────────────▼─────────────────────────────────┐
│  nodalync-mcp (Rust)                                  │
│                                                       │
│  ┌─────────┐ ┌──────────┐ ┌────────────┐            │
│  │ Budget   │ │ Tool     │ │ Session    │            │
│  │ Tracker  │ │ Router   │ │ Cache      │            │
│  └────┬────┘ └─────┬────┘ └─────┬──────┘            │
│       │            │             │                    │
│  ┌────▼────────────▼─────────────▼──────┐            │
│  │  DefaultNodeOperations               │            │
│  │  (nodalync-ops)                      │            │
│  └──┬──────────┬───────────┬────────────┘            │
│     │          │           │                          │
│  ┌──▼──┐  ┌───▼───┐  ┌───▼────┐  ┌──────────┐      │
│  │Store │  │Network│  │Settle  │  │ Crypto   │      │
│  │(sled)│  │(libp2p│  │(Hedera)│  │(ed25519) │      │
│  └──────┘  └───────┘  └────────┘  └──────────┘      │
└──────────────────────────────────────────────────────┘
```

### Components

| Component | Crate | Role |
|-----------|-------|------|
| MCP Server | `nodalync-mcp` | MCP protocol handling, tool routing, budget management |
| Operations | `nodalync-ops` | Content operations, payment channels, event handling |
| Network | `nodalync-net` | libp2p networking, peer discovery (mDNS + Kademlia), content announcement |
| Store | `nodalync-store` | Persistent storage (sled), manifests, channels, announcements |
| Settlement | `nodalync-settle` | Hedera smart contract integration, on-chain settlement |
| Crypto | `nodalync-crypto` | Ed25519 identity, content hashing, payment signatures |
| Types | `nodalync-types` | Shared types: ContentType, Visibility, L0/L1/L2/L3 hierarchy |

## MCP Tools (16 tools)

### Discovery
| Tool | Description |
|------|-------------|
| `search_network` | Search for knowledge across local store + network peers. Returns hashes, titles, prices, previews, topics. |
| `list_sources` | Browse available content with optional topic filter. Supports network-wide or local-only. |

### Query & Payment
| Tool | Description |
|------|-------------|
| `query_knowledge` | Retrieve content by hash. **Fully automated payment**: auto-deposits HBAR, auto-opens channels, returns provenance chain + transaction receipts. |
| `preview_content` | Preview content metadata and price before committing to query. |

### Publishing
| Tool | Description |
|------|-------------|
| `publish_content` | Publish content (L0/L3) to the network with pricing. Auto-extracts L1 mentions, builds provenance. |
| `update_content` | Update existing content (creates new version, preserves provenance chain). |
| `delete_content` | Remove content from local store and network. |
| `set_visibility` | Toggle content visibility (Private/Shared/Public). |
| `list_versions` | View version history of content. |

### Synthesis
| Tool | Description |
|------|-------------|
| `synthesize_content` | Create L3 synthesis from multiple source contents. Builds cross-reference provenance. |

### Payments & Channels
| Tool | Description |
|------|-------------|
| `status` | Comprehensive node status: network, budget, channels, Hedera balance. |
| `deposit_hbar` | Deposit HBAR to settlement contract for payment operations. |
| `open_channel` | Open payment channel with a peer (minimum 100 HBAR deposit). |
| `close_channel` | Close channel and settle on-chain. Auto-disputes if peer unresponsive. |
| `close_all_channels` | Graceful shutdown: close all channels, settle or dispute as needed. |
| `get_earnings` | View earnings from content you've published (per-content and total). |

## Payment Flow

The MCP server abstracts all payment complexity from the AI agent:

```
Agent calls query_knowledge(hash)
  │
  ├─ Check session budget (client-side limit)
  │
  ├─ Preview content → get price + provider
  │
  ├─ Auto-deposit check
  │   └─ If settlement contract balance < required → deposit 10 HBAR
  │
  ├─ Auto-open channel
  │   └─ If no channel with provider → open with 1 HBAR deposit
  │
  ├─ Execute query → off-chain micropayment via payment channel
  │
  ├─ Return: content + provenance + cost + payment details
  │
  └─ Background: settle channels every 5 minutes when threshold reached
```

**Budget tracking** is session-scoped:
- Set at startup (`--budget 1.0` HBAR)
- Auto-approve threshold for unattended queries (`--auto-approve 0.01`)
- Refunds on failed queries

## Integration Guide

### Claude Desktop

Add to your Claude Desktop MCP configuration (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "nodalync": {
      "command": "nodalync",
      "args": ["mcp-server", "--budget", "1.0", "--enable-network"],
      "env": {
        "RUST_LOG": "nodalync=info"
      }
    }
  }
}
```

**With Hedera testnet settlement:**

```json
{
  "mcpServers": {
    "nodalync": {
      "command": "nodalync",
      "args": [
        "mcp-server",
        "--budget", "5.0",
        "--enable-network",
        "--hedera-account-id", "0.0.YOUR_ACCOUNT",
        "--hedera-private-key", "/path/to/hedera-key.pem",
        "--hedera-network", "testnet"
      ]
    }
  }
}
```

Claude can then:
1. `search_network("quantum computing")` → discover content
2. `query_knowledge("3vR8x...")` → retrieve + pay automatically
3. `get_earnings()` → check what you've earned from your published content
4. `status()` → see network connectivity, budget, Hedera balance

### OpenClaw (Native MCP)

OpenClaw supports MCP servers natively. Add to your OpenClaw config:

```yaml
mcp:
  servers:
    nodalync:
      command: nodalync
      args: ["mcp-server", "--budget", "1.0", "--enable-network"]
```

### LangChain / LangGraph

Use the MCP-to-LangChain adapter:

```python
from langchain_mcp import MCPToolkit

# Connect to Nodalync MCP server via stdio
toolkit = MCPToolkit(
    command="nodalync",
    args=["mcp-server", "--budget", "1.0", "--enable-network"]
)
tools = toolkit.get_tools()

# Use in an agent
from langchain.agents import create_tool_calling_agent
agent = create_tool_calling_agent(llm, tools, prompt)
```

### CrewAI (via Composio)

```python
from composio_crewai import ComposioToolSet

toolset = ComposioToolSet()
# Register Nodalync MCP server
toolset.register_mcp_server(
    name="nodalync",
    command="nodalync",
    args=["mcp-server", "--budget", "1.0"]
)
tools = toolset.get_tools()
```

### Any MCP Client (stdio transport)

The server communicates via stdin/stdout using the MCP JSON-RPC protocol:

```bash
# Start the server (it reads JSON-RPC from stdin, writes to stdout)
nodalync mcp-server --budget 1.0 --enable-network

# Or with full Hedera integration
nodalync mcp-server \
  --budget 5.0 \
  --enable-network \
  --hedera-account-id 0.0.7703962 \
  --hedera-private-key ~/.nodalync/hedera-key.pem \
  --hedera-network testnet
```

## Authentication & Identity

- **Node identity:** Ed25519 keypair generated on first run, stored in `~/.nodalync/identity.key`
- **Peer ID:** Derived from public key (20-byte hash, base58-encoded with "ndl" prefix)
- **Hedera identity:** Separate Hedera account for on-chain settlement (optional)
- **MCP auth:** Currently stdio-based (inherits OS process permissions). SSE transport with API keys planned for v0.11.

## Configuration

| Flag | Default | Description |
|------|---------|-------------|
| `--budget` | 1.0 | Session budget in HBAR |
| `--auto-approve` | 0.01 | Auto-approve threshold per query (HBAR) |
| `--enable-network` | false | Connect to libp2p network for live peer search |
| `--hedera-account-id` | — | Hedera account ID for on-chain settlement |
| `--hedera-private-key` | — | Path to Hedera Ed25519 private key |
| `--hedera-contract-id` | 0.0.7729011 | Settlement smart contract ID |
| `--hedera-network` | testnet | Hedera network (testnet/mainnet) |

## Background Processes

The MCP server runs several background tasks:

1. **Network event processor** — handles incoming content announcements, query requests, payment channel messages
2. **Announcement cleanup** — purges announcements older than 7 days (runs hourly)
3. **Settlement batch** — settles payment channels that exceed threshold (100 HBAR) or time limit (1 hour) (runs every 5 minutes)

## Graceful Shutdown

On server exit, `shutdown()` is called automatically:
- Cooperatively closes all open payment channels (3-second timeout per peer)
- If peer is unresponsive, initiates on-chain dispute (24-hour settlement period)
- Logs summary: channels closed, disputed, and failed

## Content Hierarchy

| Level | Type | Description | MCP Tool |
|-------|------|-------------|----------|
| L0 | Raw | Original documents, articles, data | `publish_content` |
| L1 | Extracted | Mentions, entities, key facts (auto-extracted from L0) | Auto-generated |
| L2 | Graph | Entity relationships, knowledge graph connections | Auto-generated |
| L3 | Synthesized | Cross-content summaries, insights | `synthesize_content` |

## Error Handling

All errors return structured JSON with:
- `error`: Human-readable error type
- `code`: Machine-readable error code
- `message`: Detailed description
- `suggestion`: Recovery action for the AI agent

Error categories: `CONTENT_NOT_FOUND`, `BUDGET_EXCEEDED`, `NETWORK_ERROR`, `INVALID_HASH`, `PAYMENT_FAILED`, `HEDERA_ERROR`, `INTERNAL_ERROR`

## Testing

```bash
# Run all MCP server tests (59 tests)
cargo test -p nodalync-mcp --no-default-features

# With Hedera integration tests (requires protoc)
cargo test -p nodalync-mcp
```

## Roadmap

- **v0.11:** SSE transport (remote MCP connections), API key auth
- **v0.12:** Streaming responses for large content, batch queries
- **v0.13:** MCP Resources (knowledge://{hash} direct access)
- **v0.14:** MCP Prompts (pre-built research workflows)
