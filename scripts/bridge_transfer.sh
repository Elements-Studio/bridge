#!/bin/bash
# Bridge Transfer Script with Polling
# Usage: bridge_transfer.sh [eth-to-stc|stc-to-eth] [amount] [--token TOKEN]
# TOKEN: ETH (default), USDT, USDC, BTC

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

# Parse arguments
DIRECTION=${1:-eth-to-stc}
AMOUNT=${2:-0.1}
TOKEN="ETH"  # Default token

# Parse --token argument
shift 2 2>/dev/null || true
while [[ $# -gt 0 ]]; do
    case $1 in
        --token)
            TOKEN="$2"
            shift 2
            ;;
        *)
            shift
            ;;
    esac
done

# Token configuration
# Token ID mapping: BTC=1, ETH=2, USDC=3, USDT=4
get_token_id() {
    local token=$1
    case $token in
        BTC)  echo 1 ;;
        ETH)  echo 2 ;;
        USDC) echo 3 ;;
        USDT) echo 4 ;;
        *)    echo 2 ;;  # Default to ETH
    esac
}

# Token decimals on ETH side
get_token_decimals() {
    local token=$1
    case $token in
        BTC)  echo 8 ;;
        ETH)  echo 18 ;;
        USDC) echo 6 ;;
        USDT) echo 6 ;;
        *)    echo 18 ;;
    esac
}

# Token decimals on Starcoin side (bridge adjusted)
get_starcoin_decimals() {
    local token=$1
    case $token in
        BTC)  echo 8 ;;
        ETH)  echo 8 ;;
        USDC) echo 6 ;;
        USDT) echo 6 ;;
        *)    echo 8 ;;
    esac
}

# Get token address from deployment.txt
get_token_address() {
    local token=$1
    if [ -f "bridge-config/deployment.txt" ]; then
        grep "\[Deployed\] $token:" bridge-config/deployment.txt | awk '{print $NF}'
    else
        echo ""
    fi
}

# Get bridge contract address
get_bridge_address() {
    if [ -f "bridge-config/deployment.txt" ]; then
        grep "ERC1967Proxy=" bridge-config/deployment.txt | cut -d'=' -f2
    else
        echo ""
    fi
}

# Anvil default account private key (10000 ETH)
ANVIL_PRIVATE_KEY="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
ANVIL_ADDRESS="0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"

# Mint ERC20 tokens (for testing with MockTokens)
# Note: Always mints to ANVIL_ADDRESS since that's the account we use for bridge operations
mint_erc20_token() {
    local token=$1
    local amount=$2  # in human-readable format (e.g., 100 for 100 USDT)
    
    local token_address=$(get_token_address "$token")
    if [ -z "$token_address" ]; then
        echo -e "${RED}✗ Cannot find token address for $token${NC}" >&2
        return 1
    fi
    
    local decimals=$(get_token_decimals "$token")
    # Convert to smallest unit
    local amount_wei=$(python3 -c "print(int($amount * (10 ** $decimals)))" 2>/dev/null)
    
    echo -e "${YELLOW}Minting $amount $token to $ANVIL_ADDRESS...${NC}"
    echo -e "${BLUE}[DEBUG] Token address: $token_address, Amount: $amount_wei (decimals: $decimals)${NC}"
    
    cast send "$token_address" \
        "mint(address,uint256)" \
        "$ANVIL_ADDRESS" \
        "$amount_wei" \
        --private-key "$ANVIL_PRIVATE_KEY" \
        --rpc-url "$ETH_RPC" \
        > /dev/null 2>&1
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Minted $amount $token${NC}"
    else
        echo -e "${RED}✗ Failed to mint $token${NC}" >&2
        return 1
    fi
}

# Approve ERC20 token for bridge contract
# Note: Always approves from ANVIL_ADDRESS since that's the account we use
approve_erc20_token() {
    local token=$1
    local amount=$2  # in human-readable format
    
    local token_address=$(get_token_address "$token")
    local bridge_address=$(get_bridge_address)
    
    if [ -z "$token_address" ] || [ -z "$bridge_address" ]; then
        echo -e "${RED}✗ Cannot find token or bridge address${NC}" >&2
        return 1
    fi
    
    local decimals=$(get_token_decimals "$token")
    local amount_wei=$(python3 -c "print(int($amount * (10 ** $decimals)))" 2>/dev/null)
    
    echo -e "${YELLOW}Approving bridge to spend $amount $token...${NC}"
    
    cast send "$token_address" \
        "approve(address,uint256)" \
        "$bridge_address" \
        "$amount_wei" \
        --private-key "$ANVIL_PRIVATE_KEY" \
        --rpc-url "$ETH_RPC" \
        > /dev/null 2>&1
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Approved bridge to spend $amount $token${NC}"
    else
        echo -e "${RED}✗ Failed to approve${NC}" >&2
        return 1
    fi
}

# Deposit ERC20 token to bridge (ETH -> Starcoin)
deposit_erc20_to_bridge() {
    local token=$1
    local amount=$2  # in human-readable format
    local recipient_address=$3  # Starcoin address (hex without 0x)
    
    local token_id=$(get_token_id "$token")
    local bridge_address=$(get_bridge_address)
    local decimals=$(get_token_decimals "$token")
    local amount_wei=$(python3 -c "print(int($amount * (10 ** $decimals)))" 2>/dev/null)
    
    # Remove 0x prefix from recipient address if present
    recipient_address=${recipient_address#0x}
    
    echo -e "${YELLOW}Depositing $amount $token to bridge...${NC}"
    echo -e "${BLUE}[DEBUG] Token ID: $token_id, Amount: $amount_wei, Recipient: 0x$recipient_address${NC}"
    
    # bridgeERC20(uint8 tokenID, uint256 amount, bytes recipientAddress, uint8 destinationChainID)
    # destinationChainID = 2 (Starcoin)
    cast send "$bridge_address" \
        "bridgeERC20(uint8,uint256,bytes,uint8)" \
        "$token_id" \
        "$amount_wei" \
        "0x$recipient_address" \
        2 \
        --private-key "$ANVIL_PRIVATE_KEY" \
        --rpc-url "$ETH_RPC" \
        > /dev/null 2>&1
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Deposited $amount $token to bridge${NC}"
    else
        echo -e "${RED}✗ Failed to deposit${NC}" >&2
        return 1
    fi
}

# Get ERC20 token balance on ETH side
get_eth_erc20_balance() {
    local token=$1
    local address=$2
    
    local token_address=$(get_token_address "$token")
    if [ -z "$token_address" ]; then
        echo "0"
        return
    fi
    
    local balance_hex=$(cast call "$token_address" \
        "balanceOf(address)" \
        "$address" \
        --rpc-url "$ETH_RPC" 2>/dev/null)
    
    local decimals=$(get_token_decimals "$token")
    python3 -c "print(f'{int(\"$balance_hex\", 16) / (10 ** $decimals):g}')" 2>/dev/null || echo "0"
}

# Fund a Starcoin account with STC (creates account if not exists)
# This is needed before the account can accept tokens
fund_starcoin_account() {
    local account=$1
    local starcoin_data_dir="${STARCOIN_DATA_DIR:-/tmp}"
    local starcoin_ipc="${starcoin_data_dir}/dev/starcoin.ipc"
    local starcoin_cmd="${STARCOIN_PATH:-starcoin}"
    
    if [ ! -S "$starcoin_ipc" ]; then
        echo -e "${RED}✗ Starcoin IPC socket not found at $starcoin_ipc${NC}" >&2
        return 1
    fi
    
    # Check if account exists
    local check_result=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"state.get_resource\",\"params\":[\"$account\",\"0x00000000000000000000000000000001::Account::Account\",{\"decode\":true}],\"id\":1}" \
        "$STARCOIN_RPC")
    
    local exists=$(echo "$check_result" | jq -r '.result != null')
    if [ "$exists" = "true" ]; then
        echo -e "${GREEN}✓ Starcoin account $account already exists${NC}"
        return 0
    fi
    
    echo -e "${YELLOW}Creating Starcoin account $account with STC...${NC}"
    
    # Get the bridge deployer account from config (this is the default account with STC)
    local bridge_deployer=$(grep "starcoin-bridge-proxy-address:" bridge-config/server-config.yaml 2>/dev/null | awk '{print $2}' | tr -d '"')
    
    if [ -n "$bridge_deployer" ]; then
        # Unlock the deployer account first (required for transfer)
        echo "" | $starcoin_cmd -c "$starcoin_ipc" account unlock "$bridge_deployer" -d 300 > /dev/null 2>&1 || true
        
        # Transfer STC from deployer account to create the new account
        # Note: use -v for amount, not --amount
        local result=$(echo "" | $starcoin_cmd -c "$starcoin_ipc" account transfer \
            -s "$bridge_deployer" \
            --receiver "$account" \
            -v 1000000000 \
            -b 2>&1)
        
        if echo "$result" | grep -q '"status": "Executed"'; then
            echo -e "${GREEN}✓ Funded Starcoin account with 1 STC${NC}"
            sleep 1
            return 0
        fi
        echo -e "${YELLOW}Transfer failed, trying dev get-coin...${NC}"
    else
        echo -e "${YELLOW}No bridge deployer found, trying dev get-coin...${NC}"
    fi
    
    # Fallback to dev get-coin if transfer fails
    local result=$($starcoin_cmd -c "$starcoin_ipc" dev get-coin -v 1000000000 "$account" 2>&1)
    
    if echo "$result" | grep -q '"status": "Executed"'; then
        echo -e "${GREEN}✓ Funded Starcoin account with 1 STC${NC}"
        sleep 1
        return 0
    fi
    
    echo -e "${YELLOW}⚠ Funding may have issues, continuing anyway...${NC}"
    return 0
}

# Accept token on Starcoin for an account (required before receiving tokens)
# Uses the bridge_client.key to sign the transaction
accept_token_on_starcoin() {
    local token=$1  # ETH, USDT, USDC, BTC
    local account=$2  # Starcoin address
    
    local bridge_addr=$(grep "starcoin-bridge-proxy-address:" bridge-config/server-config.yaml 2>/dev/null | awk '{print $2}' | tr -d '"')
    
    if [ -z "$bridge_addr" ]; then
        echo -e "${RED}✗ Cannot find bridge address${NC}" >&2
        return 1
    fi
    
    local token_type="${bridge_addr}::${token}::${token}"
    
    # Check if account already accepts this token
    local resource_type="0x00000000000000000000000000000001::Account::Balance<${token_type}>"
    local check_result=$(curl -s -X POST -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"state.get_resource\",\"params\":[\"$account\",\"$resource_type\",{\"decode\":true}],\"id\":1}" \
        "$STARCOIN_RPC")
    
    local exists=$(echo "$check_result" | jq -r '.result != null')
    if [ "$exists" = "true" ]; then
        echo -e "${GREEN}✓ Account already accepts $token${NC}"
        return 0
    fi
    
    echo -e "${YELLOW}Accepting $token for account $account...${NC}"
    
    # Use starcoin CLI to execute accept_token
    # Build IPC path same way as Makefile: $(STARCOIN_DATA_DIR)/dev/starcoin.ipc
    local starcoin_data_dir="${STARCOIN_DATA_DIR:-/tmp}"
    local starcoin_ipc="${starcoin_data_dir}/dev/starcoin.ipc"
    local starcoin_cmd="${STARCOIN_PATH:-starcoin}"
    
    if [ ! -S "$starcoin_ipc" ]; then
        echo -e "${RED}✗ Starcoin IPC socket not found at $starcoin_ipc${NC}" >&2
        echo -e "${YELLOW}  Make sure Starcoin node is running with: make start-starcoin${NC}" >&2
        return 1
    fi
    
    # Fund the account with some STC first (for gas)
    $starcoin_cmd -c "$starcoin_ipc" dev get-coin -v 1000000 "$account" > /dev/null 2>&1 || true
    
    # Import the bridge_client.key if exists
    if [ -f "bridge-node/server-config/bridge_client.key" ]; then
        $starcoin_cmd -c "$starcoin_ipc" account import -i "$(cat bridge-node/server-config/bridge_client.key | head -1)" > /dev/null 2>&1 || true
    fi
    
    # Execute accept_token (use --type_tag or -t for type parameters)
    local result=$(echo "" | $starcoin_cmd -c "$starcoin_ipc" account execute-function \
        --function "0x1::Account::accept_token" \
        -t "$token_type" \
        -s "$account" \
        -b 2>&1)
    
    if echo "$result" | grep -q "error\|Error\|ERROR"; then
        echo -e "${RED}✗ Failed to accept $token: $result${NC}" >&2
        return 1
    fi
    
    echo -e "${GREEN}✓ Accepted $token for account${NC}"
    
    # Wait a moment for the transaction to be confirmed
    sleep 2
}

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
    local token=$4  # Token type: ETH, USDT, etc.
    local start_time=$(date +%s)
    
    local stc_decimals=$(get_starcoin_decimals "$token")
    
    echo -e "${YELLOW}Polling bridge status (max ${MAX_WAIT}s)...${NC}"
    
    while true; do
        local current_time=$(date +%s)
        local elapsed=$((current_time - start_time))
        
        if [ $elapsed -ge $MAX_WAIT ]; then
            echo -e "${RED}✗ Timeout waiting for bridge transfer${NC}"
            return 1
        fi
        
        if [ "$direction" = "eth-to-stc" ]; then
            # Check if token balance increased
            local current_balance=$(get_bridge_token_balance "$stc_addr" "$token")
            if [ "$current_balance" != "0" ] && [ "$current_balance" != "$initial_balance" ]; then
                local received=$((current_balance - initial_balance))
                local received_display=$(python3 -c "print(f'{$received / (10 ** $stc_decimals):g}')" 2>/dev/null || echo "0")
                echo -e "${GREEN}✓ Bridge transfer completed! Received ${received_display} ${token}${NC}"
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
    echo -e "${YELLOW}Token: $TOKEN${NC}"
    
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
    
    local stc_decimals=$(get_starcoin_decimals "$TOKEN")
    local eth_decimals=$(get_token_decimals "$TOKEN")
    
    # Execute transfer
    if [ "$DIRECTION" = "eth-to-stc" ]; then
        echo -e "${YELLOW}========================================${NC}"
        echo -e "${YELLOW}  ETH → Starcoin Transfer: $AMOUNT $TOKEN${NC}"
        echo -e "${YELLOW}========================================${NC}"
        echo ""
        
        echo -e "${YELLOW}[1/4] Ensuring accounts are funded...${NC}"
        # Fund ETH wallet (from Anvil default account) - only needed for gas
        make fund-eth-account 2>&1 | grep -E "Funded|Funding|funded|ETH" | head -3 || true
        # Fund bridge server on Starcoin (for gas)
        make fund-starcoin-bridge-account 2>&1 | grep -E "Funded|Funding|Bridge account|funded|STC" | head -3 || true
        
        # Fund recipient Starcoin account (creates account if not exists, needed for accept_token)
        echo -e "${YELLOW}Funding recipient Starcoin account...${NC}"
        fund_starcoin_account "$stc_addr"
        echo ""
        
        # Record balances AFTER funding
        echo -e "${BLUE}[DEBUG] Getting Starcoin $TOKEN balance...${NC}"
        local initial_token_balance=$(get_bridge_token_balance "$stc_addr" "$TOKEN")
        local initial_token_balance_display=$(python3 -c "print(f'{$initial_token_balance / (10 ** $stc_decimals):g}')" 2>/dev/null || echo "0")
        
        # Get ETH wallet balance for token
        if [ "$TOKEN" = "ETH" ]; then
            local eth_request="{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBalance\",\"params\":[\"$eth_addr\",\"latest\"],\"id\":1}"
            echo -e "${BLUE}[DEBUG] Executing: curl -s -X POST $ETH_RPC -H 'Content-Type: application/json' -d '$eth_request'${NC}" 
            local initial_eth_wallet=$(curl -s -X POST -H "Content-Type: application/json" \
                -d "$eth_request" \
                "$ETH_RPC" | tee >(cat >&2))
            initial_eth_wallet=$(echo "$initial_eth_wallet" | jq -r '.result // "0x0"')
            local eth_before=$(python3 -c "print(f'{int(\"$initial_eth_wallet\", 16) / 1e18:g}')" 2>/dev/null || echo "0")
        else
            # For ERC20 tokens
            local eth_before=$(get_eth_erc20_balance "$TOKEN" "$eth_addr")
        fi
        
        echo -e "${BLUE}=== Before Transfer ===${NC}"
        echo -e "  Starcoin $TOKEN:    ${GREEN}${initial_token_balance_display} ${TOKEN}${NC}"
        echo -e "  Ethereum $TOKEN:    ${GREEN}${eth_before} ${TOKEN}${NC}"
        echo ""
        
        echo -e "${YELLOW}[2/4] Depositing $AMOUNT $TOKEN to bridge contract on Ethereum...${NC}"
        
        if [ "$TOKEN" = "ETH" ]; then
            # Use existing deposit-eth-core for native ETH
            make deposit-eth-core AMOUNT="$AMOUNT" 2>&1 | grep -E "Deposited|Deposit|Recipient|submitted|INFO" | tail -5
        else
            # For ERC20 tokens: mint, approve, and deposit
            # Note: Uses ANVIL_ADDRESS for all operations
            echo -e "${YELLOW}[2a/4] Minting $AMOUNT $TOKEN for testing...${NC}"
            mint_erc20_token "$TOKEN" "$AMOUNT"
            
            echo -e "${YELLOW}[2b/4] Approving bridge contract...${NC}"
            approve_erc20_token "$TOKEN" "$AMOUNT"
            
            echo -e "${YELLOW}[2c/4] Depositing to bridge...${NC}"
            # Remove 0x prefix from stc_addr for the bridge call
            local stc_addr_hex=${stc_addr#0x}
            deposit_erc20_to_bridge "$TOKEN" "$AMOUNT" "$stc_addr_hex"
        fi
        echo ""
        
        echo -e "${YELLOW}[3/4] Waiting for bridge to approve and claim tokens...${NC}"
    else
        # Starcoin -> ETH direction
        echo -e "${YELLOW}========================================${NC}"
        echo -e "${YELLOW}  Starcoin → ETH Transfer: $AMOUNT $TOKEN${NC}"
        echo -e "${YELLOW}========================================${NC}"
        echo ""
        
        # Funding for stc-to-eth if needed
        echo -e "${YELLOW}[1/5] Funding accounts (if needed)...${NC}"
        make fund-starcoin-bridge-account 2>&1 | grep -E "Funded|Funding|Bridge account|funded|STC" | head -3 || true
        echo ""
        
        # Record initial balances AFTER funding
        echo -e "${BLUE}[DEBUG] Getting Starcoin $TOKEN balance...${NC}"
        local initial_token_balance=$(get_bridge_token_balance "$stc_addr" "$TOKEN")
        local initial_token_balance_display=$(python3 -c "print(f'{$initial_token_balance / (10 ** $stc_decimals):g}')" 2>/dev/null || echo "0")
        
        # Get ETH side balance
        if [ "$TOKEN" = "ETH" ]; then
            local eth_request="{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBalance\",\"params\":[\"$eth_addr\",\"latest\"],\"id\":1}"
            echo -e "${BLUE}[DEBUG] Executing: curl -s -X POST $ETH_RPC -H 'Content-Type: application/json' -d '$eth_request'${NC}"
            local initial_eth_wallet=$(curl -s -X POST -H "Content-Type: application/json" \
                -d "$eth_request" \
                "$ETH_RPC" | tee >(cat >&2))
            initial_eth_wallet=$(echo "$initial_eth_wallet" | jq -r '.result // "0x0"')
            local eth_before=$(python3 -c "print(f'{int(\"$initial_eth_wallet\", 16) / 1e18:g}')" 2>/dev/null || echo "0")
        else
            local eth_before=$(get_eth_erc20_balance "$TOKEN" "$eth_addr")
        fi
        
        echo -e "${BLUE}=== Before Transfer ===${NC}"
        echo -e "  Starcoin $TOKEN:    ${GREEN}${initial_token_balance_display} ${TOKEN}${NC}"
        echo -e "  Ethereum $TOKEN:    ${GREEN}${eth_before} ${TOKEN}${NC}"
        echo ""
        
        # Convert amount to smallest unit based on Starcoin decimals
        local amount_smallest=$(python3 -c "print(int($AMOUNT * (10 ** $stc_decimals)))" 2>/dev/null)
        echo -e "${YELLOW}Initiating Starcoin → ETH transfer: $AMOUNT $TOKEN ($amount_smallest units)${NC}"
        make withdraw-to-eth AMOUNT="$amount_smallest" TOKEN="$TOKEN" 2>&1 | tail -5
    fi
    
    echo ""
    
    # Poll for completion (pass stc_addr, initial balance, and token)
    if poll_bridge_status "$DIRECTION" "$stc_addr" "$initial_token_balance" "$TOKEN"; then
        echo ""
        
        if [ "$DIRECTION" = "eth-to-stc" ]; then
            echo -e "${YELLOW}[4/4] Transfer complete!${NC}"
            
            # Get final Starcoin token balance
            echo -e "${BLUE}[DEBUG] Getting final Starcoin $TOKEN balance...${NC}"
            local final_token_balance=$(get_bridge_token_balance "$stc_addr" "$TOKEN")
            local final_token_balance_display=$(python3 -c "print(f'{$final_token_balance / (10 ** $stc_decimals):g}')" 2>/dev/null || echo "0")
            
            # Get final ETH side balance
            echo -e "${BLUE}[DEBUG] Getting final Ethereum $TOKEN balance...${NC}"
            if [ "$TOKEN" = "ETH" ]; then
                local eth_request="{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBalance\",\"params\":[\"$eth_addr\",\"latest\"],\"id\":1}"
                echo -e "${BLUE}[DEBUG] Executing: curl -s -X POST $ETH_RPC -H 'Content-Type: application/json' -d '$eth_request'${NC}" 
                local final_eth_wallet=$(curl -s -X POST -H "Content-Type: application/json" \
                    -d "$eth_request" \
                    "$ETH_RPC" | tee >(cat >&2))
                final_eth_wallet=$(echo "$final_eth_wallet" | jq -r '.result // "0x0"')
                local eth_after=$(python3 -c "print(f'{int(\"$final_eth_wallet\", 16) / 1e18:g}')" 2>/dev/null || echo "0")
            else
                local eth_after=$(get_eth_erc20_balance "$TOKEN" "$eth_addr")
            fi
            
            echo -e "${BLUE}=== After Transfer ===${NC}"
            echo -e "  Starcoin $TOKEN:    ${GREEN}${final_token_balance_display} ${TOKEN}${NC}"
            echo -e "  Ethereum $TOKEN:    ${GREEN}${eth_after} ${TOKEN}${NC}"
            
            # Calculate changes
            local token_change=$(python3 -c "print(f'{($final_token_balance - $initial_token_balance) / (10 ** $stc_decimals):g}')" 2>/dev/null || echo "0")
            local eth_change=$(python3 -c "print(f'{float(\"$eth_before\") - float(\"$eth_after\"):g}')" 2>/dev/null || echo "$AMOUNT")
            echo ""
            echo -e "${GREEN}✓ Bridge transfer successful!${NC}"
            echo -e "  Starcoin: ${GREEN}+${token_change} ${TOKEN}${NC} (${initial_token_balance_display} → ${final_token_balance_display})"
            echo -e "  Ethereum: ${GREEN}-${eth_change} ${TOKEN}${NC} (${eth_before} → ${eth_after})"
        else
            echo -e "${YELLOW}[5/5] Starcoin approve complete!${NC}"
            
            # Get final balances
            echo -e "${BLUE}[DEBUG] Getting final Starcoin $TOKEN balance...${NC}"
            local final_token_balance=$(get_bridge_token_balance "$stc_addr" "$TOKEN")
            local final_token_balance_display=$(python3 -c "print(f'{$final_token_balance / (10 ** $stc_decimals):g}')" 2>/dev/null || echo "0")
            
            echo -e "${BLUE}[DEBUG] Getting final Ethereum $TOKEN balance...${NC}"
            if [ "$TOKEN" = "ETH" ]; then
                local eth_request="{\"jsonrpc\":\"2.0\",\"method\":\"eth_getBalance\",\"params\":[\"$eth_addr\",\"latest\"],\"id\":1}"
                echo -e "${BLUE}[DEBUG] Executing: curl -s -X POST $ETH_RPC -H 'Content-Type: application/json' -d '$eth_request'${NC}"
                local final_eth_wallet=$(curl -s -X POST -H "Content-Type: application/json" \
                    -d "$eth_request" \
                    "$ETH_RPC" | tee >(cat >&2))
                final_eth_wallet=$(echo "$final_eth_wallet" | jq -r '.result // "0x0"')
                local eth_after=$(python3 -c "print(f'{int(\"$final_eth_wallet\", 16) / 1e18:g}')" 2>/dev/null || echo "0")
            else
                local eth_after=$(get_eth_erc20_balance "$TOKEN" "$eth_addr")
            fi
            
            echo -e "${BLUE}=== After Approve ===${NC}"
            echo -e "  Starcoin $TOKEN:    ${GREEN}${final_token_balance_display} ${TOKEN}${NC} (tokens burned)"
            echo -e "  Ethereum $TOKEN:    ${GREEN}${eth_after} ${TOKEN}${NC} (pending claim)"
            
            # Calculate changes
            local token_change=$(python3 -c "print(f'{($initial_token_balance - $final_token_balance) / (10 ** $stc_decimals):g}')" 2>/dev/null || echo "0")
            echo ""
            echo -e "${GREEN}✓ Starcoin→ETH approve successful!${NC}"
            echo -e "  Starcoin: ${GREEN}-${token_change} ${TOKEN}${NC} (${initial_token_balance_display} → ${final_token_balance_display})"
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
