# Bridge Indexer

A multi-chain bridge indexer that monitors and records cross-chain token transfer events from both Starcoin and Ethereum networks.

## Quick Start

The easiest way to manage the indexer is through the `scripts/indexer.sh` script.

### Basic Commands

```bash
# Start PostgreSQL database
./scripts/indexer.sh start-db

# Start Starcoin-only indexer (foreground)
./scripts/indexer.sh start-indexer [start_block]

# Start indexer with ETH support (foreground)
./scripts/indexer.sh start-eth [start_block]

# Start indexer with ETH support (background)
./scripts/indexer.sh start-eth-bg [start_block]

# Clean start: reset DB and start with ETH support (background)
./scripts/indexer.sh clean-start-eth-bg [start_block]

# Check indexer status
./scripts/indexer.sh status

# Stop indexer
./scripts/indexer.sh stop

# Reset database (drop and recreate)
./scripts/indexer.sh reset-db

# View logs
./scripts/indexer.sh logs
./scripts/indexer.sh logs -f  # follow mode
```

### Configuration

The indexer reads configuration from `bridge-config/server-config.yaml`. Key settings:

- `starcoin-bridge-proxy-address`: Starcoin bridge contract address
- `eth-bridge-proxy-address`: Ethereum bridge contract address
- `indexer-db-url`: PostgreSQL connection string

Environment variables can override config file settings:
- `BRIDGE_ADDRESS`: Starcoin bridge address
- `ETH_BRIDGE_ADDRESS`: Ethereum bridge address
- `RPC_URL`: Starcoin RPC endpoint
- `ETH_RPC_URL`: Ethereum RPC endpoint

## Database Schema

The indexer writes to three main tables:

### `token_transfer`

Tracks the **lifecycle status** of each cross-chain transfer.

| Column | Description |
|--------|-------------|
| chain_id | Source chain ID (2=Starcoin, 12=ETH) |
| nonce | Sequence number for this chain |
| status | Transfer status: `Deposited`, `Approved`, `Claimed` |
| block_height | Block where this status change occurred |
| data_source | Which chain produced this event (`STARCOIN` or `ETH`) |

Primary Key: `(chain_id, nonce, status)`

Each transfer has multiple records tracking its progress:
- `Deposited` - User initiated the cross-chain transfer on source chain
- `Approved` - Bridge committee approved the transfer on destination chain
- `Claimed` - User claimed the tokens on destination chain

### `token_transfer_data`

Stores **detailed deposit information** when a transfer is initiated.

| Column | Description |
|--------|-------------|
| chain_id | Source chain ID |
| nonce | Sequence number |
| sender_address | Address that initiated the transfer |
| recipient_address | Destination address on target chain |
| destination_chain | Target chain ID |
| token_id | Token type identifier |
| amount | Transfer amount |

Primary Key: `(chain_id, nonce)`

This table only contains deposit events (one record per transfer) with full transfer details.

### `governance_actions`

Records bridge governance events like:
- Route limit updates
- Emergency operations
- Validator blocklist changes
- Token registrations

## Data Flow Example

**ETH → Starcoin Transfer:**
```
1. User deposits on ETH
   → token_transfer_data: (chain_id=12, nonce=0, sender=0x..., amount=100)
   → token_transfer: (chain_id=12, nonce=0, status=Deposited, data_source=ETH)

2. Bridge committee approves on Starcoin
   → token_transfer: (chain_id=12, nonce=0, status=Approved, data_source=STARCOIN)

3. User claims on Starcoin
   → token_transfer: (chain_id=12, nonce=0, status=Claimed, data_source=STARCOIN)
```

**Starcoin → ETH Transfer:**
```
1. User deposits on Starcoin
   → token_transfer_data: (chain_id=2, nonce=0, sender=0x..., amount=100)
   → token_transfer: (chain_id=2, nonce=0, status=Deposited, data_source=STARCOIN)

2. Bridge committee approves on Starcoin
   → token_transfer: (chain_id=2, nonce=0, status=Approved, data_source=STARCOIN)

3. User claims on ETH
   → token_transfer: (chain_id=2, nonce=0, status=Claimed, data_source=ETH)
```

## Monitoring

The indexer exposes Prometheus metrics on port 9184 (or next available port).

Check the watermark table for sync progress:
```sql
SELECT * FROM watermarks;
```

## Troubleshooting

**Connection reset errors with ETH RPC:**
- Ensure Anvil/Hardhat node is running
- Check if the RPC endpoint is correct

**Empty tables after running:**
- Verify bridge addresses match your deployment
- Check `./scripts/indexer.sh logs` for errors
- Ensure starting block is before any bridge events

**Database connection issues:**
- Run `./scripts/indexer.sh start-db` to start PostgreSQL
- Check if port 5432 is available
