#!/bin/bash
# ============================================================
# Starcoin Bridge - One-Click Setup Script
# ============================================================
# Usage: ./setup.sh [-y]
#   -y    Skip confirmation prompts (auto-confirm deletions)
#
# Steps:
#   1. Kill existing starcoin processes
#   2. Start Starcoin dev node (background)
#   3. Setup ETH network + config (auto-confirm)
#   4. Deploy Move contracts
#   5. Start Bridge server (foreground, blocking)
# ============================================================
#
# ⚠️  ATTENTION: Set these environment variables before running!
# ============================================================
#   export STARCOIN_PATH=/path/to/starcoin          # Path to starcoin binary
#   export STARCOIN_DATA_DIR=/path/to/data          # Data directory (e.g., /tmp)
#   export MPM_PATH=/path/to/mpm                    # Path to mpm (Move Package Manager)
#
# Example:
#   export STARCOIN_PATH=~/starcoin-vm1/target/debug/starcoin
#   export STARCOIN_DATA_DIR=/tmp
#   export MPM_PATH=~/starcoin-vm1/target/debug/mpm
#   ./setup.sh        # Interactive mode (asks for confirmation)
#   ./setup.sh -y     # Auto-confirm mode (no prompts)
# ============================================================

set -e

# Parse arguments
FORCE_YES=0
while getopts "y" opt; do
    case $opt in
        y) FORCE_YES=1 ;;
        *) echo "Usage: $0 [-y]"; exit 1 ;;
    esac
done

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

# Safe delete function
safe_rm() {
    local target="$1"
    local description="${2:-$1}"
    
    if [ ! -e "$target" ] && [ ! -L "$target" ]; then
        echo -e "  ${YELLOW}⚠ $description ($target) does not exist, skipping${NC}"
        return 0
    fi
    
    if [ "$FORCE_YES" = "1" ]; then
        echo -e "  ${YELLOW}Auto-deleting:${NC} ${RED}$description${NC} ($target)"
        rm -rf "$target"
        echo -e "  ${GREEN}✓ Deleted${NC}"
    else
        echo -e "  ${YELLOW}⚠ About to delete:${NC} ${RED}$description${NC}"
        echo -e "    ${RED}$target${NC}"
        printf "  ${YELLOW}Continue? (y/N): ${NC}"
        read -r REPLY
        if [ "$REPLY" = "y" ] || [ "$REPLY" = "Y" ]; then
            rm -rf "$target"
            echo -e "  ${GREEN}✓ Deleted${NC}"
        else
            echo -e "  ${YELLOW}✗ Skipped${NC}"
        fi
    fi
}

# Safe kill function
safe_kill() {
    local pattern="$1"
    local description="${2:-processes matching $1}"
    
    if ! pgrep -f "$pattern" > /dev/null 2>&1; then
        echo -e "  ${YELLOW}⚠ No $description running, skipping${NC}"
        return 0
    fi
    
    if [ "$FORCE_YES" = "1" ]; then
        echo -e "  ${YELLOW}Auto-killing:${NC} ${RED}$description${NC}"
        pkill -9 -f "$pattern" 2>/dev/null || true
        echo -e "  ${GREEN}✓ Killed${NC}"
    else
        echo -e "  ${YELLOW}⚠ About to kill:${NC} ${RED}$description${NC}"
        printf "  ${YELLOW}Continue? (y/N): ${NC}"
        read -r REPLY
        if [ "$REPLY" = "y" ] || [ "$REPLY" = "Y" ]; then
            pkill -9 -f "$pattern" 2>/dev/null || true
            echo -e "  ${GREEN}✓ Killed${NC}"
        else
            echo -e "  ${YELLOW}✗ Skipped${NC}"
        fi
    fi
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# ============================================================
# Check required environment variables
# ============================================================
MISSING_VARS=0

if [ -z "$STARCOIN_PATH" ]; then
    echo -e "${RED}✗ STARCOIN_PATH is not set${NC}"
    echo -e "  ${YELLOW}export STARCOIN_PATH=/path/to/starcoin${NC}"
    MISSING_VARS=1
fi

if [ -z "$STARCOIN_DATA_DIR" ]; then
    echo -e "${RED}✗ STARCOIN_DATA_DIR is not set${NC}"
    echo -e "  ${YELLOW}export STARCOIN_DATA_DIR=/tmp${NC}"
    MISSING_VARS=1
fi

if [ -z "$MPM_PATH" ]; then
    echo -e "${RED}✗ MPM_PATH is not set${NC}"
    echo -e "  ${YELLOW}export MPM_PATH=/path/to/mpm${NC}"
    MISSING_VARS=1
fi

if [ $MISSING_VARS -eq 1 ]; then
    echo ""
    echo -e "${RED}Please set the required environment variables and try again.${NC}"
    exit 1
fi

# Validate paths exist and convert to absolute paths
if [ ! -x "$STARCOIN_PATH" ]; then
    echo -e "${RED}✗ STARCOIN_PATH does not exist or is not executable: $STARCOIN_PATH${NC}"
    exit 1
fi
STARCOIN_PATH="$(cd "$(dirname "$STARCOIN_PATH")" && pwd)/$(basename "$STARCOIN_PATH")"

if [ ! -x "$MPM_PATH" ]; then
    echo -e "${RED}✗ MPM_PATH does not exist or is not executable: $MPM_PATH${NC}"
    exit 1
fi
MPM_PATH="$(cd "$(dirname "$MPM_PATH")" && pwd)/$(basename "$MPM_PATH")"

# Convert STARCOIN_DATA_DIR to absolute path (strip trailing whitespace/newlines)
STARCOIN_DATA_DIR="${STARCOIN_DATA_DIR%/}"  # Remove trailing slash if any
STARCOIN_DATA_DIR="$(echo "$STARCOIN_DATA_DIR" | tr -d '\n\r')"  # Remove newlines
if [ ! -d "$STARCOIN_DATA_DIR" ]; then
    mkdir -p "$STARCOIN_DATA_DIR"
fi
STARCOIN_DATA_DIR="$(cd "$STARCOIN_DATA_DIR" && pwd)"

# Export for make commands
export STARCOIN_PATH
export MPM_PATH
export STARCOIN_DATA_DIR

# Derived paths
STARCOIN_DEV_DIR="$STARCOIN_DATA_DIR/dev"
STARCOIN_RPC="$STARCOIN_DEV_DIR/starcoin.ipc"
STARCOIN_LOG="$STARCOIN_DATA_DIR/starcoin-dev.log"

echo -e "${YELLOW}╔════════════════════════════════════════╗${NC}"
echo -e "${YELLOW}║  Starcoin Bridge - One-Click Setup     ║${NC}"
echo -e "${YELLOW}╚════════════════════════════════════════╝${NC}"
echo ""

# ============================================================
# Step 0: Cleanup
# ============================================================
echo -e "${YELLOW}Step 0: Cleanup...${NC}"

# Kill existing starcoin processes
safe_kill "starcoin" "starcoin processes"
sleep 1

# Remove stale nohup.out files
safe_rm "nohup.out" "stale nohup.out"
safe_rm "$STARCOIN_LOG" "starcoin log file"

# Clean starcoin dev data for fresh start
safe_rm "$STARCOIN_DEV_DIR" "starcoin dev data ($STARCOIN_DEV_DIR)"

echo -e "${GREEN}✓ Cleanup complete${NC}"
echo ""

# ============================================================
# Step 1: Start Starcoin Dev Node (Background)
# ============================================================
echo -e "${YELLOW}Step 1: Starting Starcoin dev node (background)...${NC}"

nohup "$STARCOIN_PATH" -n dev -d "$STARCOIN_DATA_DIR" > "$STARCOIN_LOG" 2>&1 &
STARCOIN_PID=$!
echo -e "  PID: $STARCOIN_PID"
echo -e "  Log: $STARCOIN_LOG"

# Wait for node to be ready
echo -e "  Waiting for node to be ready..."
for i in $(seq 1 60); do
    if "$STARCOIN_PATH" -c "$STARCOIN_RPC" chain info >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Starcoin dev node is ready (took ${i}s)${NC}"
        break
    fi
    if ! kill -0 $STARCOIN_PID 2>/dev/null; then
        echo -e "${RED}✗ Starcoin process died unexpectedly${NC}"
        echo -e "${YELLOW}Check log: tail -50 $STARCOIN_LOG${NC}"
        exit 1
    fi
    sleep 1
done

# Verify node is running
if ! "$STARCOIN_PATH" -c "$STARCOIN_RPC" chain info >/dev/null 2>&1; then
    echo -e "${RED}✗ Starcoin node failed to start within 60s${NC}"
    echo -e "${YELLOW}Check log: tail -50 $STARCOIN_LOG${NC}"
    exit 1
fi
echo ""

# ============================================================
# Step 2: Setup ETH Network + Config (auto-confirm)
# ============================================================
echo -e "${YELLOW}Step 2: Setting up ETH network and config...${NC}"

FORCE_YES=$FORCE_YES make setup-eth-and-config

echo -e "${GREEN}✓ ETH setup complete${NC}"
echo ""

# ============================================================
# Step 3: Deploy Move Contracts
# ============================================================
echo -e "${YELLOW}Step 3: Deploying Move contracts...${NC}"

make deploy-starcoin-contracts

echo -e "${GREEN}✓ Move contracts deployed${NC}"
echo ""

# ============================================================
# Step 4: Start Bridge Server (Foreground - Blocking)
# ============================================================
echo -e "${YELLOW}Step 4: Starting Bridge server (foreground)...${NC}"
echo -e "${BLUE}Press Ctrl+C to stop the bridge server${NC}"
echo ""

# This will block until Ctrl+C
make run-bridge-server
