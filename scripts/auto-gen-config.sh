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
# ============================================================================
# Starcoin Bridge Server Configuration
# Auto-generated at: $(date)
# ============================================================================

# ----------------------------------------------------------------------------
# Server Network Settings
# ----------------------------------------------------------------------------

# Port for bridge server JSON-RPC API
# Used by: Bridge CLI and other clients to interact with the bridge
# Default: 9191
server-listen-port: 9191

# Port for Prometheus metrics endpoint
# Used by: Monitoring systems to collect bridge metrics (transfers, errors, latency)
# Access via: http://localhost:9184/metrics
# Default: 9184
metrics-port: 9184

# ----------------------------------------------------------------------------
# Bridge Authority Configuration (Validator)
# ----------------------------------------------------------------------------

# Path to bridge authority private key (ECDSA secp256k1)
# Generated by: 'keygen authority' command
# Used by: Bridge server to sign bridge actions and submit ETH transactions
# Ethereum address: $ETH_ADDRESS
# Security: Keep this key secure - it controls validator operations
bridge-authority-key-path: $AUTHORITY_KEY_PATH

# ----------------------------------------------------------------------------
# Client Mode Settings
# ----------------------------------------------------------------------------

# Enable client mode for transaction submission
# Used by: Bridge server to submit transactions to both chains
# When true: Bridge will actively submit approved transactions
# When false: Bridge only monitors and signs (passive mode)
# Default: true (required for active bridge operation)
run-client: true

# SQLite database path for bridge state storage
# Used by: Storing processed events, pending actions, and bridge state
# Required when: run-client is true
# Auto-created: Yes, if directory exists
db-path: $BRIDGE_ROOT/bridge-config/bridge.db

# ----------------------------------------------------------------------------
# Governance Configuration
# ----------------------------------------------------------------------------

# List of pre-approved governance action digests
# Used by: Auto-approving governance actions without manual intervention
# Format: Array of hex strings (action digests)
# Empty array: Auto-approve all governance actions (local testing only)
# Production: Add specific action digests for security
approved-governance-actions: []

# ----------------------------------------------------------------------------
# Ethereum Network Configuration
# ----------------------------------------------------------------------------
eth:
  # Ethereum JSON-RPC endpoint URL
  # Used by: Monitoring ETH events, submitting ETH transactions, querying state
  # Local: http://localhost:8545 (Anvil/Hardhat)
  # Mainnet: https://eth-mainnet.g.alchemy.com/v2/YOUR-API-KEY
  # Testnet: https://sepolia.infura.io/v3/YOUR-PROJECT-ID
  eth-rpc-url: http://localhost:8545
  
  # Bridge proxy contract address on Ethereum (ERC1967Proxy)
  # Used by: All bridge operations - deposits, withdrawals, event monitoring
  # Auto-filled: From ETH deployment (deployment.txt)
  # Note: This is the proxy address, NOT the implementation contract
  # Deployed at: Block 0 (local) or check deployment logs
  eth-bridge-proxy-address: $ETH_PROXY_ADDRESS
  
  # Ethereum chain identifier for bridge operations
  # Used by: Chain-specific logic, signature verification, preventing replay attacks
  # Values: 1=Mainnet, 5=Goerli, 11155111=Sepolia, 12=EthCustom (local/test)
  # Must match: The chain ID returned by eth_chainId RPC call
  eth-bridge-chain-id: 12
  
  # Fallback starting block for event scanning (if no checkpoint exists)
  # Used by: Initial event sync when bridge starts for the first time
  # 0: Scan from genesis (slow but complete)
  # N: Scan from block N (faster, may miss earlier events)
  eth-contracts-start-block-fallback: 0
  
  # Force override starting block (ignores checkpoint)
  # Used by: Re-syncing events after a reset or migration
  # 0: Use checkpoint or fallback (normal operation)
  # N: Force scan from block N (overrides saved state)
  eth-contracts-start-block-override: 0
  
  # Use 'latest' instead of 'finalized' block for querying
  # Used by: Local testing with Anvil (no finalized blocks concept)
  # true: Query 'latest' block (Anvil/Hardhat compatible)
  # false: Query 'finalized' block (Mainnet/Testnet recommended)
  # Warning: Using 'latest' on mainnet may cause reorg issues
  eth-use-latest-block: true

# ----------------------------------------------------------------------------
# Starcoin Network Configuration
# ----------------------------------------------------------------------------
starcoin:
  # Starcoin HTTP JSON-RPC endpoint URL
  # Used by: Monitoring Starcoin events, submitting transactions, querying state
  # Local Dev: http://127.0.0.1:9850 (starcoin -n dev)
  # Testnet: https://barnard-seed.starcoin.org
  # Mainnet: https://main-seed.starcoin.org
  starcoin-bridge-rpc-url: http://127.0.0.1:9850
  
  # Starcoin chain identifier for bridge operations
  # Used by: Chain-specific logic, preventing cross-chain replay attacks
  # Values: 1=Mainnet, 251=Barnard (testnet), 254=Dev, 2=StarcoinCustom (local)
  # Note: Dev network typically uses chain_id 254, but bridge uses 2 for StarcoinCustom
  starcoin-bridge-chain-id: 2
  
  # Bridge module address on Starcoin
  # Used by: All bridge operations - deposits, withdrawals, event filtering
  # Auto-filled: From Move.toml [addresses] section after contract deployment
  # Format: 0x followed by 32 hex characters (Starcoin address format)
  # Example: 0x246b237c16c761e9478783dd83f7004a
  starcoin-bridge-proxy-address: "$STARCOIN_BRIDGE_ADDRESS"
  
  # Path to Starcoin client private key (Ed25519)
  # Generated by: 'bridge-cli create-bridge-client-key' command
  # Used by: Signing and submitting Starcoin transactions (claims, approvals)
  # Starcoin address: $STARCOIN_CLIENT_ADDRESS
  # Security: Keep this key secure - it pays for Starcoin gas fees
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
