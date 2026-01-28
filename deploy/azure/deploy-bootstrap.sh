#!/bin/bash
# Deploy a Nodalync bootstrap node to Azure Container Instances
#
# Uses the pre-built Docker image from Docker Hub, imported into Azure Container Registry
# to avoid Docker Hub rate limiting.
#
# Prerequisites:
#   - Azure CLI installed and logged in (az login)
#
# Usage:
#   ./deploy-bootstrap.sh [command]
#
# Commands:
#   deploy    - Full deployment (create resources + deploy container)
#   init      - Initialize identity only
#   start     - Start stopped container
#   stop      - Stop container
#   restart   - Restart container
#   logs      - View container logs
#   status    - Show container status and bootstrap address
#   destroy   - Delete all resources (WARNING: destroys data)
#   help      - Show this help message

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source image on Docker Hub
SOURCE_IMAGE="docker.io/gabrielgiangi/nodalync:latest"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_step()  { echo -e "${BLUE}[STEP]${NC} $1"; }

# Load configuration
load_config() {
    local config_file="${NODALYNC_CONFIG:-$SCRIPT_DIR/config.env}"

    if [[ -f "$config_file" ]]; then
        source "$config_file"
    else
        log_error "Config file not found: $config_file"
        log_error "Copy config.env.example to config.env and fill in values."
        exit 1
    fi

    # Validate required settings
    local missing=()
    [[ -z "$RESOURCE_GROUP" ]] && missing+=("RESOURCE_GROUP")
    [[ -z "$LOCATION" ]] && missing+=("LOCATION")
    [[ -z "$ACR_NAME" ]] && missing+=("ACR_NAME")
    [[ -z "$STORAGE_ACCOUNT" ]] && missing+=("STORAGE_ACCOUNT")
    [[ -z "$CONTAINER_NAME" ]] && missing+=("CONTAINER_NAME")
    [[ -z "$DNS_LABEL" ]] && missing+=("DNS_LABEL")
    [[ -z "$NODALYNC_PASSWORD" ]] && missing+=("NODALYNC_PASSWORD")

    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required configuration: ${missing[*]}"
        exit 1
    fi

    # Set subscription if provided
    if [[ -n "$AZURE_SUBSCRIPTION" ]]; then
        az account set --subscription "$AZURE_SUBSCRIPTION"
    fi

    # Derived values
    SHARE_NAME="nodedata"
    ACR_LOGIN_SERVER="${ACR_NAME}.azurecr.io"
    ACR_IMAGE="${ACR_LOGIN_SERVER}/nodalync:latest"
}

# Check Azure CLI is logged in
check_azure_login() {
    if ! az account show &>/dev/null; then
        log_error "Not logged into Azure CLI. Run: az login"
        exit 1
    fi
    log_info "Azure CLI authenticated as: $(az account show --query user.name -o tsv)"
}

# Create resource group
create_resource_group() {
    log_step "Creating resource group: $RESOURCE_GROUP"
    az group create \
        --name "$RESOURCE_GROUP" \
        --location "$LOCATION" \
        --output none
    log_info "Resource group created"
}

# Create Azure Container Registry
create_acr() {
    log_step "Creating Azure Container Registry: $ACR_NAME"

    if az acr show --name "$ACR_NAME" --resource-group "$RESOURCE_GROUP" &>/dev/null; then
        log_info "ACR already exists"
    else
        az acr create \
            --resource-group "$RESOURCE_GROUP" \
            --name "$ACR_NAME" \
            --sku Basic \
            --admin-enabled true \
            --output none
        log_info "ACR created: $ACR_LOGIN_SERVER"
    fi
}

# Import image from Docker Hub to ACR
import_image() {
    log_step "Importing image to ACR"

    # Check if image already exists in ACR
    if az acr repository show --name "$ACR_NAME" --image "nodalync:latest" &>/dev/null; then
        log_info "Image already exists in ACR, skipping import"
        return 0
    fi

    log_info "Attempting to import from Docker Hub..."
    log_info "Source: $SOURCE_IMAGE"

    if az acr import \
        --name "$ACR_NAME" \
        --source "$SOURCE_IMAGE" \
        --image "nodalync:latest" \
        --force \
        --output none 2>&1; then
        log_info "Image imported successfully"
    else
        log_warn "Docker Hub import failed (likely rate limited)"
        log_warn ""
        log_warn "Push the image manually from your local machine:"
        log_warn ""
        log_warn "  az acr login --name $ACR_NAME"
        log_warn "  docker tag gabrielgiangi/nodalync:latest ${ACR_IMAGE}"
        log_warn "  docker push ${ACR_IMAGE}"
        log_warn ""
        log_warn "Then run: ./deploy-bootstrap.sh deploy"
        exit 1
    fi
}

# Create storage account and file share
create_storage() {
    log_step "Creating storage account: $STORAGE_ACCOUNT"

    if az storage account show --name "$STORAGE_ACCOUNT" --resource-group "$RESOURCE_GROUP" &>/dev/null; then
        log_info "Storage account already exists"
    else
        az storage account create \
            --resource-group "$RESOURCE_GROUP" \
            --name "$STORAGE_ACCOUNT" \
            --location "$LOCATION" \
            --sku Standard_LRS \
            --kind StorageV2 \
            --output none
        log_info "Storage account created"
    fi

    log_step "Creating file share: $SHARE_NAME"

    STORAGE_KEY=$(az storage account keys list \
        --resource-group "$RESOURCE_GROUP" \
        --account-name "$STORAGE_ACCOUNT" \
        --query '[0].value' -o tsv)

    az storage share create \
        --name "$SHARE_NAME" \
        --account-name "$STORAGE_ACCOUNT" \
        --account-key "$STORAGE_KEY" \
        --output none 2>/dev/null || true

    log_info "File share ready"
}

# Get credentials
get_credentials() {
    ACR_USERNAME=$(az acr credential show --name "$ACR_NAME" --query username -o tsv)
    ACR_PASSWORD=$(az acr credential show --name "$ACR_NAME" --query 'passwords[0].value' -o tsv)
    STORAGE_KEY=$(az storage account keys list \
        --resource-group "$RESOURCE_GROUP" \
        --account-name "$STORAGE_ACCOUNT" \
        --query '[0].value' -o tsv)
}

# Check if identity exists in storage
check_identity_exists() {
    STORAGE_KEY=$(az storage account keys list \
        --resource-group "$RESOURCE_GROUP" \
        --account-name "$STORAGE_ACCOUNT" \
        --query '[0].value' -o tsv)

    if az storage file exists \
        --share-name "$SHARE_NAME" \
        --path "identity/keypair.key" \
        --account-name "$STORAGE_ACCOUNT" \
        --account-key "$STORAGE_KEY" \
        --query exists -o tsv 2>/dev/null | grep -q "true"; then
        return 0
    else
        return 1
    fi
}

# Initialize node identity
init_identity() {
    log_step "Initializing node identity"

    get_credentials

    if check_identity_exists; then
        log_warn "Identity already exists in storage. Skipping init."
        return 0
    fi

    # Delete any existing init container
    az container delete \
        --resource-group "$RESOURCE_GROUP" \
        --name nodalync-init-temp \
        --yes 2>/dev/null || true

    log_info "Running nodalync init..."

    az container create \
        --resource-group "$RESOURCE_GROUP" \
        --name nodalync-init-temp \
        --image "$ACR_IMAGE" \
        --os-type Linux \
        --cpu 1 \
        --memory 1 \
        --registry-login-server "$ACR_LOGIN_SERVER" \
        --registry-username "$ACR_USERNAME" \
        --registry-password "$ACR_PASSWORD" \
        --azure-file-volume-account-name "$STORAGE_ACCOUNT" \
        --azure-file-volume-account-key "$STORAGE_KEY" \
        --azure-file-volume-share-name "$SHARE_NAME" \
        --azure-file-volume-mount-path /home/nodalync/.nodalync \
        --environment-variables NODALYNC_PASSWORD="$NODALYNC_PASSWORD" \
        --command-line "nodalync init" \
        --restart-policy Never \
        --output none

    log_info "Waiting for init to complete..."

    for i in {1..30}; do
        state=$(az container show \
            --resource-group "$RESOURCE_GROUP" \
            --name nodalync-init-temp \
            --query instanceView.state -o tsv 2>/dev/null || echo "Unknown")

        if [[ "$state" == "Succeeded" ]] || [[ "$state" == "Terminated" ]]; then
            break
        fi
        sleep 2
    done

    echo ""
    az container logs \
        --resource-group "$RESOURCE_GROUP" \
        --name nodalync-init-temp 2>/dev/null || true
    echo ""

    az container delete \
        --resource-group "$RESOURCE_GROUP" \
        --name nodalync-init-temp \
        --yes \
        --output none 2>/dev/null || true

    log_info "Identity initialized"
}

# Deploy the bootstrap container
deploy_container() {
    log_step "Deploying bootstrap container: $CONTAINER_NAME"

    get_credentials

    # Delete existing container if present
    az container delete \
        --resource-group "$RESOURCE_GROUP" \
        --name "$CONTAINER_NAME" \
        --yes 2>/dev/null || true

    # Build environment variables
    local env_vars="RUST_LOG=nodalync=info NODALYNC_PASSWORD=$NODALYNC_PASSWORD"

    if [[ -n "$HEDERA_ACCOUNT_ID" ]]; then
        env_vars="$env_vars HEDERA_ACCOUNT_ID=$HEDERA_ACCOUNT_ID"
    fi
    if [[ -n "$HEDERA_PRIVATE_KEY" ]]; then
        env_vars="$env_vars HEDERA_PRIVATE_KEY=$HEDERA_PRIVATE_KEY"
    fi
    if [[ -n "$HEDERA_CONTRACT_ID" ]]; then
        env_vars="$env_vars HEDERA_CONTRACT_ID=$HEDERA_CONTRACT_ID"
    fi

    az container create \
        --resource-group "$RESOURCE_GROUP" \
        --name "$CONTAINER_NAME" \
        --image "$ACR_IMAGE" \
        --os-type Linux \
        --dns-name-label "$DNS_LABEL" \
        --ports 9000 \
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
        --environment-variables $env_vars \
        --command-line "nodalync start" \
        --output none

    log_info "Container deployed"
    sleep 5
    show_status
}

# Show container status
show_status() {
    log_step "Container Status"

    local state=$(az container show \
        --resource-group "$RESOURCE_GROUP" \
        --name "$CONTAINER_NAME" \
        --query instanceView.state -o tsv 2>/dev/null || echo "Not deployed")

    local fqdn=$(az container show \
        --resource-group "$RESOURCE_GROUP" \
        --name "$CONTAINER_NAME" \
        --query ipAddress.fqdn -o tsv 2>/dev/null || echo "N/A")

    local ip=$(az container show \
        --resource-group "$RESOURCE_GROUP" \
        --name "$CONTAINER_NAME" \
        --query ipAddress.ip -o tsv 2>/dev/null || echo "N/A")

    echo ""
    echo "  State: $state"
    echo "  FQDN:  $fqdn"
    echo "  IP:    $ip"
    echo ""

    if [[ "$state" == "Running" ]]; then
        log_info "To get the bootstrap multiaddr:"
        echo ""
        echo "  ./deploy-bootstrap.sh logs"
        echo ""
        echo "  Look for libp2p_peer_id (12D3KooW...), then:"
        echo "  /dns4/${fqdn}/tcp/9000/p2p/<PEER_ID>"
        echo ""
    fi
}

# View logs
show_logs() {
    az container logs \
        --resource-group "$RESOURCE_GROUP" \
        --name "$CONTAINER_NAME" \
        --follow
}

# Stop container
stop_container() {
    log_step "Stopping container"
    az container stop \
        --resource-group "$RESOURCE_GROUP" \
        --name "$CONTAINER_NAME" \
        --output none
    log_info "Container stopped"
}

# Start container
start_container() {
    log_step "Starting container"
    az container start \
        --resource-group "$RESOURCE_GROUP" \
        --name "$CONTAINER_NAME" \
        --output none
    log_info "Container started"
    show_status
}

# Restart container
restart_container() {
    log_step "Restarting container"
    az container restart \
        --resource-group "$RESOURCE_GROUP" \
        --name "$CONTAINER_NAME" \
        --output none
    log_info "Container restarted"
    show_status
}

# Destroy all resources
destroy_resources() {
    log_warn "This will delete ALL resources in: $RESOURCE_GROUP"
    log_warn "Including node identity and stored content!"
    echo ""
    read -p "Type 'destroy' to confirm: " confirm

    if [[ "$confirm" != "destroy" ]]; then
        log_info "Aborted"
        exit 0
    fi

    log_step "Deleting resource group"
    az group delete --name "$RESOURCE_GROUP" --yes --no-wait
    log_info "Deletion initiated"
}

# Full deployment
full_deploy() {
    log_info "Starting deployment..."
    echo ""

    check_azure_login
    create_resource_group
    create_acr
    import_image
    create_storage
    init_identity
    deploy_container

    echo ""
    log_info "Deployment complete!"
}

# Show help
show_help() {
    head -23 "$0" | tail -20
}

# Main
main() {
    local command="${1:-deploy}"

    case "$command" in
        deploy)  load_config; full_deploy ;;
        init)    load_config; check_azure_login; init_identity ;;
        start)   load_config; check_azure_login; start_container ;;
        stop)    load_config; check_azure_login; stop_container ;;
        restart) load_config; check_azure_login; restart_container ;;
        logs)    load_config; check_azure_login; show_logs ;;
        status)  load_config; check_azure_login; show_status ;;
        destroy) load_config; check_azure_login; destroy_resources ;;
        help|--help|-h) show_help ;;
        *) log_error "Unknown command: $command"; show_help; exit 1 ;;
    esac
}

main "$@"
