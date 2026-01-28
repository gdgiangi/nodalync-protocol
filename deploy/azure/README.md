# Azure Bootstrap Node Deployment

Deploy a Nodalync bootstrap node to Azure Container Instances (ACI).

Uses the pre-built Docker image from Docker Hub (`gabrielgiangi/nodalync:latest`), built automatically by GitHub Actions on every push to main.

## Prerequisites

1. **Azure CLI** installed and authenticated:
   ```bash
   # Install (macOS)
   brew install azure-cli

   # Login
   az login
   ```

## Quick Start

```bash
# 1. Copy and configure
cp config.env.example config.env

# 2. Edit config.env with your values
#    - Generate password: openssl rand -base64 32
#    - Choose a unique STORAGE_ACCOUNT name (globally unique, lowercase)

# 3. Deploy (takes ~2 minutes)
./deploy-bootstrap.sh deploy
```

## Configuration

Edit `config.env`:

| Variable | Description | Example |
|----------|-------------|---------|
| `RESOURCE_GROUP` | Azure resource group name | `nodalync-bootstrap` |
| `LOCATION` | Azure region | `eastus`, `westeurope` |
| `STORAGE_ACCOUNT` | Storage account (globally unique, lowercase) | `nodalyncdata123` |
| `CONTAINER_NAME` | Container instance name | `nodalync-bootstrap-1` |
| `DNS_LABEL` | DNS prefix for public URL | `nodalync-bootstrap` |
| `NODALYNC_PASSWORD` | Node identity password | (generate securely) |

## Commands

```bash
./deploy-bootstrap.sh deploy   # Full deployment
./deploy-bootstrap.sh status   # Show status and addresses
./deploy-bootstrap.sh logs     # View container logs (Ctrl+C to exit)
./deploy-bootstrap.sh start    # Start stopped container
./deploy-bootstrap.sh stop     # Stop container
./deploy-bootstrap.sh restart  # Restart container
./deploy-bootstrap.sh destroy  # Delete all resources
```

## After Deployment

1. **Get the bootstrap multiaddr:**
   ```bash
   ./deploy-bootstrap.sh logs
   # Look for: libp2p_peer_id: 12D3KooW...

   ./deploy-bootstrap.sh status
   # Shows FQDN: nodalync-bootstrap.eastus.azurecontainer.io
   ```

2. **Construct the full multiaddr:**
   ```
   /dns4/nodalync-bootstrap.eastus.azurecontainer.io/tcp/9000/p2p/12D3KooW...
   ```

3. **Add to your local config** (`~/.nodalync/config.toml`):
   ```toml
   [network]
   bootstrap_nodes = [
     "/dns4/nodalync-bootstrap.eastus.azurecontainer.io/tcp/9000/p2p/12D3KooW..."
   ]
   ```

4. **Test connectivity:**
   ```bash
   nodalync start
   nodalync peers  # Should show the bootstrap node
   ```

## Architecture

```
Azure Container Instance (ACI)
├── gabrielgiangi/nodalync:latest (from Docker Hub)
├── Port 9000/TCP exposed
├── DNS: <dns_label>.<region>.azurecontainer.io
└── Persistent storage (Azure Files)
    └── /home/nodalync/.nodalync
        ├── identity/keypair.key
        ├── nodalync.db
        └── content/
```

## Cost Estimate

| Resource | SKU | ~Monthly Cost |
|----------|-----|---------------|
| Container Instance | 1 vCPU, 2GB RAM | $35-45 |
| Storage Account | Standard_LRS | $2-5 |
| **Total** | | **~$40-50** |

## Multi-Region Deployment

For redundancy, deploy to multiple regions:

```bash
# Create configs for each region
cp config.env config-us.env
cp config.env config-eu.env

# Edit each with unique names:
# config-us.env: LOCATION=eastus, DNS_LABEL=nodalync-us, STORAGE_ACCOUNT=nodalyncus123
# config-eu.env: LOCATION=westeurope, DNS_LABEL=nodalync-eu, STORAGE_ACCOUNT=nodalynceu123

# Deploy each
NODALYNC_CONFIG=config-us.env ./deploy-bootstrap.sh deploy
NODALYNC_CONFIG=config-eu.env ./deploy-bootstrap.sh deploy
```

## Using a Specific Image Version

By default, the script uses `gabrielgiangi/nodalync:latest`. To use a specific version:

```bash
# In config.env
NODALYNC_IMAGE="gabrielgiangi/nodalync:v0.1.0"
```

Or from a tagged release:
```bash
# Check available tags at: https://hub.docker.com/r/gabrielgiangi/nodalync/tags
```

## Troubleshooting

**Container won't start:**
```bash
./deploy-bootstrap.sh logs
# Check for error messages
```

**Identity issues:**
```bash
# Check if identity exists in storage
az storage file list \
  --share-name nodedata \
  --account-name <storage_account> \
  --path identity \
  --output table
```

**Connectivity issues:**
```bash
# Test TCP connection
nc -zv <fqdn> 9000

# Check container is running
./deploy-bootstrap.sh status
```

**Redeploy with fresh identity:**
```bash
# Delete identity from storage first
az storage file delete-batch \
  --source nodedata \
  --account-name <storage_account> \
  --pattern "identity/*"

# Then redeploy
./deploy-bootstrap.sh deploy
```

**Pull latest image:**
```bash
# Stop and redeploy to get latest image
./deploy-bootstrap.sh stop
./deploy-bootstrap.sh deploy
```

## Security Notes

- `config.env` contains secrets - never commit it (already in .gitignore)
- The node password encrypts the identity keypair at rest
- ACI provides network isolation by default
- Consider Azure Key Vault for production secrets
