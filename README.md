# Starcoin Bridge

A bidirectional bridge connecting Starcoin and Ethereum blockchains, enabling secure cross-chain asset transfers.

## Overview

The Starcoin Bridge is a decentralized cross-chain bridge that allows users to transfer assets between Starcoin and Ethereum networks. The bridge uses a validator-based architecture with signature aggregation to ensure security and trustworthiness.

### Key Features

- **Bidirectional Transfers**: Transfer assets from ETH to Starcoin and from Starcoin to ETH
- **Multi-Token Support**: BTC, ETH, USDC, USDT and other ERC20 tokens
- **Validator Network**: Byzantine fault-tolerant consensus with signature aggregation
- **Event Monitoring**: Real-time blockchain event watching and processing
- **Automated Execution**: ETH→Starcoin transfers are fully automated
- **Manual Claim**: Starcoin→ETH requires manual claim on Ethereum (security feature)

### Bridge Flow

#### ETH → Starcoin (Fully Automated)
1. User locks tokens on Ethereum bridge contract
2. Bridge nodes detect the lock event
3. Validators sign the bridge action
4. Bridge automatically claims tokens on Starcoin for the user

#### Starcoin → ETH (Approve + Manual Claim)
1. User locks tokens on Starcoin bridge module
2. Bridge nodes detect the lock event
3. Validators sign the bridge action (approve)
4. **User must manually claim tokens on Ethereum** using the aggregated signatures

> **Note**: The manual claim design for Starcoin→ETH matches Sui Bridge behavior and provides an additional security layer, requiring users to actively claim their funds on the destination chain.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Bridge System                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌──────────────┐           ┌──────────────┐                       │
│  │   Ethereum   │           │   Starcoin   │                       │
│  │   Network    │           │   Network    │                       │
│  │              │           │              │                       │
│  │ ┌──────────┐ │           │ ┌──────────┐ │                       │
│  │ │  Bridge  │ │           │ │  Bridge  │ │                       │
│  │ │ Contract │ │           │ │  Module  │ │                       │
│  │ └──────────┘ │           │ └──────────┘ │                       │
│  └──────┬───────┘           └───────┬──────┘                       │
│         │                           │                              │
│         │ Events                    │ Events                       │
│         │                           │                              │
│         └───────────┐   ┌───────────┘                              │
│                     │   │                                          │
│                     ▼   ▼                                          │
│              ┌─────────────────┐                                   │
│              │  Bridge Server  │                                   │
│              │   (Validator)   │                                   │
│              ├─────────────────┤                                   │
│              │ • Event Monitor │                                   │
│              │ • Signature Gen │                                   │
│              │ • Action Exec   │                                   │
│              └─────────────────┘                                   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Components

- **Bridge Server** (`bridge/`): Core bridge node implementation
  - Event monitoring and processing
  - Signature generation and aggregation
  - Action execution on both chains
  
- **Bridge CLI** (`bridge-cli/`): Command-line interface for bridge operations
  - Transfer initiation
  - Status checking
  - Manual claim execution

- **Smart Contracts** (`contracts/`): Ethereum bridge contracts (Solidity)
  - Token locking/unlocking
  - Signature verification
  - Committee management

- **Move Contracts** (`stc-bridge-move/`): Starcoin bridge modules (Move)
  - Token locking/unlocking
  - Bridge event emission
  - Cross-chain message handling

- **Indexer** (`bridge-indexer-alt/`): Blockchain data indexing
  - Fast event querying
  - Historical data storage

## Quick Start

The bridge deployment involves setting up two chains independently:

1. **Ethereum side** (automated): ETH network + Bridge configuration
2. **Starcoin side** (manual): Starcoin dev node + Move contracts

### Prerequisites

- Docker and Docker Compose
- Rust toolchain (cargo)
- Starcoin binary
- Move Package Manager (mpm)

### Step 1: ETH Setup (Automated)

```bash
cd bridge
make setup-eth-and-config
```

This single command:
- Stops existing ETH containers and cleans configs
- Deploys ETH network via Docker (Anvil on port 8545)
- Deploys bridge contracts
- Generates validator/client keys
- Creates `server-config.yaml` and `cli-config.yaml`

### Step 2: Starcoin Setup (Manual)

#### Terminal 1: Start Starcoin Dev Node

**First time deployment:**
```bash
make start-starcoin-dev-node-clean
```

**Resume existing node:**
```bash
make start-starcoin-dev-node
```

> Keep this terminal open - Starcoin runs in foreground

#### Terminal 2: Deploy Move Contracts

```bash
# Build contracts
make build-starcoin-contracts

# Deploy to Starcoin (skip if already deployed in resume mode)
make deploy-starcoin-contracts
```

### Step 3: Start Bridge Server

#### Terminal 3: Run Bridge

```bash
make run-bridge-server
```

The bridge will:
- Connect to ETH RPC (localhost:8545)
- Connect to Starcoin RPC (localhost:9850)
- Start listening on port 9191
- Enable metrics on port 9184

### Step 4: Test Bridge Transfers

```bash
# ETH → Starcoin transfer (fully automated)
./scripts/bridge_transfer.sh eth-to-stc <amount>

# Starcoin → ETH transfer (approve only, manual claim needed)
./scripts/bridge_transfer.sh stc-to-eth <amount>
```

For Starcoin→ETH transfers, after approval, you'll need to manually claim on Ethereum using the bridge CLI or contract interaction.

## Configuration

### Server Configuration (`bridge-config/server-config.yaml`)

The server configuration file is automatically generated by `make init-bridge-config`. Each field has a specific purpose in bridge operations:

#### Network Settings

| Field | Default | Description | Usage |
|-------|---------|-------------|-------|
| `server-listen-port` | 9191 | Bridge JSON-RPC API port | Used by CLI and clients to interact with bridge |
| `metrics-port` | 9184 | Prometheus metrics endpoint | Monitoring systems collect bridge statistics |

#### Authority & Keys

| Field | Description | Usage |
|-------|-------------|-------|
| `bridge-authority-key-path` | Path to validator's ECDSA private key | Signs bridge actions and submits ETH transactions |
| `bridge-client-key-path` (in starcoin section) | Path to Ed25519 private key | Signs and submits Starcoin transactions |

#### Client Mode

| Field | Default | Description | Usage |
|-------|---------|-------------|-------|
| `run-client` | true | Enable active transaction submission | Bridge actively submits approved transactions to both chains |
| `db-path` | `bridge.db` | SQLite database path | Stores processed events, pending actions, bridge state |

#### Governance

| Field | Description | Usage |
|-------|-------------|-------|
| `approved-governance-actions` | Pre-approved action digests | Empty array auto-approves all (testing only). Add specific digests for production |

#### Ethereum Configuration

| Field | Default | Description | Usage |
|-------|---------|-------------|-------|
| `eth-rpc-url` | `http://localhost:8545` | Ethereum RPC endpoint | Event monitoring, transaction submission, state queries |
| `eth-bridge-proxy-address` | Auto-filled | ERC1967 Proxy contract address | All bridge operations (deposits, withdrawals, events) |
| `eth-bridge-chain-id` | 12 | Chain identifier | Values: 1=Mainnet, 5=Goerli, 12=EthCustom (local) |
| `eth-contracts-start-block-fallback` | 0 | Initial scan starting block | Used when no checkpoint exists. 0=from genesis |
| `eth-contracts-start-block-override` | 0 | Force rescan from block N | 0=normal operation, N=force rescan from block N |
| `eth-use-latest-block` | true | Use 'latest' vs 'finalized' | true for Anvil/local, false for mainnet (prevents reorg issues) |

#### Starcoin Configuration

| Field | Default | Description | Usage |
|-------|---------|-------------|-------|
| `starcoin-bridge-rpc-url` | `http://127.0.0.1:9850` | Starcoin RPC endpoint | Event monitoring, transaction submission, state queries |
| `starcoin-bridge-chain-id` | 2 | Chain identifier | Values: 1=Mainnet, 251=Barnard, 254=Dev, 2=Custom |
| `starcoin-bridge-proxy-address` | Auto-filled from Move.toml | Bridge module address | Event filtering, transaction routing |

### CLI Configuration (`bridge-config/cli-config.yaml`)

The CLI configuration is simpler and focuses on network connectivity:

```yaml
# Starcoin network endpoint
starcoin-bridge-rpc-url: http://127.0.0.1:9850

# Ethereum network endpoint  
eth-rpc-url: http://localhost:8545

# Bridge contract addresses (auto-filled during setup)
starcoin-bridge-proxy-address: "0x246b237c16c761e9478783dd83f7004a"
eth-bridge-proxy-address: "0x0B306BF915C4d645ff596e518fAf3F9669b97016"

# Private keys for transaction signing
starcoin-bridge-key-path: /path/to/bridge_client.key
eth-key-path: /path/to/bridge_authority.key
```

### Key Files Generated

The bridge setup creates three critical key files:

| Key File | Type | Purpose | Used By |
|----------|------|---------|----------|
| `bridge_authority.key` | ECDSA (secp256k1) | Validator operations | Bridge server for ETH transactions |
| `bridge_client.key` | ECDSA (secp256k1) | ETH client transactions | CLI for ETH operations |
| `starcoin_client.key` | Ed25519 | Starcoin transactions | Bridge server & CLI for Starcoin ops |

**Security Notes:**
- All keys are generated locally and never transmitted
- Keys are stored unencrypted - protect the `bridge-node/server-config/` directory
- For production, use hardware wallets or key management systems
- The authority key is automatically funded with 100 ETH on local testnet

### Configuration Validation

Before starting the bridge, verify your configuration:

```bash
# Check that all required files exist
ls -la bridge-config/
# Should see: server-config.yaml, cli-config.yaml, deployment.txt, bridge.db/

ls -la bridge-node/server-config/
# Should see: bridge_authority.key, bridge_client.key, starcoin_client.key

# Verify ETH connectivity
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}'

# Verify Starcoin connectivity  
curl -X POST http://127.0.0.1:9850 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"node.info","params":[],"id":1}'

# Check bridge configuration
make bridge-info
```

## Project Structure

```
bridge/
├── bridge/                              # Core bridge implementation
│   └── src/
│       ├── orchestrator.rs              # Event monitoring & action creation
│       ├── action_executor.rs           # Action execution on chains
│       ├── starcoin_bridge_client.rs    # Starcoin client
│       └── eth_bridge_client.rs         # Ethereum client
│
├── bridge-cli/                          # Command-line interface
│   └── src/
│       └── main.rs                      # CLI commands
│
├── contracts/                           # Ethereum Solidity contracts
│   └── src/
│       ├── SuiBridge.sol                # Main bridge contract
│       ├── BridgeCommittee.sol          # Validator committee
│       └── BridgeVault.sol              # Token vault
│
├── stc-bridge-move/                     # Starcoin Move contracts
│   └── sources/
│       └── bridge.move                  # Bridge module
│
├── bridge-config/                       # Generated configurations
│   ├── server-config.yaml               # Bridge server config
│   ├── cli-config.yaml                  # CLI config
│   ├── deployment.txt                   # ETH deployment info
│   └── bridge.db/                       # Bridge database
│
├── scripts/                             # Automation scripts
│   ├── auto-gen-config.sh               # Config generation
│   └── bridge_transfer.sh               # Transfer testing
│
└── docker/                              # Docker configurations
    └── docker-compose.yml               # ETH network setup
```

## Common Commands

### Help & Status
```bash
make help                          # Show all available commands
make status                        # Check deployment status
make bridge-info                   # Display bridge configuration
```

### ETH Network
```bash
make deploy-eth-network            # Start ETH network (Docker)
make stop-eth-network              # Stop ETH containers
make clean-eth-and-config          # Clean all ETH configs
make logs-eth                      # View ETH node logs
```

### Starcoin Network
```bash
make start-starcoin-dev-node-clean # Start fresh Starcoin node
make start-starcoin-dev-node       # Resume Starcoin node
make stop-starcoin-dev-node        # Stop Starcoin node
```

### Bridge Management
```bash
make run-bridge-server             # Start bridge server
make build-starcoin-contracts      # Build Move contracts
make deploy-starcoin-contracts     # Deploy Move contracts
```

## Troubleshooting

### ETH Network Not Starting
```bash
# Check Docker status
docker ps -a | grep bridge

# View logs
make logs-deployer

# Clean restart
make clean-eth-and-config
make setup-eth-and-config
```

### Starcoin Node Issues
```bash
# Check if port is in use
lsof -i :9850

# Stop existing processes
make stop-starcoin-dev-node

# Clean restart
make start-starcoin-dev-node-clean
```

### Bridge Can't Connect
```bash
# Verify ETH is running
curl http://localhost:8545

# Verify Starcoin is running
curl -X POST http://127.0.0.1:9850 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"node.info","params":[],"id":1}'

# Check bridge config
cat bridge-config/server-config.yaml
```

### Move Contract Deployment Failed
```bash
# Verify Starcoin is running
ps aux | grep starcoin | grep dev

# Rebuild and redeploy
make build-starcoin-contracts
make deploy-starcoin-contracts
```

## API Reference

### ETH Deployment Info API
```bash
# Get deployment information
curl http://localhost:8080/deployment.json | jq

# Get specific contract address
curl -s http://localhost:8080/deployment.json | jq '.contracts.SuiBridge'
```

### Bridge RPC (Port 9191)
The bridge server exposes JSON-RPC endpoints for querying bridge status and submitting transactions.

### Metrics (Port 9184)
Prometheus-compatible metrics are available at `http://localhost:9184/metrics`

## Development

### Building from Source

```bash
# Build bridge server
cargo build --release --bin starcoin-bridge

# Build CLI
cargo build --release --bin bridge-cli

# Run tests
cargo test --workspace
```

### Environment Variables

```bash
# Starcoin configuration
STARCOIN_PATH=/path/to/starcoin    # Starcoin binary path
MPM_PATH=/path/to/mpm              # Move Package Manager path
STARCOIN_RPC=http://127.0.0.1:9850 # Starcoin RPC URL

# Bridge configuration
RUST_LOG=info,starcoin_bridge=debug # Logging level
```

## Security Considerations

- **Validator Security**: Keep validator keys secure and never expose them
- **Multi-Sig**: Multiple validators are required for bridge actions
- **Manual Claim**: Starcoin→ETH requires manual claim for additional security
- **Rate Limiting**: Bridge has built-in rate limiting to prevent abuse
- **Audits**: Contracts should be audited before mainnet deployment

## License

This project is licensed under the Apache License 2.0.

## Links

- **GitHub**: [Elements-Studio/stc-bridge-move](https://github.com/Elements-Studio/stc-bridge-move)
- **Starcoin**: [https://starcoin.org](https://starcoin.org)
- **Documentation**: See `QUICKSTART.md` for detailed deployment guide

## Support

For issues and questions:
- Open an issue on GitHub
- Join the Starcoin Discord community
