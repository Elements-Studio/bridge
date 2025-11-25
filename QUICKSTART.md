# Starcoin Bridge Quick Deployment Guide

## Quick Start

The bridge deployment involves two separate chains that need to be set up independently:

1. **Ethereum side** (automated): ETH network + Bridge configuration
2. **Starcoin side** (manual): Starcoin dev node + Move contracts

## Part 1: Ethereum Setup (Automated)

### One-Command ETH Deployment

```bash
# Complete cleanup and reinitialization for ETH side only
make setup-eth-and-config
```

**What `make setup-eth-and-config` does:**
- ✅ `make clean-eth-and-config` - Stops Docker containers, removes bridge-config/ and ~/.sui/bridge_keys/
- ✅ `make deploy-eth-network` - Starts ETH network (Anvil on :8545, deploys contracts, serves info on :8080)
- ✅ `make init-bridge-config` - Generates validator/client keys, creates server-config.yaml and client-config.yaml
- ❌ Does NOT start or deploy anything for Starcoin

### Or Execute Step by Step:

```bash
make clean-eth-and-config    # Stop all ETH containers, remove bridge-config/ and ~/.sui/bridge_keys/
make deploy-eth-network      # Start Docker Compose (eth-node, eth-deployer, deployment-info containers)
make init-bridge-config      # Run scripts/auto-gen-config.sh to generate keys and configs
```

## Part 2: Starcoin Setup (Manual)

### Step 1: Start Starcoin Dev Node

**Two startup modes available:**

#### Option A: Start from scratch (clean mode)
```bash
# In a NEW terminal window (Terminal 1), start fresh Starcoin dev node
make start-starcoin-dev-node-clean
```

**What `make start-starcoin-dev-node-clean` does:**
- ✅ Removes `~/.starcoin/dev` directory completely (fresh start)
- ✅ Runs `$(STARCOIN_PATH) -n dev console`
- ✅ Starts Starcoin RPC on `ws://127.0.0.1:9870`
- ⚠️  Runs in foreground - keep this terminal open!
- ⚠️  **Use this for first-time deployment or when you need a clean state**

#### Option B: Start with existing data (resume mode)
```bash
# In a NEW terminal window (Terminal 1), resume Starcoin dev node
make start-starcoin-dev-node
```

**What `make start-starcoin-dev-node` does:**
- ✅ Keeps existing `~/.starcoin/dev` directory (preserves blockchain state & deployed contracts)
- ✅ Runs `$(STARCOIN_PATH) -n dev console`
- ✅ Starts Starcoin RPC on `ws://127.0.0.1:9870`
- ⚠️  Runs in foreground - keep this terminal open!
- ⚠️  **Use this when contracts are already deployed and you just need to restart the node**

**Environment Variable:**
```bash
# If starcoin is not in PATH, specify the path:
STARCOIN_PATH=/path/to/starcoin make start-starcoin-dev-node
STARCOIN_PATH=/path/to/starcoin make start-starcoin-dev-node-clean
```

### Step 2: Build Move Contracts

```bash
# In ANOTHER terminal (Terminal 2), build the bridge Move contracts
make build-starcoin-contracts
```

**What `make build-starcoin-contracts` does:**
- ✅ Changes to `../stc-bridge-move` directory
- ✅ Runs `$(MPM_PATH) release` (uses MPM_PATH environment variable)
- ✅ Generates `../stc-bridge-move/release/Stc-Bridge-Move.v0.0.1.blob`

**Environment Variable:**
```bash
# If mpm is not in PATH, specify the path:
MPM_PATH=/path/to/mpm make build-starcoin-contracts
```

### Step 3: Deploy Move Contracts to Starcoin

```bash
# In the same terminal (Terminal 2), deploy the compiled Move contracts
make deploy-starcoin-contracts
```

**What `make deploy-starcoin-contracts` does:**
- ✅ Calls `make build-starcoin-contracts` first (ensures latest build)
- ✅ Checks if Starcoin node process is running
- ✅ Runs `$(STARCOIN_PATH) -c $(STARCOIN_RPC) --local-account-dir $(STARCOIN_ACCOUNT_DIR) dev deploy <blob> -b`
- ✅ Deploys bridge module to address `0x246b237c16c761e9478783dd83f7004a`

**Environment Variables:**
```bash
# Customize deployment settings if needed:
STARCOIN_PATH=/path/to/starcoin \
MPM_PATH=/path/to/mpm \
STARCOIN_RPC=ws://127.0.0.1:9870 \
MOVE_CONTRACT_DIR=../stc-bridge-move \
make deploy-starcoin-contracts
```

## Part 3: Start Bridge Server

### Prerequisites Check

Before starting the bridge, ensure:
- ✅ ETH network is running (check with `make status`)
- ✅ Bridge config exists (`bridge-config/server-config.yaml`)
- ✅ Starcoin node is running
- ✅ Move contracts are deployed

### Start the Bridge

```bash
# In another terminal (Terminal 3), start bridge server
make run-bridge-server
```

**What `make run-bridge-server` does:**
- ✅ Checks if `bridge-config/server-config.yaml` exists
- ✅ Checks if ETH node container is running
- ✅ Builds `starcoin-bridge` binary if needed (cargo build --bin starcoin-bridge)
- ✅ Runs `RUST_LOG=info,starcoin_bridge=debug ./target/debug/starcoin-bridge --config-path bridge/bridge-config/server-config.yaml`
- ✅ Connects to ETH RPC at `http://localhost:8545`
- ✅ Connects to Starcoin RPC at `ws://127.0.0.1:9870`
- ✅ Starts bridge server on port `9191`
- ✅ Enables metrics endpoint on port `9184`

## Complete Deployment Flow

```bash
# Terminal 1: ETH + Config setup
cd bridge
make setup-eth-and-config
# What it does:
#   - make clean-eth-and-config:   Remove containers & configs
#   - make deploy-eth-network:     Docker Compose up (eth-node, eth-deployer, deployment-info)
#   - make init-bridge-config:     Generate keys & server-config.yaml
# Wait for "ETH setup complete!" message...

# Terminal 2: Start Starcoin (foreground process)
# First time: use clean mode
make start-starcoin-dev-node-clean
# What it does:
#   - rm -rf ~/.starcoin/dev
#   - $(STARCOIN_PATH) -n dev console
# Keep this terminal open!
#
# Next time: use resume mode (keeps contracts deployed)
# make start-starcoin-dev-node

# Terminal 3: Deploy Starcoin contracts
make deploy-starcoin-contracts
# What it does:
#   - make build-starcoin-contracts:  $(MPM_PATH) release
#   - $(STARCOIN_PATH) dev deploy <blob> -b
# Wait for deployment confirmation...

# Terminal 4: Start Bridge
make run-bridge-server
# What it does:
#   - cargo build --bin starcoin-bridge
#   - ./target/debug/starcoin-bridge --config-path bridge/bridge-config/server-config.yaml
# Bridge will connect to both chains and start syncing...
```

## Deployment Information

### View Status

```bash
# View overall deployment status
make status

# View bridge configuration summary
make bridge-info

# View environment variables
cat bridge-config/.env
```

## Common Commands

### Help & Information
```bash
make help                     # Show all available make targets and environment variables
make status                   # View deployment status (ETH containers, config files, keys)
make bridge-info              # Display bridge configuration summary from SETUP_SUMMARY.txt
```

### ETH Network Management
```bash
make deploy-eth-network       # Deploy ETH network using Docker Compose (Anvil + contracts)
make stop-eth-network         # Stop all ETH Docker containers
make clean-eth-and-config     # Stop containers and remove all bridge config files
make setup-eth-and-config     # Complete ETH setup (clean + deploy + generate configs)
```

### Bridge Configuration
```bash
make init-bridge-config       # Generate bridge keys and config files (requires ETH running)
```

### Starcoin Node Management
```bash
make start-starcoin-dev-node-clean # Start Starcoin dev node from scratch (removes ~/.starcoin/dev)
make start-starcoin-dev-node       # Start Starcoin dev node with existing data (keeps ~/.starcoin/dev)
make stop-starcoin-dev-node        # Stop Starcoin dev node processes
```

### Starcoin Contract Deployment
```bash
make build-starcoin-contracts  # Build Move contracts using mpm (generates release/*.blob)
make deploy-starcoin-contracts # Deploy Move contracts to Starcoin (builds + deploys)
```

### Bridge Server
```bash
make run-bridge-server        # Start bridge server (requires ETH + Starcoin + configs ready)
```

### Logs & Debugging
```bash
make logs-eth                 # View ETH node container logs
make logs-deployer            # View ETH contract deployer logs
docker ps                     # Check running containers
ps aux | grep starcoin        # Check Starcoin processes
```

## Configuration Details

### Environment Variables (.env)

Generated automatically by `make init-bridge`:

```bash
# Validator information
VALIDATOR_ETH_ADDRESS         # Validator ETH address
VALIDATOR_STARCOIN_ADDRESS    # Validator Starcoin address
VALIDATOR_PUBKEY              # Validator public key
VALIDATOR_ETH_PRIVKEY         # Validator ETH private key

# Client information
CLIENT_STARCOIN_ADDRESS       # Client Starcoin address

# ETH contracts
ETH_RPC_URL                   # http://localhost:8545
ETH_CHAIN_ID                  # 31337
STARCOIN_BRIDGE_ADDRESS       # StarcoinBridge proxy contract
BRIDGE_COMMITTEE_ADDRESS      # Committee contract
BRIDGE_VAULT_ADDRESS          # Vault contract
WETH_ADDRESS                  # WETH token
```

### Starcoin Configuration (Makefile Variables)

Can be overridden via environment variables:

```bash
STARCOIN_PATH         # Path to starcoin binary (default: starcoin)
MPM_PATH              # Path to mpm binary (default: mpm)
STARCOIN_RPC          # Starcoin RPC URL (default: ws://127.0.0.1:9870)
STARCOIN_DEV_DIR      # Dev data directory (default: ~/.starcoin/dev)
MOVE_CONTRACT_DIR     # Move contracts location (default: ../stc-bridge-move)
BRIDGE_ADDRESS        # Bridge contract address (default: 0x246b237c16c761e9478783dd83f7004a)
```

### Bridge Server Configuration (server-config.yaml)
- Listen port: 9191
- Metrics port: 9184
- Connect to local ETH (localhost:8545) and Starcoin (localhost:9000)
- Auto-approve governance operations (local test mode)

### Bridge Client Configuration (client-config.yaml)
- Connect to local ETH and Starcoin networks
- Submit transactions using client key

## Troubleshooting

### ETH Deployment Failed
```bash
# Check container status
docker ps -a | grep bridge

# View deployment logs
make logs-deployer

# Redeploy ETH side only
make clean-eth-and-config && make setup-eth-and-config
```

### Bridge Initialization Failed
```bash
# Ensure ETH network is running
curl http://localhost:8080/deployment.json

# Reinitialize (keeps ETH network running)
rm -rf bridge-config ~/.starcoin/bridge_keys
make init-bridge-config
```

### Starcoin Node Failed to Start
```bash
# Check if port is already in use
lsof -i :9870   # Starcoin RPC port

# Stop existing Starcoin processes
make stop-starcoin-dev-node

# Option 1: Clean restart (fresh state)
make start-starcoin-dev-node-clean

# Option 2: Resume with existing data
make start-starcoin-dev-node
```

### Move Contract Deployment Failed
```bash
# Check if Starcoin node is running
ps aux | grep starcoin | grep dev

# Verify RPC connection
curl -X POST http://127.0.0.1:9870 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"node.info","params":[],"id":1}'

# Rebuild and redeploy
make build-starcoin-contracts
make deploy-starcoin-contracts
```

### Port Conflicts
```bash
# Check port usage
lsof -i :8545   # ETH RPC
lsof -i :8080   # Deployment info
lsof -i :9870   # Starcoin RPC
lsof -i :9191   # Bridge server

# Stop all services
make stop-eth-network        # Stops ETH
make stop-starcoin-dev-node  # Stops Starcoin
```

### Bridge Server Can't Connect
```bash
# Verify both chains are running
docker ps | grep eth-node        # ETH should be running
ps aux | grep starcoin | grep dev # Starcoin should be running

# Check RPC endpoints
curl http://localhost:8545
curl http://127.0.0.1:9870

# Verify Move contracts are deployed
# (Check Starcoin console output for deployment confirmation)
```

## API Access

### ETH Deployment Information API
```bash
# Complete deployment information
curl http://localhost:8080/deployment.json | jq

# Extract specific information
curl -s http://localhost:8080/deployment.json | jq '{
  chainId: .network.chainId,
  contracts: .contracts | keys,
  starcoinBridge: .contracts.StarcoinBridge
}'
```

### ETH RPC
```bash
# Get ChainID
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}'

# Get block height
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
```

## Architecture Overview

```
┌──────────────────────────────────────────────────────────────────┐
│                    Docker Compose (ETH Side)                     │
├──────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────────────┐   │
│  │  eth-node   │  │ eth-deployer │  │  deployment-info     │   │
│  │   (Anvil)   │─>│   (Foundry)  │─>│     (Nginx)          │   │
│  │  :8545      │  │              │  │     :8080            │   │
│  └─────────────┘  └──────────────┘  └──────────────────────┘   │
└──────────────────────────────────────────────────────────────────┘
         │                                      │
         │ RPC                                  │ HTTP API
         │                                      │
         │                                      ▼
         │                              ┌──────────────────┐
         │                              │  init-bridge.sh  │
         │                              │  (auto-config)   │
         │                              └──────────────────┘
         │                                      │
         │                                      │ Generate
         │                                      ▼
         │                              ┌──────────────────┐
         │                              │  Configuration   │
         │                              │  • Keys          │
         │                              │  • Configs       │
         │                              │  • .env          │
         │                              └──────────────────┘
         │                                      │
         │                                      │ Config
         ▼                                      ▼
┌────────────────────────────────────────────────────────────┐
│                    Bridge Server                           │
│                 (starcoin-bridge)                          │
│                     :9191                                  │
│                     :9184 (metrics)                        │
└────────────────────────────────────────────────────────────┘
         │
         │ RPC
         ▼
┌──────────────────────────────────────────────────────────────┐
│              Starcoin Dev Node (Manual Start)                │
├──────────────────────────────────────────────────────────────┤
│  ┌────────────────┐                  ┌──────────────────┐   │
│  │  starcoin      │                  │  Bridge Module   │   │
│  │  console       │<─────────────────│  (Move)          │   │
│  │  :9870 (RPC)   │   mpm deploy     │  0x246b...4a     │   │
│  └────────────────┘                  └──────────────────┘   │
└──────────────────────────────────────────────────────────────┘
```

## Deployment Checklist

### Phase 1: ETH Setup (Automated)
- [ ] Run `make setup-eth-and-config`
- [ ] Verify ETH network: `docker ps | grep eth-node`
- [ ] Verify deployment API: `curl http://localhost:8080/deployment.json`
- [ ] Check config files: `ls bridge-config/`
- [ ] Check keys: `ls ~/.starcoin/bridge_keys/`

### Phase 2: Starcoin Setup (Manual)
- [ ] Terminal 1: Run `make start-starcoin-dev-node-clean` (first time) or `make start-starcoin-dev-node` (resume)
- [ ] Terminal 2: Run `make build-starcoin-contracts`
- [ ] Terminal 2: Run `make deploy-starcoin-contracts` (skip if already deployed in resume mode)
- [ ] Verify deployment in Starcoin console output

### Phase 3: Bridge Server (Manual)
- [ ] Terminal 3: Run `make run-bridge-server`
- [ ] Verify bridge connects to both chains
- [ ] Check logs for "Bridge server started"

### Phase 4: Testing (Future)
- [ ] Register bridge committee
- [ ] Test cross-chain transfers
- [ ] Monitor bridge operations

## Quick Reference

### Key Commands
```bash
# ETH side (automated - does clean + deploy + init)
make setup-eth-and-config         # Full ETH setup: stops containers, deploys ETH, generates keys/configs

# Starcoin side (manual - requires separate terminals)
make start-starcoin-dev-node-clean # Start fresh: rm -rf ~/.starcoin/dev && $(STARCOIN_PATH) -n dev console
make start-starcoin-dev-node       # Resume: $(STARCOIN_PATH) -n dev console (keeps existing data)
make deploy-starcoin-contracts     # Deploy contracts: $(MPM_PATH) release && $(STARCOIN_PATH) dev deploy <blob> -b

# Bridge server (manual - requires ETH + Starcoin running)
make run-bridge-server            # Start bridge: cargo build && ./target/debug/starcoin-bridge --config-path ...

# Status checks
make status                       # Check: Docker containers, config files, keys
make bridge-info                  # Display: SETUP_SUMMARY.txt contents
```

### Key Ports
```
8545  - ETH RPC (Anvil)
8080  - ETH Deployment Info (Nginx)
9870  - Starcoin RPC (WebSocket)
9191  - Bridge Server
9184  - Bridge Metrics
```

### Key Directories
```
bridge-config/              - Bridge configuration files
~/.starcoin/bridge_keys/    - Validator and client keys
~/.starcoin/dev/            - Starcoin dev node data
../stc-bridge-move/         - Move contracts source
../stc-bridge-move/release/ - Compiled Move blobs
```
