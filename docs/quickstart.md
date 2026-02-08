# Nodalync Quick Start

Get your node running and connected to the network in under 5 minutes.

## Installation

Choose one of three options:

### Option A: One-Line Install (Recommended)

**macOS / Linux:**
```bash
curl -fsSL https://raw.githubusercontent.com/gdgiangi/nodalync-protocol/main/install.sh | sh
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/gdgiangi/nodalync-protocol/main/install.ps1 | iex
```

Or download the latest `.exe` from [Releases](https://github.com/gdgiangi/nodalync-protocol/releases) and add it to your PATH.

This auto-detects your platform and installs the latest binary with full Hedera settlement support.

### Option B: Docker

```bash
# Pull or build the image
docker build -t nodalync:latest https://github.com/gdgiangi/nodalync-protocol.git

# Initialize your identity
docker run -it \
  -e NODALYNC_PASSWORD=your-secure-password \
  -v ~/.nodalync:/home/nodalync/.nodalync \
  nodalync:latest init

# Start your node
docker run -d --name nodalync-node \
  -e NODALYNC_PASSWORD=your-secure-password \
  -v ~/.nodalync:/home/nodalync/.nodalync \
  -p 9000:9000 \
  nodalync:latest start
```

### Option C: Build from Source

Requires Rust 1.85+ (and `protoc` for Hedera support):

```bash
# Clone the repo
git clone https://github.com/gdgiangi/nodalync-protocol.git
cd nodalync-protocol

# Build release binary with Hedera support (default, requires protoc)
cargo build --release -p nodalync-cli

# Or build without Hedera support (smaller binary)
cargo build --release -p nodalync-cli --no-default-features

# Add to PATH (no sudo needed)
export PATH="$PWD/target/release:$PATH"

# Or install system-wide
sudo cp target/release/nodalync /usr/local/bin/
```

Pre-built binaries also available at [Releases](https://github.com/gdgiangi/nodalync-protocol/releases).

---

## Step 1: Initialize Your Identity

Set a password and initialize your node identity:

```bash
export NODALYNC_PASSWORD=your-secure-password
nodalync init
```

This will:
- Generate an Ed25519 keypair (your identity)
- Create a default configuration file (connects to bootstrap nodes automatically)
- Set up local storage (SQLite database, content directory)

> **Note:** `init` fails if an identity already exists. To reinitialize, delete your data directory first (see Troubleshooting below for paths) or use `nodalync init --wizard` in an interactive terminal to auto-reinitialize.

For an interactive experience that lets you configure network settings, pricing, and settlement mode step by step, use the wizard:

```bash
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

Search matches content titles, descriptions, and tags (not body text):

```bash
# Search local content by title/description/tags
nodalync search "research"

# Search entire network
nodalync search "research" --all
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
        "NODALYNC_HEDERA_ACCOUNT_ID": "0.0.7703962",
        "NODALYNC_HEDERA_CONTRACT_ID": "0.0.7729011",
        "NODALYNC_HEDERA_KEY_PATH": "/Users/you/.nodalync/hedera.key"
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

## Local Multi-Node Testing

Test the full publish-query-payment flow across three local nodes using Docker.

**Prerequisites:** Docker, Docker Compose, and `jq` (for `make test`) installed.

### Quick Version

```bash
# 1. Build the Docker image first (from repo root — this must complete before init)
docker compose build

# 2. Initialize node identities and configs (uses the image you just built)
cd infra/local && make init

# 3. Start the 3-node cluster
make up

# 4. Run the end-to-end test (publish on node1, query from node3)
make test
```

**Important:** Step 1 (`docker compose build`) must complete before Step 2 (`make init`),
because `make init` runs `docker run` to generate identities using the built image.

### What This Creates

| Container | Role | Host Port | Internal IP |
|-----------|------|-----------|-------------|
| `nodalync-node1` | Bootstrap / seed node | 9001, 8081 | 172.28.0.10 |
| `nodalync-node2` | Alice (publisher) | 9002, 8082 | 172.28.0.11 |
| `nodalync-node3` | Bob (querier) | 9003, 8083 | 172.28.0.12 |

All nodes use the password `testpassword` and form a full-mesh via libp2p.

### Manual Interaction

```bash
# Run any CLI command on a specific node
docker exec -e NODALYNC_PASSWORD=testpassword nodalync-node1 nodalync status
docker exec -e NODALYNC_PASSWORD=testpassword nodalync-node2 nodalync list

# Open a shell inside a node
cd infra/local && make shell-node1

# Publish test content on node1
make publish-test

# View logs
make logs

# Stop the cluster
make down

# Full reset (remove data + reinitialize)
make reset
```

### Available Makefile Targets

Run `cd infra/local && make help` to see all targets:

| Target | Description |
|--------|-------------|
| `make init` | Generate node identities and configs (required first) |
| `make up` | Start the 3-node cluster |
| `make down` | Stop the cluster |
| `make logs` | Follow cluster logs |
| `make status` | Show cluster status and peer IDs |
| `make test` | Run E2E tests (publish, propagate, query) |
| `make clean` | Remove containers, volumes, and generated configs |
| `make reset` | Clean + init (fresh start) |
| `make shell-node1` | Open shell in node1 (also node2, node3) |
| `make publish-test` | Publish test content on node1 |

### Two Docker Compose Files

There are two `docker-compose.yml` files in this repo, each for a different purpose:

| File | Location | Used by | Service names | When to use |
|------|----------|---------|---------------|-------------|
| `docker-compose.yml` | Repo root | `docker compose build` | `node-bootstrap`, `node-alice`, `node-bob` | Building the image and custom setups |
| `docker-compose.yml` | `infra/local/` | `make up/down/logs` | `node1`, `node2`, `node3` | Standard 3-node testing via Makefile |

**Typical workflow:** Run `docker compose build` from the repo root to build the image, then use `cd infra/local && make init && make up` for the standard 3-node cluster. The Makefile targets use the `infra/local/docker-compose.yml` internally.

The root `docker-compose.yml` is useful if you want to customize the cluster (add more nodes, change ports, or integrate with other services). It references `infra/local/` for data and configs, so you still need `make init` first.

**Warning:** Do not run both compose files at the same time. They use overlapping container names and ports, so running one while the other is active will cause conflicts. Use `make down` (or `docker compose down` from the root) to stop one before starting the other.

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
| `NODALYNC_DATA_DIR` | Data directory (default: platform-specific, see note below) |
| `RUST_LOG` | Log level (e.g., `nodalync=debug`) |
| `HEDERA_ACCOUNT_ID` | Hedera account ID (e.g., `0.0.7703962`) |
| `HEDERA_CONTRACT_ID` | Settlement contract ID (default: `0.0.7729011`) |
| `HEDERA_PRIVATE_KEY` | **DER-encoded ECDSA private key** as inline hex string (see note below) |

**Note:** The variables above (`HEDERA_*`) are read by the CLI settlement path. The MCP server subcommand reads `NODALYNC_HEDERA_*` prefixed variants (e.g., `NODALYNC_HEDERA_ACCOUNT_ID`). See `nodalync mcp-server --help`.

### Hedera Private Key Format

**IMPORTANT**: `HEDERA_PRIVATE_KEY` is an inline hex string (not a file path). Smart contract operations require ECDSA keys with DER encoding.

| Format | Length | Example Prefix | Works? |
|--------|--------|----------------|--------|
| DER-encoded ECDSA | 98 hex chars | `3030020100300706052b8104000a04220420...` | **Yes** |
| Raw hex (Ed25519) | 64 hex chars | `d21f3bfe69929b1d6e0f37fa9622b96f...` | No |

If you have a raw hex key, you need to DER-encode it. Check your account type at HashScan:
`https://hashscan.io/testnet/account/<account_id>`

---

## Auto-Deposit (Payment Channels)

When running a node that serves paid content, you need HBAR deposited to the settlement contract before you can accept payment channels from other peers.

> **MIGRATION (v0.8.x)**: `auto_deposit` now defaults to `false` for security.
> To restore previous behavior, explicitly set `auto_deposit = true` in your config.

### How It Works

When auto-deposit is enabled, your node will automatically:

1. **On startup**: Check if the contract balance is below the minimum (default: 100 HBAR), and deposit if needed (default: 200 HBAR)
2. **On channel acceptance**: When a peer tries to open a channel with you, auto-deposit if balance is insufficient and cooldown has elapsed

### Configuration

Configure auto-deposit behavior in your `config.toml` (in your data directory, see Troubleshooting section below for paths):

```toml
[settlement]
# Enable auto-deposit (default: false — opt-in for security)
auto_deposit = true

# Minimum balance to maintain in contract (in HBAR)
min_contract_balance_hbar = 100.0

# Amount to deposit when auto-deposit triggers (in HBAR)
auto_deposit_amount_hbar = 200.0

# Maximum deposit to accept/match per channel (in HBAR)
# Caps how much you'll commit when a peer opens a channel with you
max_accept_deposit_hbar = 500.0
```

### Security Notes

- **Deposit cap**: The `max_accept_deposit_hbar` setting limits how much you'll commit per channel, regardless of what the peer requests
- **Cooldown**: Auto-deposits are rate-limited (5 minute cooldown by default) to prevent spam-triggered deposits
- **Fixed amount**: Auto-deposit always uses the configured amount, never an amount derived from the peer's request
- **Cooldown resets on restart**: The cooldown timer doesn't persist across node restarts. The startup auto-deposit check handles the post-restart case separately.

### Manual Control

To disable auto-deposit entirely:

```toml
[settlement]
auto_deposit = false
```

Then manually deposit as needed:

```bash
nodalync deposit 200
```

---

## Common Commands Reference

### Identity & Node

| Command | Description |
|---------|-------------|
| `nodalync init` | Set up identity and config (add `--wizard` for interactive setup) |
| `nodalync whoami` | Show your identity |
| `nodalync start` | Start node (foreground) |
| `nodalync start --daemon` | Start node (background) |
| `nodalync status` | Show node status |
| `nodalync stop` | Stop daemon |
| `nodalync completions <shell>` | Generate shell completions (bash, zsh, fish, power-shell) |

### Content

| Command | Description |
|---------|-------------|
| `nodalync publish <file> [--price <hbar>] [--title "..."]` | Publish content |
| `nodalync update <hash> <file>` | Create a new version of content |
| `nodalync delete <hash>` | Delete local content |
| `nodalync visibility <hash> --level <level>` | Change content visibility |
| `nodalync versions <hash>` | Show version history |
| `nodalync list` | List your content |
| `nodalync search <query> [--all]` | Search content (matches title/description/tags) |
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

**Default data directory:**

The data directory varies by platform unless you set `NODALYNC_DATA_DIR`:
- **macOS**: `~/Library/Application Support/io.nodalync.nodalync/`
- **Linux**: `~/.local/share/nodalync/` (or `$XDG_DATA_HOME/nodalync/`)
- **Windows**: `%APPDATA%\nodalync\nodalync\`

Set `NODALYNC_DATA_DIR` to override: `export NODALYNC_DATA_DIR=~/.nodalync`

**Node won't start:**
```bash
# Check if already running
nodalync status

# View logs (path shown when starting daemon)
cat ~/Library/Application\ Support/io.nodalync.nodalync/node.stderr.log  # macOS
cat ~/.local/share/nodalync/node.stderr.log  # Linux
```

**Can't connect to peers:**
```bash
# Verify bootstrap node is reachable
curl http://nodalync-bootstrap.eastus.azurecontainer.io:8080/health

# Check your firewall allows TCP 9000
```

**Reset everything:**
```bash
# Remove data directory (check your platform above, or use your NODALYNC_DATA_DIR)
rm -rf ~/Library/Application\ Support/io.nodalync.nodalync/  # macOS
# rm -rf ~/.local/share/nodalync/  # Linux
nodalync init --wizard
```

---

## Next Steps

- Read the [Protocol Spec](./spec.md) to understand how Nodalync works
- Explore the [Architecture](./architecture.md) for module details
- Check the [FAQ](./FAQ.md) for common questions
- Join the [Discord](https://discord.gg/hYVrEAM6) community!
