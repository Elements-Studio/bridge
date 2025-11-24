#!/bin/bash
# Auto-generate bridge configuration and keys
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRIDGE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
STARCOIN_ROOT="$(cd "$BRIDGE_ROOT/.." && pwd)"

echo "🔧 Auto-generating bridge configuration..."

# 1. Generate bridge authority key
echo "📝 Generating bridge authority key..."
mkdir -p "$BRIDGE_ROOT/bridge-node/server-config"
cd "$STARCOIN_ROOT"

if [ ! -f "$STARCOIN_ROOT/target/debug/keygen" ]; then
    echo "   Building keygen tool..."
    cargo build --bin keygen --quiet
fi

AUTHORITY_KEY_PATH="$BRIDGE_ROOT/bridge-node/server-config/bridge_authority.key"
"$STARCOIN_ROOT/target/debug/keygen" authority --output "$AUTHORITY_KEY_PATH" > /tmp/keygen_output.txt 2>&1

# Extract Ethereum address from keygen output
ETH_ADDRESS=$(grep "Ethereum address:" /tmp/keygen_output.txt | awk '{print $3}')
echo "   ✅ Bridge authority key generated"
echo "   📍 Ethereum address: $ETH_ADDRESS"

# 2. Get ETH deployment info
echo "📝 Reading ETH deployment info..."
DEPLOYMENT_JSON=$(docker exec bridge-deployment-info cat /usr/share/nginx/html/deployment.json 2>/dev/null)
if [ -z "$DEPLOYMENT_JSON" ]; then
    echo "   ❌ Error: Could not read deployment.json from container"
    exit 1
fi

ETH_PROXY_ADDRESS=$(echo "$DEPLOYMENT_JSON" | jq -r '.contracts.ERC1967Proxy')
echo "   ✅ ETH Proxy Address: $ETH_PROXY_ADDRESS"

# 3. Generate server-config.yaml
echo "📝 Generating server-config.yaml..."
mkdir -p "$BRIDGE_ROOT/bridge-config"
cat > "$BRIDGE_ROOT/bridge-config/server-config.yaml" <<EOF
# Starcoin Bridge Server Configuration
# Auto-generated at: $(date)

# Server settings
server-listen-port: 9191
metrics-port: 9184

# Bridge authority key (validator) - Generated with keygen tool
# Ethereum address: $ETH_ADDRESS
bridge-authority-key-path: $AUTHORITY_KEY_PATH

# Run client mode (required)
run-client: true

# Database path (required when run-client is true)
db-path: $BRIDGE_ROOT/bridge-config/bridge.db

# Approved governance actions (empty for auto-approve in local testing)
approved-governance-actions: []

# Ethereum configuration
eth:
  eth-rpc-url: http://localhost:8545
  # Using ERC1967Proxy address as the bridge proxy (not the implementation contract)
  eth-bridge-proxy-address: $ETH_PROXY_ADDRESS
  eth-bridge-chain-id: 12  # EthCustom for local/test network
  eth-contracts-start-block-fallback: 0
  eth-contracts-start-block-override: 0

# Starcoin configuration
sui:
  starcoin-bridge-rpc-url: ws://127.0.0.1:9870
  starcoin-bridge-chain-id: 2
EOF

echo "   ✅ Configuration generated: $BRIDGE_ROOT/bridge-config/server-config.yaml"

# 4. Summary
echo ""
echo "✅ Bridge configuration complete!"
echo ""
echo "📋 Summary:"
echo "   Bridge Authority Key: $AUTHORITY_KEY_PATH"
echo "   Ethereum Address: $ETH_ADDRESS"
echo "   ETH Proxy Address: $ETH_PROXY_ADDRESS"
echo "   Config File: $BRIDGE_ROOT/bridge-config/server-config.yaml"
echo ""
