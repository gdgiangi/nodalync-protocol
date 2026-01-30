# Nodalync Quick Start

Get your node running and connected to the network in under 5 minutes.

## Installation

Choose one of three options:

### Option A: Docker (Recommended)

```bash
# Pull or build the image
docker build -t nodalync:latest https://github.com/nodalync/nodalync.git

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

### Option B: Build from Source

Requires Rust 1.85+:

```bash
# Clone the repo
git clone https://github.com/nodalync/nodalync.git
cd nodalync

# Build release binary
cargo build --release -p nodalync-cli

# Build with Hedera settlement support (requires protoc)
cargo build --release -p nodalync-cli --features hedera-sdk

# Add to PATH (or move binary to /usr/local/bin)
export PATH="$PWD/target/release:$PATH"
```

### Option C: Pre-built Binary

Download from [Releases](https://github.com/nodalync/nodalync/releases) (when available).

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
        "NODALYNC_HEDERA_ACCOUNT_ID": "0.0.XXXXX",
        "NODALYNC_HEDERA_KEY_PATH": "/path/to/hedera.key",
        "NODALYNC_HEDERA_NETWORK": "testnet"
      }
    }
  }
}
```

Now Claude can:
- Search knowledge in the Nodalync network
- Query and pay for content automatically
- Track provenance of information
- Settle payments on Hedera (when configured)

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

| Variable | Description |
|----------|-------------|
| `NODALYNC_PASSWORD` | Identity encryption password |
| `NODALYNC_DATA_DIR` | Data directory (default: `~/.nodalync`) |
| `RUST_LOG` | Log level (e.g., `nodalync=debug`) |
| `NODALYNC_HEDERA_ACCOUNT_ID` | Hedera account ID for MCP settlement |
| `NODALYNC_HEDERA_KEY_PATH` | Path to Hedera private key file |
| `NODALYNC_HEDERA_CONTRACT_ID` | Settlement contract ID (default: 0.0.7729011) |
| `NODALYNC_HEDERA_NETWORK` | Network: testnet, mainnet, or previewnet |

---

## Common Commands Reference

| Command | Description |
|---------|-------------|
| `nodalync init --wizard` | Set up identity and config |
| `nodalync whoami` | Show your identity |
| `nodalync start` | Start node (foreground) |
| `nodalync start --daemon` | Start node (background) |
| `nodalync status` | Show node status |
| `nodalync stop` | Stop daemon |
| `nodalync publish <file>` | Publish content |
| `nodalync list` | List your content |
| `nodalync search <query>` | Search content |
| `nodalync preview <hash>` | View metadata (free) |
| `nodalync query <hash>` | Get full content (paid) |
| `nodalync balance` | Check HBAR balance |
| `nodalync earnings` | View earnings |
| `nodalync mcp-server` | Start MCP server for AI |

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

- Read the [Protocol Spec](docs/spec.md) to understand how Nodalync works
- Check [WHATS_NEXT.md](WHATS_NEXT.md) for roadmap and current progress
- Join the network and start publishing your knowledge!
