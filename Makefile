# ============================================================
# Starcoin Bridge - Makefile
# ============================================================
# Automates deployment and management of Starcoin <-> Ethereum bridge
# Prerequisites: Docker, Rust, Starcoin CLI, mpm (Move Package Manager)
# ============================================================

.PHONY: help deploy-eth-network deploy-native deploy-docker start stop restart logs clean info test init-bridge-config deploy-sui register test-bridge stop-eth-network clean-eth-and-config setup-eth-and-config status logs-deployer start-starcoin-dev-node start-starcoin-dev-node-clean run-bridge-server build-starcoin-contracts deploy-starcoin-contracts stop-starcoin-dev-node

# ============================================================
# Colors for terminal output
# ============================================================
GREEN  := \033[0;32m
YELLOW := \033[1;33m
RED    := \033[0;31m
NC     := \033[0m # No Color

# ============================================================
# Safe Delete Function - Requires user confirmation (unless FORCE_YES=1)
# ============================================================
define safe_rm
	@if [ "$(FORCE_YES)" = "1" ]; then \
		echo "$(YELLOW)⚠️  Auto-deleting (forced):$(NC) $(RED)$(1)$(NC)"; \
		rm -rf $(1); \
		echo "$(GREEN)✓ Deleted$(NC)"; \
	else \
		echo "$(YELLOW)⚠️  Warning: About to delete:$(NC)"; \
		echo "$(RED)  $(1)$(NC)"; \
		printf "$(YELLOW)Do you want to continue? (y/N): $(NC)"; \
		read -n 1 -r REPLY; \
		echo; \
		if [ "$$REPLY" = "y" ] || [ "$$REPLY" = "Y" ]; then \
			rm -rf $(1); \
			echo "$(GREEN)✓ Deleted$(NC)"; \
		else \
			echo "$(YELLOW)✗ Cancelled$(NC)"; \
			exit 1; \
		fi; \
	fi
endef

# ============================================================
# Configuration Variables (override with env vars)
# ============================================================
# Parent directory for dev node
STARCOIN_DEV_PARENT_DIR ?= /tmp
# Dev node data directory
STARCOIN_DEV_DIR ?= $(STARCOIN_DEV_PARENT_DIR)/dev
# IPC socket for RPC
STARCOIN_RPC ?= $(STARCOIN_DEV_DIR)/starcoin.ipc
# Account vaults directory
STARCOIN_ACCOUNT_DIR ?= $(STARCOIN_DEV_DIR)/account_vaults
# Move contracts location
MOVE_CONTRACT_DIR ?= ../stc-bridge-move
# Deployed bridge address
BRIDGE_ADDRESS ?= 0x246b237c16c761e9478783dd83f7004a

# ============================================================
# Help & Documentation
# ============================================================
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

# ============================================================
# Ethereum Network Management
# ============================================================
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

# ============================================================
# Bridge Configuration Setup
# ============================================================
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

clean-eth-and-config: ## Clean ETH containers, bridge-config/ and keys. Use FORCE_YES=1 to skip confirmation
	@echo "$(YELLOW)Cleaning ETH network and bridge configuration...$(NC)"
	$(call safe_rm,bridge-config)
	$(call safe_rm,~/.sui/bridge_keys)
	@docker-compose down -v
	@echo "$(GREEN)✓ Cleaned$(NC)"

# ============================================================
# Automated ETH + Config Setup (one command deployment)
# ============================================================
setup-eth-and-config: ## Complete ETH setup (clean + deploy ETH network + generate bridge config). Use FORCE_YES=1 to skip confirmation
	@echo "$(YELLOW)╔════════════════════════════════════════╗$(NC)"
	@echo "$(YELLOW)║  ETH Network & Config Setup            ║$(NC)"
	@echo "$(YELLOW)╚════════════════════════════════════════╝$(NC)"
	@echo ""
	# Clean old data
	@echo "$(YELLOW)Step 1/6: Cleaning old data...$(NC)"
	@docker-compose down -v 2>/dev/null || true
	$(call safe_rm,bridge-config bridge-node)
	@echo "$(GREEN)✓ Cleaned$(NC)"
	@echo ""
	# Build keygen tool for config generation
	@echo "$(YELLOW)Step 2/6: Building keygen and CLI tools...$(NC)"
	@cargo build --bin keygen --bin starcoin-bridge-cli --quiet
	@echo "$(GREEN)✓ Tools built$(NC)"
	@echo ""
	# Generate bridge authority key BEFORE deploying ETH contracts
	@echo "$(YELLOW)Step 3/6: Generating bridge authority key...$(NC)"
	@mkdir -p bridge-node/server-config
	@./target/debug/keygen authority --output bridge-node/server-config/bridge_authority.key > /tmp/keygen_output.txt 2>&1
	@ETH_ADDRESS=$$(grep "Ethereum address:" /tmp/keygen_output.txt | awk '{print $$3}'); \
	if [ -z "$$ETH_ADDRESS" ]; then \
		echo "$(RED)✗ Failed to generate bridge authority key$(NC)"; \
		exit 1; \
	fi; \
	echo "$(GREEN)✓ Bridge authority key generated$(NC)"; \
	echo "   📍 Ethereum address: $$ETH_ADDRESS"
	@echo ""
	# Generate Starcoin client key (Ed25519)
	@echo "$(YELLOW)Step 4/6: Generating Starcoin client key (Ed25519)...$(NC)"
	@./target/debug/starcoin-bridge-cli create-bridge-client-key bridge-node/server-config/starcoin_client.key > /tmp/starcoin_key_output.txt 2>&1
	@STARCOIN_CLIENT_ADDRESS=$$(grep "Starcoin address:" /tmp/starcoin_key_output.txt | awk '{print $$NF}'); \
	echo "$(GREEN)✓ Starcoin client key generated$(NC)"; \
	echo "   📍 Starcoin address: $$STARCOIN_CLIENT_ADDRESS"
	@echo ""
	# Update ETH deploy config with the generated committee member address, then deploy
	@echo "$(YELLOW)Step 5/6: Updating ETH config and deploying...$(NC)"
	@ETH_ADDRESS=$$(grep "Ethereum address:" /tmp/keygen_output.txt | awk '{print $$3}'); \
	ETH_ADDRESS_LOWER=$$(echo "$$ETH_ADDRESS" | tr '[:upper:]' '[:lower:]'); \
	CONFIG_FILE="contracts/evm/deploy_configs/31337.json"; \
	echo "   Updating $$CONFIG_FILE with committee: $$ETH_ADDRESS_LOWER"; \
	jq --arg addr "$$ETH_ADDRESS_LOWER" '.committeeMembers = [$$addr] | .committeeMemberStake = [10000]' "$$CONFIG_FILE" > /tmp/31337_new.json && \
	mv /tmp/31337_new.json "$$CONFIG_FILE"
	@echo "   Starting ETH network..."
	@docker-compose up -d
	@echo "   Waiting for ETH contracts deployment..."
	@SUCCESS=0; \
	for i in $$(seq 1 120); do \
		if curl -sf http://localhost:8080/deployment.json > /dev/null 2>&1 || \
		   curl -sf http://localhost:8080/deployment.txt > /dev/null 2>&1; then \
			SUCCESS=1; \
			echo "   $(GREEN)✓ ETH contracts deployed (took $$((i*2))s)$(NC)"; \
			break; \
		fi; \
		printf "   ⏳ %ds/%ds\r" "$$((i*2))" "240"; \
		sleep 2; \
	done; \
	if [ $$SUCCESS -eq 0 ]; then \
		echo "\n   $(RED)✗ Timeout after 240s$(NC)"; \
		echo "   $(YELLOW)Check: docker logs bridge-eth-deployer$(NC)"; \
		exit 1; \
	fi
	@echo ""
	# Generate bridge configuration files (skip key generation since we already did it)
	@echo "$(YELLOW)Step 6/6: Auto-generating bridge configuration...$(NC)"
	@./scripts/auto-gen-config.sh --skip-keygen
	@echo ""
	# Verify configuration
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

# ============================================================
# Status & Monitoring
# ============================================================
status: ## Show current deployment status
	@echo "$(YELLOW)╔════════════════════════════════════════╗$(NC)"
	@echo "$(YELLOW)║  Starcoin Bridge - Status              ║$(NC)"
	@echo "$(YELLOW)╚════════════════════════════════════════╝$(NC)"
	@echo ""
	@echo "$(YELLOW)ETH Network:$(NC)"
	@if docker ps --filter "name=bridge-eth-node" --format "{{.Names}}" 2>/dev/null | grep -q bridge-eth-node; then \
		ETH_STATUS=$$(docker ps --filter "name=bridge-eth-node" --format "{{.Status}}" 2>/dev/null); \
		echo "  $(GREEN)✓ bridge-eth-node$(NC) - $$ETH_STATUS"; \
		echo "  RPC: http://localhost:8545"; \
	else \
		echo "  $(RED)✗ Not running$(NC)"; \
		echo "  $(YELLOW)Start: make deploy-eth-network$(NC)"; \
	fi
	@echo ""
	@echo "$(YELLOW)Starcoin Node:$(NC)"
	@if $(STARCOIN_PATH) -c $(STARCOIN_RPC) chain info >/dev/null 2>&1; then \
		STARCOIN_PID=$$(ps aux | grep '[s]tarcoin.*-n dev.*-d /tmp' | awk '{print $$2}' | head -1); \
		BLOCK_NUM=$$($(STARCOIN_PATH) -c $(STARCOIN_RPC) chain info 2>/dev/null | grep '"number"' | head -1 | awk -F'"' '{print $$4}'); \
		if [ -n "$$STARCOIN_PID" ]; then \
			echo "  $(GREEN)✓ Running$(NC) (PID: $$STARCOIN_PID, Block: $$BLOCK_NUM)"; \
		else \
			echo "  $(GREEN)✓ Running$(NC) (Block: $$BLOCK_NUM)"; \
		fi; \
		echo "  RPC: $(STARCOIN_RPC)"; \
	else \
		echo "  $(RED)✗ Not running or unreachable$(NC)"; \
		echo "  $(YELLOW)Start: make start-starcoin-dev-node$(NC)"; \
	fi
	@echo ""
	@echo "$(YELLOW)Configuration:$(NC)"
	@if [ -f bridge-config/server-config.yaml ]; then \
		echo "  $(GREEN)✓ server-config.yaml$(NC)"; \
		if [ -f bridge-node/server-config/bridge_authority.key ]; then \
			ETH_ADDR=$$(grep "Ethereum address:" bridge-config/server-config.yaml | awk '{print $$4}' || echo "N/A"); \
			echo "    └─ ETH Address: $$ETH_ADDR"; \
		fi; \
	else \
		echo "  $(RED)✗ server-config.yaml (missing)$(NC)"; \
		echo "  $(YELLOW)Run: make setup-eth-and-config$(NC)"; \
	fi
	@if [ -f bridge-node/server-config/bridge_authority.key ]; then \
		echo "  $(GREEN)✓ bridge_authority.key$(NC)"; \
	else \
		echo "  $(RED)✗ bridge_authority.key (missing)$(NC)"; \
	fi
	@if [ -f bridge-config/bridge.db ]; then \
		echo "  $(GREEN)✓ bridge.db$(NC)"; \
	else \
		echo "  $(YELLOW)⚠ bridge.db (will be created on first run)$(NC)"; \
	fi
	@echo ""
	@echo "$(YELLOW)Deployed Contracts:$(NC)"
	@if docker exec bridge-deployment-info cat /usr/share/nginx/html/deployment.txt 2>/dev/null | grep "ERC1967Proxy" > /dev/null; then \
		echo "  $(GREEN)✓ ETH Contracts:$(NC)"; \
		docker exec bridge-deployment-info cat /usr/share/nginx/html/deployment.txt 2>/dev/null | while read line; do echo "    $$line"; done; \
	else \
		echo "  $(RED)✗ No ETH deployment info$(NC)"; \
	fi
	@echo ""
	@echo "$(YELLOW)Bridge Server:$(NC)"
	@if pgrep -f "bridge-node" > /dev/null 2>&1; then \
		echo "  $(GREEN)✓ Running$(NC) (PID: $$(pgrep -f 'bridge-node'))"; \
		echo "  Port: 9191"; \
	else \
		echo "  $(RED)✗ Not running$(NC)"; \
		echo "  $(YELLOW)Start: make run-bridge-server$(NC)"; \
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

# ============================================================
# Starcoin Node Management
# ============================================================
start-starcoin-dev-node: ## Start Starcoin dev node with existing data (resume mode)
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
	@$(STARCOIN_PATH) -n dev -d $(STARCOIN_DEV_PARENT_DIR) console

stop-starcoin-dev-node: ## Stop Starcoin dev node processes
	@echo "$(YELLOW)Stopping Starcoin dev node...$(NC)"
	@pkill -f "starcoin.*dev.*console" 2>/dev/null || true
	@echo "$(GREEN)✓ Starcoin node stopped$(NC)"

# ============================================================
# Move Contracts Build & Deploy
# ============================================================
build-starcoin-contracts: ## Build Starcoin Move contracts using mpm
	@echo "$(YELLOW)Building Move contracts...$(NC)"
	@echo "$(YELLOW)Contract directory: $(MOVE_CONTRACT_DIR)$(NC)"
	@echo "$(YELLOW)Using: $(MPM_PATH)$(NC)"
	@if [ ! -d "$(MOVE_CONTRACT_DIR)" ]; then \
		echo "$(RED)✗ Move contract directory not found: $(MOVE_CONTRACT_DIR)$(NC)"; \
		exit 1; \
	fi
	# Auto-detect default account and update Move.toml Bridge address
	@echo "$(YELLOW)Getting default account for Bridge address...$(NC)"
	@DEFAULT_ACCOUNT=$$($(STARCOIN_PATH) -c $(STARCOIN_RPC) account list 2>/dev/null | grep -B 1 '"is_default": true' | grep '"address"' | head -1 | sed 's/.*"\(0x[a-fA-F0-9]*\)".*/\1/'); \
	if [ -z "$$DEFAULT_ACCOUNT" ]; then \
		DEFAULT_ACCOUNT=$$($(STARCOIN_PATH) -c $(STARCOIN_RPC) account list 2>/dev/null | grep '"address"' | head -1 | sed 's/.*"\(0x[a-fA-F0-9]*\)".*/\1/'); \
	fi; \
	if [ -z "$$DEFAULT_ACCOUNT" ]; then \
		echo "$(RED)✗ No account found. Is Starcoin node running?$(NC)"; \
		exit 1; \
	fi; \
	echo "$(GREEN)✓ Bridge address: $$DEFAULT_ACCOUNT$(NC)"; \
	echo "$(YELLOW)Updating Move.toml...$(NC)"; \
	sed -i.bak "s/^Bridge = \"0x[a-fA-F0-9]*\"/Bridge = \"$$DEFAULT_ACCOUNT\"/" $(MOVE_CONTRACT_DIR)/Move.toml; \
	rm -f $(MOVE_CONTRACT_DIR)/Move.toml.bak; \
	echo "$(GREEN)✓ Move.toml updated$(NC)"
	@cd $(MOVE_CONTRACT_DIR) && $(MPM_PATH) release
	@echo "$(GREEN)✓ Move package built$(NC)"
	@echo ""
	@echo "$(YELLOW)Package location:$(NC)"
	@ls -lh $(MOVE_CONTRACT_DIR)/release/*.blob

deploy-starcoin-contracts: build-starcoin-contracts ## Deploy Move contracts + initialize committee (full automation)
	@echo ""
	@echo "$(YELLOW)╔════════════════════════════════════════╗$(NC)"
	@echo "$(YELLOW)║  Deploying Move Contracts              ║$(NC)"
	@echo "$(YELLOW)╚════════════════════════════════════════╝$(NC)"
	@echo ""
	# ============================================================
	# Phase 1: Pre-deployment checks and setup
	# ============================================================
	@echo "$(YELLOW)Checking Starcoin node...$(NC)"
	@if ! pgrep -f "starcoin.*dev.*console" > /dev/null; then \
		echo "$(RED)✗ Starcoin node not running$(NC)"; \
		echo "$(YELLOW)Start it with: make start-starcoin-dev-node$(NC)"; \
		exit 1; \
	fi
	@echo "$(GREEN)✓ Starcoin node is running$(NC)"
	@echo ""
	# Auto-detect default account for deployment and gas payment
	@echo "$(YELLOW)Getting default account address...$(NC)"
	@DEFAULT_ACCOUNT=$$($(STARCOIN_PATH) -c $(STARCOIN_RPC) account list 2>/dev/null | grep -B 1 '"is_default": true' | grep '"address"' | head -1 | sed 's/.*"\(0x[a-fA-F0-9]*\)".*/\1/'); \
	if [ -z "$$DEFAULT_ACCOUNT" ]; then \
		echo "$(RED)✗ No default account found$(NC)"; \
		echo "$(YELLOW)Trying to get first account...$(NC)"; \
		DEFAULT_ACCOUNT=$$($(STARCOIN_PATH) -c $(STARCOIN_RPC) account list 2>/dev/null | grep '"address"' | head -1 | sed 's/.*"\(0x[a-fA-F0-9]*\)".*/\1/'); \
		if [ -z "$$DEFAULT_ACCOUNT" ]; then \
			echo "$(RED)✗ No accounts found$(NC)"; \
			exit 1; \
		fi; \
	fi; \
	echo "$(GREEN)✓ Default account: $$DEFAULT_ACCOUNT$(NC)"; \
	echo ""; \
	echo "$(YELLOW)Initializing account on-chain...$(NC)"; \
	echo "$(YELLOW)Getting test coins for deployment (this also initializes the account)...$(NC)"; \
	echo "$(BLUE)Executing: $(STARCOIN_PATH) -c $(STARCOIN_RPC) dev get-coin -v 10000000$(NC)"; \
	$(STARCOIN_PATH) -c $(STARCOIN_RPC) dev get-coin -v 10000000 2>&1 | grep -v "^[0-9].*INFO" && \
	echo "$(GREEN)✓ Got 1000 STC for gas$(NC)" || { \
		echo "$(RED)✗ Failed to get coins for account $$DEFAULT_ACCOUNT$(NC)"; \
		echo "$(YELLOW)Trying without specifying account...$(NC)"; \
		$(STARCOIN_PATH) -c $(STARCOIN_RPC) dev get-coin -v 10000000 2>&1 | grep -v "^[0-9].*INFO" && \
		echo "$(GREEN)✓ Got coins$(NC)" || { \
			echo "$(RED)✗ Failed to get coins$(NC)"; \
			exit 1; \
		}; \
	}; \
	echo ""; \
	echo "$(YELLOW)Unlocking account...$(NC)"; \
	echo "" | $(STARCOIN_PATH) -c $(STARCOIN_RPC) account unlock $$DEFAULT_ACCOUNT -d 300 2>&1 | grep -v "^[0-9].*INFO" && \
	echo "$(GREEN)✓ Account unlocked$(NC)" || \
	echo "$(YELLOW)⚠ Failed to unlock (might already be unlocked)$(NC)"; \
	echo ""; \
	echo "$(YELLOW)Deployment Configuration:$(NC)"; \
	echo "  RPC URL: $(STARCOIN_RPC)"; \
	echo "  Account: $$DEFAULT_ACCOUNT"; \
	echo "  Bridge Address: $$DEFAULT_ACCOUNT"; \
	echo "  Using: $(STARCOIN_PATH)"; \
	echo ""; \
	BLOB_FILE=$$(ls $(MOVE_CONTRACT_DIR)/release/*.blob | head -1); \
	if [ -z "$$BLOB_FILE" ]; then \
		echo "$(RED)✗ No blob file found$(NC)"; \
		exit 1; \
	fi; \
	echo "$(YELLOW)Deploying: $$BLOB_FILE$(NC)"; \
	echo "$(YELLOW)This may take 10-30 seconds...$(NC)"; \
	echo ""; \
	echo "$(BLUE)Executing: $(STARCOIN_PATH) -c $(STARCOIN_RPC) dev deploy $$BLOB_FILE -s $$DEFAULT_ACCOUNT -b$(NC)"; \
	if $(STARCOIN_PATH) -c $(STARCOIN_RPC) dev deploy $$BLOB_FILE -s $$DEFAULT_ACCOUNT -b 2>&1 | tee /tmp/deploy.log | grep -v "^[0-9].*INFO"; then \
		echo ""; \
		echo "$(GREEN)✓ Bridge contract deployed successfully$(NC)"; \
		echo ""; \
		echo "$(YELLOW)Contract Address: $$DEFAULT_ACCOUNT$(NC)"; \
		echo ""; \
	else \
		echo ""; \
		echo "$(RED)✗ Deployment failed$(NC)"; \
		echo "$(YELLOW)Error details:$(NC)"; \
		grep -i "error\|failed\|ERROR" /tmp/deploy.log | head -5; \
		exit 1; \
	fi; \
	echo "$(YELLOW)╔════════════════════════════════════════╗$(NC)" && \
	echo "$(YELLOW)║  Initializing Bridge                   ║$(NC)" && \
	echo "$(YELLOW)╚════════════════════════════════════════╝$(NC)" && \
	echo "" && \
	echo "$(YELLOW)Step 1/3: Initializing Bridge resource...$(NC)" && \
	echo "  Function: $$DEFAULT_ACCOUNT::Bridge::initialize_bridge"; \
	echo "  Chain ID: 254 (devnet)"; \
	echo ""; \
	if echo "" | $(STARCOIN_PATH) -c $(STARCOIN_RPC) account execute-function \
		--function $$DEFAULT_ACCOUNT::Bridge::initialize_bridge \
		--arg 254u8 \
		-s $$DEFAULT_ACCOUNT -b 2>&1 | tee /tmp/init_bridge.log | grep -v "^[0-9].*INFO"; then \
		echo ""; \
		echo "$(GREEN)✓ Bridge initialized successfully$(NC)"; \
	else \
		echo ""; \
		echo "$(RED)✗ Bridge initialization failed$(NC)"; \
		echo "$(YELLOW)Error details:$(NC)"; \
		grep -i "error\|failed\|ERROR" /tmp/init_bridge.log | head -5; \
		exit 1; \
	fi; \
	echo "" && \
	echo "$(YELLOW)Step 2/3: Registering bridge authority...$(NC)" && \
	if [ ! -f bridge-config/server-config.yaml ]; then \
		echo "$(RED)✗ Bridge config not found$(NC)"; \
		echo "$(YELLOW)Please run: make setup-eth-and-config$(NC)"; \
		exit 1; \
	fi; \
	BRIDGE_KEY_PATH=$$(grep "bridge-authority-key-path:" bridge-config/server-config.yaml | awk '{print $$2}'); \
	ETH_ADDRESS=$$(grep "Ethereum address:" bridge-config/server-config.yaml | awk '{print $$4}'); \
	echo "  Bridge key: $$BRIDGE_KEY_PATH"; \
	echo "  ETH address: $$ETH_ADDRESS"; \
	if [ ! -f "$$BRIDGE_KEY_PATH" ]; then \
		echo "$(RED)✗ Bridge authority key not found: $$BRIDGE_KEY_PATH$(NC)"; \
		exit 1; \
	fi; \
	echo ""; \
	echo "$(YELLOW)Extracting public key from key file...$(NC)"; \
	if [ ! -f target/debug/keygen ]; then \
		echo "$(YELLOW)Building keygen tool...$(NC)"; \
		cargo build --bin keygen --quiet || { \
			echo "$(RED)✗ Failed to build keygen$(NC)"; \
			exit 1; \
		}; \
	fi; \
	BRIDGE_PUBKEY=$$(target/debug/keygen examine "$$BRIDGE_KEY_PATH" 2>/dev/null | grep "Public key (hex):" | awk '{print $$NF}'); \
	if [ -z "$$BRIDGE_PUBKEY" ]; then \
		echo "$(RED)✗ Failed to extract public key from $$BRIDGE_KEY_PATH$(NC)"; \
		echo "$(YELLOW)Try running: target/debug/keygen examine $$BRIDGE_KEY_PATH$(NC)"; \
		exit 1; \
	fi; \
	echo "$(GREEN)✓ Public key: $$BRIDGE_PUBKEY$(NC)"; \
	echo ""; \
	echo "$(YELLOW)Step 3/3: Registering on Starcoin chain...$(NC)"; \
	URL_HEX="68747470733a2f2f3132372e302e302e313a39313931"; \
	echo "  Function: $$DEFAULT_ACCOUNT::Bridge::register_committee_member"; \
	echo "  Public key: $$BRIDGE_PUBKEY"; \
	echo "  URL (hex): $$URL_HEX"; \
	echo ""; \
	echo "$(BLUE)Executing registration transaction...$(NC)"; \
	if echo "" | $(STARCOIN_PATH) -c $(STARCOIN_RPC) account execute-function \
		--function $$DEFAULT_ACCOUNT::Bridge::register_committee_member \
		--arg 0x$$BRIDGE_PUBKEY \
		--arg 0x$$URL_HEX \
		-s $$DEFAULT_ACCOUNT -b 2>&1 | tee /tmp/register.log | grep -v "^[0-9].*INFO"; then \
		echo ""; \
		echo "$(GREEN)✓ Bridge authority registered successfully$(NC)"; \
	else \
		echo ""; \
		echo "$(RED)✗ Registration failed$(NC)"; \
		echo "$(YELLOW)Error details:$(NC)"; \
		grep -i "error\|failed\|ERROR" /tmp/register.log | head -5; \
		exit 1; \
	fi; \
	echo "" && \
	echo "$(YELLOW)╔════════════════════════════════════════╗$(NC)" && \
	echo "$(YELLOW)║  Creating Committee                    ║$(NC)" && \
	echo "$(YELLOW)╚════════════════════════════════════════╝$(NC)" && \
	echo "" && \
	echo "$(YELLOW)Validator Configuration:$(NC)" && \
	echo "  Address: $$DEFAULT_ACCOUNT" && \
	echo "  Voting power: 10000 (100%)" && \
	echo "  Min stake: 5000 (50%)" && \
	echo "  Epoch: 0" && \
	echo "" && \
	echo "$(BLUE)Executing: $(STARCOIN_PATH) account execute-function --function $$DEFAULT_ACCOUNT::Bridge::create_committee$(NC)"; \
	if echo "" | $(STARCOIN_PATH) -c $(STARCOIN_RPC) account execute-function \
		--function $$DEFAULT_ACCOUNT::Bridge::create_committee \
		--arg $$DEFAULT_ACCOUNT \
		--arg 10000u64 \
		--arg 5000u64 \
		--arg 0u64 \
		-s $$DEFAULT_ACCOUNT -b 2>&1 | tee /tmp/committee.log | grep -v "^[0-9].*INFO"; then \
		echo ""; \
		echo "$(GREEN)✓ Committee created successfully$(NC)"; \
	else \
		echo ""; \
		echo "$(RED)✗ Committee creation failed$(NC)"; \
		echo "$(YELLOW)Error details:$(NC)"; \
		grep -i "error\|failed\|ERROR" /tmp/committee.log | head -5; \
		exit 1; \
	fi; \
	echo "" && \
	echo "$(YELLOW)Updating server-config.yaml with bridge address...$(NC)" && \
	if [ -f bridge-config/server-config.yaml ]; then \
		sed -i.bak "s|starcoin-bridge-proxy-address:.*|starcoin-bridge-proxy-address: \"$$DEFAULT_ACCOUNT\"|" bridge-config/server-config.yaml; \
		rm -f bridge-config/server-config.yaml.bak; \
		echo "$(GREEN)✓ Config updated with bridge address: $$DEFAULT_ACCOUNT$(NC)"; \
	else \
		echo "$(YELLOW)⚠ server-config.yaml not found, skipping update$(NC)"; \
	fi && \
	echo "" && \
	echo "$(GREEN)╔════════════════════════════════════════╗$(NC)" && \
	echo "$(GREEN)║  ✅ Deployment Complete!               ║$(NC)" && \
	echo "$(GREEN)╚════════════════════════════════════════╝$(NC)" && \
	echo "" && \
	echo "$(YELLOW)Summary:$(NC)" && \
	echo "  • Bridge contract: $$DEFAULT_ACCOUNT" && \
	echo "  • Committee member: $$DEFAULT_ACCOUNT (voting power: 100%)" && \
	echo "  • Bridge authority: $$ETH_ADDRESS" && \
	echo "" && \
	echo "$(YELLOW)Next step:$(NC)" && \
	echo "  $(GREEN)make run-bridge-server$(NC) - Start the bridge server"

# ============================================================
# Bridge Server
# ============================================================
run-bridge-server: ## Start bridge server (requires ETH network + Starcoin node + configs)
	@echo "$(YELLOW)Starting Starcoin Bridge server...$(NC)"
	@echo ""
	# Verify prerequisites
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
	# Always rebuild bridge binary to use latest code
	@echo "$(YELLOW)Building bridge binary...$(NC)"
	@cargo build --bin starcoin-bridge --quiet
	@echo "$(GREEN)✓ Bridge binary built$(NC)"
	@echo ""
	# Show configuration summary
	@echo "$(YELLOW)Bridge Configuration:$(NC)"
	@ETH_ADDR=$$(grep "Ethereum address:" bridge-config/server-config.yaml | awk '{print $$4}' || echo "N/A"); \
	ETH_PROXY=$$(grep "eth-bridge-proxy-address:" bridge-config/server-config.yaml | awk '{print $$2}'); \
	echo "  Bridge Authority ETH Address: $$ETH_ADDR"; \
	echo "  ETH Proxy Contract: $$ETH_PROXY"; \
	echo "  ETH RPC: http://localhost:8545"; \
	echo "  Starcoin RPC: ws://127.0.0.1:9870"
	@echo ""
	# Start bridge server with logging
	@echo "$(GREEN)Starting bridge server...$(NC)"
	@echo ""
	@NO_PROXY=localhost,127.0.0.1 RUST_LOG=info,starcoin_bridge=debug \
		./target/debug/starcoin-bridge \
		--config-path bridge-config/server-config.yaml
