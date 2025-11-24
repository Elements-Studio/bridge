# Starcoin Bridge Local Deployment Guide

This guide provides instructions for deploying and testing Starcoin Bridge in a local development environment.

## Reference
- Based on Sui Bridge implementation
- Original Sui Commit: `94dd9c77c02307b4a1053e64f791c34d8e712b62`

## Prerequisites

### Build Starcoin Bridge Tools
```bash
# Build from local source
cargo build --release -p starcoin-bridge-bridge -p starcoin-bridge-bridge-cli
```

### Install Foundry
Follow: https://getfoundry.sh/introduction/installation/

### Create Starcoin Account
```bash
# Use Starcoin client to create account
# Details depend on Starcoin setup
```

## Code Modifications for Local Testing

### 1. Governance Verifier Bypass

Edit `crates/starcoin-bridge-bridge/src/server/governance_verifier.rs`:

```rust
#[async_trait::async_trait]
impl ActionVerifier<BridgeAction> for GovernanceVerifier {
    async fn verify(&self, key: BridgeAction) -> BridgeResult<BridgeAction> {
        if !key.is_governace_action() {
            return Err(BridgeError::ActionIsNotGovernanceAction(key));
        }
        
        // Auto-approve for local testing
        if self.approved_goverance_actions.is_empty() {
            return Ok(key);
        }
        return Ok(key);
    }
}
```

### 2. ETH Client Finalized Block Fix

Edit `crates/starcoin-bridge-bridge/src/eth_client.rs`:

```rust
pub async fn get_last_finalized_block_id(&self) -> BridgeResult<u64> {
    let block: Result<Option<Block<ethers::types::TxHash>>, ethers::prelude::ProviderError> =
        self.provider.request("eth_getBlockByNumber", ("finalized", false)).await;

    if let Ok(Some(block)) = block {
        let number = block.number.ok_or(BridgeError::TransientProviderError(
            "Provider returns block without number".into(),
        ))?;
        let block_num = number.as_u64();
        
        // Anvil returns 0 for finalized, use latest block instead
        if block_num == 0 {
            tracing::warn!("Finalized block is 0 (Anvil). Using latest block.");
            let latest = self.provider.get_block_number().await?;
            return Ok(latest.as_u64());
        }
        Ok(block_num)
    } else {
        let block_number = self.provider.get_block_number().await?;
        Ok(block_number.as_u64())
    }
}
```

## Setup Bridge Validator

### 1. Create Validator Keys

```bash
# Bridge authority key
starcoin-bridge-bridge-cli create-bridge-validator-key ~/.starcoin/bridge_keys/validator_0_bridge_key

# Bridge client key
starcoin-bridge-bridge-cli create-bridge-client-key ~/.starcoin/bridge_keys/bridge_client_key

# Verify key
starcoin-bridge-bridge-cli examine-key ~/.starcoin/bridge_keys/validator_0_bridge_key

# Fund validator address
# Request test tokens for validator address on Starcoin
```

### 2. Convert Key to ETH Format

```bash
# Extract ETH private key
export BRIDGE_KEY_FILE=~/.starcoin/bridge_keys/validator_0_bridge_key
cat "$BRIDGE_KEY_FILE" | base64 -d | xxd -p -c 32

# Import to Anvil
cast wallet address --private-key 0x<private_key>

# Set balance (100 ETH)
cast rpc anvil_setBalance <eth_address> 0x16345785D8A0000 --rpc-url http://127.0.0.1:8545
```

## Start Local Networks

### Start Sui Node

```bash
# Generate genesis with 60s epoch
sui genesis --epoch-duration-ms=60000 --force

# Start node with faucet
RUST_LOG="off,starcoin_bridge_node=debug" sui start --with-faucet
```

### Start Anvil

```bash
anvil
```

## Deploy Ethereum Contracts

### Configure Environment

Create `bridge/evm/.env`:

```env
PRIVATE_KEY=0x<validator_eth_private_key>
```

### Deploy Contracts

```bash
cd bridge/evm

# Update remappings.txt if needed
forge script script/deploy_bridge.s.sol --fork-url anvil --broadcast
```

Record deployed contract addresses, especially `StarcoinBridge` proxy address.

## Register Bridge Committee

### 1. Create Node Config

```bash
starcoin-bridge-bridge-cli create-bridge-node-config-template --run-client bridge_node_config.yaml
```

### 2. Edit Configuration

Edit `bridge_node_config.yaml`:

```yaml
server-listen-port: 9191
metrics-port: 9184
bridge-authority-key-path: /path/to/validator_0_bridge_key
run-client: true
db-path: ./client_db
approved-governance-actions: []

starcoin:
  starcoin-bridge-rpc-url: http://127.0.0.1:9000
  starcoin-bridge-bridge-chain-id: 2
  bridge-client-key-path: /path/to/bridge_client_key

eth:
  eth-rpc-url: http://127.0.0.1:8545
  eth-bridge-proxy-address: "0x<StarcoinBridge_proxy_address>"
  eth-bridge-chain-id: 11
  eth-contracts-start-block-fallback: 0
```

### 3. Register Committee

Create `bridge_committee.yaml`:

```yaml
bridge-authority-port-and-key-path:
  - [9191, "/path/to/validator_0_bridge_key"]
```

Register:

```bash
# Register bridge committee on Starcoin
# Use starcoin-bridge-bridge-cli to register committee
starcoin-bridge-bridge-cli view-bridge-registration --starcoin-bridge-rpc-url http://127.0.0.1:9000
```

Wait for epoch transition, then verify:

```bash
starcoin-bridge-bridge-cli view-starcoin-bridge-bridge --starcoin-bridge-rpc-url http://127.0.0.1:9000
```

## Start Bridge Node

```bash
# Clean database (if needed)
rm -rf ./client_db

# Start node
RUST_LOG=debug,starcoin_bridge_bridge=debug ./target/release/starcoin-bridge-bridge --config-path bridge_node_config.yaml
```

## Register Assets on Starcoin

### 1. Deploy Token Contracts

```bash
# Deploy token contracts on Starcoin
# Use Starcoin Move CLI to publish modules
```

### 2. Register to Treasury

```bash
# Register tokens using Starcoin bridge CLI
```

### 3. Activate via Governance

Create `bridge_cli_config.yaml`:

```yaml
starcoin-bridge-rpc-url: "http://127.0.0.1:9000"
eth-rpc-url: "http://127.0.0.1:8545"
eth-bridge-proxy-address: "0x<StarcoinBridge_address>"
starcoin-bridge-key-path: "/path/to/validator_0_bridge_key"
eth-key-path: "/path/to/eth_key"
```

Add tokens:

```bash
# ETH (token_id=2)
starcoin-bridge-bridge-cli governance \
  --config-path bridge_cli_config.yaml \
  --chain-id 2 \
  add-tokens-on-starcoin \
  --nonce 0 \
  --token-ids 2 \
  --token-type-names "<package>::eth::ETH" \
  --token-prices 259696000000
```

## Cross-Chain Transfers

### ETH to Starcoin

```bash
starcoin-bridge-bridge-cli client \
  --config-path bridge_cli_config.yaml \
  deposit-native-ether-on-eth \
  --ether-amount 5 \
  --target-chain 2 \
  --starcoin-bridge-recipient-address <starcoin_bridge_address>

# Verify balance on Starcoin
# Use Starcoin client to check balance
```

### Starcoin to ETH

```bash
# Find coin object on Starcoin
# Use Starcoin client to query objects

# Transfer
starcoin-bridge-bridge-cli client \
  --config-path bridge_cli_config.yaml \
  deposit-on-starcoin \
  --coin-object-id <coin_object_id> \
  --coin-type "<package>::eth::ETH" \
  --target-chain 12 \
  --recipient-address <eth_address>

# Verify ETH balance
cast balance <eth_address> --rpc-url http://127.0.0.1:8545
```

## Verification

### Check Node Status

```bash
curl -v http://127.0.0.1:9191
```

### Monitor Metrics

```bash
curl http://127.0.0.1:9184/metrics
```

Key metrics:
- `bridge_last_synced_starcoin_bridge_checkpoints`
- `bridge_last_synced_eth_blocks`
- `bridge_starcoin_bridge_watcher_received_events`
- `bridge_eth_watcher_received_events`
- `bridge_gas_coin_balance`

## Key Conversion Script

Create `bridge_convert_key.py`:

```python
#!/usr/bin/env python3
import base64
import sys
import argparse

def anvil_to_starcoin_bridge_key(hex_privkey: str) -> str:
    if hex_privkey.startswith('0x'):
        hex_privkey = hex_privkey[2:]
    
    privkey_bytes = bytes.fromhex(hex_privkey)
    if len(privkey_bytes) != 32:
        raise ValueError(f"Invalid key length: {len(privkey_bytes)}")
    
    flag = bytes([0x01])  # Secp256k1
    combined = flag + privkey_bytes
    return base64.b64encode(combined).decode('ascii')

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('private_key', type=str)
    parser.add_argument('-o', '--output', type=str)
    args = parser.parse_args()
    
    starcoin_bridge_key = anvil_to_starcoin_bridge_key(args.private_key)
    
    if args.output:
        with open(args.output, 'w') as f:
            f.write(starcoin_bridge_key)
        print(f"Starcoin key written to: {args.output}")
    else:
        print(starcoin_bridge_key)

if __name__ == "__main__":
    main()
```

Usage:

```bash
python bridge_convert_key.py 0x<eth_private_key> -o ~/.starcoin/bridge_keys/eth_key
starcoin-bridge-bridge-cli examine-key ~/.starcoin/bridge_keys/eth_key
```

## Notes

- This guide is for local testing only
- Production deployment requires additional security configurations
- Ensure sufficient STC balance for gas fees
- Monitor bridge node logs for debugging

## Troubleshooting

**Q: Why must I enable Bridge Client?**  
A: Bridge Client monitors chain events and submits transactions. Without it, the node only provides signature services.

**Q: Gas balance insufficient?**  
A: Monitor `bridge_gas_coin_balance` metric and maintain sufficient STC balance.

**Q: What's the difference between Bridge Authority Key and Bridge Client Key?**  
A:
- **Bridge Authority Key**: Secp256k1 key for signing bridge actions, must be committee member
- **Bridge Client Key**: Any StarcoinKeyPair type for submitting Starcoin transactions, needs STC for gas
