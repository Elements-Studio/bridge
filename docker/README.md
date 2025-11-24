# Bridge Docker Deployment

This directory contains Docker Compose configuration for running a local Ethereum testnet with Bridge contracts pre-deployed.

## Quick Start

```bash
# Make scripts executable
chmod +x scripts/*.sh

# Deploy everything
./scripts/deploy-local.sh

# Get deployment information
./scripts/get-deployment-info.sh
```

## Services

### eth-node
- **Port**: 8545
- **Type**: Anvil (Ethereum local testnet)
- **Chain ID**: 31337
- **Block Time**: 2 seconds

### eth-deployer
- Automatically deploys Bridge contracts on startup
- Saves deployment addresses to shared volume

### deployment-info
- **Port**: 8080
- Nginx server exposing deployment artifacts
- Access at: http://localhost:8080

## Default Accounts

The Anvil instance comes with pre-funded accounts:

**Account #0** (Deployer)
- Address: `0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266`
- Private Key: `0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80`
- Balance: 10000 ETH

**Account #1**
- Address: `0x70997970C51812dc3A010C7d01b50e0d17dc79C8`
- Private Key: `0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d`
- Balance: 10000 ETH

## Contract Addresses

After deployment, contract addresses are available at:
- HTTP: http://localhost:8080/addresses.txt
- Docker volume: `bridge-eth-deployments`

Typical deployment includes:
- BridgeConfig
- SuiBridge (Proxy)
- BridgeLimiter
- BridgeCommittee
- BridgeVault
- Mock Tokens (BTC, ETH, USDC, USDT)

## Commands

### Start Services
```bash
docker-compose up -d
```

### View Logs
```bash
docker-compose logs -f
docker-compose logs eth-node
docker-compose logs eth-deployer
```

### Stop Services
```bash
docker-compose down
```

### Clean Everything
```bash
docker-compose down -v
```

### Interact with Ethereum
```bash
# Get current block
docker exec bridge-eth-node cast block-number --rpc-url http://localhost:8545

# Get account balance
docker exec bridge-eth-node cast balance 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 --rpc-url http://localhost:8545

# Send transaction
docker exec bridge-eth-node cast send <TO_ADDRESS> --value 1ether --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 --rpc-url http://localhost:8545
```

## Integration with Starcoin Bridge

Update your bridge configuration to use the deployed contracts:

```yaml
eth:
  eth-rpc-url: http://localhost:8545
  eth-bridge-proxy-address: "<BRIDGE_PROXY_ADDRESS>"  # Get from deployment info
  eth-bridge-chain-id: 31337
  eth-contracts-start-block-fallback: 0
```

## Troubleshooting

### Container fails to start
```bash
# Check logs
docker-compose logs

# Restart specific service
docker-compose restart eth-node
```

### Deployment fails
```bash
# View deployer logs
docker-compose logs eth-deployer

# Redeploy
docker-compose up eth-deployer --force-recreate
```

### Cannot access RPC
```bash
# Check if eth-node is running
docker ps | grep bridge-eth-node

# Test connection
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
```

### Reset environment
```bash
# Stop all services and remove volumes
docker-compose down -v

# Redeploy from scratch
./scripts/deploy-local.sh
```

## Network Details

- **Network Name**: bridge-network
- **Driver**: bridge
- **Services Communication**: Internal DNS resolution

## Volumes

- `bridge-eth-deployments`: Persistent storage for deployment artifacts

## Health Checks

The eth-node service includes health checks:
- **Test Command**: `cast client --rpc-url http://localhost:8545`
- **Interval**: 5 seconds
- **Timeout**: 3 seconds
- **Retries**: 10

## Notes

- This setup is for **development and testing only**
- Do not use the default private keys in production
- The Anvil instance resets on restart unless volumes are configured
- Contracts are deployed automatically on first startup
