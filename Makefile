.PHONY: help deploy deploy-native deploy-docker start stop restart logs clean info test init-bridge deploy-sui register test-bridge stop-all restart-all status logs-deployer starcoin-node deploy-starcoin-bridge start-bridge

# Colors
GREEN  := \033[0;32m
YELLOW := \033[1;33m
RED    := \033[0;31m
NC     := \033[0m # No Color

help: ## Show this help message
	@echo '$(GREEN)Sui Bridge Deployment Automation$(NC)'
	@echo ''
	@echo '$(YELLOW)Available targets:$(NC)'
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

deploy: ## Deploy Ethereum network using Docker Compose
	@echo "$(YELLOW)Starting Ethereum network...$(NC)"
	@docker-compose up -d
	@echo "$(GREEN)Waiting for deployment to complete...$(NC)"
	@sleep 5
	@curl -s http://localhost:8080/deployment.json > /dev/null && echo "$(GREEN)✓ ETH network ready$(NC)" || echo "$(RED)✗ ETH network failed$(NC)"

start: ## Start all services
	@docker-compose up -d

stop: ## Stop all services
	@docker-compose down

restart: ## Restart all services
	@docker-compose restart

logs: ## Show logs from all services
	@docker-compose logs -f

logs-eth: ## Show Ethereum node logs
	@docker-compose logs -f eth-node

logs-deployer: ## Show deployer logs
	@docker-compose logs eth-deployer

clean: ## Stop services and remove volumes
	@./scripts/stop-anvil.sh 2>/dev/null || true
	@docker-compose down -v 2>/dev/null || true
	@echo "All services stopped and volumes removed"

stop-anvil: ## Stop native Anvil process
	@./scripts/stop-anvil.sh

info: ## Show deployment information
	@./scripts/get-deployment-info.sh

test-rpc: ## Test RPC connection
	@echo "Testing Ethereum RPC..."
	@curl -s -X POST http://localhost:8545 \
		-H "Content-Type: application/json" \
		-d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' | jq
	@echo ""
	@curl -s -X POST http://localhost:8545 \
		-H "Content-Type: application/json" \
		-d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}' | jq

ps: ## Show running containers
	@docker-compose ps

redeploy: clean deploy ## Clean everything and redeploy

# === Bridge Setup Targets ===

init-bridge: ## Initialize Bridge keys and configs
	@echo "$(YELLOW)Initializing Bridge configuration...$(NC)"
	@./scripts/init-bridge.sh

deploy-sui: ## Deploy Sui Bridge contracts
	@echo "$(YELLOW)Deploying Sui Bridge contracts...$(NC)"
	@if [ ! -f bridge-config/.env ]; then \
		echo "$(RED)✗ Please run 'make init-bridge' first$(NC)"; \
		exit 1; \
	fi
	@./scripts/deploy-starcoin-bridge-bridge.sh

register: ## Register bridge committee
	@echo "$(YELLOW)Registering bridge committee...$(NC)"
	@if [ ! -f bridge-config/.env ]; then \
		echo "$(RED)✗ Please run 'make init-bridge' first$(NC)"; \
		exit 1; \
	fi
	@./scripts/register-committee.sh

test-bridge: ## Run bridge transfer tests
	@echo "$(YELLOW)Testing bridge transfers...$(NC)"
	@./scripts/test-bridge.sh

stop-all: ## Stop all containers and processes
	@echo "$(YELLOW)Stopping all services...$(NC)"
	@docker-compose down
	@pkill -f "sui start" || true
	@pkill -f "starcoin-bridge-bridge" || true
	@echo "$(GREEN)✓ All services stopped$(NC)"

clean-all: ## Clean all generated files and containers
	@echo "$(YELLOW)Cleaning all generated files...$(NC)"
	@rm -rf bridge-config
	@rm -rf ~/.sui/bridge_keys
	@docker-compose down -v
	@echo "$(GREEN)✓ Cleaned$(NC)"

restart-all: ## Full restart (clean + rebuild + deploy ETH + auto-generate bridge config)
	@echo "$(YELLOW)╔════════════════════════════════════════╗$(NC)"
	@echo "$(YELLOW)║  Starcoin Bridge - Full Restart       ║$(NC)"
	@echo "$(YELLOW)╚════════════════════════════════════════╝$(NC)"
	@echo ""
	@echo "$(YELLOW)Step 1/5: Cleaning old data...$(NC)"
	@docker-compose down -v 2>/dev/null || true
	@rm -rf bridge-config bridge-node 2>/dev/null || true
	@echo "$(GREEN)✓ Cleaned$(NC)"
	@echo ""
	@echo "$(YELLOW)Step 2/5: Building Rust binaries...$(NC)"
	@cd .. && cargo build --bin starcoin-bridge --bin keygen --quiet
	@echo "$(GREEN)✓ Binaries built$(NC)"
	@echo ""
	@echo "$(YELLOW)Step 3/5: Starting ETH network...$(NC)"
	@docker-compose up -d
	@echo "   Waiting for ETH contracts deployment..."
	@SUCCESS=0; \
	for i in $$(seq 1 30); do \
		if curl -sf http://localhost:8080/deployment.json > /dev/null 2>&1; then \
			SUCCESS=1; \
			echo "   $(GREEN)✓ ETH contracts deployed (took $$((i*2))s)$(NC)"; \
			break; \
		fi; \
		printf "   ⏳ %ds/%ds\r" "$$((i*2))" "60"; \
		sleep 2; \
	done; \
	if [ $$SUCCESS -eq 0 ]; then \
		echo "\n   $(RED)✗ Timeout after 60s$(NC)"; \
		echo "   $(YELLOW)Check: docker logs bridge-eth-deployer$(NC)"; \
		exit 1; \
	fi
	@echo ""
	@echo "$(YELLOW)Step 4/5: Auto-generating bridge configuration...$(NC)"
	@./scripts/auto-gen-config.sh
	@echo ""
	@echo "$(YELLOW)Step 5/5: Verifying setup...$(NC)"
	@if [ -f bridge-config/server-config.yaml ]; then \
		echo "$(GREEN)✓ Configuration file ready$(NC)"; \
	else \
		echo "$(RED)✗ Configuration generation failed$(NC)"; \
		exit 1; \
	fi
	@echo ""
	@echo "$(GREEN)╔════════════════════════════════════════╗$(NC)"
	@echo "$(GREEN)║  ✅ Full restart complete!             ║$(NC)"
	@echo "$(GREEN)╚════════════════════════════════════════╝$(NC)"
	@echo ""
	@echo "$(YELLOW)Bridge is ready to start!$(NC)"
	@echo "Run: $(GREEN)make start-bridge$(NC)"
	@echo ""
	@echo "$(YELLOW)Or run manually:$(NC)"
	@echo "  $(GREEN)cd .. && RUST_LOG=info,starcoin_bridge=debug \\$(NC)"
	@echo "  $(GREEN)  ./target/debug/starcoin-bridge \\$(NC)"
	@echo "  $(GREEN)  --config-path bridge/bridge-config/server-config.yaml$(NC)"

status: ## Show current deployment status
	@echo "$(YELLOW)=== Deployment Status ===$(NC)"
	@echo ""
	@echo "$(YELLOW)ETH Network:$(NC)"
	@docker ps --filter "name=bridge-eth" --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}" 2>/dev/null || echo "  $(RED)✗ Not running$(NC)"
	@echo ""
	@echo "$(YELLOW)Bridge Config:$(NC)"
	@if [ -d bridge-config ]; then \
		for file in client-config.yaml eth-deployment.json server-config.yaml SETUP_SUMMARY.txt; do \
			if [ -f "bridge-config/$$file" ]; then \
				echo "  $(GREEN)✓$(NC) $$file $(shell pwd)/bridge-config/$$file"; \
			else \
				echo "  $(RED)✗$(NC) $$file (missing)"; \
			fi; \
		done; \
	else \
		echo "  $(RED)✗ Not initialized (run: make init-bridge)$(NC)"; \
	fi
	@echo ""
	@echo "$(YELLOW)Bridge Keys:$(NC)"
	@if [ -d ~/.sui/bridge_keys ]; then \
		for file in bridge_client_key validator_0_bridge_key; do \
			if [ -f "$$HOME/.sui/bridge_keys/$$file" ]; then \
				echo "  $(GREEN)✓$(NC) $$file $$HOME/.sui/bridge_keys/$$file"; \
			else \
				echo "  $(RED)✗$(NC) $$file (missing)"; \
			fi; \
		done; \
	else \
		echo "  $(RED)✗ Not generated (run: make init-bridge)$(NC)"; \
	fi

bridge-info: ## Show bridge deployment information
	@if [ -f bridge-config/SETUP_SUMMARY.txt ]; then \
		cat bridge-config/SETUP_SUMMARY.txt; \
	else \
		echo "$(RED)✗ No setup summary found. Run 'make init-bridge' first.$(NC)"; \
	fi

check: ## Check if services are healthy
	@echo "Checking Ethereum node..."
	@docker exec bridge-eth-node cast block-number --rpc-url http://localhost:8545 || echo "❌ Ethereum node not accessible"
	@echo "Checking deployment info..."
	@curl -s http://localhost:8080/health > /dev/null && echo "✅ Deployment info server is running" || echo "❌ Deployment info not accessible"

# === Starcoin Bridge Integration ===

starcoin-node: ## Start fresh Starcoin dev node (foreground)
	@echo "$(YELLOW)Starting fresh Starcoin dev node...$(NC)"
	@echo "$(RED)Warning: This will remove existing dev data!$(NC)"
	@rm -rf /Users/manager/dev
	@rm -rf ~/.starcoin
	@cd .. && cargo build --bin starcoin
	@echo "$(GREEN)✓ Built starcoin binary$(NC)"
	@echo "$(YELLOW)Starting Starcoin console...$(NC)"
	@cd .. && ./target/debug/starcoin -n dev -d /Users/manager console

deploy-starcoin-bridge: ## Deploy bridge Move contract to Starcoin
	@echo "$(YELLOW)Deploying Starcoin Bridge Move contract...$(NC)"
	@if [ ! -f ../target/debug/starcoin ]; then \
		echo "$(RED)✗ Starcoin binary not found. Run 'make starcoin-node' first$(NC)"; \
		exit 1; \
	fi
	@if ! pgrep -f "starcoin.*dev.*console" > /dev/null; then \
		echo "$(RED)✗ Starcoin node not running. Start it with 'make starcoin-node'$(NC)"; \
		exit 1; \
	fi
	@echo "$(YELLOW)Building Move package...$(NC)"
	@cd stc-bridge-move && mpm release
	@echo "$(GREEN)✓ Move package built$(NC)"
	@echo "$(YELLOW)Deploying to Starcoin...$(NC)"
	@cd stc-bridge-move && starcoin -c ws://127.0.0.1:9870 --local-account-dir ~/.starcoin/dev/account_vaults dev deploy release/Stc-Bridge-Move.v0.0.1.blob -b
	@echo "$(GREEN)✓ Bridge contract deployed$(NC)"
	@echo ""
	@echo "$(YELLOW)Contract Address:$(NC) $$(grep -A1 '\[addresses\]' stc-bridge-move/Move.toml | grep Bridge | cut -d'"' -f2)"

start-bridge: ## Start bridge node
	@echo "$(YELLOW)Starting Starcoin Bridge node...$(NC)"
	@echo ""
	@echo "$(YELLOW)Checking prerequisites...$(NC)"
	@if [ ! -f bridge-config/server-config.yaml ]; then \
		echo "$(RED)✗ Bridge config not found$(NC)"; \
		echo "$(YELLOW)Run: make restart-all$(NC)"; \
		exit 1; \
	fi
	@echo "$(GREEN)✓ Config found$(NC)"
	@if ! docker ps | grep -q bridge-eth-node; then \
		echo "$(RED)✗ ETH node not running$(NC)"; \
		echo "$(YELLOW)Run: make restart-all$(NC)"; \
		exit 1; \
	fi
	@echo "$(GREEN)✓ ETH node running$(NC)"
	@if [ ! -f ../target/debug/starcoin-bridge ]; then \
		echo "$(YELLOW)Building bridge binary...$(NC)"; \
		cd .. && cargo build --bin starcoin-bridge --quiet; \
		echo "$(GREEN)✓ Bridge binary built$(NC)"; \
	else \
		echo "$(GREEN)✓ Bridge binary exists$(NC)"; \
	fi
	@echo ""
	@echo "$(YELLOW)Bridge Configuration:$(NC)"
	@ETH_ADDR=$$(grep "Ethereum address:" bridge-config/server-config.yaml | awk '{print $$4}' || echo "N/A"); \
	ETH_PROXY=$$(grep "eth-bridge-proxy-address:" bridge-config/server-config.yaml | awk '{print $$2}'); \
	echo "  Bridge Authority ETH Address: $$ETH_ADDR"; \
	echo "  ETH Proxy Contract: $$ETH_PROXY"; \
	echo "  ETH RPC: http://localhost:8545"; \
	echo "  Starcoin RPC: ws://127.0.0.1:9870"
	@echo ""
	@echo "$(GREEN)Starting bridge server...$(NC)"
	@echo ""
	@cd .. && RUST_LOG=info,starcoin_bridge=debug \
		./target/debug/starcoin-bridge \
		--config-path bridge/bridge-config/server-config.yaml
