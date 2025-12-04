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

STARCOIN_RPC="http://localhost:9850"
ETH_RPC="http://localhost:8545"
POLL_INTERVAL=3
MAX_WAIT=120

# Get direction and amount
DIRECTION=${1:-eth-to-stc}
AMOUNT=${2:-0.1}

# Ensure binary and keys exist before proceeding
ensure_prerequisites() {
    # Wait for binary to exist
    if [ ! -f "./target/debug/starcoin-bridge-cli" ]; then
        echo -e "${YELLOW}Waiting for starcoin-bridge-cli to be built...${NC}" >&2
        for i in {1..30}; do
            if [ -f "./target/debug/starcoin-bridge-cli" ]; then
                break
            fi
            sleep 1
        done
    fi
    
    if [ ! -f "./target/debug/starcoin-bridge-cli" ]; then
        echo -e "${RED}✗ starcoin-bridge-cli not found. Run 'make build-bridge-cli' first.${NC}" >&2
        exit 1
    fi
    
    # Generate bridge_client.key if missing
    if [ ! -f "bridge-node/server-config/bridge_client.key" ]; then
        echo -e "${YELLOW}Creating bridge client key...${NC}" >&2
        mkdir -p bridge-node/server-config
        ./target/debug/starcoin-bridge-cli create-bridge-client-key bridge-node/server-config/bridge_client.key >&2
    fi
}

# Get addresses from keys
get_starcoin_address() {
    ensure_prerequisites
    ./target/debug/starcoin-bridge-cli examine-key bridge-node/server-config/bridge_client.key 2>/dev/null | grep -i "Starcoin address:" | awk '{print $NF}'
}

get_eth_address() {
    local addr=$(./target/debug/starcoin-bridge-cli examine-key bridge-node/server-config/bridge_authority.key 2>/dev/null | grep -i "ethereum address:" | awk '{print $NF}')
    if [ -n "$addr" ]; then
        # Add 0x prefix if missing
        if [[ "$addr" != 0x* ]]; then
            addr="0x$addr"
        fi
        echo "$addr"
    else
        echo "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
    fi
}

# Get Starcoin token balances
get_starcoin_balances() {
    local addr=$1
    echo -e "${BLUE}=== Starcoin Token Balances for $addr ===${NC}"
    
    # Get all resources
    local request="{\"jsonrpc\":\"2.0\",\"method\":\"state.list_resource\",\"params\":[\"$addr\",{\"decode\":true}],\"id\":1}"
    echo -e "${BLUE}[DEBUG] Executing: curl -s -X POST $STARCOIN_RPC -H 'Content-Type: application/json' -d '$request'${NC}"
    
    local result=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "$request" \
        "$STARCOIN_RPC" | tee >(cat >&2))
    
    local resources=$(echo "$result" | jq -r '.result.resources // {}')
    
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

# Get specific bridge token balance (returns raw number)
get_bridge_token_balance() {
    local addr=$1
    local token=$2  # ETH, BTC, USDC, USDT
    local bridge_addr=$(grep "starcoin-bridge-proxy-address:" bridge-config/server-config.yaml 2>/dev/null | awk '{print $2}' | tr -d '"')
    
    if [ -z "$bridge_addr" ]; then
        echo "0"
        return
    fi
    
    # Note: use full address format 0x00000000000000000000000000000001 instead of 0x1
    local resource_type="0x00000000000000000000000000000001::Account::Balance<${bridge_addr}::${token}::${token}>"
    local request="{\"jsonrpc\":\"2.0\",\"method\":\"state.get_resource\",\"params\":[\"$addr\",\"$resource_type\",{\"decode\":true}],\"id\":1}"
    
    echo -e "${BLUE}[DEBUG] Executing: curl -s -X POST $STARCOIN_RPC -H 'Content-Type: application/json' -d '$request'${NC}" >&2
    
    curl -s -X POST -H "Content-Type: application/json" \
        -d "$request" \
        "$STARCOIN_RPC" | tee >(cat >&2) | jq -r '.result.json.token.value // "0"'
}

# Get ETH token balances
get_eth_balances() {
    local addr=$1
    echo -e "${BLUE}=== ETH Balances for $addr ===${NC}"
    
    # Get ETH balance using python for big number handling
    local request="{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBalance\",\"params\":[\"$addr\",\"latest\"],\"id\":1}"
    echo -e "${BLUE}[DEBUG] Executing: curl -s -X POST $ETH_RPC -H 'Content-Type: application/json' -d '$request'${NC}"
    
    local result=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "$request" \
        "$ETH_RPC" | tee >(cat >&2))
    
    local eth_balance_hex=$(echo "$result" | jq -r '.result // "0x0"')
    
    local eth_balance=$(python3 -c "print(f'{int(\"$eth_balance_hex\", 16) / 1e18:.6f}')" 2>/dev/null || echo "0")
    echo -e "  ETH: ${GREEN}${eth_balance} ETH${NC}"
}

# Get latest Starcoin transaction status
get_latest_stc_tx_status() {
    local request='{"jsonrpc":"2.0","method":"chain.info","params":[],"id":1}'
    echo -e "${BLUE}[DEBUG] Executing: curl -s -X POST $STARCOIN_RPC -H 'Content-Type: application/json' -d '$request'${NC}" >&2
    
    curl -s -X POST -H "Content-Type: application/json" \
        -d "$request" \
        "$STARCOIN_RPC" | tee >(cat >&2) | jq -r '.result.head.number // "0"'
}

# Check bridge transfer record
check_bridge_record() {
    local source_chain=$1
    local seq_num=$2
    local bridge_addr=$(grep "starcoin-bridge-proxy-address:" bridge-config/server-config.yaml 2>/dev/null | awk '{print $2}' | tr -d '"')
    
    if [ -z "$bridge_addr" ]; then
        return 1
    fi
    
    local request="{\"jsonrpc\":\"2.0\",\"method\":\"state.get_resource\",\"params\":[\"$bridge_addr\",\"${bridge_addr}::Bridge::Bridge\",{\"decode\":true}],\"id\":1}"
    echo -e "${BLUE}[DEBUG] Executing: curl -s -X POST $STARCOIN_RPC -H 'Content-Type: application/json' -d '$request'${NC}" >&2
    
    local result=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "$request" \
        "$STARCOIN_RPC" | tee >(cat >&2))
    
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

# Wait for transaction and poll status by checking token balance
poll_bridge_status() {
    local direction=$1
    local stc_addr=$2
    local initial_balance=$3
    local start_time=$(date +%s)
    
    echo -e "${YELLOW}Polling bridge status (max ${MAX_WAIT}s)...${NC}"
    
    while true; do
        local current_time=$(date +%s)
        local elapsed=$((current_time - start_time))
        
        if [ $elapsed -ge $MAX_WAIT ]; then
            echo -e "${RED}✗ Timeout waiting for bridge transfer${NC}"
            return 1
        fi
        
        if [ "$direction" = "eth-to-stc" ]; then
            # Check if ETH token balance increased
            local current_balance=$(get_bridge_token_balance "$stc_addr" "ETH")
            if [ "$current_balance" != "0" ] && [ "$current_balance" != "$initial_balance" ]; then
                local received=$((current_balance - initial_balance))
                local received_eth=$(python3 -c "print(f'{$received / 1e8:g}')" 2>/dev/null || echo "0")
                echo -e "${GREEN}✓ Bridge transfer completed! Received ${received_eth} ETH${NC}"
                return 0
            fi
            echo -e "${YELLOW}... Waiting for token transfer... (${elapsed}s)${NC}"
        else
            # For stc-to-eth, check if approve status on Starcoin chain
            # Bridge node only does approve on Starcoin; user must claim on ETH manually
            local status=$(check_bridge_record 2 0)  # source_chain=2 (Starcoin), seq_num=0
            if [ "$status" = "approved" ] || [ "$status" = "claimed" ]; then
                echo -e "${GREEN}✓ Bridge approve completed on Starcoin!${NC}"
                echo -e "${YELLOW}Note: To receive tokens on ETH, you need to call transferBridgedTokensWithSignatures on the ETH bridge contract.${NC}"
                return 0
            fi
            echo -e "${YELLOW}... Waiting for Starcoin approve... (${elapsed}s)${NC}"
        fi
        
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
    
    # Validate addresses
    if [ -z "$stc_addr" ]; then
        echo -e "${RED}✗ Failed to get Starcoin address. Check that bridge_client.key exists.${NC}"
        exit 1
    fi
    if [ -z "$eth_addr" ]; then
        echo -e "${RED}✗ Failed to get ETH address. Check that bridge_authority.key exists.${NC}"
        exit 1
    fi
    
    echo -e "${YELLOW}Starcoin Address: $stc_addr${NC}"
    echo -e "${YELLOW}ETH Address: $eth_addr${NC}"
    echo ""
    
    # Execute transfer
    if [ "$DIRECTION" = "eth-to-stc" ]; then
        echo -e "${YELLOW}========================================${NC}"
        echo -e "${YELLOW}  ETH → Starcoin Transfer: $AMOUNT ETH${NC}"
        echo -e "${YELLOW}========================================${NC}"
        echo ""
        
        echo -e "${YELLOW}[1/4] Ensuring accounts are funded...${NC}"
        # Fund ETH wallet (from Anvil default account)
        make fund-eth-account 2>&1 | grep -E "Funded|Funding|funded|ETH" | head -3 || true
        # Fund bridge server on Starcoin (for gas)
        make fund-starcoin-bridge-account 2>&1 | grep -E "Funded|Funding|Bridge account|funded|STC" | head -3 || true
        echo ""
        
        # Record balances AFTER funding
        echo -e "${BLUE}[DEBUG] Getting Starcoin wETH balance...${NC}"
        local initial_eth_balance=$(get_bridge_token_balance "$stc_addr" "ETH")
        local initial_eth_balance_eth=$(python3 -c "print(f'{$initial_eth_balance / 1e8:g}')" 2>/dev/null || echo "0")
        
        local eth_request="{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBalance\",\"params\":[\"$eth_addr\",\"latest\"],\"id\":1}"
        echo -e "${BLUE}[DEBUG] Executing: curl -s -X POST $ETH_RPC -H 'Content-Type: application/json' -d '$eth_request'${NC}" 
        local initial_eth_wallet=$(curl -s -X POST -H "Content-Type: application/json" \
            -d "$eth_request" \
            "$ETH_RPC" | tee >(cat >&2))
        initial_eth_wallet=$(echo "$initial_eth_wallet" | jq -r '.result // "0x0"')
        local eth_before=$(python3 -c "print(f'{int(\"$initial_eth_wallet\", 16) / 1e18:g}')" 2>/dev/null || echo "0")
        
        echo -e "${BLUE}=== Before Transfer ===${NC}"
        echo -e "  Starcoin wETH:      ${GREEN}${initial_eth_balance_eth} ETH${NC}"
        echo -e "  Ethereum wallet:    ${GREEN}${eth_before} ETH${NC}"
        echo ""
        
        echo -e "${YELLOW}[2/4] Depositing $AMOUNT ETH to bridge contract on Ethereum...${NC}"
        # Use deposit-eth-core (no fund-eth-account dependency) to avoid affecting Before/After display
        make deposit-eth-core AMOUNT="$AMOUNT" 2>&1 | grep -E "Deposited|Deposit|Recipient|submitted|INFO" | tail -5
        echo ""
        
        echo -e "${YELLOW}[3/4] Waiting for bridge to approve and claim tokens...${NC}"
    else
        # Funding for stc-to-eth if needed
        echo -e "${YELLOW}[1/5] Funding accounts (if needed)...${NC}"
        make fund-starcoin-bridge-account 2>&1 | grep -E "Funded|Funding|Bridge account|funded|STC" | head -3 || true
        echo ""
        
        # Record initial balances AFTER funding
        echo -e "${BLUE}[DEBUG] Getting Starcoin wETH balance...${NC}"
        local initial_eth_balance=$(get_bridge_token_balance "$stc_addr" "ETH")
        local initial_eth_balance_eth=$(python3 -c "print(f'{$initial_eth_balance / 1e8:g}')" 2>/dev/null || echo "0")
        
        local eth_request="{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBalance\",\"params\":[\"$eth_addr\",\"latest\"],\"id\":1}"
        echo -e "${BLUE}[DEBUG] Executing: curl -s -X POST $ETH_RPC -H 'Content-Type: application/json' -d '$eth_request'${NC}"
        local initial_eth_wallet=$(curl -s -X POST -H "Content-Type: application/json" \
            -d "$eth_request" \
            "$ETH_RPC" | tee >(cat >&2))
        initial_eth_wallet=$(echo "$initial_eth_wallet" | jq -r '.result // "0x0"')
        local eth_before=$(python3 -c "print(f'{int(\"$initial_eth_wallet\", 16) / 1e18:g}')" 2>/dev/null || echo "0")
        
        echo -e "${BLUE}=== Before Transfer ===${NC}"
        echo -e "  Starcoin wETH:      ${GREEN}${initial_eth_balance_eth} ETH${NC}"
        echo -e "  Ethereum wallet:    ${GREEN}${eth_before} ETH${NC}"
        echo ""
        
        # Convert ETH amount to smallest unit (8 decimals)
        local amount_wei=$(echo "$AMOUNT * 100000000" | bc | cut -d. -f1)
        echo -e "${YELLOW}Initiating Starcoin → ETH transfer: $AMOUNT ETH ($amount_wei units)${NC}"
        make withdraw-to-eth AMOUNT="$amount_wei" TOKEN=ETH 2>&1 | tail -5
    fi
    
    echo ""
    
    # Poll for completion (pass stc_addr and initial balance)
    if poll_bridge_status "$DIRECTION" "$stc_addr" "$initial_eth_balance"; then
        echo ""
        
        if [ "$DIRECTION" = "eth-to-stc" ]; then
            echo -e "${YELLOW}[4/4] Transfer complete!${NC}"
            
            # Get final Starcoin token balance
            echo -e "${BLUE}[DEBUG] Getting final Starcoin wETH balance...${NC}"
            local final_eth_balance=$(get_bridge_token_balance "$stc_addr" "ETH")
            local final_eth_balance_eth=$(python3 -c "print(f'{$final_eth_balance / 1e8:g}')" 2>/dev/null || echo "0")
            
            echo -e "${BLUE}[DEBUG] Getting final ETH Wallet balance...${NC}"
            local eth_request="{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBalance\",\"params\":[\"$eth_addr\",\"latest\"],\"id\":1}"
            echo -e "${BLUE}[DEBUG] Executing: curl -s -X POST $ETH_RPC -H 'Content-Type: application/json' -d '$eth_request'${NC}" 
            local final_eth_wallet=$(curl -s -X POST -H "Content-Type: application/json" \
                -d "$eth_request" \
                "$ETH_RPC" | tee >(cat >&2))
            final_eth_wallet=$(echo "$final_eth_wallet" | jq -r '.result // "0x0"')
            local eth_after=$(python3 -c "print(f'{int(\"$final_eth_wallet\", 16) / 1e18:g}')" 2>/dev/null || echo "0")
            
            echo -e "${BLUE}=== After Transfer ===${NC}"
            echo -e "  Starcoin wETH:      ${GREEN}${final_eth_balance_eth} ETH${NC}"
            echo -e "  Ethereum wallet:    ${GREEN}${eth_after} ETH${NC}"
            
            # Calculate changes
            local token_change=$(python3 -c "print(f'{($final_eth_balance - $initial_eth_balance) / 1e8:g}')" 2>/dev/null || echo "0")
            local eth_change=$(python3 -c "print(f'{float(\"$eth_before\") - float(\"$eth_after\"):g}')" 2>/dev/null || echo "$AMOUNT")
            echo ""
            echo -e "${GREEN}✓ Bridge transfer successful!${NC}"
            echo -e "  Starcoin: ${GREEN}+${token_change} ETH${NC} (${initial_eth_balance_eth} → ${final_eth_balance_eth})"
            echo -e "  Ethereum: ${GREEN}-${eth_change} ETH${NC} (${eth_before} → ${eth_after})"
        else
            echo -e "${YELLOW}[5/5] Starcoin approve complete!${NC}"
            
            # Get final balances
            echo -e "${BLUE}[DEBUG] Getting final Starcoin wETH balance...${NC}"
            local final_eth_balance=$(get_bridge_token_balance "$stc_addr" "ETH")
            local final_eth_balance_eth=$(python3 -c "print(f'{$final_eth_balance / 1e8:g}')" 2>/dev/null || echo "0")
            
            echo -e "${BLUE}[DEBUG] Getting final ETH Wallet balance...${NC}"
            local eth_request="{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBalance\",\"params\":[\"$eth_addr\",\"latest\"],\"id\":1}"
            echo -e "${BLUE}[DEBUG] Executing: curl -s -X POST $ETH_RPC -H 'Content-Type: application/json' -d '$eth_request'${NC}"
            local final_eth_wallet=$(curl -s -X POST -H "Content-Type: application/json" \
                -d "$eth_request" \
                "$ETH_RPC" | tee >(cat >&2))
            final_eth_wallet=$(echo "$final_eth_wallet" | jq -r '.result // "0x0"')
            local eth_after=$(python3 -c "print(f'{int(\"$final_eth_wallet\", 16) / 1e18:g}')" 2>/dev/null || echo "0")
            
            echo -e "${BLUE}=== After Approve ===${NC}"
            echo -e "  Starcoin wETH:      ${GREEN}${final_eth_balance_eth} ETH${NC} (tokens burned)"
            echo -e "  Ethereum wallet:    ${GREEN}${eth_after} ETH${NC} (pending claim)"
            
            # Calculate changes
            local token_change=$(python3 -c "print(f'{($initial_eth_balance - $final_eth_balance) / 1e8:g}')" 2>/dev/null || echo "0")
            echo ""
            echo -e "${GREEN}✓ Starcoin→ETH approve successful!${NC}"
            echo -e "  Starcoin: ${GREEN}-${token_change} ETH${NC} (${initial_eth_balance_eth} → ${final_eth_balance_eth})"
            echo ""
            echo -e "${YELLOW}  Next step: Claim tokens on ETH by calling:${NC}"
            echo -e "${YELLOW}    Bridge.transferBridgedTokensWithSignatures(signatures, message)${NC}"
            echo -e "${YELLOW}  You can use 'make claim-on-eth' to complete the transfer.${NC}"
        fi
    else
        echo ""
        echo -e "${RED}✗ Bridge transfer may have failed. Check logs:${NC}"
        echo -e "${YELLOW}  - Bridge server logs: make logs${NC}"
        echo -e "${YELLOW}  - Starcoin node logs${NC}"
        
        # Show current balances anyway
        echo ""
        echo -e "${BLUE}=== Current Balances ===${NC}"
        get_starcoin_balances "$stc_addr"
        get_eth_balances "$eth_addr"
    fi
}

main
