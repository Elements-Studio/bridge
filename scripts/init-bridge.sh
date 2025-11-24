#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
BRIDGE_KEYS_DIR="$HOME/.sui/bridge_keys"
BRIDGE_CONFIG_DIR="$(pwd)/bridge-config"
ETH_DEPLOYMENT_URL="http://localhost:8080/deployment.json"

echo -e "${GREEN}=== Sui Bridge Initialization ===${NC}"

# Step 1: Clean previous configurations
echo -e "\n${YELLOW}Step 1: Cleaning previous configurations...${NC}"
rm -rf "$BRIDGE_KEYS_DIR"
rm -rf "$BRIDGE_CONFIG_DIR"
mkdir -p "$BRIDGE_KEYS_DIR"
mkdir -p "$BRIDGE_CONFIG_DIR"
echo -e "${GREEN}✓ Cleaned${NC}"

# Step 2: Check ETH deployment (with polling)
echo -e "\n${YELLOW}Step 2: Checking ETH deployment...${NC}"
MAX_WAIT=60  # Maximum wait time in seconds
POLL_INTERVAL=3  # Poll every 3 seconds
ELAPSED=0

while [ $ELAPSED -lt $MAX_WAIT ]; do
    RESPONSE=$(curl -s --connect-timeout 2 "$ETH_DEPLOYMENT_URL" 2>/dev/null)
    if echo "$RESPONSE" | jq -e '.network.chainId' > /dev/null 2>&1; then
        echo -e "${GREEN}✓ ETH deployment ready (waited ${ELAPSED}s)${NC}"
        break
    fi
    
    echo "  Polling... (${ELAPSED}/${MAX_WAIT}s) - waiting for deployer to finish"
    sleep $POLL_INTERVAL
    ELAPSED=$((ELAPSED + POLL_INTERVAL))
    
    if [ $ELAPSED -ge $MAX_WAIT ]; then
        echo -e "${RED}✗ Deployment not ready after ${MAX_WAIT}s${NC}"
        echo -e "${YELLOW}Check logs: docker logs bridge-eth-deployer${NC}"
        exit 1
    fi
done

# Step 3: Generate Bridge keys
echo -e "\n${YELLOW}Step 3: Generating Bridge keys...${NC}"

# Generate validator key
echo "Generating validator key..."
starcoin-bridge-cli create-bridge-validator-key "$BRIDGE_KEYS_DIR/validator_0_bridge_key"

# Generate client key
echo "Generating client key..."
starcoin-bridge-cli create-bridge-client-key "$BRIDGE_KEYS_DIR/bridge_client_key"

# Extract key information
echo -e "\n${YELLOW}Extracting key information...${NC}"
VALIDATOR_INFO=$(starcoin-bridge-cli examine-key "$BRIDGE_KEYS_DIR/validator_0_bridge_key")
VALIDATOR_ETH_ADDRESS=$(echo "$VALIDATOR_INFO" | grep "Corresponding Ethereum address:" | awk '{print $NF}')
VALIDATOR_SUI_ADDRESS=$(echo "$VALIDATOR_INFO" | grep "Corresponding Sui address:" | awk '{print $NF}')
VALIDATOR_PUBKEY=$(echo "$VALIDATOR_INFO" | grep "Corresponding PublicKey:" | tr -d '"' | awk '{print $NF}')

CLIENT_INFO=$(starcoin-bridge-cli examine-key "$BRIDGE_KEYS_DIR/bridge_client_key")
CLIENT_SUI_ADDRESS=$(echo "$CLIENT_INFO" | grep "Corresponding Sui address:" | awk '{print $NF}')

echo -e "${GREEN}✓ Keys generated${NC}"
echo "  Validator ETH Address: 0x$VALIDATOR_ETH_ADDRESS"
echo "  Validator Sui Address: $VALIDATOR_SUI_ADDRESS"
echo "  Validator PublicKey: $VALIDATOR_PUBKEY"
echo "  Client Sui Address: $CLIENT_SUI_ADDRESS"

# Step 4: Fetch ETH deployment info
echo -e "\n${YELLOW}Step 4: Fetching ETH deployment info...${NC}"
ETH_DEPLOYMENT=$(curl -s "$ETH_DEPLOYMENT_URL")
echo "$ETH_DEPLOYMENT" > "$BRIDGE_CONFIG_DIR/eth-deployment.json"

# Extract contract addresses using jq
SUI_BRIDGE_ADDRESS=$(echo "$ETH_DEPLOYMENT" | jq -r '.contracts.SuiBridge // empty')
BRIDGE_COMMITTEE_ADDRESS=$(echo "$ETH_DEPLOYMENT" | jq -r '.contracts.BridgeCommittee // empty')
BRIDGE_LIMITER_ADDRESS=$(echo "$ETH_DEPLOYMENT" | jq -r '.contracts.BridgeLimiter // empty')
BRIDGE_VAULT_ADDRESS=$(echo "$ETH_DEPLOYMENT" | jq -r '.contracts.BridgeVault // empty')
WETH_ADDRESS=$(echo "$ETH_DEPLOYMENT" | jq -r '.contracts.WETH // empty')
ETH_RPC_URL=$(echo "$ETH_DEPLOYMENT" | jq -r '.network.rpcUrl // empty')
ETH_CHAIN_ID=$(echo "$ETH_DEPLOYMENT" | jq -r '.network.chainId // empty')

echo -e "${GREEN}✓ ETH deployment info fetched${NC}"
echo "  Chain ID: $ETH_CHAIN_ID"
echo "  RPC URL: $ETH_RPC_URL"
echo "  SuiBridge: $SUI_BRIDGE_ADDRESS"
echo "  BridgeCommittee: $BRIDGE_COMMITTEE_ADDRESS"

# Step 5: Extract validator ETH private key
echo -e "\n${YELLOW}Step 5: Extracting ETH private key...${NC}"
VALIDATOR_ETH_PRIVKEY=$(cat "$BRIDGE_KEYS_DIR/validator_0_bridge_key" | base64 -d | xxd -p -c 32 | head -1)
echo -e "${GREEN}✓ ETH private key extracted${NC}"
echo "  Private Key: 0x$VALIDATOR_ETH_PRIVKEY"

# Step 6: Generate bridge server config
echo -e "\n${YELLOW}Step 6: Generating bridge server config...${NC}"
cat > "$BRIDGE_CONFIG_DIR/server-config.yaml" <<EOF
# Sui Bridge Server Configuration
# Generated at: $(date)

# Server settings
server-listen-port: 9191
metrics-port: 9184

# Bridge authority key (validator)
bridge-authority-key-path: $BRIDGE_KEYS_DIR/validator_0_bridge_key

# Ethereum configuration
eth-rpc-url: $ETH_RPC_URL
eth-bridge-proxy-address: $SUI_BRIDGE_ADDRESS
eth-bridge-chain-id: $ETH_CHAIN_ID
eth-contracts-start-block-fallback: 0
eth-contracts-start-block-override: 0

# Sui configuration
starcoin-bridge-rpc-url: http://127.0.0.1:9000
starcoin-bridge-chain-id: 0
# Will be set after deploying Sui Bridge contracts
# starcoin-bridge-module-last-processed-event-id-override:

# Database
db-path: $BRIDGE_CONFIG_DIR/bridge.db

# Approved governance actions (empty for auto-approve in local testing)
approved-governance-actions: []
EOF

echo -e "${GREEN}✓ Bridge server config generated${NC}"
echo "  Config file: $BRIDGE_CONFIG_DIR/server-config.yaml"

# Step 7: Generate bridge client config
echo -e "\n${YELLOW}Step 7: Generating bridge client config...${NC}"
cat > "$BRIDGE_CONFIG_DIR/client-config.yaml" <<EOF
# Sui Bridge Client Configuration
# Generated at: $(date)

# Sui configuration
starcoin-bridge-rpc-url: http://127.0.0.1:9000
bridge-client-key-path: $BRIDGE_KEYS_DIR/bridge_client_key
bridge-client-gas-object:  # Will be set after getting gas from faucet

# Ethereum configuration
eth-rpc-url: $ETH_RPC_URL
eth-bridge-proxy-address: $SUI_BRIDGE_ADDRESS
eth-bridge-chain-id: $ETH_CHAIN_ID
eth-contracts-start-block-fallback: 0

# Sui Bridge configuration
starcoin-bridge-chain-id: 0
# Will be set after deploying Sui Bridge contracts
# starcoin-bridge-module-last-processed-event-id-override:

# Database
db-path: $BRIDGE_CONFIG_DIR/bridge_client.db
EOF

echo -e "${GREEN}✓ Bridge client config generated${NC}"
echo "  Config file: $BRIDGE_CONFIG_DIR/client-config.yaml"

# Step 8: Generate environment variables file
echo -e "\n${YELLOW}Step 8: Generating environment file...${NC}"
cat > "$BRIDGE_CONFIG_DIR/.env" <<EOF
# Bridge Environment Variables
# Generated at: $(date)

# Validator addresses
VALIDATOR_ETH_ADDRESS=0x$VALIDATOR_ETH_ADDRESS
VALIDATOR_SUI_ADDRESS=$VALIDATOR_SUI_ADDRESS
VALIDATOR_PUBKEY=$VALIDATOR_PUBKEY
VALIDATOR_ETH_PRIVKEY=0x$VALIDATOR_ETH_PRIVKEY

# Client address
CLIENT_SUI_ADDRESS=$CLIENT_SUI_ADDRESS

# Ethereum contracts
ETH_RPC_URL=$ETH_RPC_URL
ETH_CHAIN_ID=$ETH_CHAIN_ID
SUI_BRIDGE_ADDRESS=$SUI_BRIDGE_ADDRESS
BRIDGE_COMMITTEE_ADDRESS=$BRIDGE_COMMITTEE_ADDRESS
BRIDGE_LIMITER_ADDRESS=$BRIDGE_LIMITER_ADDRESS
BRIDGE_VAULT_ADDRESS=$BRIDGE_VAULT_ADDRESS
WETH_ADDRESS=$WETH_ADDRESS

# Key paths
BRIDGE_KEYS_DIR=$BRIDGE_KEYS_DIR
BRIDGE_CONFIG_DIR=$BRIDGE_CONFIG_DIR
EOF

echo -e "${GREEN}✓ Environment file generated${NC}"
echo "  Env file: $BRIDGE_CONFIG_DIR/.env"

# Step 9: Generate summary
echo -e "\n${GREEN}=== Initialization Complete ===${NC}"
echo -e "\n${YELLOW}Generated files:${NC}"
echo "  Keys:"
echo "    - $BRIDGE_KEYS_DIR/validator_0_bridge_key"
echo "    - $BRIDGE_KEYS_DIR/bridge_client_key"
echo "  Configs:"
echo "    - $BRIDGE_CONFIG_DIR/server-config.yaml"
echo "    - $BRIDGE_CONFIG_DIR/client-config.yaml"
echo "    - $BRIDGE_CONFIG_DIR/.env"
echo "    - $BRIDGE_CONFIG_DIR/eth-deployment.json"

echo -e "\n${YELLOW}Next steps:${NC}"
echo "  1. Source environment variables:"
echo "     ${GREEN}source $BRIDGE_CONFIG_DIR/.env${NC}"
echo ""
echo "  2. Start Sui local network (in another terminal):"
echo "     ${GREEN}RUST_LOG=info,sui=debug sui start --force-regenesis${NC}"
echo ""
echo "  3. Deploy Sui Bridge contracts:"
echo "     ${GREEN}./scripts/deploy-starcoin-bridge.sh${NC}"
echo ""
echo "  4. Register bridge committee:"
echo "     ${GREEN}./scripts/register-committee.sh${NC}"

# Save summary to file
cat > "$BRIDGE_CONFIG_DIR/SETUP_SUMMARY.txt" <<EOF
Sui Bridge Setup Summary
Generated: $(date)

=== Validator Information ===
ETH Address: 0x$VALIDATOR_ETH_ADDRESS
Sui Address: $VALIDATOR_SUI_ADDRESS
PublicKey: $VALIDATOR_PUBKEY

=== Client Information ===
Sui Address: $CLIENT_SUI_ADDRESS

=== Ethereum Contracts ===
Chain ID: $ETH_CHAIN_ID
RPC URL: $ETH_RPC_URL
SuiBridge: $SUI_BRIDGE_ADDRESS
BridgeCommittee: $BRIDGE_COMMITTEE_ADDRESS
BridgeLimiter: $BRIDGE_LIMITER_ADDRESS
BridgeVault: $BRIDGE_VAULT_ADDRESS
WETH: $WETH_ADDRESS

=== Configuration Files ===
Keys: $BRIDGE_KEYS_DIR/
Configs: $BRIDGE_CONFIG_DIR/

=== Next Steps ===
1. source $BRIDGE_CONFIG_DIR/.env
2. Start Sui: RUST_LOG=info,sui=debug sui start --force-regenesis
3. Deploy Sui Bridge: ./scripts/deploy-starcoin-bridge.sh
4. Register committee: ./scripts/register-committee.sh
EOF

echo -e "\n${GREEN}Summary saved to: $BRIDGE_CONFIG_DIR/SETUP_SUMMARY.txt${NC}"
