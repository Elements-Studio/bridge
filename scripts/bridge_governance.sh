#!/bin/bash
# Execute System Message Script (Governance Actions)
# 
# This is a simple wrapper around bridge-cli for executing governance actions.
# It's much simpler to use bridge-cli directly, which handles the entire flow:
#   1. Creates the governance action
#   2. Gets committee signatures from bridge server
#
# Usage: bridge_governance.sh <OP_TYPE> [--seq-num <SEQ_NUM>]
#   OP_TYPE: pause or unpause
#   SEQ_NUM: Optional sequence number (if not provided, will query from bridge)
#
# Examples:
#   ./bridge_governance.sh pause
#   ./bridge_governance.sh  unpause --seq-num 5
#
# Note: This script requires bridge-cli to be built:
#   make build-bridge-cli

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

BRIDGE_CLI="./target/debug/starcoin-bridge-cli"
CLIENT_CONFIG="./bridge-config/cli-config.yaml"
SERVER_CONFIG="./bridge-config/server-config.yaml"
SEQ_NUM=""
CHAIN_ID=""

# Parse arguments
OP_TYPE_STR=$1
shift || true

while [[ $# -gt 0 ]]; do
    case $1 in
        --seq-num)
            SEQ_NUM="$2"
            shift 2
            ;;
        --chain-id)
            CHAIN_ID="$2"
            shift 2
            ;;
        *)
            echo "Error: Unknown option $1" >&2
            exit 1
            ;;
    esac
done

# Validate OP_TYPE
if [ -z "$OP_TYPE_STR" ]; then
    echo "Error: OP_TYPE is required (pause or unpause)" >&2
    echo "Usage: $0 <OP_TYPE> [--seq-num <SEQ_NUM>]" >&2
    exit 1
fi

# Convert OP_TYPE string to bridge-cli format
OP_TYPE_NAME=""
case "$OP_TYPE_STR" in
    pause|0)
        OP_TYPE_NAME="pause"
        ;;
    unpause|1)
        OP_TYPE_NAME="unpause"
        ;;
    *)
        echo "Error: Invalid OP_TYPE: $OP_TYPE_STR. Must be 'pause' or 'unpause'" >&2
        exit 1
        ;;
esac

# Check if bridge-cli exists
if [ ! -f "$BRIDGE_CLI" ]; then
    echo -e "${RED}✗ bridge-cli not found at $BRIDGE_CLI${NC}" >&2
    echo -e "${YELLOW}Please build it first:${NC}" >&2
    echo -e "${YELLOW}  make build-bridge-cli${NC}" >&2
    exit 1
fi

# Check if client config exists
if [ ! -f "$CLIENT_CONFIG" ]; then
    echo -e "${RED}✗ Client config not found at $CLIENT_CONFIG${NC}" >&2
    echo -e "${YELLOW}Please generate it first:${NC}" >&2
    echo -e "${YELLOW}  make init-bridge-config${NC}" >&2
    exit 1
fi

# Get bridge address from config
get_bridge_address() {
    if [ -f "$CLIENT_CONFIG" ]; then
        local bridge_addr=$(grep "starcoin-bridge-proxy-address:" "$CLIENT_CONFIG" 2>/dev/null | awk '{print $2}' | tr -d '"')
        if [ -n "$bridge_addr" ]; then
            echo "$bridge_addr"
            return
        fi
    fi
    
    if [ -f "$SERVER_CONFIG" ]; then
        local bridge_addr=$(grep "starcoin-bridge-proxy-address:" "$SERVER_CONFIG" 2>/dev/null | awk '{print $2}' | tr -d '"')
        if [ -n "$bridge_addr" ]; then
            echo "$bridge_addr"
            return
        fi
    fi
    
    echo ""
}

# Get Starcoin RPC URL from config
get_starcoin_rpc() {
    if [ -f "$CLIENT_CONFIG" ]; then
        local rpc=$(grep "starcoin-bridge-rpc-url:" "$CLIENT_CONFIG" 2>/dev/null | awk '{print $2}' | tr -d '"')
        if [ -n "$rpc" ]; then
            echo "$rpc"
            return
        fi
    fi
    
    if [ -f "$SERVER_CONFIG" ]; then
        local rpc=$(grep "starcoin-bridge-rpc-url:" "$SERVER_CONFIG" 2>/dev/null | awk '{print $2}' | tr -d '"')
        if [ -n "$rpc" ]; then
            echo "$rpc"
            return
        fi
    fi
    
    # Default
    echo "http://127.0.0.1:9850"
}

# Get chain ID from config if not provided
get_chain_id() {
    if [ -n "$CHAIN_ID" ]; then
        echo "$CHAIN_ID"
        return
    fi
    
    # Try to get from server config
    if [ -f "$SERVER_CONFIG" ]; then
        local chain_id=$(grep "starcoin-bridge-chain-id:" "$SERVER_CONFIG" 2>/dev/null | awk '{print $2}' | tr -d '"')
        if [ -n "$chain_id" ]; then
            echo "$chain_id"
            return
        fi
    fi
    
    # Try to get from client config
    if [ -f "$CLIENT_CONFIG" ]; then
        local chain_id=$(grep -A 10 "starcoin:" "$CLIENT_CONFIG" 2>/dev/null | grep "starcoin-bridge-chain-id:" | awk '{print $2}' | tr -d '"')
        if [ -n "$chain_id" ]; then
            echo "$chain_id"
            return
        fi
    fi
    
    # Default to 2 (StarcoinCustom) if not found
    echo "2"
}

# Get current nonce for emergency_op message type (message_type = 2)
get_emergency_op_nonce() {
    local bridge_addr=$(get_bridge_address)
    local starcoin_rpc=$(get_starcoin_rpc)
    
    if [ -z "$bridge_addr" ]; then
        echo ""
        return 1
    fi
    
    # Query bridge contract for sequence_nums
    # message_type 2 = emergency_op
    local request="{\"jsonrpc\":\"2.0\",\"method\":\"state.get_resource\",\"params\":[\"$bridge_addr\",\"${bridge_addr}::Bridge::Bridge\",{\"decode\":true}],\"id\":1}"
    
    local result=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "$request" \
        "$starcoin_rpc" 2>/dev/null)
    
    # The sequence_nums map stores: key = message_type (u8), value = next_seq_num (u64)
    local nonce=$(echo "$result" | jq -r '.result.json.inner.sequence_nums.data[] | select(.key == 2) | .value // 0' 2>/dev/null)
    
    if [ -z "$nonce" ] || [ "$nonce" = "null" ]; then
        echo "0"
    else
        echo "$nonce"
    fi
}

# Build bridge-cli command
build_bridge_cli_cmd() {
    local chain_id=$(get_chain_id)
    
    # Get nonce if not provided
    if [ -z "$SEQ_NUM" ]; then
        echo -e "${YELLOW}Getting current nonce from bridge contract...${NC}" >&2
        SEQ_NUM=$(get_emergency_op_nonce)
        if [ -z "$SEQ_NUM" ]; then
            echo -e "${RED}✗ Failed to get nonce${NC}" >&2
            echo -e "${YELLOW}Please provide --seq-num manually${NC}" >&2
            return 1
        fi
        echo -e "${GREEN}✓ Current nonce: $SEQ_NUM${NC}" >&2
    fi
    
    local cmd="$BRIDGE_CLI governance --config-path $CLIENT_CONFIG --chain-id $chain_id emergency-button"
    cmd="$cmd --nonce $SEQ_NUM"
    cmd="$cmd --action-type $OP_TYPE_NAME"
    
    echo "$cmd"
}

# Main execution
main() {
    local chain_id=$(get_chain_id)
    
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}  Governance Actions ($OP_TYPE_NAME) ${NC}"
    echo -e "${BLUE}========================================${NC}"
    echo -e "${YELLOW}Operation: ${OP_TYPE_NAME}${NC}"
    echo -e "${YELLOW}Chain ID: ${chain_id}${NC}"
    echo ""
    
    # Build command (this will also get nonce if not provided)
    local cmd=$(build_bridge_cli_cmd)
    
    if [ $? -ne 0 ]; then
        exit 1
    fi
    
    echo -e "${BLUE}Executing: $cmd${NC}"
    echo ""
    
    # Execute bridge-cli
    eval "$cmd"
    
    if [ $? -eq 0 ]; then
        echo ""
        echo -e "${GREEN}✓ Emergency operation executed successfully!${NC}"
        exit 0
    else
        echo ""
        echo -e "${RED}✗ Failed to execute emergency operation${NC}" >&2
        exit 1
    fi
}

main
