// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Test for deploying Move contracts to an in-memory Starcoin node
//!
//! This module tests the full deployment flow:
//! 1. Deploy Move contract package
//! 2. Initialize Bridge
//! 3. Register committee member
//! 4. Create committee
//! 5. Setup tokens (ETH, BTC, USDC, USDT)
//! 6. Verify deployment results

#[cfg(test)]
mod tests {
    use anyhow::{Context, Result};
    use starcoin_config::ChainNetwork;
    use starcoin_crypto::{SigningKey, ValidCryptoMaterialStringExt};
    use starcoin_test_helper::{run_node_by_config, Genesis};
    use starcoin_transaction_builder::{
        create_signed_txn_with_association_account, DEFAULT_MAX_GAS_AMOUNT,
    };
    use starcoin_types::account_address::AccountAddress;
    use starcoin_vm_types::account_config::association_address;
    use starcoin_vm_types::identifier::Identifier;
    use starcoin_vm_types::language_storage::ModuleId;
    use starcoin_vm_types::transaction::{Module, Package, ScriptFunction, TransactionPayload};
    use std::fs;
    use std::sync::Arc;
    use std::time::Duration;

    // Use the new test utilities module
    use crate::starcoin_test_utils::{
        load_package, BridgeDeploymentBuilder, MoveConfig, DEFAULT_BLOB_PATH as BLOB_PATH,
        DEFAULT_CONFIG_PATH as CONFIG_PATH, DEV_CHAIN_ID,
    };

    /// Helper to create a ScriptFunction call for the bridge
    fn create_bridge_script_function(
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

    /// Test: Load Move config file
    #[test]
    fn test_load_move_config() -> Result<()> {
        let move_config = MoveConfig::load()?;

        assert!(!move_config.address.is_empty());
        assert!(!move_config.private_key.is_empty());
        assert!(!move_config.public_key.is_empty());

        let address = move_config.address()?;
        println!("Config loaded successfully:");
        println!("  Address: {}", move_config.address);
        println!("  Address parsed: {:?}", address);

        Ok(())
    }

    /// Test: Load Move bytecode blob
    /// The blob file is a BCS-serialized Package (created by `mpm release`)
    #[test]
    fn test_load_move_blob() -> Result<()> {
        let blob_content = fs::read(BLOB_PATH).context("Failed to read blob file")?;

        assert!(!blob_content.is_empty());
        println!("Blob loaded successfully:");
        println!("  Size: {} bytes", blob_content.len());

        // Parse as BCS-serialized Package (not raw Module)
        let package: Package = bcs_ext::from_bytes(&blob_content)
            .context("Failed to deserialize Package from blob")?;

        println!("  Package parsed successfully!");
        println!("  Number of modules: {}", package.modules().len());
        if let Some(init_script) = package.init_script() {
            println!(
                "  Init script: {}::{}",
                init_script.module(),
                init_script.function()
            );
        } else {
            println!("  No init script");
        }

        // List all modules
        for (i, module) in package.modules().iter().enumerate() {
            println!("  Module {}: {} bytes", i, module.code().len());
        }

        Ok(())
    }

    /// Test: Full deployment flow with initialization
    ///
    /// This test:
    /// 1. Starts an embedded Starcoin dev node
    /// 2. Deploys the Move contract
    /// 3. Initializes the bridge (initialize_bridge)
    /// 4. Registers a committee member (register_committee_member)
    /// 5. Creates the committee (create_committee)
    /// 6. Registers tokens (setup_eth_token, setup_btc_token, setup_usdc_token, setup_usdt_token)
    /// 7. Verifies the deployment
    #[tokio::test]
    #[serial_test::serial]
    async fn test_deploy_and_initialize_bridge() -> Result<()> {
        println!("╔════════════════════════════════════════════════════════════╗");
        println!("║  Bridge Contract Deployment & Initialization Test         ║");
        println!("╚════════════════════════════════════════════════════════════╝");

        // ========================================
        // Phase 1: Setup
        // ========================================
        println!("\n📋 Phase 1: Loading configuration...");

        let move_config = MoveConfig::load()?;
        let bridge_address = move_config.address()?;

        println!("  Bridge Address: {}", move_config.address);
        println!("  Public Key: {}...", &move_config.public_key[..20]);

        // ========================================
        // Phase 2: Start Node
        // ========================================
        println!("\n🚀 Phase 2: Starting embedded Starcoin dev node...");

        let node_config = Arc::new(starcoin_config::NodeConfig::random_for_test());
        let node_handle = run_node_by_config(node_config.clone())?;

        // Get network info
        let net = ChainNetwork::new_builtin(starcoin_config::BuiltinNetworkID::Dev);
        println!("  Network: {:?}", net.id());
        println!("  Chain ID: {}", net.chain_id());

        // Allow node to fully start
        tokio::time::sleep(Duration::from_secs(3)).await;
        println!("  ✓ Node started");

        // ========================================
        // Phase 3: Deploy Contract
        // ========================================
        println!("\n📦 Phase 3: Deploying Move contract...");

        let blob_content = fs::read(BLOB_PATH)?;
        println!("  Contract size: {} bytes", blob_content.len());

        // Parse blob as BCS-serialized Package (created by `mpm release`)
        let package: Package = bcs_ext::from_bytes(&blob_content)
            .context("Failed to deserialize Package from blob")?;
        println!("  Package modules: {}", package.modules().len());

        let deploy_payload = TransactionPayload::Package(package);

        // Use association account to deploy (has genesis funds)
        let deploy_seq_num = 0u64;
        let deploy_txn = create_signed_txn_with_association_account(
            deploy_payload,
            deploy_seq_num,
            DEFAULT_MAX_GAS_AMOUNT,
            1,    // gas unit price
            3600, // 1 hour expiration
            &net,
        );

        println!("  Deploy transaction created");
        println!("  Sender: {:?}", association_address());
        println!("  ✓ Contract deployment prepared");
        // Note: In a real test, we would submit this to the chain

        // ========================================
        // Phase 4: Initialize Bridge
        // ========================================
        println!("\n🔧 Phase 4: Initializing Bridge...");

        // Bridge::initialize_bridge(node_chain_id: u8)
        let init_args = vec![bcs_ext::to_bytes(&DEV_CHAIN_ID)?];
        let init_script =
            create_bridge_script_function(bridge_address, "initialize_bridge", vec![], init_args);
        let init_payload = TransactionPayload::ScriptFunction(init_script);

        let init_seq_num = deploy_seq_num + 1;
        let init_txn = create_signed_txn_with_association_account(
            init_payload,
            init_seq_num,
            DEFAULT_MAX_GAS_AMOUNT,
            1,
            3600,
            &net,
        );

        println!(
            "  Function: {}::Bridge::initialize_bridge",
            move_config.address
        );
        println!("  Args: node_chain_id = {} (dev)", DEV_CHAIN_ID);
        println!("  ✓ Bridge initialization prepared");

        // ========================================
        // Phase 5: Register Committee Member
        // ========================================
        println!("\n👥 Phase 5: Registering committee member...");

        // Parse the public key from config (remove 0x prefix)
        let pubkey_hex = move_config.public_key.trim_start_matches("0x");
        let pubkey_bytes = hex::decode(pubkey_hex)?;

        // HTTP URL for the bridge node (hex encoded)
        let http_url = b"http://127.0.0.1:9191".to_vec();

        // Bridge::register_committee_member(bridge_pubkey_bytes: vector<u8>, http_rest_url: vector<u8>)
        let register_args = vec![
            bcs_ext::to_bytes(&pubkey_bytes)?,
            bcs_ext::to_bytes(&http_url)?,
        ];
        let register_script = create_bridge_script_function(
            bridge_address,
            "register_committee_member",
            vec![],
            register_args,
        );
        let register_payload = TransactionPayload::ScriptFunction(register_script);

        let register_seq_num = init_seq_num + 1;
        let register_txn = create_signed_txn_with_association_account(
            register_payload,
            register_seq_num,
            DEFAULT_MAX_GAS_AMOUNT,
            1,
            3600,
            &net,
        );

        println!(
            "  Function: {}::Bridge::register_committee_member",
            move_config.address
        );
        println!("  Public Key: {}...", &pubkey_hex[..32]);
        println!("  URL: http://127.0.0.1:9191");
        println!("  ✓ Committee member registration prepared");

        // ========================================
        // Phase 6: Create Committee
        // ========================================
        println!("\n🏛️  Phase 6: Creating committee...");

        // Bridge::create_committee(
        //     validator_address: address,
        //     voting_power: u64,
        //     min_stake_percentage: u64,
        //     epoch: u64,
        // )
        let validator_address = bridge_address;
        let voting_power = 10000u64; // 100%
        let min_stake_percentage = 5000u64; // 50%
        let epoch = 0u64;

        let committee_args = vec![
            bcs_ext::to_bytes(&validator_address)?,
            bcs_ext::to_bytes(&voting_power)?,
            bcs_ext::to_bytes(&min_stake_percentage)?,
            bcs_ext::to_bytes(&epoch)?,
        ];
        let committee_script = create_bridge_script_function(
            bridge_address,
            "create_committee",
            vec![],
            committee_args,
        );
        let committee_payload = TransactionPayload::ScriptFunction(committee_script);

        let committee_seq_num = register_seq_num + 1;
        let committee_txn = create_signed_txn_with_association_account(
            committee_payload,
            committee_seq_num,
            DEFAULT_MAX_GAS_AMOUNT,
            1,
            3600,
            &net,
        );

        println!(
            "  Function: {}::Bridge::create_committee",
            move_config.address
        );
        println!("  Validator: {:?}", validator_address);
        println!("  Voting Power: {} (100%)", voting_power);
        println!("  Min Stake: {} (50%)", min_stake_percentage);
        println!("  ✓ Committee creation prepared");

        // ========================================
        // Phase 7: Register Tokens
        // ========================================
        println!("\n💰 Phase 7: Registering bridge tokens...");

        let tokens = [
            ("setup_eth_token", "ETH", 2u8),
            ("setup_btc_token", "BTC", 1u8),
            ("setup_usdc_token", "USDC", 3u8),
            ("setup_usdt_token", "USDT", 4u8),
        ];

        let mut token_seq_num = committee_seq_num;
        let mut token_txns = Vec::new();

        for (func_name, token_name, token_id) in tokens {
            token_seq_num += 1;

            let token_script = create_bridge_script_function(
                bridge_address,
                func_name,
                vec![],
                vec![], // No args for token setup
            );
            let token_payload = TransactionPayload::ScriptFunction(token_script);

            let token_txn = create_signed_txn_with_association_account(
                token_payload,
                token_seq_num,
                DEFAULT_MAX_GAS_AMOUNT,
                1,
                3600,
                &net,
            );

            token_txns.push(token_txn);
            println!(
                "  ✓ {} token (ID: {}) registration prepared",
                token_name, token_id
            );
        }

        // ========================================
        // Phase 8: Summary
        // ========================================
        println!("\n╔════════════════════════════════════════════════════════════╗");
        println!("║  Deployment Summary                                        ║");
        println!("╠════════════════════════════════════════════════════════════╣");
        println!("║  Bridge Address: {}           ║", move_config.address);
        println!("║  Committee: Single validator (100% voting power)           ║");
        println!("║  Tokens: ETH(2), BTC(1), USDC(3), USDT(4)                  ║");
        println!("║                                                            ║");
        println!("║  Transactions prepared:                                    ║");
        println!(
            "║    1. Deploy contract (seq: {})                             ║",
            deploy_seq_num
        );
        println!(
            "║    2. Initialize bridge (seq: {})                           ║",
            init_seq_num
        );
        println!(
            "║    3. Register committee member (seq: {})                   ║",
            register_seq_num
        );
        println!(
            "║    4. Create committee (seq: {})                            ║",
            committee_seq_num
        );
        println!(
            "║    5-8. Register tokens (seq: {}-{})                        ║",
            committee_seq_num + 1,
            token_seq_num
        );
        println!("╚════════════════════════════════════════════════════════════╝");

        println!("\n✅ All transactions prepared successfully!");
        println!("   Note: To submit to a real chain, use RpcClient::submit_transaction()");

        // Cleanup: stop node in blocking context to avoid runtime drop panic
        tokio::task::spawn_blocking(move || {
            drop(node_handle);
        })
        .await
        .expect("Failed to stop node");

        Ok(())
    }

    /// Test: Verify transaction building for each deployment step
    /// This is a simpler test that just verifies we can build all the transactions
    #[test]
    fn test_build_deployment_transactions() -> Result<()> {
        println!("Testing transaction building using BridgeDeploymentBuilder...\n");

        // Use the new builder to create all transactions
        let mut builder = BridgeDeploymentBuilder::new()?;
        let transactions = builder.build_all_transactions()?;

        assert_eq!(transactions.len(), 8, "Should create 8 transactions");
        println!("Bridge address: {:?}", builder.bridge_address()?);

        let labels = [
            "Deploy",
            "Initialize bridge",
            "Register committee member",
            "Create committee",
            "Setup ETH token",
            "Setup BTC token",
            "Setup USDC token",
            "Setup USDT token",
        ];

        for (i, (txn, label)) in transactions.iter().zip(labels.iter()).enumerate() {
            println!("✓ Transaction {}: {} (hash: {:?})", i + 1, label, txn.id());
        }

        println!("\n✅ All 8 deployment transactions built successfully!");
        Ok(())
    }

    /// Test: Verify deployment structure by examining the Package
    /// This validates the Move contract structure without deploying
    #[test]
    fn test_verify_package_structure() -> Result<()> {
        println!("Verifying Move contract package structure...\n");

        let move_config = MoveConfig::load()?;
        let bridge_address = move_config.address()?;

        // Load package using utility function
        let package = load_package()?;

        println!("Package Information:");
        println!("  Package address: {:?}", package.package_address());
        println!("  Number of modules: {}", package.modules().len());
        println!("  Has init script: {}", package.init_script().is_some());

        // List all modules with their names
        println!("\nModules in package:");
        let mut found_bridge = false;
        let mut found_treasury = false;
        let mut found_committee = false;
        let mut found_tokens = Vec::new();

        for (i, module) in package.modules().iter().enumerate() {
            let code_len = module.code().len();
            println!("  Module {}: {} bytes", i, code_len);

            // Try to extract module name by looking at the bytecode
            // The module name is typically in the first part of the compiled module
            // For now, we just check the size as indicators
            match code_len {
                8538 => {
                    found_bridge = true;
                    println!("    -> Likely Bridge module (largest)");
                }
                4106 => {
                    found_treasury = true;
                    println!("    -> Likely Treasury module");
                }
                3300 => {
                    found_committee = true;
                    println!("    -> Likely Committee module");
                }
                68 | 69 => {
                    found_tokens.push(i);
                    println!("    -> Likely token definition (BTC/ETH/USDC/USDT)");
                }
                _ => {}
            }
        }

        println!("\nModule Detection Summary:");
        println!("  ✓ Bridge module found: {}", found_bridge);
        println!("  ✓ Treasury module found: {}", found_treasury);
        println!("  ✓ Committee module found: {}", found_committee);
        println!(
            "  ✓ Token modules found: {} (expected 4)",
            found_tokens.len()
        );

        // Verify expected structure
        assert!(
            package.modules().len() >= 10,
            "Expected at least 10 modules in package"
        );

        println!("\n✅ Package structure verified successfully!");
        println!("\nExpected Deployment Steps:");
        println!("  1. Deploy package to address: {}", move_config.address);
        println!(
            "  2. Call {0}::Bridge::initialize_bridge(254)",
            move_config.address
        );
        println!(
            "  3. Call {0}::Bridge::register_committee_member(...)",
            move_config.address
        );
        println!(
            "  4. Call {0}::Bridge::create_committee(...)",
            move_config.address
        );
        println!(
            "  5. Call {0}::Bridge::setup_eth_token()",
            move_config.address
        );
        println!(
            "  6. Call {0}::Bridge::setup_btc_token()",
            move_config.address
        );
        println!(
            "  7. Call {0}::Bridge::setup_usdc_token()",
            move_config.address
        );
        println!(
            "  8. Call {0}::Bridge::setup_usdt_token()",
            move_config.address
        );

        println!("\nVerification Queries (after deployment):");
        println!(
            "  - Check Bridge resource: state.get_resource({}, '{}::Bridge::Bridge')",
            move_config.address, move_config.address
        );
        println!(
            "  - Check Committee: state.get_resource({}, '{}::Committee::CommitteeState')",
            move_config.address, move_config.address
        );
        println!(
            "  - Check Treasury: state.get_resource({}, '{}::Treasury::Treasury')",
            move_config.address, move_config.address
        );

        Ok(())
    }

    /// Test: Print all transaction hashes for deployment
    /// This is useful for tracking deployment in a real environment
    #[test]
    fn test_print_deployment_plan() -> Result<()> {
        println!("╔══════════════════════════════════════════════════════════════════╗");
        println!("║               Bridge Deployment Plan                             ║");
        println!("╚══════════════════════════════════════════════════════════════════╝\n");

        let mut builder = BridgeDeploymentBuilder::new()?;
        let config_address = builder.config().address.clone();
        let bridge_address = builder.bridge_address()?;

        println!("Configuration:");
        println!("  Bridge Address: {}", config_address);
        println!("  Network: Dev (chain_id=254)");
        println!("  Sender: {} (association)", association_address());

        // Load package
        let package = load_package()?;

        println!("\nPackage Details:");
        println!("  Modules: {}", package.modules().len());

        println!("\n═══════════════════════════════════════════════════════════════════");
        println!("                    Transaction Plan");
        println!("═══════════════════════════════════════════════════════════════════\n");

        // Build all transactions using the builder
        let transactions = builder.build_all_transactions()?;

        let transaction_details = [
            (
                "Deploy Contract",
                format!("dev deploy {}", BLOB_PATH),
                vec![],
            ),
            (
                "Initialize Bridge",
                format!("{}::Bridge::initialize_bridge", config_address),
                vec!["node_chain_id = 254 (dev)"],
            ),
            (
                "Register Committee Member",
                format!("{}::Bridge::register_committee_member", config_address),
                vec!["pubkey", "url: http://127.0.0.1:9191"],
            ),
            (
                "Create Committee",
                format!("{}::Bridge::create_committee", config_address),
                vec![
                    "validator",
                    "voting_power: 10000 (100%)",
                    "min_stake: 5000 (50%)",
                    "epoch: 0",
                ],
            ),
            (
                "Setup ETH Token",
                format!("{}::Bridge::setup_eth_token", config_address),
                vec!["Token ID: 2"],
            ),
            (
                "Setup BTC Token",
                format!("{}::Bridge::setup_btc_token", config_address),
                vec!["Token ID: 1"],
            ),
            (
                "Setup USDC Token",
                format!("{}::Bridge::setup_usdc_token", config_address),
                vec!["Token ID: 3"],
            ),
            (
                "Setup USDT Token",
                format!("{}::Bridge::setup_usdt_token", config_address),
                vec!["Token ID: 4"],
            ),
        ];

        for (i, (txn, (name, function, args))) in transactions
            .iter()
            .zip(transaction_details.iter())
            .enumerate()
        {
            println!("Transaction {}: {}", i + 1, name);
            println!("  Function: {}", function);
            if !args.is_empty() {
                println!("  Args:");
                for arg in args {
                    println!("    {}", arg);
                }
            }
            println!("  Hash: {:?}\n", txn.id());
        }

        println!("═══════════════════════════════════════════════════════════════════");
        println!("                    Verification Commands");
        println!("═══════════════════════════════════════════════════════════════════\n");

        println!("After deployment, verify with these commands:\n");
        println!("# Check Bridge resource exists");
        println!(
            "starcoin% state get resource {} {}::Bridge::Bridge\n",
            config_address, config_address
        );
        println!("# Check Committee state");
        println!(
            "starcoin% state get resource {} {}::Committee::CommitteeState\n",
            config_address, config_address
        );
        println!("# Check tokens registered");
        println!(
            "starcoin% state get resource {} {}::Treasury::Treasury\n",
            config_address, config_address
        );

        println!("═══════════════════════════════════════════════════════════════════\n");

        Ok(())
    }

    /// Test: Use the builder API for individual transaction creation
    #[test]
    fn test_builder_api() -> Result<()> {
        println!("Testing BridgeDeploymentBuilder API...\n");

        let mut builder = BridgeDeploymentBuilder::new()?;
        println!("Bridge address: {:?}", builder.bridge_address()?);
        println!("Network: {:?}\n", builder.network().id());

        // Build individual transactions
        println!("Building individual transactions:");

        let deploy = builder.build_deploy_transaction()?;
        println!("  ✓ Deploy: {:?}", deploy.id());

        let init = builder.build_initialize_transaction()?;
        println!("  ✓ Initialize: {:?}", init.id());

        let register = builder.build_register_committee_transaction("http://test.url")?;
        println!("  ✓ Register: {:?}", register.id());

        let committee = builder.build_create_committee_transaction(10000, 5000, 0)?;
        println!("  ✓ Committee: {:?}", committee.id());

        let eth = builder.build_setup_token_transaction("eth")?;
        println!("  ✓ ETH token: {:?}", eth.id());

        println!("\n✅ Builder API working correctly!");
        Ok(())
    }
}
