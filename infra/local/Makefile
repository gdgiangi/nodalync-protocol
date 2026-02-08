# Makefile for Nodalync 3-node cluster
# Usage: make [target]

.PHONY: init up down logs status test clean reset shell-node1 shell-node2 shell-node3 help

DOCKER_COMPOSE := docker compose
SCRIPTS_DIR := scripts

# Default target
help:
	@echo "Nodalync Multi-Node Cluster"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  init        Generate node identities and configs"
	@echo "  up          Start the 3-node cluster"
	@echo "  down        Stop the cluster"
	@echo "  logs        Follow cluster logs"
	@echo "  status      Show cluster status"
	@echo "  test        Run E2E tests"
	@echo "  clean       Remove containers and volumes"
	@echo "  reset       Clean + init (fresh start)"
	@echo "  shell-%     Open shell in node (e.g., shell-node1)"
	@echo ""
	@echo "Environment Variables:"
	@echo "  HEDERA_ACCOUNT_ID     Hedera account for settlement"
	@echo "  HEDERA_PRIVATE_KEY    Hedera private key"
	@echo "  HEDERA_CONTRACT_ID    Settlement contract (default: 0.0.7729011)"
	@echo "  SETTLEMENT_NETWORK    mock|hedera-testnet|hedera-mainnet (default: mock)"
	@echo ""

# Initialize cluster (generate identities and configs)
init:
	@echo "Initializing cluster..."
	@chmod +x $(SCRIPTS_DIR)/init-cluster.sh
	@$(SCRIPTS_DIR)/init-cluster.sh

# Start the cluster
up:
	@if [ ! -f config/node1-config.toml ]; then \
		echo "Config not found. Run 'make init' first."; \
		exit 1; \
	fi
	@echo "Starting cluster..."
	$(DOCKER_COMPOSE) up -d
	@echo ""
	@echo "Waiting for nodes to become healthy..."
	@sleep 5
	@$(MAKE) status

# Stop the cluster
down:
	@echo "Stopping cluster..."
	$(DOCKER_COMPOSE) down

# Follow logs
logs:
	$(DOCKER_COMPOSE) logs -f

# Show logs for specific node
logs-%:
	$(DOCKER_COMPOSE) logs -f $*

# Show cluster status
status:
	@echo "Container Status:"
	@$(DOCKER_COMPOSE) ps
	@echo ""
	@echo "Node Status:"
	@for node in node1 node2 node3; do \
		echo -n "$$node: "; \
		docker exec -e NODALYNC_PASSWORD=testpassword nodalync-$$node nodalync status 2>/dev/null || echo "not running"; \
	done
	@echo ""
	@if [ -f config/peer-ids.env ]; then \
		echo "Peer IDs:"; \
		cat config/peer-ids.env | grep -E "^NODE[0-9]_PEER_ID"; \
	fi

# Run E2E tests
test:
	@if [ ! -f config/peer-ids.env ]; then \
		echo "Cluster not initialized. Run 'make init' first."; \
		exit 1; \
	fi
	@echo "Running E2E tests..."
	@chmod +x $(SCRIPTS_DIR)/test-e2e.sh
	@$(SCRIPTS_DIR)/test-e2e.sh

# Clean up (remove containers, volumes, and generated configs)
clean:
	@echo "Cleaning up..."
	$(DOCKER_COMPOSE) down -v --remove-orphans 2>/dev/null || true
	rm -rf data/*
	rm -f config/*.toml config/peer-ids.env
	@echo "Cleanup complete"

# Full reset (clean + init)
reset: clean init

# Open shell in a node
shell-node1:
	docker exec -it -e NODALYNC_PASSWORD=testpassword nodalync-node1 /bin/sh

shell-node2:
	docker exec -it -e NODALYNC_PASSWORD=testpassword nodalync-node2 /bin/sh

shell-node3:
	docker exec -it -e NODALYNC_PASSWORD=testpassword nodalync-node3 /bin/sh

# Generic shell target
shell-%:
	docker exec -it -e NODALYNC_PASSWORD=testpassword nodalync-$* /bin/sh

# Quick CLI access to a node
cli-%:
	@docker exec -it -e NODALYNC_PASSWORD=testpassword nodalync-$* nodalync $(ARGS)

# Publish test content
publish-test:
	@echo "Publishing test content on node1..."
	@docker exec -e NODALYNC_PASSWORD=testpassword nodalync-node1 \
		sh -c 'echo "Test content $(shell date +%s)" > /tmp/test.txt && nodalync publish /tmp/test.txt --title "Test"'

# Check connected peers
peers:
	@command -v jq >/dev/null 2>&1 || { echo "ERROR: jq is required. Install with: brew install jq (macOS) or apt install jq (Linux)"; exit 1; }
	@for node in node1 node2 node3; do \
		echo "$$node peers:"; \
		docker exec -e NODALYNC_PASSWORD=testpassword nodalync-$$node nodalync status --format json 2>/dev/null | jq -r '.connected_peers // "unknown"'; \
	done

# Watch cluster health
watch:
	watch -n 2 'docker compose ps && echo "" && for node in node1 node2 node3; do echo -n "$$node: "; docker exec -e NODALYNC_PASSWORD=testpassword nodalync-$$node nodalync status 2>/dev/null || echo "not running"; done'
