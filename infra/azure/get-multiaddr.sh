#!/bin/bash
# Get the full bootstrap multiaddr for a deployed node
#
# Usage: ./get-multiaddr.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Load config
if [[ -f "$SCRIPT_DIR/config.env" ]]; then
    source "$SCRIPT_DIR/config.env"
else
    echo "Error: config.env not found"
    exit 1
fi

# Get FQDN
FQDN=$(az container show \
    --resource-group "$RESOURCE_GROUP" \
    --name "$CONTAINER_NAME" \
    --query ipAddress.fqdn -o tsv 2>/dev/null)

if [[ -z "$FQDN" ]]; then
    echo "Error: Container not found or not running"
    exit 1
fi

# Get storage key
STORAGE_KEY=$(az storage account keys list \
    --resource-group "$RESOURCE_GROUP" \
    --account-name "$STORAGE_ACCOUNT" \
    --query '[0].value' -o tsv)

# Get ACR credentials
ACR_USERNAME=$(az acr credential show --name "$ACR_NAME" --query username -o tsv)
ACR_PASSWORD=$(az acr credential show --name "$ACR_NAME" --query 'passwords[0].value' -o tsv)

# Run whoami to get peer ID
echo "Fetching peer ID..."

WHOAMI_OUTPUT=$(az container exec \
    --resource-group "$RESOURCE_GROUP" \
    --name "$CONTAINER_NAME" \
    --exec-command "nodalync whoami --format json" 2>/dev/null || true)

# Try to extract libp2p peer ID from output
LIBP2P_PEER_ID=$(echo "$WHOAMI_OUTPUT" | grep -o '"libp2p_peer_id"[[:space:]]*:[[:space:]]*"[^"]*"' | sed 's/.*"\([^"]*\)"$/\1/' | head -1)

if [[ -z "$LIBP2P_PEER_ID" ]]; then
    echo ""
    echo "Could not extract peer ID automatically."
    echo "Check the logs manually:"
    echo ""
    echo "  ./deploy-bootstrap.sh logs"
    echo ""
    echo "Look for 'libp2p_peer_id' or 'PeerId' in the output."
    echo ""
    echo "Then construct the multiaddr as:"
    echo "  /dns4/${FQDN}/tcp/9000/p2p/<PEER_ID>"
    exit 1
fi

MULTIADDR="/dns4/${FQDN}/tcp/9000/p2p/${LIBP2P_PEER_ID}"

echo ""
echo "Bootstrap Multiaddr:"
echo ""
echo "  $MULTIADDR"
echo ""
echo "Add this to your config.toml:"
echo ""
echo '  [network]'
echo '  bootstrap_nodes = ['
echo "    \"$MULTIADDR\""
echo '  ]'
echo ""
