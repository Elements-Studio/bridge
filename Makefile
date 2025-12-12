# ============================================================
# Starcoin Bridge - Makefile
# ============================================================
# Automates deployment and management of Starcoin <-> Ethereum bridge
# Prerequisites: Foundry (anvil, forge, cast), Rust, Starcoin CLI, mpm
# ============================================================

.PHONY: help deploy-eth-network deploy-native deploy-docker start stop restart logs clean info test init-bridge-config deploy-sui register test-bridge stop-eth-network clean-eth-and-config setup-eth-and-config status logs-deployer start-starcoin-dev-node start-starcoin-dev-node-clean run-bridge-server build-starcoin-contracts deploy-starcoin-contracts stop-starcoin-dev-node build-bridge-cli view-bridge deposit-eth deposit-eth-test withdraw-to-eth withdraw-to-eth-test init-cli-config fund-starcoin-bridge-account stop-all bridge-transfer deposit-usdt deposit-usdt-test withdraw-usdt withdraw-usdt-test

# ============================================================
# Colors for terminal output
# ============================================================
GREEN  := \033[0;32m
YELLOW := \033[1;33m
RED    := \033[0;31m
BLUE   := \033[0;34m
NC     := \033[0m # No Color

# ============================================================
# Debug Command Execution Helper
# Usage: $(call debug_exec,command_description,actual_command)
# ============================================================
define debug_exec
	@echo "$(BLUE)[DEBUG] Executing:$(NC)" >&2; \
	echo "  $(2)" >&2; \
	OUTPUT=$$($(2) 2>&1); \
	EXIT_CODE=$$?; \
	echo "$(BLUE)[DEBUG] Response:$(NC)" >&2; \
	echo "$$OUTPUT" >&2; \
	if [ $$EXIT_CODE -ne 0 ]; then \
		echo "$(YELLOW)⚠ Command exited with code $$EXIT_CODE$(NC)" >&2; \
	fi; \
	exit $$EXIT_CODE
endef

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
# Configuration Variables (REQUIRED - must be set by user)
# ============================================================
# Parent directory for dev node (REQUIRED)
ifndef STARCOIN_DATA_DIR
$(error STARCOIN_DATA_DIR is not set. Please set it to the Starcoin data directory, e.g., export STARCOIN_DATA_DIR=/path/to/starcoin/data)
endif
# Dev node data directory
STARCOIN_DEV_DIR = $(STARCOIN_DATA_DIR)/dev
# IPC socket for RPC
STARCOIN_RPC = $(STARCOIN_DEV_DIR)/starcoin.ipc
# Move contracts location (REQUIRED)
ifndef MOVE_CONTRACT_DIR
$(error MOVE_CONTRACT_DIR is not set. Please set it to the stc-bridge-move directory, e.g., export MOVE_CONTRACT_DIR=/path/to/stc-bridge-move)
endif

# ============================================================
# Help & Documentation
# ============================================================
help: ## Show this help message
	@echo '$(GREEN)Starcoin Bridge Deployment Automation$(NC)'
	@echo ''
	@echo '$(RED)Required Environment Variables:$(NC)'
	@echo '  STARCOIN_DATA_DIR    Path to Starcoin data directory (REQUIRED)'
	@echo '  MOVE_CONTRACT_DIR    Path to stc-bridge-move directory (REQUIRED)'
	@echo '  STARCOIN_PATH        Path to starcoin binary (REQUIRED)'
	@echo ''
	@echo '$(YELLOW)Optional Environment Variables:$(NC)'
	@echo '  MPM_PATH             Path to mpm binary (default: mpm)'
	@echo '  STARCOIN_RPC         Starcoin RPC URL (default: ws://127.0.0.1:9870)'
	@echo '  MOVE_CONTRACT_DIR    Move contracts directory (default: ../stc-bridge-move)'
	@echo ''
	@echo '$(YELLOW)Available targets:$(NC)'
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  \033[36m%-25s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# ============================================================
# Ethereum Network Management (Native - no Docker)
# ============================================================
# Anvil data directory (local to project)
ANVIL_DATA_DIR := $(PWD)/.anvil
ANVIL_PID_FILE := $(ANVIL_DATA_DIR)/anvil.pid
ETH_RPC_URL := http://127.0.0.1:8545
ANVIL_PRIVATE_KEY := 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80

start-anvil: ## Start local Anvil node (reuse if running)
	@echo "$(YELLOW)Starting Anvil...$(NC)"
	@mkdir -p $(ANVIL_DATA_DIR)
	@if [ -f $(ANVIL_PID_FILE) ] && kill -0 $$(cat $(ANVIL_PID_FILE)) 2>/dev/null; then \
		echo "$(GREEN)✓ Anvil already running (PID: $$(cat $(ANVIL_PID_FILE)))$(NC)"; \
	else \
		pkill -9 -f "anvil.*8545" 2>/dev/null || true; \
		sleep 1; \
		anvil --host 127.0.0.1 --port 8545 --chain-id 31337 --silent > $(ANVIL_DATA_DIR)/anvil.log 2>&1 & \
		echo $$! > $(ANVIL_PID_FILE); \
		sleep 2; \
		if curl -sf $(ETH_RPC_URL) -X POST -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}' > /dev/null; then \
			echo "$(GREEN)✓ Anvil started (PID: $$(cat $(ANVIL_PID_FILE)))$(NC)"; \
		else \
			echo "$(RED)✗ Anvil failed to start$(NC)"; \
			cat $(ANVIL_DATA_DIR)/anvil.log; \
			exit 1; \
		fi; \
	fi

stop-anvil: ## Stop local Anvil node
	@echo "$(YELLOW)Stopping Anvil...$(NC)"
	@if [ -f $(ANVIL_PID_FILE) ]; then \
		kill $$(cat $(ANVIL_PID_FILE)) 2>/dev/null || true; \
		rm -f $(ANVIL_PID_FILE); \
	fi
	@pgrep -x anvil > /dev/null 2>&1 && pkill -9 -x anvil || true
	@echo "$(GREEN)✓ Anvil stopped$(NC)"

restart-anvil: ## Restart Anvil with clean state (like docker-compose down -v && up)
	@echo "$(YELLOW)Restarting Anvil (clean state)...$(NC)"
	@$(MAKE) stop-anvil
	@rm -rf $(ANVIL_DATA_DIR)
	@mkdir -p $(ANVIL_DATA_DIR)
	@pgrep -x anvil > /dev/null 2>&1 && pkill -9 -x anvil || true
	@sleep 1
	@anvil --host 127.0.0.1 --port 8545 --chain-id 31337 --silent > $(ANVIL_DATA_DIR)/anvil.log 2>&1 & \
	echo $$! > $(ANVIL_PID_FILE); \
	sleep 2; \
	if curl -sf $(ETH_RPC_URL) -X POST -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}' > /dev/null; then \
		echo "$(GREEN)✓ Anvil restarted (PID: $$(cat $(ANVIL_PID_FILE)))$(NC)"; \
	else \
		echo "$(RED)✗ Anvil failed to start$(NC)"; \
		cat $(ANVIL_DATA_DIR)/anvil.log; \
		exit 1; \
	fi

deploy-eth-contracts: start-anvil ## Deploy ETH contracts using local forge
	@echo "$(YELLOW)Deploying ETH contracts...$(NC)"
	@cd contracts/evm && PRIVATE_KEY=$(ANVIL_PRIVATE_KEY) forge script script/deploy_bridge.s.sol \
		--rpc-url $(ETH_RPC_URL) \
		--private-key $(ANVIL_PRIVATE_KEY) \
		--broadcast \
		--legacy \
		-vvv 2>&1 | tee /tmp/forge_deploy.log | grep -E "Deployed|COMPLETE|Error"
	@echo "$(GREEN)✓ ETH contracts deployed$(NC)"

clean: ## Stop Anvil and clean all data
	@$(MAKE) stop-anvil
	@rm -rf $(ANVIL_DATA_DIR)
	@echo "$(GREEN)✓ Cleaned$(NC)"

test-rpc: ## Test RPC connection
	@echo "Testing Ethereum RPC..."
	@curl -s -X POST $(ETH_RPC_URL) \
		-H "Content-Type: application/json" \
		-d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' | jq
	@echo ""
	@curl -s -X POST $(ETH_RPC_URL) \
		-H "Content-Type: application/json" \
		-d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}' | jq

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
# Automated ETH + Config Setup (Native - no Docker)
# ============================================================
setup-eth-and-config: ## Complete ETH setup (clean + deploy + generate config). Use FORCE_YES=1 to skip confirmation
	@echo "$(YELLOW)╔════════════════════════════════════════╗$(NC)"
	@echo "$(YELLOW)║  ETH Network & Config Setup (Native)   ║$(NC)"
	@echo "$(YELLOW)╚════════════════════════════════════════╝$(NC)"
	@echo ""
	# Clean old data
	@echo "$(YELLOW)Step 1/6: Cleaning old data...$(NC)"
	@$(MAKE) stop-anvil 2>/dev/null || true
	$(call safe_rm,bridge-config bridge-node .anvil)
	@echo "$(GREEN)✓ Cleaned$(NC)"
	@echo ""
	# Build keygen tool for config generation
	@echo "$(YELLOW)Step 2/6: Building keygen and CLI tools...$(NC)"
	@cargo build --bin keygen --bin starcoin-bridge-cli --quiet
	@echo "$(GREEN)✓ Tools built$(NC)"
	@echo ""
	# Generate bridge authority key
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
	# Generate Starcoin client key
	@echo "$(YELLOW)Step 4/6: Generating Starcoin client key (Ed25519)...$(NC)"
	@./target/debug/starcoin-bridge-cli create-bridge-client-key bridge-node/server-config/starcoin_client.key > /tmp/starcoin_key_output.txt 2>&1
	@STARCOIN_CLIENT_ADDRESS=$$(grep "Starcoin address:" /tmp/starcoin_key_output.txt | awk '{print $$NF}'); \
	echo "$(GREEN)✓ Starcoin client key generated$(NC)"; \
	echo "   📍 Starcoin address: $$STARCOIN_CLIENT_ADDRESS"
	@echo ""
	# Update ETH deploy config and deploy
	@echo "$(YELLOW)Step 5/6: Deploying ETH contracts (native forge)...$(NC)"
	@ETH_ADDRESS=$$(grep "Ethereum address:" /tmp/keygen_output.txt | awk '{print $$3}'); \
	ETH_ADDRESS_LOWER=$$(echo "$$ETH_ADDRESS" | tr '[:upper:]' '[:lower:]'); \
	CONFIG_FILE="contracts/evm/deploy_configs/31337.json"; \
	TEMPLATE_FILE="contracts/evm/deploy_configs/31337.json.template"; \
	echo "   Creating $$CONFIG_FILE from template with committee member: $$ETH_ADDRESS"; \
	if [ ! -f "$$TEMPLATE_FILE" ]; then \
		echo "$(RED)✗ Template file not found: $$TEMPLATE_FILE$(NC)"; \
		exit 1; \
	fi; \
	jq --arg addr "$$ETH_ADDRESS_LOWER" '.committeeMembers = [$$addr] | .committeeMemberStake = [10000]' "$$TEMPLATE_FILE" > "$$CONFIG_FILE"; \
	echo "$(GREEN)✓ Deploy config created$(NC)"
	# Restart Anvil with clean state
	@$(MAKE) restart-anvil
	# Deploy contracts with forge
	@echo "   Deploying contracts..."
	@cd contracts/evm && PRIVATE_KEY=$(ANVIL_PRIVATE_KEY) forge script script/deploy_bridge.s.sol \
		--rpc-url $(ETH_RPC_URL) \
		--private-key $(ANVIL_PRIVATE_KEY) \
		--broadcast \
		--legacy \
		-vvv 2>&1 | tee /tmp/forge_deploy.log | grep -E "Deployed|COMPLETE" || true
	@echo "$(GREEN)✓ ETH contracts deployed$(NC)"
	# Extract proxy address from forge output
	@ETH_PROXY=$$(grep "\[Deployed\] SuiBridge:" /tmp/forge_deploy.log | grep -o '0x[a-fA-F0-9]*' | head -1); \
	if [ -z "$$ETH_PROXY" ]; then \
		echo "$(RED)✗ Failed to extract proxy address$(NC)"; \
		cat /tmp/forge_deploy.log | tail -20; \
		exit 1; \
	fi; \
	echo "   📍 ETH Proxy Address: $$ETH_PROXY"; \
	echo "$$ETH_PROXY" > /tmp/eth_proxy_address.txt
	@echo ""
	# Generate bridge configuration
	@echo "$(YELLOW)Step 6/6: Generating bridge configuration...$(NC)"
	@mkdir -p bridge-config
	# Generate deployment.txt from forge output
	@echo "NETWORK=Anvil Local Network" > bridge-config/deployment.txt
	@echo "CHAIN_ID=31337" >> bridge-config/deployment.txt
	@echo "RPC_URL=http://localhost:8545" >> bridge-config/deployment.txt
	@echo "DEPLOYER=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266" >> bridge-config/deployment.txt
	@echo "PRIVATE_KEY=$(ANVIL_PRIVATE_KEY)" >> bridge-config/deployment.txt
	@echo "" >> bridge-config/deployment.txt
	@grep "\[Deployed\]" /tmp/forge_deploy.log >> bridge-config/deployment.txt || true
	@ETH_PROXY=$$(cat /tmp/eth_proxy_address.txt); \
	echo "ERC1967Proxy=$$ETH_PROXY" >> bridge-config/deployment.txt
	@echo "$(GREEN)✓ deployment.txt generated$(NC)"
	# Generate server-config.yaml
	@ETH_ADDRESS=$$(grep "Ethereum address:" /tmp/keygen_output.txt | awk '{print $$3}'); \
	ETH_PROXY=$$(cat /tmp/eth_proxy_address.txt); \
	STARCOIN_CLIENT_ADDRESS=$$(grep "Starcoin address:" /tmp/starcoin_key_output.txt | awk '{print $$NF}'); \
	echo "# Starcoin Bridge Server Configuration" > bridge-config/server-config.yaml; \
	echo "# Auto-generated at: $$(date)" >> bridge-config/server-config.yaml; \
	echo "" >> bridge-config/server-config.yaml; \
	echo "# Server settings" >> bridge-config/server-config.yaml; \
	echo "server-listen-port: 9191" >> bridge-config/server-config.yaml; \
	echo "metrics-port: 9184" >> bridge-config/server-config.yaml; \
	echo "" >> bridge-config/server-config.yaml; \
	echo "# Bridge authority key (validator)" >> bridge-config/server-config.yaml; \
	echo "# Ethereum address: $$ETH_ADDRESS" >> bridge-config/server-config.yaml; \
	echo "bridge-authority-key-path: $(PWD)/bridge-node/server-config/bridge_authority.key" >> bridge-config/server-config.yaml; \
	echo "" >> bridge-config/server-config.yaml; \
	echo "# Run client mode" >> bridge-config/server-config.yaml; \
	echo "run-client: true" >> bridge-config/server-config.yaml; \
	echo "" >> bridge-config/server-config.yaml; \
	echo "# Database path" >> bridge-config/server-config.yaml; \
	echo "db-path: $(PWD)/bridge-config/bridge.db" >> bridge-config/server-config.yaml; \
	echo "" >> bridge-config/server-config.yaml; \
	echo "# Approved governance actions" >> bridge-config/server-config.yaml; \
	echo "approved-governance-actions: []" >> bridge-config/server-config.yaml; \
	echo "" >> bridge-config/server-config.yaml; \
	echo "# Ethereum configuration" >> bridge-config/server-config.yaml; \
	echo "eth:" >> bridge-config/server-config.yaml; \
	echo "  eth-rpc-url: http://localhost:8545" >> bridge-config/server-config.yaml; \
	echo "  eth-bridge-proxy-address: $$ETH_PROXY" >> bridge-config/server-config.yaml; \
	echo "  eth-bridge-chain-id: 12" >> bridge-config/server-config.yaml; \
	echo "  eth-contracts-start-block-fallback: 0" >> bridge-config/server-config.yaml; \
	echo "  eth-contracts-start-block-override: 0" >> bridge-config/server-config.yaml; \
	echo "  eth-use-latest-block: true" >> bridge-config/server-config.yaml; \
	echo "" >> bridge-config/server-config.yaml; \
	echo "# Starcoin configuration" >> bridge-config/server-config.yaml; \
	echo "starcoin:" >> bridge-config/server-config.yaml; \
	echo "  starcoin-bridge-rpc-url: http://127.0.0.1:9850" >> bridge-config/server-config.yaml; \
	echo "  starcoin-bridge-chain-id: 2" >> bridge-config/server-config.yaml; \
	echo "  starcoin-bridge-proxy-address: \"\"" >> bridge-config/server-config.yaml; \
	echo "  bridge-client-key-path: $(PWD)/bridge-node/server-config/starcoin_client.key" >> bridge-config/server-config.yaml
	@echo "$(GREEN)✓ server-config.yaml generated$(NC)"
	# Fund bridge authority with ETH
	@ETH_ADDRESS=$$(grep "Ethereum address:" /tmp/keygen_output.txt | awk '{print $$3}'); \
	echo "   Funding bridge authority with 100 ETH..."; \
	cast send $$ETH_ADDRESS --value 100ether --private-key $(ANVIL_PRIVATE_KEY) --rpc-url $(ETH_RPC_URL) > /dev/null 2>&1 && \
	echo "$(GREEN)✓ Funded 100 ETH$(NC)" || echo "$(YELLOW)⚠ Funding skipped$(NC)"
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
	@echo "$(YELLOW)ETH Network (Anvil):$(NC)"
	@if [ -f $(ANVIL_PID_FILE) ] && kill -0 $$(cat $(ANVIL_PID_FILE)) 2>/dev/null; then \
		echo "  $(GREEN)✓ Anvil running$(NC) (PID: $$(cat $(ANVIL_PID_FILE)))"; \
		BLOCK=$$(curl -s -X POST $(ETH_RPC_URL) -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' 2>/dev/null | jq -r '.result' | xargs printf "%d" 2>/dev/null || echo "?"); \
		echo "  Block: $$BLOCK"; \
		echo "  RPC: $(ETH_RPC_URL)"; \
	elif curl -sf $(ETH_RPC_URL) -X POST -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}' > /dev/null 2>&1; then \
		echo "  $(GREEN)✓ Anvil running$(NC) (external)"; \
		echo "  RPC: $(ETH_RPC_URL)"; \
	else \
		echo "  $(RED)✗ Not running$(NC)"; \
		echo "  $(YELLOW)Start: make start-anvil$(NC)"; \
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
	@if [ -f bridge-config/server-config.yaml ]; then \
		ETH_PROXY=$$(grep "eth-bridge-proxy-address:" bridge-config/server-config.yaml | awk '{print $$2}'); \
		if [ -n "$$ETH_PROXY" ]; then \
			echo "  $(GREEN)✓ ETH Proxy:$(NC) $$ETH_PROXY"; \
		else \
			echo "  $(RED)✗ No ETH proxy address in config$(NC)"; \
		fi; \
	else \
		echo "  $(RED)✗ No config file$(NC)"; \
	fi
	@echo ""
	@echo "$(YELLOW)Bridge Server:$(NC)"
	@if pgrep -f "starcoin-bridge" > /dev/null 2>&1; then \
		echo "  $(GREEN)✓ Running$(NC) (PID: $$(pgrep -f 'starcoin-bridge'))"; \
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
	@cast block-number --rpc-url $(ETH_RPC_URL) 2>/dev/null && echo "✅ Ethereum node accessible" || echo "❌ Ethereum node not accessible"
	@echo "Checking Anvil PID..."
	@if [ -f $(ANVIL_PID_FILE) ] && kill -0 $$(cat $(ANVIL_PID_FILE)) 2>/dev/null; then \
		echo "✅ Anvil running (PID: $$(cat $(ANVIL_PID_FILE)))"; \
	else \
		echo "⚠️ Anvil PID file not found or process not running"; \
	fi

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
	@$(STARCOIN_PATH) -n dev -d $(STARCOIN_DATA_DIR) console

stop-starcoin-dev-node: ## Stop Starcoin dev node processes
	@echo "$(YELLOW)Stopping Starcoin dev node...$(NC)"
	@pkill -x "starcoin" 2>/dev/null || true
	@echo "$(GREEN)✓ Starcoin node stopped$(NC)"

stop-all: ## Stop all nodes (Starcoin, Anvil, Bridge server)
	@echo "$(YELLOW)╔════════════════════════════════════════╗$(NC)"
	@echo "$(YELLOW)║  Stopping All Bridge Nodes             ║$(NC)"
	@echo "$(YELLOW)╚════════════════════════════════════════╝$(NC)"
	@echo ""
	@echo "$(BLUE)[1/3] Stopping Bridge Server...$(NC)"
	@pkill -x "starcoin-bridge" 2>/dev/null && echo "$(GREEN)✓ Bridge server stopped$(NC)" || echo "$(YELLOW)⚠ Bridge server not running$(NC)"
	@echo ""
	@echo "$(BLUE)[2/3] Stopping Anvil (ETH node)...$(NC)"
	@$(MAKE) stop-anvil 2>/dev/null || true
	@echo ""
	@echo "$(BLUE)[3/3] Stopping Starcoin dev node...$(NC)"
	@$(MAKE) stop-starcoin-dev-node 2>/dev/null || true
	@echo ""
	@echo "$(GREEN)╔════════════════════════════════════════╗$(NC)"
	@echo "$(GREEN)║  ✅ All Nodes Stopped                  ║$(NC)"
	@echo "$(GREEN)╚════════════════════════════════════════╝$(NC)"
	@echo ""
	@echo "$(YELLOW)Or use the script: ./scripts/stop-all.sh$(NC)"

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
	@if ! $(STARCOIN_PATH) -c $(STARCOIN_RPC) chain info >/dev/null 2>&1; then \
		echo "$(RED)✗ Starcoin node not running or unreachable$(NC)"; \
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
	echo "" | $(STARCOIN_PATH) -c $(STARCOIN_RPC) account execute-function \
		--function $$DEFAULT_ACCOUNT::Bridge::initialize_bridge \
		--arg 254u8 \
		-s $$DEFAULT_ACCOUNT -b 2>&1 | tee /tmp/init_bridge.log | grep -v "^[0-9].*INFO"; \
	if grep -q '"status": "Executed"' /tmp/init_bridge.log; then \
		echo ""; \
		echo "$(GREEN)✓ Bridge initialized successfully$(NC)"; \
	else \
		echo ""; \
		echo "$(RED)✗ Bridge initialization failed$(NC)"; \
		echo "$(YELLOW)Transaction status:$(NC)"; \
		grep -E '"status"|"status_code"|ABORTED|abort_code' /tmp/init_bridge.log | head -5; \
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
	URL_HEX="687474703a2f2f3132372e302e302e313a39313931"; \
	echo "  Function: $$DEFAULT_ACCOUNT::Bridge::register_committee_member"; \
	echo "  Public key: $$BRIDGE_PUBKEY"; \
	echo "  URL (hex): $$URL_HEX"; \
	echo ""; \
	echo "$(BLUE)Executing registration transaction...$(NC)"; \
	echo "" | $(STARCOIN_PATH) -c $(STARCOIN_RPC) account execute-function \
		--function $$DEFAULT_ACCOUNT::Bridge::register_committee_member \
		--arg 0x$$BRIDGE_PUBKEY \
		--arg 0x$$URL_HEX \
		-s $$DEFAULT_ACCOUNT -b 2>&1 | tee /tmp/register.log | grep -v "^[0-9].*INFO"; \
	if grep -q '"status": "Executed"' /tmp/register.log; then \
		echo ""; \
		echo "$(GREEN)✓ Bridge authority registered successfully$(NC)"; \
	else \
		echo ""; \
		echo "$(RED)✗ Registration failed$(NC)"; \
		echo "$(YELLOW)Transaction status:$(NC)"; \
		grep -E '"status"|"status_code"|ABORTED|abort_code' /tmp/register.log | head -5; \
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
	echo "" | $(STARCOIN_PATH) -c $(STARCOIN_RPC) account execute-function \
		--function $$DEFAULT_ACCOUNT::Bridge::create_committee \
		--arg $$DEFAULT_ACCOUNT \
		--arg 10000u64 \
		--arg 5000u64 \
		--arg 0u64 \
		-s $$DEFAULT_ACCOUNT -b 2>&1 | tee /tmp/committee.log | grep -v "^[0-9].*INFO"; \
	if grep -q '"status": "Executed"' /tmp/committee.log; then \
		echo ""; \
		echo "$(GREEN)✓ Committee created successfully$(NC)"; \
	else \
		echo ""; \
		echo "$(RED)✗ Committee creation failed$(NC)"; \
		echo "$(YELLOW)Transaction status:$(NC)"; \
		grep -E '"status"|"status_code"|ABORTED|abort_code' /tmp/committee.log | head -5; \
		exit 1; \
	fi; \
	echo "" && \
	echo "$(YELLOW)╔════════════════════════════════════════╗$(NC)" && \
	echo "$(YELLOW)║  Registering Bridge Tokens             ║$(NC)" && \
	echo "$(YELLOW)╚════════════════════════════════════════╝$(NC)" && \
	echo "" && \
	echo "$(YELLOW)Registering ETH token (ID: 2)...$(NC)" && \
	echo "" | $(STARCOIN_PATH) -c $(STARCOIN_RPC) account execute-function \
		--function $$DEFAULT_ACCOUNT::Bridge::setup_eth_token \
		-s $$DEFAULT_ACCOUNT -b 2>&1 | tee /tmp/setup_eth.log | grep -v "^[0-9].*INFO"; \
	if grep -q '"status": "Executed"' /tmp/setup_eth.log; then \
		echo "$(GREEN)✓ ETH token registered$(NC)"; \
	else \
		echo "$(RED)✗ ETH token registration failed$(NC)"; \
		grep -E '"status"|"status_code"|ABORTED|abort_code|FUNCTION_RESOLUTION_FAILURE' /tmp/setup_eth.log | head -3; \
		exit 1; \
	fi && \
	echo "" && \
	echo "$(YELLOW)Registering BTC token (ID: 1)...$(NC)" && \
	echo "" | $(STARCOIN_PATH) -c $(STARCOIN_RPC) account execute-function \
		--function $$DEFAULT_ACCOUNT::Bridge::setup_btc_token \
		-s $$DEFAULT_ACCOUNT -b 2>&1 | tee /tmp/setup_btc.log | grep -v "^[0-9].*INFO"; \
	if grep -q '"status": "Executed"' /tmp/setup_btc.log; then \
		echo "$(GREEN)✓ BTC token registered$(NC)"; \
	else \
		echo "$(RED)✗ BTC token registration failed$(NC)"; \
		grep -E '"status"|"status_code"|ABORTED|abort_code|FUNCTION_RESOLUTION_FAILURE' /tmp/setup_btc.log | head -3; \
		exit 1; \
	fi && \
	echo "" && \
	echo "$(YELLOW)Registering USDC token (ID: 3)...$(NC)" && \
	echo "" | $(STARCOIN_PATH) -c $(STARCOIN_RPC) account execute-function \
		--function $$DEFAULT_ACCOUNT::Bridge::setup_usdc_token \
		-s $$DEFAULT_ACCOUNT -b 2>&1 | tee /tmp/setup_usdc.log | grep -v "^[0-9].*INFO"; \
	if grep -q '"status": "Executed"' /tmp/setup_usdc.log; then \
		echo "$(GREEN)✓ USDC token registered$(NC)"; \
	else \
		echo "$(RED)✗ USDC token registration failed$(NC)"; \
		grep -E '"status"|"status_code"|ABORTED|abort_code|FUNCTION_RESOLUTION_FAILURE' /tmp/setup_usdc.log | head -3; \
		exit 1; \
	fi && \
	echo "" && \
	echo "$(YELLOW)Registering USDT token (ID: 4)...$(NC)" && \
	echo "" | $(STARCOIN_PATH) -c $(STARCOIN_RPC) account execute-function \
		--function $$DEFAULT_ACCOUNT::Bridge::setup_usdt_token \
		-s $$DEFAULT_ACCOUNT -b 2>&1 | tee /tmp/setup_usdt.log | grep -v "^[0-9].*INFO"; \
	if grep -q '"status": "Executed"' /tmp/setup_usdt.log; then \
		echo "$(GREEN)✓ USDT token registered$(NC)"; \
	else \
		echo "$(RED)✗ USDT token registration failed$(NC)"; \
		grep -E '"status"|"status_code"|ABORTED|abort_code|FUNCTION_RESOLUTION_FAILURE' /tmp/setup_usdt.log | head -3; \
		exit 1; \
	fi && \
	echo "" && \
	echo "$(YELLOW)Updating configuration files with bridge address...$(NC)" && \
	if [ -f bridge-config/server-config.yaml ]; then \
		sed -i.bak 's|starcoin-bridge-proxy-address:.*|starcoin-bridge-proxy-address: "'"$$DEFAULT_ACCOUNT"'"|' bridge-config/server-config.yaml; \
		rm -f bridge-config/server-config.yaml.bak; \
		echo "$(GREEN)✓ server-config.yaml updated$(NC)"; \
	fi && \
	ETH_PROXY=$$(grep "eth-bridge-proxy-address:" bridge-config/server-config.yaml | head -1 | awk '{print $$2}') && \
	echo "$(YELLOW)Generating cli-config.yaml...$(NC)" && \
	echo "# Starcoin Bridge CLI Configuration" > bridge-config/cli-config.yaml && \
	echo "# Auto-generated by move-deploy" >> bridge-config/cli-config.yaml && \
	echo "" >> bridge-config/cli-config.yaml && \
	echo "# Starcoin RPC URL" >> bridge-config/cli-config.yaml && \
	echo "starcoin-bridge-rpc-url: http://127.0.0.1:9850" >> bridge-config/cli-config.yaml && \
	echo "" >> bridge-config/cli-config.yaml && \
	echo "# Ethereum RPC URL" >> bridge-config/cli-config.yaml && \
	echo "eth-rpc-url: http://localhost:8545" >> bridge-config/cli-config.yaml && \
	echo "" >> bridge-config/cli-config.yaml && \
	echo "# Bridge contract address on Starcoin" >> bridge-config/cli-config.yaml && \
	echo "starcoin-bridge-proxy-address: \"$$DEFAULT_ACCOUNT\"" >> bridge-config/cli-config.yaml && \
	echo "" >> bridge-config/cli-config.yaml && \
	echo "# Bridge proxy address on Ethereum" >> bridge-config/cli-config.yaml && \
	echo "eth-bridge-proxy-address: $$ETH_PROXY" >> bridge-config/cli-config.yaml && \
	echo "" >> bridge-config/cli-config.yaml && \
	echo "# Key file paths" >> bridge-config/cli-config.yaml && \
	echo "starcoin-bridge-key-path: $(PWD)/bridge-node/server-config/bridge_authority.key" >> bridge-config/cli-config.yaml && \
	echo "eth-key-path: $(PWD)/bridge-node/server-config/bridge_authority.key" >> bridge-config/cli-config.yaml && \
	echo "$(GREEN)✓ cli-config.yaml generated$(NC)" && \
	echo "" && \
	echo "$(YELLOW)Funding bridge authority with ETH...$(NC)" && \
	if cast send $$ETH_ADDRESS --value 100ether --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 --rpc-url $(ETH_RPC_URL) > /dev/null 2>&1; then \
		echo "$(GREEN)✓ Bridge authority funded with 100 ETH$(NC)"; \
	else \
		echo "$(YELLOW)⚠ Could not fund (may already have balance)$(NC)"; \
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
	echo "  • Tokens: ETH(2), BTC(1), USDC(3), USDT(4)" && \
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
		echo "$(YELLOW)Run: make setup-eth-and-config$(NC)"; \
		exit 1; \
	fi
	@echo "$(GREEN)✓ Config found$(NC)"
	@if ! curl -sf $(ETH_RPC_URL) -X POST -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}' > /dev/null 2>&1; then \
		echo "$(RED)✗ ETH node not running$(NC)"; \
		echo "$(YELLOW)Run: make start-anvil$(NC)"; \
		exit 1; \
	fi
	@echo "$(GREEN)✓ ETH node running$(NC)"
	@if ! $(STARCOIN_PATH) -c $(STARCOIN_RPC) chain info >/dev/null 2>&1; then \
		echo "$(RED)✗ Starcoin node not running or unreachable$(NC)"; \
		echo "$(YELLOW)Run: make start-starcoin-dev-node or check STARCOIN_RPC=$(STARCOIN_RPC)$(NC)"; \
		exit 1; \
	fi
	@echo "$(GREEN)✓ Starcoin node running$(NC)"
	# Always rebuild bridge binary to use latest code
	@echo "$(YELLOW)Building bridge binary...$(NC)"
	@cargo build --bin starcoin-bridge --bin starcoin-bridge-cli --quiet
	@echo "$(GREEN)✓ Bridge binary built$(NC)"
	# Ensure bridge client account is initialized on Starcoin
	@echo "$(YELLOW)Ensuring bridge client account is initialized...$(NC)"
	@$(MAKE) fund-starcoin-bridge-account 2>&1 | grep -E "Bridge account|Funded|Funding|STC|✓|✗" || true
	@echo ""
	# Show configuration summary
	@echo "$(YELLOW)Bridge Configuration:$(NC)"
	@ETH_ADDR=$$(grep "Ethereum address:" bridge-config/server-config.yaml | awk '{print $$4}' || echo "N/A"); \
	ETH_PROXY=$$(grep "eth-bridge-proxy-address:" bridge-config/server-config.yaml | awk '{print $$2}'); \
	STARCOIN_CLIENT_KEY=$$(grep "bridge-client-key-path:" bridge-config/server-config.yaml | awk '{print $$2}'); \
	if [ -f "$$STARCOIN_CLIENT_KEY" ]; then \
		STARCOIN_CLIENT_ADDR=$$(./target/debug/starcoin-bridge-cli examine-key "$$STARCOIN_CLIENT_KEY" 2>/dev/null | grep "Starcoin address:" | awk '{print $$NF}' || echo "N/A"); \
		echo "  Bridge Client Starcoin Address: $$STARCOIN_CLIENT_ADDR"; \
	fi; \
	echo "  Bridge Authority ETH Address: $$ETH_ADDR"; \
	echo "  ETH Proxy Contract: $$ETH_PROXY"; \
	echo "  ETH RPC: http://localhost:8545"; \
	echo "  Starcoin RPC: $(STARCOIN_RPC)"
	@echo ""
	# Start bridge server with logging
	@echo "$(GREEN)Starting bridge server...$(NC)"
	@echo ""
	@NO_PROXY=localhost,127.0.0.1 RUST_LOG=info,starcoin_bridge=debug \
		./target/debug/starcoin-bridge \
		--config-path bridge-config/server-config.yaml

# ============================================================
# Bridge CLI Commands
# ============================================================
BRIDGE_CLI := ./target/debug/starcoin-bridge-cli
CLI_CONFIG := bridge-config/cli-config.yaml
STARCOIN_BRIDGE_ADDRESS := $(shell grep "starcoin-bridge-proxy-address:" bridge-config/server-config.yaml 2>/dev/null | awk '{print $$2}' | tr -d '"')

build-bridge-cli: ## Build bridge CLI tool
	@echo "$(YELLOW)Building bridge CLI...$(NC)"
	@cargo build --bin starcoin-bridge-cli --quiet
	@echo "$(GREEN)✓ Bridge CLI built$(NC)"

view-bridge: build-bridge-cli ## View Starcoin bridge status
	@echo "$(YELLOW)Querying Starcoin Bridge...$(NC)"
	@NO_PROXY=localhost,127.0.0.1 $(BRIDGE_CLI) view-starcoin-bridge \
		--starcoin-bridge-rpc-url http://127.0.0.1:9850 \
		--starcoin-bridge-proxy-address $(STARCOIN_BRIDGE_ADDRESS)

# Transfer parameters (REQUIRED for transfer commands)
# AMOUNT: Required for deposit/withdraw commands
# RECIPIENT: Defaults to STARCOIN_BRIDGE_ADDRESS if not set
RECIPIENT := $(STARCOIN_BRIDGE_ADDRESS)

# Anvil default account private key (10000 ETH)
ANVIL_PRIVATE_KEY := 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80

fund-eth-account: ## Fund ETH account from Anvil default account
	@echo "$(YELLOW)Funding bridge authority with ETH...$(NC)"
	@ETH_ADDRESS=$$($(BRIDGE_CLI) examine-key bridge-node/server-config/bridge_authority.key 2>/dev/null | grep "Ethereum address:" | awk '{print $$NF}'); \
	if [ -z "$$ETH_ADDRESS" ]; then \
		echo "$(RED)✗ Could not get ETH address from key$(NC)"; \
		exit 1; \
	fi; \
	echo "$(YELLOW)Funding $$ETH_ADDRESS with 100 ETH...$(NC)"; \
	echo "$(BLUE)[DEBUG] Executing:$(NC)"; \
	echo "  cast send $$ETH_ADDRESS --value 100ether --private-key $(ANVIL_PRIVATE_KEY) --rpc-url $(ETH_RPC_URL)"; \
	if cast send $$ETH_ADDRESS --value 100ether --private-key $(ANVIL_PRIVATE_KEY) --rpc-url $(ETH_RPC_URL) 2>&1 | tee /tmp/cast_send.log; then \
		echo "$(GREEN)✓ Funded 100 ETH to $$ETH_ADDRESS$(NC)"; \
	else \
		echo "$(YELLOW)⚠ Funding failed (may already have balance)$(NC)"; \
	fi

# Deposit ETH to Starcoin (ETH -> Starcoin)
# AMOUNT: in ETH (e.g., 0.1)
# TOKEN: ETH (currently only native ETH supported for deposit)
deposit-eth: build-bridge-cli fund-eth-account fund-starcoin-bridge-account ## Deposit ETH to Starcoin (usage: make deposit-eth AMOUNT=0.1)
	@$(MAKE) deposit-eth-core AMOUNT=$(AMOUNT)

# Core deposit logic without funding (for scripts that handle funding separately)
deposit-eth-core: build-bridge-cli
	@if [ ! -f bridge-node/server-config/bridge_client.key ]; then \
		echo "$(YELLOW)Creating bridge client key (Ed25519 for Starcoin)...$(NC)"; \
		$(BRIDGE_CLI) create-bridge-client-key bridge-node/server-config/bridge_client.key; \
	fi
	@# Fund recipient account with STC so it exists on chain (required for token transfer)
	@RECIPIENT=$$($(BRIDGE_CLI) examine-key bridge-node/server-config/bridge_client.key 2>/dev/null | grep "Starcoin address:" | awk '{print $$NF}'); \
	if [ -n "$$RECIPIENT" ]; then \
		echo "$(YELLOW)Ensuring recipient account exists: $$RECIPIENT$(NC)"; \
		echo "$(BLUE)[DEBUG] Executing:$(NC)"; \
		echo "  $(STARCOIN_PATH) -c $(STARCOIN_RPC) dev get-coin -v 1000000 $$RECIPIENT"; \
		$(STARCOIN_PATH) -c $(STARCOIN_RPC) dev get-coin -v 1000000 $$RECIPIENT 2>&1 | grep -v "^[0-9].*INFO" || true; \
	fi
	@RECIPIENT=$$($(BRIDGE_CLI) examine-key bridge-node/server-config/bridge_client.key 2>/dev/null | grep "Starcoin address:" | awk '{print $$NF}'); \
	if [ -z "$$AMOUNT" ]; then \
		echo "$(RED)✗ AMOUNT is required. Usage: make deposit-eth AMOUNT=0.1$(NC)"; \
		exit 1; \
	fi; \
	echo "$(YELLOW)Depositing $$AMOUNT ETH to Starcoin...$(NC)"; \
	echo "$(YELLOW)Recipient: $$RECIPIENT$(NC)"; \
	echo "$(BLUE)[DEBUG] Executing:$(NC)"; \
	echo "  $(BRIDGE_CLI) client --config-path $(CLI_CONFIG) deposit-native-ether-on-eth --ether-amount $$AMOUNT --target-chain 2 --starcoin-bridge-recipient-address $$RECIPIENT"; \
	NO_PROXY=localhost,127.0.0.1 $(BRIDGE_CLI) client \
		--config-path $(CLI_CONFIG) \
		deposit-native-ether-on-eth \
		--ether-amount $$AMOUNT \
		--target-chain 2 \
		--starcoin-bridge-recipient-address $$RECIPIENT; \
	echo "$(GREEN)✓ Deposit transaction submitted$(NC)"

# Fund the bridge server's Starcoin account with STC for gas
fund-starcoin-bridge-account: build-bridge-cli ## Fund the bridge server account with STC for gas fees
	@echo "$(YELLOW)Funding bridge server Starcoin account...$(NC)"
	@if [ ! -f bridge-node/server-config/starcoin_client.key ]; then \
		echo "$(RED)✗ Bridge client key not found: bridge-node/server-config/starcoin_client.key$(NC)"; \
		exit 1; \
	fi
	@BRIDGE_ACCOUNT=$$($(BRIDGE_CLI) examine-key bridge-node/server-config/starcoin_client.key 2>/dev/null | grep "Starcoin address:" | awk '{print $$NF}'); \
	if [ -z "$$BRIDGE_ACCOUNT" ]; then \
		echo "$(RED)✗ Failed to get bridge account address$(NC)"; \
		exit 1; \
	fi; \
	echo "$(YELLOW)Bridge account: $$BRIDGE_ACCOUNT$(NC)"; \
	echo "$(YELLOW)Getting STC for bridge account...$(NC)"; \
	echo "$(BLUE)[DEBUG] Executing:$(NC)"; \
	echo "  $(STARCOIN_PATH) -c $(STARCOIN_RPC) dev get-coin -v 10000000 $$BRIDGE_ACCOUNT"; \
	if $(STARCOIN_PATH) -c $(STARCOIN_RPC) dev get-coin -v 10000000 $$BRIDGE_ACCOUNT 2>&1 | grep -v "^[0-9].*INFO"; then \
		echo "$(GREEN)✓ Funded bridge account for account initialization$(NC)"; \
	else \
		echo "$(YELLOW)⚠ Funding may have failed (account might already have balance)$(NC)"; \
	fi

# Quick deposit test with default amount (ETH -> Starcoin)
deposit-eth-test: build-bridge-cli fund-eth-account fund-starcoin-bridge-account ## Quick test: deposit 0.1 ETH to Starcoin
	@$(MAKE) deposit-eth AMOUNT=0.1

# Bridge transfer with token support
# Usage: make bridge-transfer DIRECTION=eth-to-stc AMOUNT=0.1 TOKEN=ETH
# All parameters are REQUIRED

bridge-transfer: build-bridge-cli ## Bridge transfer with token support (usage: make bridge-transfer DIRECTION=eth-to-stc AMOUNT=0.1 TOKEN=USDT)
	@if [ -z "$$DIRECTION" ]; then echo "$(RED)✗ DIRECTION is required (eth-to-stc or stc-to-eth)$(NC)"; exit 1; fi
	@if [ -z "$$AMOUNT" ]; then echo "$(RED)✗ AMOUNT is required$(NC)"; exit 1; fi
	@if [ -z "$$TOKEN" ]; then echo "$(RED)✗ TOKEN is required (ETH, USDT, USDC, BTC)$(NC)"; exit 1; fi
	@./scripts/bridge_transfer.sh $(DIRECTION) $(AMOUNT) --token $(TOKEN)

# Quick USDT transfer tests
deposit-usdt: build-bridge-cli fund-eth-account fund-starcoin-bridge-account ## Deposit USDT from ETH to Starcoin (usage: make deposit-usdt AMOUNT=100)
	@./scripts/bridge_transfer.sh eth-to-stc $(AMOUNT) --token USDT

deposit-usdt-test: build-bridge-cli fund-eth-account fund-starcoin-bridge-account ## Quick test: deposit 10 USDT to Starcoin
	@./scripts/bridge_transfer.sh eth-to-stc 10 --token USDT

withdraw-usdt: build-bridge-cli init-cli-config ## Withdraw USDT from Starcoin to ETH (usage: make withdraw-usdt AMOUNT=1000000)
	@./scripts/bridge_transfer.sh stc-to-eth $(AMOUNT) --token USDT

withdraw-usdt-test: build-bridge-cli init-cli-config ## Quick test: withdraw 10 USDT from Starcoin to ETH
	@./scripts/bridge_transfer.sh stc-to-eth 10 --token USDT

# Withdraw from Starcoin to ETH (Starcoin -> ETH)
withdraw-to-eth: build-bridge-cli init-cli-config ## Withdraw tokens from Starcoin to ETH
	@BRIDGE_ADDR=$$(grep "starcoin-bridge-proxy-address:" bridge-config/server-config.yaml 2>/dev/null | awk '{print $$2}' | tr -d '"'); \
	ETH_RECIPIENT=$$($(BRIDGE_CLI) examine-key bridge-node/server-config/bridge_authority.key 2>/dev/null | grep "Corresponding Ethereum address:" | awk '{print $$NF}'); \
	if [ -z "$$ETH_RECIPIENT" ]; then \
		ETH_RECIPIENT="0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"; \
	fi; \
	if [ -z "$$AMOUNT" ]; then \
		echo "$(RED)✗ AMOUNT is required. Usage: make withdraw-to-eth AMOUNT=10000000 TOKEN=ETH$(NC)"; \
		exit 1; \
	fi; \
	if [ -z "$$TOKEN" ]; then \
		echo "$(RED)✗ TOKEN is required (ETH, USDT, USDC, BTC)$(NC)"; \
		exit 1; \
	fi; \
	COIN_TYPE="$${BRIDGE_ADDR}::$${TOKEN}::$${TOKEN}"; \
	echo "$(YELLOW)Withdrawing $$AMOUNT $$TOKEN to ETH...$(NC)"; \
	echo "$(YELLOW)Coin type: $$COIN_TYPE$(NC)"; \
	echo "$(YELLOW)ETH Recipient: $$ETH_RECIPIENT$(NC)"; \
	echo "$(BLUE)[DEBUG] Executing:$(NC)"; \
	echo "  $(BRIDGE_CLI) client --config-path $(CLI_CONFIG) deposit-on-starcoin --amount $$AMOUNT --coin-type $$COIN_TYPE --target-chain 12 --recipient-address $$ETH_RECIPIENT"; \
	NO_PROXY=localhost,127.0.0.1 $(BRIDGE_CLI) client \
		--config-path $(CLI_CONFIG) \
		deposit-on-starcoin \
		--amount $$AMOUNT \
		--coin-type "$$COIN_TYPE" \
		--target-chain 12 \
		--recipient-address $$ETH_RECIPIENT; \
	echo "$(GREEN)✓ Withdraw transaction submitted$(NC)"

# Quick withdraw test with default amount
withdraw-to-eth-test: build-bridge-cli ## Quick test: withdraw 0.01 ETH from Starcoin to ETH
	@$(MAKE) withdraw-to-eth AMOUNT=10000000 TOKEN=ETH

init-cli-config: ## Generate CLI config file
	@echo "$(YELLOW)Generating CLI config...$(NC)"
	@if [ ! -f bridge-config/server-config.yaml ]; then \
		echo "$(RED)✗ Server config not found. Run: make setup-eth-and-config$(NC)"; \
		exit 1; \
	fi
	@ETH_PROXY=$$(grep "eth-bridge-proxy-address:" bridge-config/server-config.yaml | awk '{print $$2}'); \
	STARCOIN_ADDR=$$(grep "starcoin-bridge-proxy-address:" bridge-config/server-config.yaml | awk '{print $$2}' | tr -d '"'); \
	if [ ! -f bridge-node/server-config/bridge_client.key ]; then \
		echo "$(YELLOW)Creating bridge client key (Ed25519 for Starcoin)...$(NC)"; \
		cargo build --bin starcoin-bridge-cli --quiet; \
		$(BRIDGE_CLI) create-bridge-client-key bridge-node/server-config/bridge_client.key; \
	fi; \
	echo "# Starcoin Bridge CLI Configuration" > $(CLI_CONFIG); \
	echo "starcoin-bridge-rpc-url: http://127.0.0.1:9850" >> $(CLI_CONFIG); \
	echo "eth-rpc-url: http://localhost:8545" >> $(CLI_CONFIG); \
	echo "starcoin-bridge-proxy-address: \"$$STARCOIN_ADDR\"" >> $(CLI_CONFIG); \
	echo "eth-bridge-proxy-address: \"$$ETH_PROXY\"" >> $(CLI_CONFIG); \
	echo "starcoin-bridge-key-path: $(PWD)/bridge-node/server-config/bridge_client.key" >> $(CLI_CONFIG); \
	echo "eth-key-path: $(PWD)/bridge-node/server-config/bridge_authority.key" >> $(CLI_CONFIG)
	@echo "$(GREEN)✓ CLI config generated: $(CLI_CONFIG)$(NC)"

# Manual claim on ETH for Starcoin->ETH transfers
claim-on-eth: ## Manually claim tokens on ETH (usage: make claim-on-eth SEQ_NUM=1 TOKEN=ETH)
	@echo "$(YELLOW)Manual ETH Claim$(NC)"
	@if [ -z "$(SEQ_NUM)" ]; then \
		echo "$(RED)✗ SEQ_NUM required. Usage: make claim-on-eth SEQ_NUM=1 TOKEN=ETH$(NC)"; \
		exit 1; \
	fi
	@TOKEN=$${TOKEN:-ETH}; \
	cd scripts && bash bridge_transfer.sh claim-on-eth 2 $(SEQ_NUM) $$TOKEN

