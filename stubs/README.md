# Starcoin Bridge Stubs

This directory contains stub implementations of Sui dependencies that need to be replaced with Starcoin equivalents.

## Purpose

These stubs provide minimal type signatures to satisfy Cargo dependency resolution, but **do not implement actual business logic**. Each stub needs to be filled with Starcoin-specific implementations.

## Stub Packages (12 remaining)

### Core Types (Highest Priority)
1. **starcoin-bridge-types** ⭐ - Most critical! All other code depends on this
   - Contains all blockchain type definitions
   - Maps Sui types → Starcoin types
   - See TODOs for specific Starcoin equivalents

### SDK & Client
2. **starcoin-bridge-sdk** - Client implementation
3. **starcoin-bridge-keys** - Key management

### RPC & API
4. **starcoin-bridge-rpc-api** - RPC protocol definitions
5. **starcoin-bridge-json-rpc-api** - JSON-RPC API
6. **starcoin-bridge-json-rpc-types** - JSON-RPC types

### Data & Storage
7. **starcoin-bridge-data-ingestion-core** - Data ingestion pipeline
8. **starcoin-bridge-storage** - Storage layer (Blob)
9. **starcoin-bridge-authority-aggregation** - BFT signature aggregation

### Testing
10. **test-cluster** - Test cluster utilities
11. **starcoin-bridge-test-transaction-builder** - Test transaction builder
12. **starcoin-bridge-synthetic-ingestion** - Synthetic data generation

## Already Replaced (Not in stubs/)

✅ **move-core-types** - Directly using Starcoin's Move VM1 types
   - Defined in main `Cargo.toml`
   - Uses: `git = "https://github.com/starcoinorg/move", rev = "babf994a..."`

✅ **starcoin-bridge-config** - Implemented with Starcoin config wrapper
   - Location: `bridge/starcoin-bridge-config/`
   - Wraps `starcoin-config` with Sui-compatible API
   - Provides `Config` trait and `local_ip_utils::get_available_port()`

## Implementation Order

Implement in dependency order (bottom-up):

1. ✅ **move-core-types** - Already done! Using Starcoin's Move VM1 directly

2. **starcoin-bridge-types** - Start here! Everything depends on it
   - Fill in missing fields in `BridgeSummary`, `CheckpointSummary`, etc.
   - Add enum variants to `CallArg`, `Owner`, etc.
   - Implement missing functions like `parse_starcoin_bridge_type_tag`

3. **starcoin-bridge-json-rpc-types** - RPC types
   - Export `SuiEvent`, `SuiExecutionStatus`, etc.

4. **starcoin-bridge-sdk** - Client implementation
   - Implement `WalletContext` methods
   - Implement `StarcoinClient::read_api()`, etc.

5. **Other stubs** - Implement as needed based on compilation errors

## How to Use

Each stub file contains `TODO` comments indicating which Starcoin types/modules should replace the Sui equivalents. For example:

```rust
// In starcoin-bridge-types/src/lib.rs
pub mod base_types {
    // TODO: Replace with starcoin_types::account_address::AccountAddress
    pub type SuiAddress = [u8; 32];
}
```

Means you should replace `SuiAddress` with Starcoin's `AccountAddress` type.

## Current Status

- ✅ **move-core-types** - Replaced with Starcoin's Move VM1 (git dependency)
- ✅ **starcoin-bridge-config** - Implemented with Starcoin config wrapper
- ✅ 12 stubs remaining with minimal type signatures
- ⏳ Business logic implementation needed (see bridge compilation errors)
- 🎯 Target: 131 compilation errors → 0 (as stubs are implemented)

## Related Files

- `/Volumes/SSD/bridge-migration/starcoin/bridge/Cargo.toml` - Stub path references
- `/Volumes/SSD/bridge-migration/starcoin/bridge/bridge/` - Main bridge code that uses these stubs
