.PHONY: help deploy-eth-network deploy-native deploy-docker start stop restart logs clean info test init-bridge-config deploy-sui register test-bridge stop-eth-network clean-eth-and-config setup-eth-and-config status logs-deployer start-starcoin-dev-node start-starcoin-dev-node-clean run-bridge-server build-starcoin-contracts deploy-starcoin-contracts stop-starcoin-dev-node

# Colors
GREEN  := \033[0;32m
YELLOW := \033[1;33m
RED    := \033[0;31m
NC     := \033[0m # No Color

# Starcoin Configuration
STARCOIN_PATH ?= starcoin
MPM_PATH ?= mpm
STARCOIN_DEV_DIR ?= /Users/manager/dev
STARCOIN_RPC ?= $(STARCOIN_DEV_DIR)/starcoin.ipc
STARCOIN_ACCOUNT_DIR ?= $(STARCOIN_DEV_DIR)/account_vaults
MOVE_CONTRACT_DIR ?= ../stc-bridge-move
BRIDGE_ADDRESS ?= 0x246b237c16c761e9478783dd83f7004a

help: ## Show this help message
	@echo '$(GREEN)Starcoin Bridge Deployment Automation$(NC)'
	@echo ''
	@echo '$(YELLOW)Environment Variables:$(NC)'
	@echo '  STARCOIN_PATH        Path to starcoin binary (default: starcoin)'
	@echo '  MPM_PATH             Path to mpm binary (default: mpm)'
	@echo '  STARCOIN_RPC         Starcoin RPC URL (default: ws://127.0.0.1:9870)'
	@echo '  MOVE_CONTRACT_DIR    Move contracts directory (default: ../stc-bridge-move)'
	@echo '  BRIDGE_ADDRESS       Bridge contract address (default: 0x246b237c16c761e9478783dd83f7004a)'
	@echo ''
	@echo '$(YELLOW)Available targets:$(NC)'
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  \033[36m%-25s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

deploy-eth-network: ## Deploy Ethereum network using Docker Compose (Anvil + contracts)
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

init-bridge-config: ## Initialize Bridge keys and configs (requires ETH network running)
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

stop-eth-network: ## Stop ETH Docker containers
	@echo "$(YELLOW)Stopping ETH network...$(NC)"
	@docker-compose down
	@echo "$(GREEN)✓ ETH network stopped$(NC)"

clean-eth-and-config: ## Clean ETH containers, bridge-config/ and keys
	@echo "$(YELLOW)Cleaning ETH network and bridge configuration...$(NC)"
	@rm -rf bridge-config
	@rm -rf ~/.sui/bridge_keys
	@docker-compose down -v
	@echo "$(GREEN)✓ Cleaned$(NC)"

setup-eth-and-config: ## Complete ETH setup (clean + deploy ETH network + generate bridge config)
	@echo "$(YELLOW)╔════════════════════════════════════════╗$(NC)"
	@echo "$(YELLOW)║  ETH Network & Config Setup            ║$(NC)"
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
	@echo "$(GREEN)║  ✅ ETH setup complete!                ║$(NC)"
	@echo "$(GREEN)╚════════════════════════════════════════╝$(NC)"
	@echo ""
	@echo "$(YELLOW)Next steps:$(NC)"
	@echo "1. Start Starcoin: $(GREEN)make start-starcoin-dev-node$(NC)"
	@echo "2. Deploy contracts: $(GREEN)make deploy-starcoin-contracts$(NC)"
	@echo "3. Start bridge: $(GREEN)make run-bridge-server$(NC)"

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

start-starcoin-dev-node-clean: ## Start Starcoin dev node from scratch (removes ~/.starcoin/dev)
	@echo "$(YELLOW)╔════════════════════════════════════════╗$(NC)"
	@echo "$(YELLOW)║  Starting Starcoin Dev Node (Clean)   ║$(NC)"
	@echo "$(YELLOW)╚════════════════════════════════════════╝$(NC)"
	@echo ""
	@echo "$(RED)Warning: This will remove existing dev data!$(NC)"
	@echo "$(YELLOW)Dev directory: $(STARCOIN_DEV_DIR)$(NC)"
	@echo ""
	@rm -rf $(STARCOIN_DEV_DIR)
	@echo "$(GREEN)✓ Cleaned dev data$(NC)"
	@echo "$(YELLOW)Starting Starcoin console...$(NC)"
	@echo "$(YELLOW)Using: $(STARCOIN_PATH)$(NC)"
	@$(STARCOIN_PATH) -n dev console

start-starcoin-dev-node: ## Start Starcoin dev node with existing data (keeps ~/.starcoin/dev)
	@echo "$(YELLOW)╔════════════════════════════════════════╗$(NC)"
	@echo "$(YELLOW)║  Starting Starcoin Dev Node            ║$(NC)"
	@echo "$(YELLOW)╚════════════════════════════════════════╝$(NC)"
	@echo ""
	@if [ -d "$(STARCOIN_DEV_DIR)" ]; then \
		echo "$(GREEN)✓ Using existing dev data: $(STARCOIN_DEV_DIR)$(NC)"; \
	else \
		echo "$(YELLOW)⚠ No existing dev data found, will create new$(NC)"; \
	fi
	@echo "$(YELLOW)Starting Starcoin console...$(NC)"
	@echo "$(YELLOW)Using: $(STARCOIN_PATH)$(NC)"
	@$(STARCOIN_PATH) -n dev console

stop-starcoin-dev-node: ## Stop Starcoin dev node processes
	@echo "$(YELLOW)Stopping Starcoin dev node...$(NC)"
	@pkill -f "starcoin.*dev.*console" 2>/dev/null || true
	@echo "$(GREEN)✓ Starcoin node stopped$(NC)"

build-starcoin-contracts: ## Build Starcoin Move contracts using mpm
	@echo "$(YELLOW)Building Move contracts...$(NC)"
	@echo "$(YELLOW)Contract directory: $(MOVE_CONTRACT_DIR)$(NC)"
	@echo "$(YELLOW)Using: $(MPM_PATH)$(NC)"
	@if [ ! -d "$(MOVE_CONTRACT_DIR)" ]; then \
		echo "$(RED)✗ Move contract directory not found: $(MOVE_CONTRACT_DIR)$(NC)"; \
		exit 1; \
	fi
	@cd $(MOVE_CONTRACT_DIR) && $(MPM_PATH) release
	@echo "$(GREEN)✓ Move package built$(NC)"
	@echo ""
	@echo "$(YELLOW)Package location:$(NC)"
	@ls -lh $(MOVE_CONTRACT_DIR)/release/*.blob

deploy-starcoin-contracts: build-starcoin-contracts ## Deploy Move contracts to Starcoin dev network (builds first)
	@echo ""
	@echo "$(YELLOW)╔════════════════════════════════════════╗$(NC)"
	@echo "$(YELLOW)║  Deploying Move Contracts              ║$(NC)"
	@echo "$(YELLOW)╚════════════════════════════════════════╝$(NC)"
	@echo ""
	@echo "$(YELLOW)Checking Starcoin node...$(NC)"
	@if ! pgrep -f "starcoin.*dev.*console" > /dev/null; then \
		echo "$(RED)✗ Starcoin node not running$(NC)"; \
		echo "$(YELLOW)Start it with: make start-starcoin-dev-node$(NC)"; \
		exit 1; \
	fi
	@echo "$(GREEN)✓ Starcoin node is running$(NC)"
	@echo ""
	@echo "$(YELLOW)Getting test coins for deployment...$(NC)"
	@echo "$(BLUE)Executing: RUST_LOG=info $(STARCOIN_PATH) -c $(STARCOIN_RPC) --local-account-dir $(STARCOIN_ACCOUNT_DIR) dev get-coin -v 1000000000$(NC)"
	@RUST_LOG=info $(STARCOIN_PATH) -c $(STARCOIN_RPC) --local-account-dir $(STARCOIN_ACCOUNT_DIR) dev get-coin -v 1000000000 && \
	echo "$(GREEN)✓ Got 1000 STC for gas$(NC)" || \
	echo "$(YELLOW)⚠ Failed to get coins (might already have enough)$(NC)"
	@echo ""
	@echo "$(YELLOW)Deployment Configuration:$(NC)"
	@echo "  RPC URL: $(STARCOIN_RPC)"
	@echo "  Account Dir: $(STARCOIN_ACCOUNT_DIR)"
	@echo "  Bridge Address: $(BRIDGE_ADDRESS)"
	@echo "  Using: $(STARCOIN_PATH)"
	@echo ""
	@BLOB_FILE=$$(ls $(MOVE_CONTRACT_DIR)/release/*.blob | head -1); \
	if [ -z "$$BLOB_FILE" ]; then \
		echo "$(RED)✗ No blob file found$(NC)"; \
		exit 1; \
	fi; \
	echo "$(YELLOW)Deploying: $$BLOB_FILE$(NC)"; \
	echo "$(YELLOW)This may take 10-30 seconds...$(NC)"; \
	echo ""; \
	echo "$(BLUE)Executing: RUST_LOG=info $(STARCOIN_PATH) -c $(STARCOIN_RPC) --local-account-dir $(STARCOIN_ACCOUNT_DIR) dev deploy $$BLOB_FILE -b$(NC)"; \
	RUST_LOG=info $(STARCOIN_PATH) -c $(STARCOIN_RPC) --local-account-dir $(STARCOIN_ACCOUNT_DIR) dev deploy $$BLOB_FILE -b && \
	echo "" && \
	echo "$(GREEN)✓ Bridge contract deployed successfully$(NC)" && \
	echo "" && \
	echo "$(YELLOW)Contract Address: $(BRIDGE_ADDRESS)$(NC)"

run-bridge-server: ## Start bridge server (requires ETH network + Starcoin node + configs)
	@echo "$(YELLOW)Starting Starcoin Bridge server...$(NC)"
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
