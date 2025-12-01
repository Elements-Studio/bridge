# End-to-End (E2E) Tests

This directory contains E2E tests for the Starcoin Bridge.

## Test Structure

### `local_env_tests.rs` - Local Environment Tests

Tests that run against a **real local environment** (Anvil + Starcoin dev node). These tests verify the full bridge infrastructure is correctly deployed and operational.

#### Prerequisites

1. **Start the local environment:**
   ```bash
   ./setup.sh -y --without-bridge-server
   # or use alias:
   bsw
   ```

2. **Verify services are running:**
   - Anvil (Ethereum): http://127.0.0.1:8545
   - Starcoin dev node: http://127.0.0.1:9850
   - Contracts deployed on both chains

#### Running the Tests

```bash
# Run all local environment E2E tests
cargo test --package starcoin-bridge --lib e2e_tests::local_env_tests -- --nocapture

# Run a specific test
cargo test --package starcoin-bridge --lib e2e_tests::local_env_tests::test_local_env_eth_connection -- --nocapture
```

#### Test Coverage

1. **`test_local_env_eth_connection`**
   - ✅ Connects to Anvil (Ethereum local node)
   - ✅ Verifies ETH chain ID
   - ✅ Gets current block number
   - ✅ Verifies bridge proxy contract deployment

2. **`test_local_env_starcoin_connection`**
   - ✅ Connects to Starcoin RPC
   - ✅ Gets bridge summary
   - ✅ Verifies chain ID
   - ✅ Checks frozen status

3. **`test_local_env_eth_bridge_committee`**
   - ✅ Queries ETH bridge proxy contract
   - ✅ Gets committee contract address
   - ✅ Verifies committee contract is accessible
   - ✅ Queries committee stake data

4. **`test_local_env_bridge_authority_key`**
   - ✅ Loads bridge authority key from file
   - ✅ Decodes Secp256k1 keypair
   - ✅ Verifies key format (33-byte public key)

5. **`test_local_env_full_chain_verification`** (Integration Test)
   - ✅ Verifies complete ETH contract deployment (proxy, committee, limiter)
   - ✅ Verifies Starcoin bridge contract
   - ✅ Verifies bridge authority key loading
   - ✅ End-to-end infrastructure validation

#### Test Behavior

- **Environment Not Running**: Tests will skip gracefully with warning message
- **Environment Running**: Tests connect and verify all components
- **Key File Not Found**: Authority key tests will skip with informative message

#### Implementation Details

- Uses **hardcoded constants** for local testing (no config file parsing needed)
- Tries multiple possible paths for key file location
- Uses production code paths (`StarcoinBridgeClient`, contract ABIs)
- Tests fail fast if critical infrastructure is missing

#### Configuration

Tests use these hardcoded values for local environment:
```rust
const ETH_RPC_URL: &str = "http://127.0.0.1:8545";
const STARCOIN_RPC_URL: &str = "http://127.0.0.1:9850";
const ETH_PROXY_ADDRESS: &str = "0x0B306BF915C4d645ff596e518fAf3F9669b97016";
const STARCOIN_BRIDGE_ADDRESS: &str = "0x02003d916c06ba52a678e02e364524e6";
const BRIDGE_AUTHORITY_KEY_PATH: &str = "bridge-node/server-config/bridge_authority.key";
```

> **Note**: These values are automatically configured by `setup.sh`. If you manually deploy contracts, update these constants accordingly.

## Other Test Modules

### `basic.rs` (Commented Out)
- Original E2E tests using Sui TestCluster framework
- Not applicable for Starcoin bridge (Starcoin doesn't have TestCluster)
- Kept for reference

### `test_utils.rs` (Commented Out)
- Utilities for Sui-based testing
- Contains Sui-specific code that doesn't compile with Starcoin
- May be adapted in the future if needed

## Test Philosophy

These E2E tests follow a **pragmatic approach**:

1. **Real Environment**: Test against actual running services, not mocks
2. **Happy Path Focus**: Verify normal operation flow
3. **Infrastructure Validation**: Ensure all components are correctly deployed
4. **Graceful Degradation**: Skip tests when environment is not available
5. **Fast Feedback**: Tests complete in < 1 second when environment is ready

## Troubleshooting

### Tests Skip with "Environment not running"

**Solution**: Start the local environment:
```bash
./setup.sh -y --without-bridge-server
```

### "Could not find bridge authority key file"

**Solution**: Ensure you've run the full setup which generates keys:
```bash
./setup.sh -y --without-bridge-server
```

The key file should be at: `bridge-node/server-config/bridge_authority.key`

### Contract Address Mismatch

**Solution**: The setup script generates deployment addresses automatically. If you see mismatches, you may have:
1. Restarted Anvil (generates new addresses)
2. Manually deployed contracts

Re-run `setup.sh` to reset everything:
```bash
./setup.sh -y --without-bridge-server
```

## Future Enhancements

Potential additions:
- [ ] Token transfer tests
- [ ] Committee member verification
- [ ] Message signing and verification
- [ ] Multi-authority scenarios
- [ ] Error case testing (contract reverts, invalid inputs)
