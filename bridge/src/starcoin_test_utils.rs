// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Starcoin test utilities for bridge deployment
//!
//! This module provides reusable utilities for testing Starcoin bridge deployment:
//! - Loading Move config and contract packages
//! - Building deployment transactions
//! - Creating test fixtures

use anyhow::{Context, Result};
use starcoin_config::ChainNetwork;
use starcoin_transaction_builder::{
    create_signed_txn_with_association_account, DEFAULT_MAX_GAS_AMOUNT,
};
use starcoin_txpool_api::TxPoolSyncService;
use starcoin_types::account_address::AccountAddress;
use starcoin_vm_types::identifier::Identifier;
use starcoin_vm_types::language_storage::ModuleId;
use starcoin_vm_types::transaction::{
    Package, ScriptFunction, SignedUserTransaction, TransactionPayload,
};
use std::fs;

pub const DEFAULT_CONFIG_PATH: &str = "../contracts/move/config.json";
pub const DEFAULT_BLOB_PATH: &str = "../contracts/move/Stc-Bridge-Move.v0.0.1.blob";
pub const DEV_CHAIN_ID: u8 = 254;

/// Move contract configuration
#[derive(serde::Deserialize, Clone, Debug)]
pub struct MoveConfig {
    pub address: String,
    pub public_key: String,
    pub private_key: String,
}

impl MoveConfig {
    /// Load config from default path
    pub fn load() -> Result<Self> {
        Self::load_from(DEFAULT_CONFIG_PATH)
    }

    /// Load config from specific path
    pub fn load_from(path: &str) -> Result<Self> {
        let config_content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config from {}", path))?;
        serde_json::from_str(&config_content).context("Failed to parse config.json")
    }

    /// Parse address from config
    pub fn address(&self) -> Result<AccountAddress> {
        AccountAddress::from_hex_literal(&self.address)
            .context("Failed to parse address from config")
    }

    /// Get public key bytes
    pub fn public_key_bytes(&self) -> Result<Vec<u8>> {
        let pubkey_hex = self.public_key.trim_start_matches("0x");
        hex::decode(pubkey_hex).context("Failed to decode public key")
    }
}

/// Load Move package from blob file
pub fn load_package() -> Result<Package> {
    load_package_from(DEFAULT_BLOB_PATH)
}

/// Load Move package from specific blob file
pub fn load_package_from(path: &str) -> Result<Package> {
    let blob_content =
        fs::read(path).with_context(|| format!("Failed to read blob from {}", path))?;
    bcs_ext::from_bytes(&blob_content).context("Failed to deserialize Package from blob")
}

/// Create a ScriptFunction call for the bridge
pub fn create_bridge_script_function(
    bridge_address: AccountAddress,
    function_name: &str,
    ty_args: Vec<starcoin_vm_types::language_storage::TypeTag>,
    args: Vec<Vec<u8>>,
) -> ScriptFunction {
    ScriptFunction::new(
        ModuleId::new(bridge_address, Identifier::new("Bridge").unwrap()),
        Identifier::new(function_name).unwrap(),
        ty_args,
        args,
    )
}

/// Builder for creating bridge deployment transactions
pub struct BridgeDeploymentBuilder {
    config: MoveConfig,
    package: Package,
    network: ChainNetwork,
    sequence_number: u64,
}

impl BridgeDeploymentBuilder {
    /// Create a new builder with default config and blob
    pub fn new() -> Result<Self> {
        Self::with_paths(DEFAULT_CONFIG_PATH, DEFAULT_BLOB_PATH)
    }

    /// Create a new builder with custom paths
    pub fn with_paths(config_path: &str, blob_path: &str) -> Result<Self> {
        let config = MoveConfig::load_from(config_path)?;
        let package = load_package_from(blob_path)?;
        let network = ChainNetwork::new_builtin(starcoin_config::BuiltinNetworkID::Dev);

        Ok(Self {
            config,
            package,
            network,
            sequence_number: 0,
        })
    }

    /// Get the bridge address
    pub fn bridge_address(&self) -> Result<AccountAddress> {
        self.config.address()
    }

    /// Get the Move config
    pub fn config(&self) -> &MoveConfig {
        &self.config
    }

    /// Get the network
    pub fn network(&self) -> &ChainNetwork {
        &self.network
    }

    /// Build deploy transaction
    pub fn build_deploy_transaction(&mut self) -> Result<SignedUserTransaction> {
        let payload = TransactionPayload::Package(self.package.clone());
        let txn = create_signed_txn_with_association_account(
            payload,
            self.sequence_number,
            DEFAULT_MAX_GAS_AMOUNT,
            1,
            3600,
            &self.network,
        );
        self.sequence_number += 1;
        Ok(txn)
    }

    /// Build initialize bridge transaction
    pub fn build_initialize_transaction(&mut self) -> Result<SignedUserTransaction> {
        let bridge_address = self.bridge_address()?;
        let args = vec![bcs_ext::to_bytes(&DEV_CHAIN_ID)?];
        let script =
            create_bridge_script_function(bridge_address, "initialize_bridge", vec![], args);
        let txn = create_signed_txn_with_association_account(
            TransactionPayload::ScriptFunction(script),
            self.sequence_number,
            DEFAULT_MAX_GAS_AMOUNT,
            1,
            3600,
            &self.network,
        );
        self.sequence_number += 1;
        Ok(txn)
    }

    /// Build register committee member transaction
    pub fn build_register_committee_transaction(
        &mut self,
        url: &str,
    ) -> Result<SignedUserTransaction> {
        let bridge_address = self.bridge_address()?;
        let pubkey_bytes = self.config.public_key_bytes()?;
        let url_bytes = url.as_bytes().to_vec();

        let args = vec![
            bcs_ext::to_bytes(&pubkey_bytes)?,
            bcs_ext::to_bytes(&url_bytes)?,
        ];
        let script = create_bridge_script_function(
            bridge_address,
            "register_committee_member",
            vec![],
            args,
        );
        let txn = create_signed_txn_with_association_account(
            TransactionPayload::ScriptFunction(script),
            self.sequence_number,
            DEFAULT_MAX_GAS_AMOUNT,
            1,
            3600,
            &self.network,
        );
        self.sequence_number += 1;
        Ok(txn)
    }

    /// Build create committee transaction
    pub fn build_create_committee_transaction(
        &mut self,
        voting_power: u64,
        min_stake_percentage: u64,
        epoch: u64,
    ) -> Result<SignedUserTransaction> {
        let bridge_address = self.bridge_address()?;

        let args = vec![
            bcs_ext::to_bytes(&bridge_address)?,
            bcs_ext::to_bytes(&voting_power)?,
            bcs_ext::to_bytes(&min_stake_percentage)?,
            bcs_ext::to_bytes(&epoch)?,
        ];
        let script =
            create_bridge_script_function(bridge_address, "create_committee", vec![], args);
        let txn = create_signed_txn_with_association_account(
            TransactionPayload::ScriptFunction(script),
            self.sequence_number,
            DEFAULT_MAX_GAS_AMOUNT,
            1,
            3600,
            &self.network,
        );
        self.sequence_number += 1;
        Ok(txn)
    }

    /// Build token setup transaction
    pub fn build_setup_token_transaction(
        &mut self,
        token_name: &str,
    ) -> Result<SignedUserTransaction> {
        let bridge_address = self.bridge_address()?;
        let function_name = format!("setup_{}_token", token_name.to_lowercase());

        let script = create_bridge_script_function(bridge_address, &function_name, vec![], vec![]);
        let txn = create_signed_txn_with_association_account(
            TransactionPayload::ScriptFunction(script),
            self.sequence_number,
            DEFAULT_MAX_GAS_AMOUNT,
            1,
            3600,
            &self.network,
        );
        self.sequence_number += 1;
        Ok(txn)
    }

    /// Build all deployment transactions in order
    pub fn build_all_transactions(&mut self) -> Result<Vec<SignedUserTransaction>> {
        let mut transactions = Vec::new();

        // 1. Deploy
        transactions.push(self.build_deploy_transaction()?);

        // 2. Initialize
        transactions.push(self.build_initialize_transaction()?);

        // 3. Register committee
        transactions.push(self.build_register_committee_transaction("http://127.0.0.1:9191")?);

        // 4. Create committee
        transactions.push(self.build_create_committee_transaction(10000, 5000, 0)?);

        // 5-8. Setup tokens
        for token in ["eth", "btc", "usdc", "usdt"] {
            transactions.push(self.build_setup_token_transaction(token)?);
        }

        Ok(transactions)
    }
}

impl Default for BridgeDeploymentBuilder {
    fn default() -> Self {
        Self::new().expect("Failed to create default BridgeDeploymentBuilder")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_move_config() -> Result<()> {
        let config = MoveConfig::load()?;
        assert!(!config.address.is_empty());
        assert!(!config.public_key.is_empty());
        assert!(!config.private_key.is_empty());

        let address = config.address()?;
        println!("Loaded config address: {:?}", address);
        Ok(())
    }

    #[test]
    fn test_load_package() -> Result<()> {
        let package = load_package()?;
        assert!(!package.modules().is_empty());
        println!("Package has {} modules", package.modules().len());
        Ok(())
    }

    #[test]
    fn test_builder_creates_all_transactions() -> Result<()> {
        let mut builder = BridgeDeploymentBuilder::new()?;
        let transactions = builder.build_all_transactions()?;

        assert_eq!(transactions.len(), 8, "Should create 8 transactions");

        for (i, txn) in transactions.iter().enumerate() {
            println!("Transaction {}: hash={:?}", i + 1, txn.id());
        }

        Ok(())
    }

    #[test]
    fn test_builder_individual_transactions() -> Result<()> {
        let mut builder = BridgeDeploymentBuilder::new()?;

        let deploy = builder.build_deploy_transaction()?;
        println!("Deploy: {:?}", deploy.id());

        let init = builder.build_initialize_transaction()?;
        println!("Initialize: {:?}", init.id());

        let register = builder.build_register_committee_transaction("http://test.com")?;
        println!("Register: {:?}", register.id());

        let committee = builder.build_create_committee_transaction(10000, 5000, 0)?;
        println!("Committee: {:?}", committee.id());

        let eth = builder.build_setup_token_transaction("eth")?;
        println!("ETH token: {:?}", eth.id());

        Ok(())
    }
}

/// Embedded Starcoin node for testing
///
/// This manages a Starcoin node running in memory for testing.
/// The node is automatically started and stopped.
/// It uses internal services (TxPool, Chain, Storage) directly instead of RPC.
pub struct EmbeddedStarcoinNode {
    handle: starcoin_test_helper::NodeHandle,
}

impl EmbeddedStarcoinNode {
    /// Start a new embedded Starcoin dev node
    ///
    /// The node will automatically choose random available ports for RPC.
    /// Multiple nodes can be started simultaneously without port conflicts.
    pub fn start() -> Result<Self> {
        let config = std::sync::Arc::new(starcoin_config::NodeConfig::random_for_test());

        let handle = starcoin_test_helper::run_node_by_config(config)
            .context("Failed to start embedded Starcoin node")?;

        // Give the node time to start
        std::thread::sleep(std::time::Duration::from_secs(2));

        Ok(Self { handle })
    }

    /// Get the node handle for direct service access
    pub fn handle(&self) -> &starcoin_test_helper::NodeHandle {
        &self.handle
    }

    /// Get the node config
    pub fn config(&self) -> std::sync::Arc<starcoin_config::NodeConfig> {
        self.handle.config()
    }

    /// Get the network
    pub fn network(&self) -> ChainNetwork {
        self.handle.config().net().clone()
    }

    /// Submit a transaction directly to the node (no RPC needed)
    pub fn submit_transaction(&self, txn: SignedUserTransaction) -> Result<()> {
        // Access txpool service through the handle
        let txpool = self.handle.txpool();
        let results = txpool.add_txns(vec![txn]);

        // Check if any transaction failed
        for (i, result) in results.into_iter().enumerate() {
            result.map_err(|e| anyhow::anyhow!("Transaction {} failed: {:?}", i, e))?;
        }
        Ok(())
    }

    /// Generate a block (for testing)
    pub fn generate_block(&self) -> Result<starcoin_types::block::Block> {
        self.handle.generate_block()
    }

    /// Stop the embedded node gracefully
    pub fn stop(self) {
        // Just drop - the handle's Drop impl will clean up
        drop(self.handle);
    }
}

/// Complete test environment with embedded Starcoin node and deployed bridge
pub struct StarcoinBridgeTestEnv {
    node: EmbeddedStarcoinNode,
    config: MoveConfig,
    bridge_address: AccountAddress,
}

impl StarcoinBridgeTestEnv {
    /// Create and initialize a complete test environment
    ///
    /// This will:
    /// 1. Start an embedded Starcoin node
    /// 2. Deploy the bridge contract
    /// 3. Initialize the bridge
    /// 4. Set up committee and tokens
    pub fn new() -> Result<Self> {
        let node = EmbeddedStarcoinNode::start()?;
        let config = MoveConfig::load()?;
        let bridge_address = config.address()?;

        println!("Starting Starcoin bridge test environment...");
        println!("  Network: {:?}", node.network().id());
        println!("  Bridge address: {:?}", bridge_address);

        // TODO: In a real implementation, we would:
        // 1. Build all deployment transactions
        // 2. Submit them via node.submit_transaction()
        // 3. Generate blocks via node.generate_block()
        // 4. Verify deployment
        // For now, we just return the environment

        Ok(Self {
            node,
            config,
            bridge_address,
        })
    }

    /// Create and fully deploy the bridge (all transactions submitted and confirmed)
    pub fn new_with_deployment() -> Result<Self> {
        let mut env = Self::new()?;
        env.deploy_bridge()?;
        Ok(env)
    }

    /// Deploy the bridge contract and initialize it
    fn deploy_bridge(&mut self) -> Result<()> {
        let mut builder = BridgeDeploymentBuilder::new()?;

        println!("Deploying bridge contracts...");

        // Build all transactions
        let transactions = builder.build_all_transactions()?;

        // Submit each transaction and generate a block
        for (i, txn) in transactions.iter().enumerate() {
            println!(
                "  Submitting transaction {}/{}...",
                i + 1,
                transactions.len()
            );
            self.node.submit_transaction(txn.clone())?;

            // Generate a block to include the transaction
            self.node
                .generate_block()
                .with_context(|| format!("Failed to generate block for transaction {}", i + 1))?;

            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        println!("✓ Bridge deployment complete!");
        Ok(())
    }

    /// Get the embedded node
    pub fn node(&self) -> &EmbeddedStarcoinNode {
        &self.node
    }

    /// Get the Move config
    pub fn config(&self) -> &MoveConfig {
        &self.config
    }

    /// Get the bridge address
    pub fn bridge_address(&self) -> AccountAddress {
        self.bridge_address
    }
}
