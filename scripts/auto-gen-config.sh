#!/bin/bash
# Auto-generate bridge configuration and keys
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRIDGE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
STARCOIN_ROOT="$(cd "$BRIDGE_ROOT/.." && pwd)"

# Parse arguments
SKIP_KEYGEN=false
for arg in "$@"; do
    case $arg in
        --skip-keygen)
            SKIP_KEYGEN=true
            shift
            ;;
    esac
done

echo "🔧 Auto-generating bridge configuration..."

AUTHORITY_KEY_PATH="$BRIDGE_ROOT/bridge-node/server-config/bridge_authority.key"
STARCOIN_CLIENT_KEY_PATH="$BRIDGE_ROOT/bridge-node/server-config/starcoin_client.key"

if [ "$SKIP_KEYGEN" = "true" ]; then
    echo "📝 Skipping key generation (keys already exist)..."
    # Read existing key info
    if [ -f "$AUTHORITY_KEY_PATH" ]; then
        ETH_ADDRESS=$("$BRIDGE_ROOT/target/debug/keygen" examine "$AUTHORITY_KEY_PATH" 2>/dev/null | grep "Ethereum address:" | awk '{print $NF}')
        echo "   📍 Ethereum address: $ETH_ADDRESS"
    else
        echo "   ❌ Error: Bridge authority key not found at $AUTHORITY_KEY_PATH"
        exit 1
    fi
    if [ -f "$STARCOIN_CLIENT_KEY_PATH" ]; then
        STARCOIN_CLIENT_ADDRESS=$("$BRIDGE_ROOT/target/debug/starcoin-bridge-cli" examine-key "$STARCOIN_CLIENT_KEY_PATH" 2>/dev/null | grep "Starcoin address:" | awk '{print $NF}')
        echo "   📍 Starcoin address: $STARCOIN_CLIENT_ADDRESS"
    else
        echo "   ❌ Error: Starcoin client key not found at $STARCOIN_CLIENT_KEY_PATH"
        exit 1
    fi
else
    # 1. Generate bridge authority key
    echo "📝 Generating bridge authority key..."
    mkdir -p "$BRIDGE_ROOT/bridge-node/server-config"

    if [ ! -f "$BRIDGE_ROOT/target/debug/keygen" ]; then
        echo "   Building keygen tool..."
        cd "$BRIDGE_ROOT"
        cargo build --bin keygen --quiet
    fi

    "$BRIDGE_ROOT/target/debug/keygen" authority --output "$AUTHORITY_KEY_PATH" > /tmp/keygen_output.txt 2>&1

    # Extract Ethereum address from keygen output
    ETH_ADDRESS=$(grep "Ethereum address:" /tmp/keygen_output.txt | awk '{print $3}')
    echo "   ✅ Bridge authority key generated"
    echo "   📍 Ethereum address: $ETH_ADDRESS"

    # 1b. Generate Starcoin client key (Ed25519 for Starcoin transaction signing)
    echo "📝 Generating Starcoin client key (Ed25519)..."
    "$BRIDGE_ROOT/target/debug/starcoin-bridge-cli" create-bridge-client-key "$STARCOIN_CLIENT_KEY_PATH" > /tmp/starcoin_key_output.txt 2>&1
    STARCOIN_CLIENT_ADDRESS=$(grep "Starcoin address:" /tmp/starcoin_key_output.txt | awk '{print $NF}')
    echo "   ✅ Starcoin client key generated"
    echo "   📍 Starcoin address: $STARCOIN_CLIENT_ADDRESS"
fi

# 2. Get ETH deployment info
echo "📝 Reading ETH deployment info..."

# Try JSON first (for compatibility), fallback to text format
if docker exec bridge-deployment-info test -f /usr/share/nginx/html/deployment.json 2>/dev/null; then
    ETH_PROXY_ADDRESS=$(docker exec bridge-deployment-info cat /usr/share/nginx/html/deployment.json 2>/dev/null | grep -o '"ERC1967Proxy":"0x[a-fA-F0-9]*"' | cut -d'"' -f4)
fi

# Fallback to text format
if [ -z "$ETH_PROXY_ADDRESS" ] && docker exec bridge-deployment-info test -f /usr/share/nginx/html/deployment.txt 2>/dev/null; then
    ETH_PROXY_ADDRESS=$(docker exec bridge-deployment-info grep "^ERC1967Proxy=" /usr/share/nginx/html/deployment.txt 2>/dev/null | cut -d'=' -f2)
fi

if [ -z "$ETH_PROXY_ADDRESS" ]; then
    echo "   ❌ Error: Could not read deployment info from container"
    exit 1
fi

echo "   ✅ ETH Proxy Address: $ETH_PROXY_ADDRESS"

# 3. Generate server-config.yaml
echo "📝 Generating server-config.yaml..."
mkdir -p "$BRIDGE_ROOT/bridge-config"

# Get Starcoin bridge address from Move.toml if available
STARCOIN_BRIDGE_ADDRESS=""
MOVE_TOML_PATH="$STARCOIN_ROOT/stc-bridge-move/Move.toml"
if [ -f "$MOVE_TOML_PATH" ]; then
    STARCOIN_BRIDGE_ADDRESS=$(grep "^Bridge = " "$MOVE_TOML_PATH" | sed 's/Bridge = "\(.*\)"/\1/')
    echo "   📍 Starcoin Bridge Address: $STARCOIN_BRIDGE_ADDRESS"
fi

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
  # Use 'latest' block instead of 'finalized' for local testing with Anvil
  eth-use-latest-block: true

# Starcoin configuration
starcoin:
  starcoin-bridge-rpc-url: http://127.0.0.1:9850
  starcoin-bridge-chain-id: 2
  starcoin-bridge-proxy-address: "$STARCOIN_BRIDGE_ADDRESS"
  # Ed25519 key for signing Starcoin transactions
  bridge-client-key-path: $STARCOIN_CLIENT_KEY_PATH
EOF

echo "   ✅ Configuration generated: $BRIDGE_ROOT/bridge-config/server-config.yaml"

# 4. Fund ETH addresses for bridge operations
echo "📝 Funding ETH addresses for bridge operations..."

# Anvil default account with 10000 ETH
ANVIL_PRIVATE_KEY="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
FUND_AMOUNT="100ether"

# Fund bridge authority address
echo "   Funding bridge authority: $ETH_ADDRESS"
docker run --rm --network host ghcr.io/foundry-rs/foundry:latest \
    "cast send --rpc-url http://host.docker.internal:8545 --private-key $ANVIL_PRIVATE_KEY $ETH_ADDRESS --value $FUND_AMOUNT" \
    > /dev/null 2>&1 && echo "   ✅ Funded $FUND_AMOUNT to $ETH_ADDRESS" || echo "   ⚠️ Could not fund (may already have balance)"

# 5. Summary
echo ""
echo "✅ Bridge configuration complete!"
echo ""
echo "📋 Summary:"
echo "   Bridge Authority Key: $AUTHORITY_KEY_PATH"
echo "   Ethereum Address: $ETH_ADDRESS (funded with $FUND_AMOUNT)"
echo "   ETH Proxy Address: $ETH_PROXY_ADDRESS"
echo "   Config File: $BRIDGE_ROOT/bridge-config/server-config.yaml"
echo ""
