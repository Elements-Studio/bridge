#!/bin/bash
# Bridge Indexer Management Script
# Usage: ./scripts/indexer.sh <command>

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
CONFIG_FILE="${PROJECT_DIR}/bridge-config/server-config.yaml"
LOG_FILE="/tmp/indexer.log"
POSTGRES_CONTAINER="bridge-postgres"
DATABASE_URL="postgres://postgres:postgrespw@localhost:5432/bridge"

# Read from config file if it exists
if [ -f "$CONFIG_FILE" ]; then
    # Extract RPC URL from config
    RPC_URL=$(grep -E "^\s*starcoin-bridge-rpc-url:" "$CONFIG_FILE" | sed 's/.*starcoin-bridge-rpc-url:\s*//' | tr -d '"' | tr -d "'" | xargs)
    # Extract bridge address from config  
    BRIDGE_ADDRESS=$(grep -E "^\s*starcoin-bridge-proxy-address:" "$CONFIG_FILE" | sed 's/.*starcoin-bridge-proxy-address:\s*//' | tr -d '"' | tr -d "'" | xargs)
    # Extract metrics port from config
    METRICS_PORT=$(grep -E "^metrics-port:" "$CONFIG_FILE" | sed 's/.*metrics-port:\s*//' | xargs)
    # Extract ETH settings from config
    ETH_RPC_URL=$(grep -E "^\s*eth-rpc-url:" "$CONFIG_FILE" | sed 's/.*eth-rpc-url:\s*//' | tr -d '"' | tr -d "'" | xargs)
    ETH_BRIDGE_ADDRESS=$(grep -E "^\s*eth-bridge-proxy-address:" "$CONFIG_FILE" | sed 's/.*eth-bridge-proxy-address:\s*//' | tr -d '"' | tr -d "'" | xargs)
fi

# Fallback to environment variables or defaults
RPC_URL="${RPC_URL:-http://localhost:9850}"
BRIDGE_ADDRESS="${BRIDGE_ADDRESS:-0x8410d7aa5a55957450fa2493499eabcf}"
ETH_RPC_URL="${ETH_RPC_URL:-http://localhost:8545}"
ETH_BRIDGE_ADDRESS="${ETH_BRIDGE_ADDRESS:-0x0B306BF915C4d645ff596e518fAf3F9669b97016}"
ETH_START_BLOCK="${ETH_START_BLOCK:-0}"
RUST_LOG="${RUST_LOG:-info}"

# Find an available port starting from the given port
find_available_port() {
    local start_port="${1:-9184}"
    local port=$start_port
    local max_port=$((start_port + 100))
    
    while [ $port -lt $max_port ]; do
        if ! lsof -i :$port >/dev/null 2>&1; then
            echo $port
            return 0
        fi
        port=$((port + 1))
    done
    
    # Fallback: return original port
    echo $start_port
    return 1
}

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if PostgreSQL container is running
check_postgres() {
    if docker ps --format '{{.Names}}' | grep -q "^${POSTGRES_CONTAINER}$"; then
        return 0
    else
        return 1
    fi
}

# Start PostgreSQL container
start_postgres() {
    log_info "Starting PostgreSQL container..."
    if check_postgres; then
        log_info "PostgreSQL is already running"
    else
        if docker ps -a --format '{{.Names}}' | grep -q "^${POSTGRES_CONTAINER}$"; then
            docker start "$POSTGRES_CONTAINER"
        else
            docker run -d \
                --name "$POSTGRES_CONTAINER" \
                -e POSTGRES_PASSWORD=postgrespw \
                -p 5432:5432 \
                postgres:15
            sleep 3
        fi
        log_info "PostgreSQL started"
    fi
    
    # Ensure database exists
    log_info "Ensuring database 'bridge' exists..."
    docker exec "$POSTGRES_CONTAINER" psql -U postgres -tc "SELECT 1 FROM pg_database WHERE datname = 'bridge'" | grep -q 1 || \
        docker exec "$POSTGRES_CONTAINER" psql -U postgres -c "CREATE DATABASE bridge;"
    log_info "Database ready"
}

# Stop PostgreSQL container
stop_postgres() {
    log_info "Stopping PostgreSQL container..."
    if check_postgres; then
        docker stop "$POSTGRES_CONTAINER"
        log_info "PostgreSQL stopped"
    else
        log_warn "PostgreSQL is not running"
    fi
}

# Reset database (drop and recreate)
reset_database() {
    log_info "Resetting database..."
    if ! check_postgres; then
        log_error "PostgreSQL is not running"
        exit 1
    fi
    
    # Stop indexer first to release database connections
    log_info "Stopping indexer to release database connections..."
    pkill -f bridge-indexer-alt 2>/dev/null || true
    sleep 2
    
    # Terminate all connections to the database
    docker exec "$POSTGRES_CONTAINER" psql -U postgres -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = 'bridge' AND pid <> pg_backend_pid();" >/dev/null 2>&1 || true
    
    docker exec "$POSTGRES_CONTAINER" psql -U postgres -c "DROP DATABASE IF EXISTS bridge;"
    docker exec "$POSTGRES_CONTAINER" psql -U postgres -c "CREATE DATABASE bridge;"
    log_info "Database reset complete"
}

# Build indexer
build_indexer() {
    log_info "Building indexer..."
    cd "$PROJECT_DIR"
    cargo build -p starcoin-bridge-indexer-alt
    log_info "Build complete"
}

# Start indexer
start_indexer() {
    local first_block="${1:-}"
    local background="${2:-false}"
    local enable_eth="${3:-false}"
    
    # Auto-detect available metrics port if not specified
    local preferred_port="${METRICS_PORT:-9184}"
    if [ -n "$METRICS_ADDRESS" ]; then
        # Extract port from METRICS_ADDRESS if set
        preferred_port=$(echo "$METRICS_ADDRESS" | sed 's/.*://')
    fi
    local available_port=$(find_available_port "$preferred_port")
    local metrics_addr="0.0.0.0:$available_port"
    
    if [ "$available_port" != "$preferred_port" ]; then
        log_warn "Port $preferred_port is in use, using port $available_port instead"
    fi
    
    log_info "Starting Bridge Indexer..."
    log_info "  Config: $CONFIG_FILE"
    log_info "  RPC URL: $RPC_URL"
    log_info "  Bridge Address: $BRIDGE_ADDRESS"
    log_info "  Database: $DATABASE_URL"
    log_info "  Metrics: $metrics_addr"
    log_info "  Log File: $LOG_FILE"
    if [ "$enable_eth" = "true" ]; then
        log_info "  ETH Indexer: ENABLED"
        log_info "  ETH RPC URL: $ETH_RPC_URL"
        log_info "  ETH Bridge Address: $ETH_BRIDGE_ADDRESS"
        log_info "  ETH Start Block: $ETH_START_BLOCK"
    fi
    
    cd "$PROJECT_DIR"
    
    # Check if indexer binary exists
    if [ ! -f "./target/debug/bridge-indexer-alt" ]; then
        log_error "Indexer binary not found. Run '$0 build' first."
        exit 1
    fi
    
    # Stop existing indexer if running
    pkill -f bridge-indexer-alt 2>/dev/null || true
    sleep 1
    
    # Build the command arguments
    local args="--rpc-api-url $RPC_URL --bridge-address $BRIDGE_ADDRESS"
    
    if [ -n "$first_block" ]; then
        args="$args --first-checkpoint $first_block"
        log_info "  Starting from block: $first_block"
    fi
    
    # Add ETH indexer arguments if enabled
    if [ "$enable_eth" = "true" ]; then
        args="$args --enable-eth --eth-rpc-url $ETH_RPC_URL --eth-bridge-address $ETH_BRIDGE_ADDRESS --eth-start-block $ETH_START_BLOCK"
    fi

    if [ "$background" = "true" ]; then
        log_info "Running in background, logging to $LOG_FILE"
        # Clear log file and start fresh
        > "$LOG_FILE"
        # Start in background with proper redirection
        (RUST_LOG=$RUST_LOG METRICS_ADDRESS=$metrics_addr exec ./target/debug/bridge-indexer-alt $args) >> "$LOG_FILE" 2>&1 &
        local pid=$!
        echo $pid > /tmp/indexer.pid
        log_info "Indexer started with PID $pid"
        sleep 5
        if ps -p $pid > /dev/null 2>&1; then
            log_info "Indexer is running"
            if [ -s "$LOG_FILE" ]; then
                tail -20 "$LOG_FILE"
            else
                log_warn "Log file is empty, checking process directly..."
                # Check if process is working by querying database
                local watermarks=$(docker exec "$POSTGRES_CONTAINER" psql -U postgres -d bridge -t -c "SELECT checkpoint_hi_inclusive FROM watermarks LIMIT 1;" 2>/dev/null | tr -d ' ')
                if [ -n "$watermarks" ] && [ "$watermarks" != "" ]; then
                    log_info "Indexer is working (processed up to block $watermarks)"
                fi
            fi
        else
            log_error "Indexer failed to start. Check $LOG_FILE for details"
            cat "$LOG_FILE"
            exit 1
        fi
    else
        RUST_LOG=$RUST_LOG METRICS_ADDRESS=$metrics_addr ./target/debug/bridge-indexer-alt $args
    fi
}

# Stop indexer
stop_indexer() {
    log_info "Stopping indexer..."
    if pkill -f bridge-indexer-alt 2>/dev/null; then
        log_info "Indexer stopped"
    else
        log_warn "Indexer was not running"
    fi
    rm -f /tmp/indexer.pid
}

# Show indexer status
status() {
    echo "=== Bridge Indexer Status ==="
    echo ""
    
    # PostgreSQL status
    if check_postgres; then
        echo -e "PostgreSQL: ${GREEN}Running${NC}"
        
        # Check database
        if docker exec "$POSTGRES_CONTAINER" psql -U postgres -tc "SELECT 1 FROM pg_database WHERE datname = 'bridge'" 2>/dev/null | grep -q 1; then
            echo -e "Database 'bridge': ${GREEN}Exists${NC}"
            
            # Check watermarks
            echo ""
            echo "Watermarks:"
            docker exec "$POSTGRES_CONTAINER" psql -U postgres -d bridge -c \
                "SELECT pipeline, checkpoint_hi_inclusive as block_height FROM watermarks;" 2>/dev/null || echo "  (No watermarks table yet)"
            
            # Check data counts
            echo ""
            echo "Data counts:"
            docker exec "$POSTGRES_CONTAINER" psql -U postgres -d bridge -c \
                "SELECT 'token_transfer' as table_name, COUNT(*) as count FROM token_transfer
                 UNION ALL
                 SELECT 'token_transfer_data', COUNT(*) FROM token_transfer_data
                 UNION ALL
                 SELECT 'governance_actions', COUNT(*) FROM governance_actions;" 2>/dev/null || echo "  (Tables not created yet)"
        else
            echo -e "Database 'bridge': ${YELLOW}Not created${NC}"
        fi
    else
        echo -e "PostgreSQL: ${RED}Not running${NC}"
    fi
    
    echo ""
}

# Show logs from database
logs() {
    if ! check_postgres; then
        log_error "PostgreSQL is not running"
        exit 1
    fi
    
    echo "=== Recent Token Transfers ==="
    docker exec "$POSTGRES_CONTAINER" psql -U postgres -d bridge -c \
        "SELECT * FROM token_transfer ORDER BY block_height DESC LIMIT 10;" 2>/dev/null || echo "(No data)"
    
    echo ""
    echo "=== Recent Token Transfer Data ==="
    docker exec "$POSTGRES_CONTAINER" psql -U postgres -d bridge -c \
        "SELECT * FROM token_transfer_data ORDER BY block_height DESC LIMIT 10;" 2>/dev/null || echo "(No data)"
}

# Print help
print_help() {
    echo "Bridge Indexer Management Script"
    echo ""
    echo "Usage: $0 <command> [options]"
    echo ""
    echo "Commands:"
    echo "  start               Start PostgreSQL and Starcoin indexer (foreground)"
    echo "  start-bg            Start PostgreSQL and Starcoin indexer (background)"
    echo "  start-eth           Start PostgreSQL and indexer with ETH support (foreground)"
    echo "  start-eth-bg        Start PostgreSQL and indexer with ETH support (background)"
    echo "  clean-start         Reset database and start fresh from block 0 (foreground)"
    echo "  clean-start-bg      Reset database and start fresh from block 0 (background)"
    echo "  clean-start-eth     Reset database and start with ETH support from block 0"
    echo "  clean-start-eth-bg  Reset database and start with ETH support (background)"
    echo "  stop                Stop indexer"
    echo "  stop-all            Stop indexer and PostgreSQL"
    echo "  build               Build the indexer"
    echo "  reset-db            Reset database (drop and recreate)"
    echo "  status              Show current status"
    echo "  logs                Show recent data from database"
    echo "  tail-logs           Tail the indexer log file"
    echo "  psql                Open psql shell to bridge database"
    echo "  help                Show this help message"
    echo ""
    echo "Config file: $CONFIG_FILE"
    echo ""
    echo "Current settings (from config):"
    echo "  RPC_URL:            $RPC_URL"
    echo "  BRIDGE_ADDRESS:     $BRIDGE_ADDRESS"
    echo "  METRICS:            $METRICS_ADDRESS"
    echo "  ETH_RPC_URL:        $ETH_RPC_URL"
    echo "  ETH_BRIDGE_ADDRESS: $ETH_BRIDGE_ADDRESS"
    echo ""
    echo "Environment variables (override config):"
    echo "  RPC_URL             Starcoin RPC URL"
    echo "  BRIDGE_ADDRESS      Bridge contract address"
    echo "  METRICS_ADDRESS     Metrics server address"
    echo "  ETH_RPC_URL         Ethereum RPC URL"
    echo "  ETH_BRIDGE_ADDRESS  Ethereum bridge contract address"
    echo "  ETH_START_BLOCK     ETH indexer start block (default: 0)"
    echo "  RUST_LOG            Log level (default: info)"
    echo ""
    echo "Examples:"
    echo "  $0 start                    # Start Starcoin indexer in foreground"
    echo "  $0 start-eth-bg             # Start with ETH support in background"
    echo "  $0 clean-start-eth-bg       # Fresh start with ETH from block 0 (background)"
    echo "  RUST_LOG=debug $0 start-bg  # Start with debug logging"
    echo "  $0 tail-logs                # Follow indexer logs"
    echo ""
}

# Main command handler
case "${1:-help}" in
    start)
        start_postgres
        start_indexer
        ;;
    start-bg)
        start_postgres
        start_indexer "" true
        ;;
    start-eth)
        start_postgres
        start_indexer "" false true
        ;;
    start-eth-bg)
        start_postgres
        start_indexer "" true true
        ;;
    clean-start)
        start_postgres
        reset_database
        start_indexer 0
        ;;
    clean-start-bg)
        start_postgres
        reset_database
        start_indexer 0 true
        ;;
    clean-start-eth)
        start_postgres
        reset_database
        start_indexer 0 false true
        ;;
    clean-start-eth-bg)
        start_postgres
        reset_database
        start_indexer 0 true true
        ;;
    start-indexer)
        if ! check_postgres; then
            log_error "PostgreSQL is not running. Run '$0 start' first."
            exit 1
        fi
        start_indexer "${2:-}"
        ;;
    stop)
        stop_indexer
        ;;
    stop-all)
        stop_indexer
        stop_postgres
        ;;
    build)
        build_indexer
        ;;
    reset-db)
        reset_database
        ;;
    status)
        status
        ;;
    logs)
        logs
        ;;
    tail-logs)
        if [ -f "$LOG_FILE" ]; then
            tail -f "$LOG_FILE"
        else
            log_error "Log file not found: $LOG_FILE"
            exit 1
        fi
        ;;
    psql)
        if ! check_postgres; then
            log_error "PostgreSQL is not running"
            exit 1
        fi
        docker exec -it "$POSTGRES_CONTAINER" psql -U postgres -d bridge
        ;;
    help|--help|-h)
        print_help
        ;;
    *)
        log_error "Unknown command: $1"
        echo ""
        print_help
        exit 1
        ;;
esac
