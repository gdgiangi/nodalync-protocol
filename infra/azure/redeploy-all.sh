#!/bin/bash
# Redeploy all bootstrap containers with the latest image
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

redeploy_region() {
    local config_file="$1"
    local region_name="$2"
    
    echo "=== Redeploying $region_name ==="
    
    # Source the config
    source "$SCRIPT_DIR/$config_file"
    
    # Derived values
    ACR_LOGIN_SERVER="${ACR_NAME}.azurecr.io"
    ACR_IMAGE="${ACR_LOGIN_SERVER}/nodalync:latest"
    SHARE_NAME="nodedata"
    
    echo "Deleting old container..."
    az container delete --resource-group "$RESOURCE_GROUP" --name "$CONTAINER_NAME" --yes 2>/dev/null || true
    
    echo "Getting credentials..."
    ACR_USERNAME=$(az acr credential show --name "$ACR_NAME" --query username -o tsv)
    ACR_PASSWORD=$(az acr credential show --name "$ACR_NAME" --query 'passwords[0].value' -o tsv)
    STORAGE_KEY=$(az storage account keys list --resource-group "$RESOURCE_GROUP" --account-name "$STORAGE_ACCOUNT" --query '[0].value' -o tsv)
    
    echo "Creating new container with latest image..."
    az container create \
        --resource-group "$RESOURCE_GROUP" \
        --name "$CONTAINER_NAME" \
        --image "$ACR_IMAGE" \
        --os-type Linux \
        --dns-name-label "$DNS_LABEL" \
        --ports 9000 8080 \
        --protocol TCP \
        --cpu 1 \
        --memory 2 \
        --restart-policy Always \
        --registry-login-server "$ACR_LOGIN_SERVER" \
        --registry-username "$ACR_USERNAME" \
        --registry-password "$ACR_PASSWORD" \
        --azure-file-volume-account-name "$STORAGE_ACCOUNT" \
        --azure-file-volume-account-key "$STORAGE_KEY" \
        --azure-file-volume-share-name "$SHARE_NAME" \
        --azure-file-volume-mount-path /home/nodalync/.nodalync \
        --environment-variables \
            RUST_LOG=nodalync=info \
            NODALYNC_PASSWORD="$NODALYNC_PASSWORD" \
            HEDERA_NETWORK=hedera-testnet \
            HEDERA_ACCOUNT_ID="$HEDERA_ACCOUNT_ID" \
            HEDERA_PRIVATE_KEY="$HEDERA_PRIVATE_KEY" \
            HEDERA_CONTRACT_ID="$HEDERA_CONTRACT_ID" \
        --command-line "nodalync start --health" \
        --output none
    
    echo "$region_name deployed successfully!"
    echo ""
}

echo "Starting redeployment of all bootstrap nodes..."
echo ""

# Verify all config files exist before starting
for cfg in config.env config-eu.env config-asia.env; do
    if [ ! -f "$SCRIPT_DIR/$cfg" ]; then
        echo "ERROR: Missing config file: $SCRIPT_DIR/$cfg"
        echo "Each region needs its own config file. See config.env as a template."
        exit 1
    fi
done

# Redeploy all three regions
redeploy_region "config.env" "US East"
redeploy_region "config-eu.env" "EU North"
redeploy_region "config-asia.env" "Asia Southeast"

echo "=== All regions redeployed! ==="
echo ""
echo "Wait 1-2 minutes for containers to start, then verify versions with:"
echo "  curl http://nodalync-bootstrap.eastus.azurecontainer.io:8080/health"
echo "  curl http://nodalync-eu.northeurope.azurecontainer.io:8080/health"
echo "  curl http://nodalync-asia.southeastasia.azurecontainer.io:8080/health"
