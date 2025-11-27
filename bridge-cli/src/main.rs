// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use ethers::providers::Middleware;
use ethers::types::Address as EthAddress;
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::traits::ToFromBytes;
use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentMessage;
use starcoin_bridge::client::bridge_authority_aggregator::BridgeAuthorityAggregator;
use starcoin_bridge::crypto::{BridgeAuthorityPublicKey, BridgeAuthorityPublicKeyBytes};
use starcoin_bridge::eth_transaction_builder::build_eth_transaction;
use starcoin_bridge::metrics::BridgeMetrics;
use starcoin_bridge::starcoin_bridge_client::StarcoinBridgeClient;
use starcoin_bridge::starcoin_bridge_transaction_builder::build_starcoin_bridge_transaction;
use starcoin_bridge::types::BridgeActionType;
use starcoin_bridge::utils::{
    examine_key, generate_bridge_authority_key_and_write_to_file,
    generate_bridge_client_key_and_write_to_file, generate_bridge_node_config_and_write_to_file,
};
use starcoin_bridge::utils::{get_eth_contracts, EthBridgeContracts};
use starcoin_bridge_cli::{
    make_action, select_contract_address, Args, BridgeCliConfig, BridgeCommand,
    LoadedBridgeCliConfig, Network, SEPOLIA_BRIDGE_PROXY_ADDR,
};
use starcoin_bridge_config::Config;
use starcoin_bridge_types::crypto::Signature;
use starcoin_bridge_types::transaction::Transaction;
use starcoin_bridge_vm_types::bridge::base_types::StarcoinAddress;
use starcoin_bridge_vm_types::bridge::bridge::{
    BridgeChainId, MoveTypeCommitteeMember, MoveTypeCommitteeMemberRegistration,
};
use starcoin_bridge_vm_types::bridge::committee::TOTAL_VOTING_POWER;
use starcoin_bridge_vm_types::bridge::crypto::AuthorityPublicKeyBytes;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::str::from_utf8;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Init logging
    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();
    let args = Args::parse();

    match args.command {
        BridgeCommand::CreateBridgeValidatorKey { path } => {
            generate_bridge_authority_key_and_write_to_file(&path)?;
            println!("Bridge validator key generated at {}", path.display());
        }
        BridgeCommand::CreateBridgeClientKey { path, use_ecdsa } => {
            generate_bridge_client_key_and_write_to_file(&path, use_ecdsa)?;
            println!("Bridge client key generated at {}", path.display());
        }
        BridgeCommand::ExamineKey {
            path,
            is_validator_key,
        } => {
            examine_key(&path, is_validator_key)?;
        }
        BridgeCommand::CreateBridgeNodeConfigTemplate { path, run_client } => {
            generate_bridge_node_config_and_write_to_file(&path, run_client)?;
            println!(
                "Bridge node config template generated at {}",
                path.display()
            );
        }

        BridgeCommand::Governance {
            config_path,
            chain_id,
            cmd,
            dry_run,
        } => {
            let chain_id = BridgeChainId::try_from(chain_id).expect("Invalid chain id");
            println!("Chain ID: {:?}", chain_id);
            let config = BridgeCliConfig::load(config_path).expect("Couldn't load BridgeCliConfig");
            let config = LoadedBridgeCliConfig::load(config).await?;
            let metrics = Arc::new(BridgeMetrics::new_for_testing());
            let starcoin_bridge_client = StarcoinBridgeClient::with_metrics(
                &config.starcoin_bridge_rpc_url,
                &config.starcoin_bridge_proxy_address,
                metrics.clone(),
            );

            let (starcoin_bridge_key, starcoin_bridge_address, gas_object_ref) = config
                .get_starcoin_bridge_account_info()
                .await
                .expect("Failed to get starcoin account info");
            let bridge_summary = starcoin_bridge_client
                .get_bridge_summary()
                .await
                .expect("Failed to get bridge summary");
            let bridge_committee = Arc::new(
                starcoin_bridge_client
                    .get_bridge_committee()
                    .await
                    .expect("Failed to get bridge committee"),
            );
            let agg = BridgeAuthorityAggregator::new(
                bridge_committee,
                metrics,
                Arc::new(BTreeMap::new()),
            );

            // Handle Starcoin Side
            if chain_id.is_starcoin_bridge_chain() {
                let starcoin_bridge_chain_id =
                    BridgeChainId::try_from(bridge_summary.chain_id).unwrap();
                assert_eq!(
                    starcoin_bridge_chain_id, chain_id,
                    "Chain ID mismatch, expected: {:?}, got from url: {:?}",
                    chain_id, starcoin_bridge_chain_id
                );
                // Create BridgeAction
                let starcoin_bridge_action = make_action(starcoin_bridge_chain_id, &cmd);
                println!(
                    "Action to execute on Starcoin: {:?}",
                    starcoin_bridge_action
                );
                let certified_action = agg
                    .request_committee_signatures(starcoin_bridge_action)
                    .await
                    .expect("Failed to request committee signatures");
                if dry_run {
                    println!("Dryrun succeeded.");
                    return Ok(());
                }
                let bridge_arg = starcoin_bridge_client
                    .get_mutable_bridge_object_arg_must_succeed()
                    .await;
                let rgp = starcoin_bridge_client
                    .get_reference_gas_price_until_success()
                    .await;
                let id_token_map = starcoin_bridge_client.get_token_id_map().await.unwrap();
                let tx = build_starcoin_bridge_transaction(
                    starcoin_bridge_address,
                    &gas_object_ref,
                    certified_action,
                    bridge_arg,
                    &id_token_map,
                    rgp,
                )
                .expect("Failed to build starcoin transaction");
                let starcoin_bridge_sig = Signature::new_secure(
                    &IntentMessage::new(Intent::starcoin_bridge_transaction(), tx.clone()),
                    &starcoin_bridge_key,
                );
                let tx = Transaction::from_data(tx, vec![starcoin_bridge_sig]);
                let resp = starcoin_bridge_client
                    .execute_transaction_block_with_effects(tx)
                    .await
                    .expect("Failed to execute transaction block with effects");
                if resp.status_ok().unwrap() {
                    println!("Starcoin Transaction succeeded: {:?}", resp.digest);
                } else {
                    println!(
                        "Starcoin Transaction failed: {:?}. Effects: {:?}",
                        resp.digest, resp.effects
                    );
                }
                return Ok(());
            }

            // Handle eth side
            // TODO assert chain id returned from rpc matches chain_id
            let eth_signer_client = config.eth_signer();
            // Create BridgeAction
            let eth_action = make_action(chain_id, &cmd);
            println!("Action to execute on Eth: {:?}", eth_action);
            // Create Eth Signer Client
            // TODO if a validator is blocklisted on eth, ignore their signatures?
            let certified_action = agg
                .request_committee_signatures(eth_action)
                .await
                .expect("Failed to request committee signatures");
            if dry_run {
                println!("Dryrun succeeded.");
                return Ok(());
            }
            let contract_address = select_contract_address(&config, &cmd);
            let tx = build_eth_transaction(
                contract_address,
                eth_signer_client.clone(),
                certified_action,
            )
            .await
            .expect("Failed to build eth transaction");
            println!("sending Eth tx: {:?}", tx);
            match tx.send().await {
                Ok(tx_hash) => {
                    println!("Transaction sent with hash: {:?}", tx_hash);
                }
                Err(err) => {
                    let revert = err.as_revert();
                    println!("Transaction reverted: {:?}", revert);
                }
            };

            return Ok(());
        }

        BridgeCommand::ViewEthBridge {
            network,
            bridge_proxy,
            eth_rpc_url,
        } => {
            let bridge_proxy = match network {
                Some(Network::Testnet) => {
                    Ok(EthAddress::from_str(SEPOLIA_BRIDGE_PROXY_ADDR).unwrap())
                }
                None => bridge_proxy.ok_or(anyhow::anyhow!(
                    "Network or bridge proxy address must be provided"
                )),
            }?;
            let provider = Arc::new(
                ethers::prelude::Provider::<ethers::providers::Http>::try_from(eth_rpc_url)
                    .unwrap()
                    .interval(std::time::Duration::from_millis(2000)),
            );
            let chain_id = provider.get_chainid().await?;
            let EthBridgeContracts {
                bridge,
                committee,
                limiter,
                vault,
                config,
            } = get_eth_contracts(bridge_proxy, &provider).await?;
            let message_type = BridgeActionType::EvmContractUpgrade as u8;
            let bridge_upgrade_next_nonce: u64 = bridge.nonces(message_type).call().await?;
            let committee_upgrade_next_nonce: u64 = committee.nonces(message_type).call().await?;
            let limiter_upgrade_next_nonce: u64 = limiter.nonces(message_type).call().await?;
            let config_upgrade_next_nonce: u64 = config.nonces(message_type).call().await?;

            let token_transfer_next_nonce: u64 = bridge
                .nonces(BridgeActionType::TokenTransfer as u8)
                .call()
                .await?;
            let blocklist_update_nonce: u64 = committee
                .nonces(BridgeActionType::UpdateCommitteeBlocklist as u8)
                .call()
                .await?;
            let emergency_button_nonce: u64 = bridge
                .nonces(BridgeActionType::EmergencyButton as u8)
                .call()
                .await?;
            let limit_update_nonce: u64 = limiter
                .nonces(BridgeActionType::LimitUpdate as u8)
                .call()
                .await?;
            let asset_price_update_nonce: u64 = config
                .nonces(BridgeActionType::AssetPriceUpdate as u8)
                .call()
                .await?;
            let add_tokens_nonce: u64 = config
                .nonces(BridgeActionType::AddTokensOnEvm as u8)
                .call()
                .await?;

            let print = OutputEthBridge {
                chain_id: chain_id.as_u64(),
                bridge_proxy: bridge.address(),
                committee_proxy: committee.address(),
                limiter_proxy: limiter.address(),
                config_proxy: config.address(),
                vault: vault.address(),
                nonces: Nonces {
                    token_transfer: token_transfer_next_nonce,
                    blocklist_update: blocklist_update_nonce,
                    emergency_button: emergency_button_nonce,
                    limit_update: limit_update_nonce,
                    asset_price_update: asset_price_update_nonce,
                    add_evm_tokens: add_tokens_nonce,
                    contract_upgrade_bridge: bridge_upgrade_next_nonce,
                    contract_upgrade_committee: committee_upgrade_next_nonce,
                    contract_upgrade_limiter: limiter_upgrade_next_nonce,
                    contract_upgrade_config: config_upgrade_next_nonce,
                },
            };
            println!("{}", serde_json::to_string_pretty(&print).unwrap());
            return Ok(());
        }

        BridgeCommand::ViewBridgeRegistration {
            starcoin_bridge_rpc_url,
            starcoin_bridge_proxy_address,
        } => {
            let metrics = Arc::new(BridgeMetrics::new_for_testing());
            let starcoin_bridge_client =
                StarcoinBridgeClient::with_metrics(&starcoin_bridge_rpc_url, &starcoin_bridge_proxy_address, metrics);
            let bridge_summary = starcoin_bridge_client
                .get_bridge_summary()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get bridge summary: {:?}", e))?;
            let move_type_bridge_committee = bridge_summary.committee;
            
            // TODO: The stake and name lookups require Starcoin-specific APIs
            // For now, create empty maps as placeholders
            let stakes: HashMap<StarcoinAddress, u64> = HashMap::new();
            let names: HashMap<StarcoinAddress, String> = move_type_bridge_committee
                .member_registration
                .iter()
                .map(|(addr, entry)| {
                    (
                        *addr,
                        String::from_utf8_lossy(&entry.http_rest_url).to_string(),
                    )
                })
                .collect();

            let mut authorities = vec![];
            let mut output_wrapper = Output::<OutputStarcoinBridgeRegistration>::default();
            for (_, member) in move_type_bridge_committee.member_registration {
                let MoveTypeCommitteeMemberRegistration {
                    starcoin_bridge_address,
                    bridge_pubkey_bytes,
                    http_rest_url,
                } = member;
                let Ok(pubkey) = BridgeAuthorityPublicKey::from_bytes(&bridge_pubkey_bytes) else {
                    output_wrapper.add_error(format!(
                        "Invalid bridge pubkey for validator {}: {:?}",
                        starcoin_bridge_address, bridge_pubkey_bytes
                    ));
                    continue;
                };
                let eth_address = BridgeAuthorityPublicKeyBytes::from(&pubkey).to_eth_address();
                let Ok(url) = from_utf8(&http_rest_url) else {
                    output_wrapper.add_error(format!(
                        "Invalid bridge http url for validator: {}: {:?}",
                        starcoin_bridge_address, http_rest_url
                    ));
                    continue;
                };
                let url = url.to_string();

                // Get name from names map, or use URL as fallback
                let name = names
                    .get(&starcoin_bridge_address)
                    .cloned()
                    .unwrap_or_else(|| url.clone());
                let stake = stakes.get(&starcoin_bridge_address).copied().unwrap_or(0);
                authorities.push((
                    name,
                    starcoin_bridge_address,
                    pubkey,
                    eth_address,
                    url,
                    stake,
                ));
            }
            let total_stake = authorities
                .iter()
                .map(|(_, _, _, _, _, stake)| *stake)
                .sum::<u64>();
            let mut output = OutputStarcoinBridgeRegistration {
                total_registered_stake: total_stake as f32 / TOTAL_VOTING_POWER as f32 * 100.0,
                ..Default::default()
            };
            for (name, starcoin_bridge_address, pubkey, eth_address, url, stake) in authorities {
                output.committee.push(OutputMember {
                    name: name.clone(),
                    starcoin_bridge_address,
                    eth_address,
                    pubkey: Hex::encode(pubkey.as_bytes()),
                    url,
                    stake,
                    blocklisted: None,
                    status: None,
                });
            }
            output_wrapper.inner = output;
            println!("{}", serde_json::to_string_pretty(&output_wrapper).unwrap());
        }

        BridgeCommand::ViewStarcoinBridge {
            starcoin_bridge_rpc_url,
            starcoin_bridge_proxy_address,
            hex,
            ping,
        } => {
            let metrics = Arc::new(BridgeMetrics::new_for_testing());
            let starcoin_bridge_client =
                StarcoinBridgeClient::with_metrics(&starcoin_bridge_rpc_url, &starcoin_bridge_proxy_address, metrics);
            let bridge_summary = starcoin_bridge_client
                .get_bridge_summary()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get bridge summary: {:?}", e))?;
            let move_type_bridge_committee = bridge_summary.committee;

            // TODO: Name lookups require Starcoin-specific validator APIs
            // For now, create empty map as placeholder
            let names: HashMap<StarcoinAddress, (AuthorityPublicKeyBytes, String)> = HashMap::new();

            let mut authorities = vec![];
            let mut ping_tasks = vec![];
            let client = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap();
            let mut output_wrapper = Output::<OutputStarcoinBridge>::default();
            for (_, member) in move_type_bridge_committee.members {
                let MoveTypeCommitteeMember {
                    starcoin_bridge_address,
                    bridge_pubkey_bytes,
                    voting_power,
                    http_rest_url,
                    blocklisted,
                } = member;
                let Ok(pubkey) = BridgeAuthorityPublicKey::from_bytes(&bridge_pubkey_bytes) else {
                    output_wrapper.add_error(format!(
                        "Invalid bridge pubkey for validator {}: {:?}",
                        starcoin_bridge_address, bridge_pubkey_bytes
                    ));
                    continue;
                };
                let eth_address = BridgeAuthorityPublicKeyBytes::from(&pubkey).to_eth_address();
                let Ok(url) = from_utf8(&http_rest_url) else {
                    output_wrapper.add_error(format!(
                        "Invalid bridge http url for validator: {}: {:?}",
                        starcoin_bridge_address, http_rest_url
                    ));
                    continue;
                };
                let url = url.to_string();

                // Use the address directly since names is HashMap<StarcoinAddress, ...>
                let name = if let Some((_, n)) = names.get(&starcoin_bridge_address) {
                    n.clone()
                } else {
                    url.clone()
                };

                if ping {
                    let client_clone = client.clone();
                    ping_tasks.push(client_clone.get(url.clone()).send());
                }
                authorities.push((
                    name,
                    starcoin_bridge_address,
                    pubkey,
                    eth_address,
                    url,
                    voting_power,
                    blocklisted,
                ));
            }
            let total_stake = authorities
                .iter()
                .map(|(_, _, _, _, _, stake, _)| *stake)
                .sum::<u64>();
            let mut output = OutputStarcoinBridge {
                total_stake: total_stake as f32 / TOTAL_VOTING_POWER as f32 * 100.0,
                ..Default::default()
            };
            let ping_tasks_resp = if !ping_tasks.is_empty() {
                futures::future::join_all(ping_tasks)
                    .await
                    .into_iter()
                    .map(|resp| {
                        Some(match resp {
                            Ok(resp) => resp.status().is_success(),
                            Err(_e) => false,
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![None; authorities.len()]
            };
            let mut total_online_stake = 0;
            for (
                (name, starcoin_bridge_address, pubkey, eth_address, url, stake, blocklisted),
                ping_resp,
            ) in authorities.into_iter().zip(ping_tasks_resp)
            {
                let pubkey = if hex {
                    Hex::encode(pubkey.as_bytes())
                } else {
                    pubkey.to_string()
                };
                match ping_resp {
                    Some(resp) => {
                        if resp {
                            total_online_stake += stake;
                        }
                        output.committee.push(OutputMember {
                            name: name.clone(),
                            starcoin_bridge_address,
                            eth_address,
                            pubkey,
                            url,
                            stake,
                            blocklisted: Some(blocklisted),
                            status: Some(if resp {
                                "online".to_string()
                            } else {
                                "offline".to_string()
                            }),
                        });
                    }
                    None => {
                        output.committee.push(OutputMember {
                            name: name.clone(),
                            starcoin_bridge_address,
                            eth_address,
                            pubkey,
                            url,
                            stake,
                            blocklisted: Some(blocklisted),
                            status: None,
                        });
                    }
                }
            }
            if ping {
                output.total_online_stake =
                    Some(total_online_stake as f32 / TOTAL_VOTING_POWER as f32 * 100.0);
            }

            // sequence nonces
            for (type_, nonce) in bridge_summary.sequence_nums {
                output
                    .nonces
                    .insert(BridgeActionType::try_from(type_).unwrap(), nonce);
            }

            output_wrapper.inner = output;
            println!("{}", serde_json::to_string_pretty(&output_wrapper).unwrap());
        }
        BridgeCommand::Client { config_path, cmd } => {
            let config = BridgeCliConfig::load(config_path).expect("Couldn't load BridgeCliConfig");
            let config = LoadedBridgeCliConfig::load(config).await?;
            let metrics = Arc::new(BridgeMetrics::new_for_testing());
            let starcoin_bridge_client =
                StarcoinBridgeClient::with_metrics(&config.starcoin_bridge_rpc_url, &config.starcoin_bridge_proxy_address, metrics);
            cmd.handle(&config, starcoin_bridge_client).await?;
            return Ok(());
        }
    }

    Ok(())
}

#[derive(serde::Serialize, Default)]
struct OutputEthBridge {
    chain_id: u64,
    bridge_proxy: EthAddress,
    committee_proxy: EthAddress,
    limiter_proxy: EthAddress,
    config_proxy: EthAddress,
    vault: EthAddress,
    nonces: Nonces,
}

#[derive(serde::Serialize, Default)]
struct Nonces {
    token_transfer: u64,
    blocklist_update: u64,
    emergency_button: u64,
    limit_update: u64,
    asset_price_update: u64,
    add_evm_tokens: u64,
    contract_upgrade_bridge: u64,
    contract_upgrade_committee: u64,
    contract_upgrade_limiter: u64,
    contract_upgrade_config: u64,
}

#[derive(serde::Serialize, Default)]
struct Output<P: Default> {
    #[serde(skip_serializing_if = "Option::is_none")]
    errors: Option<Vec<String>>,
    inner: P,
}

impl<P: Default> Output<P> {
    fn add_error(&mut self, error: String) {
        if self.errors.is_none() {
            self.errors = Some(vec![]);
        }
        self.errors.as_mut().unwrap().push(error);
    }
}

#[derive(serde::Serialize, Default)]
struct OutputStarcoinBridge {
    total_stake: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_online_stake: Option<f32>,
    committee: Vec<OutputMember>,
    nonces: HashMap<BridgeActionType, u64>,
}

#[derive(serde::Serialize)]
struct OutputMember {
    name: String,
    starcoin_bridge_address: StarcoinAddress,
    eth_address: EthAddress,
    pubkey: String,
    url: String,
    stake: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocklisted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
}

#[derive(serde::Serialize, Default)]
struct OutputStarcoinBridgeRegistration {
    total_registered_stake: f32,
    committee: Vec<OutputMember>,
}
