# Nodalync Quick Start

Get your node running and connected to the network in under 5 minutes.

## Installation

Choose one of three options:

### Option A: One-Line Install (Recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/gdgiangi/nodalync-protocol/main/install.sh | sh
```

This auto-detects your platform (macOS, Linux, Windows) and installs the latest binary with full Hedera settlement support.

### Option B: Docker

```bash
# Pull or build the image
docker build -t nodalync:latest https://github.com/gdgiangi/nodalync-protocol.git

# Initialize your identity
docker run -it \
  -e NODALYNC_PASSWORD=your-secure-password \
  -v ~/.nodalync:/home/nodalync/.nodalync \
  nodalync:latest init --wizard

# Start your node
docker run -d --name nodalync-node \
  -e NODALYNC_PASSWORD=your-secure-password \
  -v ~/.nodalync:/home/nodalync/.nodalync \
  -p 9000:9000 \
  nodalync:latest start
```

### Option C: Build from Source

Requires Rust 1.85+:

```bash
# Clone the repo
git clone https://github.com/gdgiangi/nodalync-protocol.git
cd nodalync-protocol

# Build release binary with Hedera support (default, requires protoc)
cargo build --release -p nodalync-cli

# Or build without Hedera support (smaller binary)
cargo build --release -p nodalync-cli --no-default-features

# Install to /usr/local/bin
sudo cp target/release/nodalync /usr/local/bin/

# Or add to PATH
export PATH="$PWD/target/release:$PATH"
```

Pre-built binaries also available at [Releases](https://github.com/gdgiangi/nodalync-protocol/releases).

---

## Step 1: Initialize Your Identity

Run the interactive wizard to set up your node:

```bash
nodalync init --wizard
```

This will:
- Generate an Ed25519 keypair (your identity)
- Configure network settings (connects to bootstrap node by default)
- Set your default pricing
- Choose settlement mode (testnet for free testing)

You'll be prompted for a password to encrypt your keypair. You can also set it via environment variable:

```bash
export NODALYNC_PASSWORD=your-secure-password
nodalync init --wizard
```

Check your identity:

```bash
nodalync whoami
```

---

## Step 2: Start Your Node

**Foreground mode** (see logs, Ctrl+C to stop):

```bash
nodalync start
```

**Background mode** (daemon):

```bash
nodalync start --daemon
nodalync status    # Check status
nodalync stop      # Stop the node
```

Your node will automatically:
- Connect to the bootstrap node
- Discover other peers via DHT
- Start serving your published content

---

## Step 3: Publish Content

Share knowledge on the network:

```bash
# Publish a file with default settings
nodalync publish my-research.md

# Publish with custom price and metadata
nodalync publish my-research.md \
  --price 0.01 \
  --title "My Research Paper" \
  --visibility shared
```

**Visibility levels:**
- `private` - Local only, never shared
- `unlisted` - Available if someone knows the hash
- `shared` - Announced to network (default)

List your published content:

```bash
nodalync list
```

---

## Step 4: Search & Query Content

**Search the network:**

```bash
# Search local content
nodalync search "climate change"

# Search entire network
nodalync search "climate change" --all
```

**Preview content** (free, shows metadata only):

```bash
nodalync preview <content-hash>
```

**Query full content** (paid):

```bash
nodalync query <content-hash>

# Save to file
nodalync query <content-hash> --output result.txt
```

---

## Step 5: Check Your Earnings

When others query your content, you earn HBAR:

```bash
# View balance
nodalync balance

# View earnings breakdown
nodalync earnings

# Force settlement (batch payments on-chain)
nodalync settle
```

---

## Claude / MCP Integration

Connect Claude to your Nodalync node for AI-powered knowledge queries.

### Start the MCP Server

**Basic (local content only):**
```bash
nodalync mcp-server \
  --budget 1.0 \
  --auto-approve 0.01
```

**With network search:**
```bash
nodalync mcp-server \
  --budget 1.0 \
  --auto-approve 0.01 \
  --enable-network
```

**With Hedera settlement (testnet):**
```bash
nodalync mcp-server \
  --budget 1.0 \
  --auto-approve 0.01 \
  --enable-network \
  --hedera-account-id 0.0.XXXXX \
  --hedera-private-key ~/.nodalync/hedera.key \
  --hedera-contract-id 0.0.7729011 \
  --hedera-network testnet
```

Options:
- `--budget` - Maximum HBAR for this session (default: 1.0)
- `--auto-approve` - Auto-approve queries below this price (default: 0.01)
- `--enable-network` - Search network peers, not just local content
- `--hedera-account-id` - Your Hedera account ID for settlement
- `--hedera-private-key` - Path to your Hedera private key file
- `--hedera-contract-id` - Settlement contract ID (default: 0.0.7729011)
- `--hedera-network` - Network to use: testnet, mainnet, or previewnet

### Configure Claude Desktop

Add to your Claude Desktop config (`~/.config/claude/mcp.json` or similar):

```json
{
  "mcpServers": {
    "nodalync": {
      "command": "nodalync",
      "args": ["mcp-server", "--budget", "1.0", "--auto-approve", "0.01", "--enable-network"],
      "env": {
        "NODALYNC_PASSWORD": "your-secure-password",
        "HEDERA_ACCOUNT_ID": "0.0.7703962",
        "HEDERA_CONTRACT_ID": "0.0.7729011",
        "HEDERA_PRIVATE_KEY": "3030020100300706052b8104000a04220420..."
      }
    }
  }
}
```

**Note**: The private key must be DER-encoded ECDSA format (98 hex characters starting with `303002...`).

### MCP Tools

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

---

## Docker Compose (Multi-Node Cluster)

For testing or running multiple nodes:

```bash
cd infra/local/
docker compose up -d

# View logs
docker compose logs -f

# Stop
docker compose down
```

---

## Environment Variables

The easiest way to configure Hedera is to use the `.env` file in the repo root:

```bash
# Export all variables from .env
set -a && source .env && set +a
```

| Variable | Description |
|----------|-------------|
| `NODALYNC_PASSWORD` | Identity encryption password |
| `NODALYNC_DATA_DIR` | Data directory (default: `~/.nodalync`) |
| `RUST_LOG` | Log level (e.g., `nodalync=debug`) |
| `HEDERA_ACCOUNT_ID` | Hedera account ID (e.g., `0.0.7703962`) |
| `HEDERA_CONTRACT_ID` | Settlement contract ID (default: `0.0.7729011`) |
| `HEDERA_PRIVATE_KEY` | **DER-encoded ECDSA private key** (see note below) |

### Hedera Private Key Format

**IMPORTANT**: Smart contract operations require ECDSA keys with DER encoding.

| Format | Length | Example Prefix | Works? |
|--------|--------|----------------|--------|
| DER-encoded ECDSA | 98 hex chars | `3030020100300706052b8104000a04220420...` | **Yes** |
| Raw hex (Ed25519) | 64 hex chars | `d21f3bfe69929b1d6e0f37fa9622b96f...` | No |

If you have a raw hex key, you need to DER-encode it. Check your account type at HashScan:
`https://hashscan.io/testnet/account/<account_id>`

---

## Common Commands Reference

### Identity & Node

| Command | Description |
|---------|-------------|
| `nodalync init --wizard` | Set up identity and config |
| `nodalync whoami` | Show your identity |
| `nodalync start` | Start node (foreground) |
| `nodalync start --daemon` | Start node (background) |
| `nodalync status` | Show node status |
| `nodalync stop` | Stop daemon |
| `nodalync completions <shell>` | Generate shell completions (bash, zsh, fish, powershell) |

### Content

| Command | Description |
|---------|-------------|
| `nodalync publish <file>` | Publish content |
| `nodalync update <hash> <file>` | Create a new version of content |
| `nodalync delete <hash>` | Delete local content |
| `nodalync visibility <hash> --level <level>` | Change content visibility |
| `nodalync versions <hash>` | Show version history |
| `nodalync list` | List your content |
| `nodalync search <query>` | Search content |
| `nodalync preview <hash>` | View metadata (free) |
| `nodalync query <hash>` | Get full content (paid) |

### Synthesis

| Command | Description |
|---------|-------------|
| `nodalync synthesize --sources <h1>,<h2> --output <file>` | Create L3 synthesis |
| `nodalync build-l2 <hash1> <hash2> ...` | Build L2 entity graph from L1 sources |
| `nodalync merge-l2 <graph1> <graph2> ...` | Merge L2 entity graphs |
| `nodalync reference <hash>` | Reference external L3 as L0 source |

### Economics & Channels

| Command | Description |
|---------|-------------|
| `nodalync balance` | Check HBAR balance |
| `nodalync earnings` | View earnings breakdown |
| `nodalync deposit <amount>` | Deposit HBAR to protocol balance |
| `nodalync withdraw <amount>` | Withdraw HBAR from protocol balance |
| `nodalync settle` | Force settlement of pending payments |
| `nodalync open-channel <peer-id> --deposit <amount>` | Open payment channel (min 100 HBAR) |
| `nodalync close-channel <peer-id>` | Close payment channel (cooperative) |
| `nodalync dispute-channel <peer-id>` | Initiate dispute close (24h waiting period) |
| `nodalync resolve-dispute <peer-id>` | Resolve dispute after waiting period |
| `nodalync list-channels` | List all payment channels |

### MCP

| Command | Description |
|---------|-------------|
| `nodalync mcp-server` | Start MCP server for AI agents |

---

## Bootstrap Node

Your node connects to this bootstrap node by default:

```
/dns4/nodalync-bootstrap.eastus.azurecontainer.io/tcp/9000/p2p/12D3KooWMqrUmZm4e1BJTRMWqKHCe1TSX9Vu83uJLEyCGr2dUjYm
```

Health check: http://nodalync-bootstrap.eastus.azurecontainer.io:8080/health

---

## Troubleshooting

**Node won't start:**
```bash
# Check if already running
nodalync status

# View logs
cat ~/.nodalync/node.stderr.log
```

**Can't connect to peers:**
```bash
# Verify bootstrap node is reachable
curl http://nodalync-bootstrap.eastus.azurecontainer.io:8080/health

# Check your firewall allows TCP 9000
```

**Reset everything:**
```bash
rm -rf ~/.nodalync
nodalync init --wizard
```

---

## Next Steps

- Read the [Protocol Spec](./spec.md) to understand how Nodalync works
- Explore the [Architecture](./architecture.md) for module details
- Check the [FAQ](./FAQ.md) for common questions
- Join the [Discord](https://discord.gg/hYVrEAM6) community!
