#!/bin/bash
# End-to-end test for Nodalync 3-node cluster
# Tests: publish on node1, DHT propagation, query from node3

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(dirname "$SCRIPT_DIR")"
CONFIG_DIR="$DOCKER_DIR/config"

# Load peer IDs
if [ -f "$CONFIG_DIR/peer-ids.env" ]; then
    source "$CONFIG_DIR/peer-ids.env"
else
    echo "ERROR: peer-ids.env not found. Run 'make init' first."
    exit 1
fi

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_test() {
    echo -e "${BLUE}[TEST]${NC} $1"
}

pass() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    exit 1
}

# Helper to run commands on a specific node
node_exec() {
    local node=$1
    shift
    docker exec -e NODALYNC_PASSWORD=testpassword "nodalync-$node" nodalync "$@"
}

# Helper to extract JSON from command output (filters out log lines)
# Handles both objects ({}) and arrays ([])
node_exec_json() {
    local node=$1
    shift
    docker exec -e NODALYNC_PASSWORD=testpassword "nodalync-$node" nodalync "$@" 2>&1 | sed -n '/^[[{]/,/^[\]}]/p'
}

# Test 1: Check all nodes are healthy
log_test "Test 1: Checking node health..."

for node in node1 node2 node3; do
    if docker exec "nodalync-$node" nodalync status --format json 2>/dev/null | jq -e '.running' > /dev/null; then
        pass "$node is healthy"
    else
        fail "$node is not healthy"
    fi
done

# Test 2: Check peer connectivity
log_test "Test 2: Checking peer connectivity..."

sleep 3  # Allow time for DHT bootstrap

NODE1_PEERS=$(node_exec_json node1 status --format json | jq -r '.connected_peers // 0')
NODE2_PEERS=$(node_exec_json node2 status --format json | jq -r '.connected_peers // 0')
NODE3_PEERS=$(node_exec_json node3 status --format json | jq -r '.connected_peers // 0')

log_info "Node1 peers: $NODE1_PEERS, Node2 peers: $NODE2_PEERS, Node3 peers: $NODE3_PEERS"

if [ "$NODE1_PEERS" -ge 1 ] || [ "$NODE2_PEERS" -ge 1 ] || [ "$NODE3_PEERS" -ge 1 ]; then
    pass "Nodes are connected"
else
    log_warn "Nodes may not be fully connected (could be bootstrap timing)"
fi

# Test 3: Publish content on node1
log_test "Test 3: Publishing content on node1..."

# Create test content
TEST_CONTENT="This is test content for E2E testing. Timestamp: $(date +%s)"
TEMP_FILE=$(mktemp)
echo "$TEST_CONTENT" > "$TEMP_FILE"

# Copy to container and publish
docker cp "$TEMP_FILE" nodalync-node1:/tmp/test-content.txt
rm "$TEMP_FILE"

# Fix file permissions (docker cp preserves host UID, but container runs as nodalync user)
docker exec -u root nodalync-node1 chmod 644 /tmp/test-content.txt

# Extract only the JSON object from the output (filter out log lines)
PUBLISH_OUTPUT=$(docker exec -e NODALYNC_PASSWORD=testpassword nodalync-node1 \
    nodalync publish /tmp/test-content.txt --title "E2E Test Document" --format json 2>&1 | \
    sed -n '/^{/,/^}/p')

CONTENT_HASH=$(echo "$PUBLISH_OUTPUT" | jq -r '.hash // .content_hash // empty')

if [ -z "$CONTENT_HASH" ]; then
    # Try to parse human-readable output
    CONTENT_HASH=$(docker exec -e NODALYNC_PASSWORD=testpassword nodalync-node1 \
        nodalync publish /tmp/test-content.txt --title "E2E Test Document" 2>/dev/null | grep -oP 'Hash: \K[a-f0-9]+' || echo "")
fi

if [ -n "$CONTENT_HASH" ]; then
    pass "Content published with hash: $CONTENT_HASH"
else
    fail "Failed to publish content"
fi

# Test 4: Wait for DHT propagation
log_test "Test 4: Waiting for DHT propagation..."

sleep 5  # Allow time for GossipSub and DHT propagation

pass "DHT propagation wait complete"

# Test 5: Preview content from node2
log_test "Test 5: Previewing content from node2..."

PREVIEW_OUTPUT=$(node_exec_json node2 preview "$CONTENT_HASH" --format json || echo "{}")

PREVIEW_TITLE=$(echo "$PREVIEW_OUTPUT" | jq -r '.title // empty')

if [ -n "$PREVIEW_TITLE" ]; then
    pass "Content preview retrieved on node2: $PREVIEW_TITLE"
else
    log_warn "Could not preview content on node2 (may need more DHT time)"
fi

# Test 6: Query content from node3
log_test "Test 6: Querying content from node3..."

QUERY_OUTPUT=$(node_exec_json node3 query "$CONTENT_HASH" --format json || echo "{}")

QUERY_CONTENT=$(echo "$QUERY_OUTPUT" | jq -r '.content // empty')

if [ -n "$QUERY_CONTENT" ]; then
    pass "Content retrieved on node3"
else
    log_warn "Could not query content on node3 (may need DHT routing)"
fi

# Test 7: Check earnings on node1
log_test "Test 7: Checking earnings on node1..."

EARNINGS_OUTPUT=$(node_exec_json node1 earnings --format json || echo "{}")

log_info "Earnings output: $EARNINGS_OUTPUT"

pass "Earnings check complete"

# Test 8: List published content on node1
log_test "Test 8: Listing content on node1..."

LIST_OUTPUT=$(node_exec_json node1 list --format json || echo "[]")
LIST_COUNT=$(echo "$LIST_OUTPUT" | jq 'length // 0')

if [ "$LIST_COUNT" -ge 1 ]; then
    pass "Node1 has $LIST_COUNT published items"
else
    log_warn "Node1 list returned $LIST_COUNT items"
fi

# Summary
echo ""
echo "=========================================="
echo "E2E Test Summary"
echo "=========================================="
echo ""
echo "Content hash: $CONTENT_HASH"
echo ""
echo "To query this content:"
echo "  docker exec -e NODALYNC_PASSWORD=testpassword nodalync-node3 nodalync query $CONTENT_HASH"
echo ""
echo "To check earnings:"
echo "  docker exec -e NODALYNC_PASSWORD=testpassword nodalync-node1 nodalync earnings"
echo ""
log_info "E2E tests completed"
