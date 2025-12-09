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
┌───────────────────────────────────────────────────────────────────────┐
│                          Bridge System                                │
├───────────────────────────────────────────────────────────────────────┤
│                                                                       │
│    ┌──────────────┐                     ┌──────────────┐              │
│    │   Ethereum   │                     │   Starcoin   │              │
│    │   Network    │                     │   Network    │              │
│    │              │                     │              │              │
│    │ ┌──────────┐ │                     │ ┌──────────┐ │              │
│    │ │  Bridge  │ │                     │ │  Bridge  │ │              │
│    │ │ Contract │ │                     │ │  Module  │ │              │
│    │ └──────────┘ │                     │ └──────────┘ │              │
│    └──────┬───────┘                     └───────┬──────┘              │
│           │                                     │                     │
│           │ Events                       Events │                     │
│           │                                     │                     │
│           └─────────────┐     ┌─────────────────┘                     │
│                         │     │                                       │
│                         ▼     ▼                                       │
│                  ┌─────────────────┐                                  │
│                  │  Bridge Server  │                                  │
│                  │   (Validator)   │                                  │
│                  ├─────────────────┤                                  │
│                  │ • Event Monitor │                                  │
│                  │ • Signature Gen │                                  │
│                  │ • Action Exec   │                                  │
│                  └─────────────────┘                                  │
│                                                                       │
└───────────────────────────────────────────────────────────────────────┘
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

### Prerequisites

- Docker and Docker Compose
- Rust toolchain (cargo)
- Starcoin binary (`starcoin`)
- Move Package Manager (`mpm`)

### Required Environment Variables

The following environment variables **must be set** before running any bridge commands:

| Variable | Description | Example |
|----------|-------------|----------|
| `STARCOIN_PATH` | Path to starcoin binary | `/path/to/starcoin` |
| `STARCOIN_DATA_DIR` | Parent directory for dev node data | `/tmp` |
| `MPM_PATH` | Path to Move Package Manager binary | `/path/to/mpm` |
| `MOVE_CONTRACT_DIR` | Path to stc-bridge-move directory | `/path/to/stc-bridge-move` |

Add these to your shell profile (e.g., `~/.zshrc` or `~/.bashrc`):

```bash
export STARCOIN_PATH='/path/to/starcoin'
export STARCOIN_DATA_DIR='/tmp'
export MPM_PATH='/path/to/mpm'
export MOVE_CONTRACT_DIR='/path/to/stc-bridge-move'
```

### One-Click Deployment

With environment variables set, run:

```bash
cd bridge
./setup.sh -y   # -y for auto-confirm, omit for interactive mode
```

This script will:
1. Start Starcoin dev node (background)
2. Deploy ETH network + contracts
3. Deploy Move contracts to Starcoin
4. Start Bridge server (foreground)

### Bridge Transfers

Use the transfer script for cross-chain operations:

```bash
# Usage: bridge_transfer.sh <DIRECTION> <AMOUNT> --token <TOKEN>
# TOKEN options: ETH, USDT, USDC, BTC

# ETH → Starcoin: Transfer 0.5 ETH
./scripts/bridge_transfer.sh eth-to-stc 0.5 --token ETH

# Starcoin → ETH: Transfer 0.1 ETH
./scripts/bridge_transfer.sh stc-to-eth 0.1 --token ETH

# ETH → Starcoin: Transfer 10 USDT
./scripts/bridge_transfer.sh eth-to-stc 10 --token USDT

# Starcoin → ETH: Transfer 10 USDT
./scripts/bridge_transfer.sh stc-to-eth 10 --token USDT
```

> **Note**: The `--token` parameter is **required**. For Starcoin→ETH, the bridge only approves on Starcoin. You need to manually claim on Ethereum.

### Manual Deployment (Step by Step)

If you prefer manual control:

```bash
# Step 1: Deploy ETH
make setup-eth-and-config

# Step 2: Start Starcoin (Terminal 1, keep open)
make start-starcoin-dev-node

# Step 3: Deploy contracts (Terminal 2)
make deploy-starcoin-contracts

# Step 4: Start bridge (Terminal 3)
make run-bridge-server
```

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

## Make Commands Reference

```bash
# ETH network
make setup-eth-and-config      # Deploy ETH network + generate configs

# Starcoin network  
make start-starcoin-dev-node   # Start Starcoin dev node

# Contracts
make deploy-starcoin-contracts # Build and deploy Move contracts

# Bridge
make run-bridge-server         # Start bridge server

# Status
make status                    # Check deployment status
```

## Troubleshooting

```bash
# Check status
make status

# Clean restart ETH
make clean-eth-and-config && make setup-eth-and-config

# Clean restart Starcoin  
make stop-starcoin-dev-node
rm -rf /tmp/dev
make start-starcoin-dev-node

# Verify connectivity
curl http://localhost:8545                    # ETH RPC
curl -X POST http://127.0.0.1:9850 -d '{}'    # Starcoin RPC
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
