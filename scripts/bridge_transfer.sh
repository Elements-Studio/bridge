#!/bin/bash
# Bridge Transfer Script with Polling
# Usage: bridge_transfer.sh [eth-to-stc|stc-to-eth] [amount]

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

BRIDGE_DIR="/Volumes/SSD/chidanta/bridge"
STARCOIN_RPC="http://localhost:9850"
ETH_RPC="http://localhost:8545"
POLL_INTERVAL=3
MAX_WAIT=120

# Get direction and amount
DIRECTION=${1:-eth-to-stc}
AMOUNT=${2:-0.1}

cd "$BRIDGE_DIR"

# Get addresses from keys
get_starcoin_address() {
    ./target/debug/starcoin-bridge-cli examine-key bridge-node/server-config/bridge_client.key 2>/dev/null | grep "Starcoin address:" | awk '{print $NF}'
}

get_eth_address() {
    local addr=$(./target/debug/starcoin-bridge-cli examine-key bridge-node/server-config/bridge_authority.key 2>/dev/null | grep "Eth address:" | awk '{print $NF}')
    if [ -z "$addr" ]; then
        echo "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
    else
        echo "$addr"
    fi
}

# Get Starcoin token balances
get_starcoin_balances() {
    local addr=$1
    echo -e "${BLUE}=== Starcoin Token Balances for $addr ===${NC}"
    
    # Get all resources
    local resources=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"state.list_resource\",\"params\":[\"$addr\",{\"decode\":true}],\"id\":1}" \
        "$STARCOIN_RPC" | jq -r '.result.resources // {}')
    
    # Parse STC balance
    local stc_balance=$(echo "$resources" | jq -r '.["0x00000000000000000000000000000001::Account::Balance<0x00000000000000000000000000000001::STC::STC>"]?.json?.token?.value // "0"')
    echo -e "  STC: ${GREEN}$(echo "scale=9; $stc_balance / 1000000000" | bc) STC${NC}"
    
    # Parse bridge token balances (if any)
    echo "$resources" | jq -r 'to_entries[] | select(.key | contains("Balance")) | .key' | while read key; do
        if [[ "$key" != *"STC::STC"* ]]; then
            local token_name=$(echo "$key" | sed 's/.*Balance<\(.*\)>/\1/' | sed 's/.*:://')
            local balance=$(echo "$resources" | jq -r ".[\"$key\"]?.json?.token?.value // \"0\"")
            echo -e "  $token_name: ${GREEN}$balance${NC}"
        fi
    done
}

# Get ETH token balances
get_eth_balances() {
    local addr=$1
    echo -e "${BLUE}=== ETH Balances for $addr ===${NC}"
    
    # Get ETH balance
    local eth_balance=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBalance\",\"params\":[\"$addr\",\"latest\"],\"id\":1}" \
        "$ETH_RPC" | jq -r '.result // "0x0"')
    local eth_decimal=$(printf "%d" "$eth_balance" 2>/dev/null || echo "0")
    echo -e "  ETH: ${GREEN}$(echo "scale=6; $eth_decimal / 1000000000000000000" | bc) ETH${NC}"
}

# Get latest Starcoin transaction status
get_latest_stc_tx_status() {
    local result=$(curl -s -X POST -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"chain.info","params":[],"id":1}' \
        "$STARCOIN_RPC")
    echo "$result" | jq -r '.result.head.number // "0"'
}

# Check bridge transfer record
check_bridge_record() {
    local source_chain=$1
    local seq_num=$2
    local bridge_addr=$(grep "starcoin-bridge-proxy-address:" bridge-config/server-config.yaml 2>/dev/null | awk '{print $2}' | tr -d '"')
    
    if [ -z "$bridge_addr" ]; then
        return 1
    fi
    
    local result=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"state.get_resource\",\"params\":[\"$bridge_addr\",\"${bridge_addr}::Bridge::Bridge\",{\"decode\":true}],\"id\":1}" \
        "$STARCOIN_RPC")
    
    local record=$(echo "$result" | jq -r ".result.json.inner.token_transfer_records.data[] | select(.key.source_chain == $source_chain and .key.bridge_seq_num == $seq_num)")
    
    if [ -n "$record" ]; then
        local claimed=$(echo "$record" | jq -r '.value.claimed')
        local has_sigs=$(echo "$record" | jq -r '.value.verified_signatures.vec | length')
        
        if [ "$claimed" = "true" ]; then
            echo "claimed"
        elif [ "$has_sigs" -gt 0 ]; then
            echo "approved"
        else
            echo "pending"
        fi
    else
        echo "not_found"
    fi
}

# Wait for transaction and poll status
poll_bridge_status() {
    local direction=$1
    local start_time=$(date +%s)
    local source_chain
    local seq_num=0
    
    if [ "$direction" = "eth-to-stc" ]; then
        source_chain=12
    else
        source_chain=2
    fi
    
    echo -e "${YELLOW}Polling bridge status (max ${MAX_WAIT}s)...${NC}"
    
    while true; do
        local current_time=$(date +%s)
        local elapsed=$((current_time - start_time))
        
        if [ $elapsed -ge $MAX_WAIT ]; then
            echo -e "${RED}✗ Timeout waiting for bridge transfer${NC}"
            return 1
        fi
        
        local status=$(check_bridge_record $source_chain $seq_num)
        
        case $status in
            "claimed")
                echo -e "${GREEN}✓ Bridge transfer completed! (claimed)${NC}"
                return 0
                ;;
            "approved")
                echo -e "${YELLOW}... Transfer approved, waiting for claim... (${elapsed}s)${NC}"
                ;;
            "pending")
                echo -e "${YELLOW}... Transfer pending signature... (${elapsed}s)${NC}"
                ;;
            "not_found")
                echo -e "${YELLOW}... Waiting for transfer record... (${elapsed}s)${NC}"
                ;;
        esac
        
        sleep $POLL_INTERVAL
    done
}

# Main execution
main() {
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}  Starcoin Bridge Transfer${NC}"
    echo -e "${BLUE}========================================${NC}"
    
    local stc_addr=$(get_starcoin_address)
    local eth_addr=$(get_eth_address)
    
    echo -e "${YELLOW}Starcoin Address: $stc_addr${NC}"
    echo -e "${YELLOW}ETH Address: $eth_addr${NC}"
    echo ""
    
    # Show balances before
    echo -e "${BLUE}=== Before Transfer ===${NC}"
    get_starcoin_balances "$stc_addr"
    get_eth_balances "$eth_addr"
    echo ""
    
    # Execute transfer
    if [ "$DIRECTION" = "eth-to-stc" ]; then
        echo -e "${YELLOW}Initiating ETH → Starcoin transfer: $AMOUNT ETH${NC}"
        make deposit-eth AMOUNT="$AMOUNT" 2>&1 | tail -5
    else
        # Convert ETH amount to smallest unit (8 decimals)
        local amount_wei=$(echo "$AMOUNT * 100000000" | bc | cut -d. -f1)
        echo -e "${YELLOW}Initiating Starcoin → ETH transfer: $AMOUNT ETH ($amount_wei units)${NC}"
        make withdraw-to-eth AMOUNT="$amount_wei" TOKEN=ETH 2>&1 | tail -5
    fi
    
    echo ""
    
    # Poll for completion
    if poll_bridge_status "$DIRECTION"; then
        echo ""
        echo -e "${BLUE}=== After Transfer ===${NC}"
        sleep 2  # Wait a bit for state to update
        get_starcoin_balances "$stc_addr"
        get_eth_balances "$eth_addr"
        echo ""
        echo -e "${GREEN}✓ Bridge transfer successful!${NC}"
    else
        echo ""
        echo -e "${RED}✗ Bridge transfer may have failed. Check logs:${NC}"
        echo -e "${YELLOW}  - Bridge server logs: make logs${NC}"
        echo -e "${YELLOW}  - Starcoin node logs${NC}"
        
        # Try to get error details
        local bridge_addr=$(grep "starcoin-bridge-proxy-address:" bridge-config/server-config.yaml 2>/dev/null | awk '{print $2}' | tr -d '"')
        if [ -n "$bridge_addr" ]; then
            echo ""
            echo -e "${YELLOW}Bridge state:${NC}"
            curl -s -X POST -H "Content-Type: application/json" \
                -d "{\"jsonrpc\":\"2.0\",\"method\":\"state.get_resource\",\"params\":[\"$bridge_addr\",\"${bridge_addr}::Bridge::Bridge\",{\"decode\":true}],\"id\":1}" \
                "$STARCOIN_RPC" | jq '.result.json.inner.token_transfer_records'
        fi
    fi
}

main
