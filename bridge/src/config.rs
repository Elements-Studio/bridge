// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::EthBridgeConfig;
use crate::crypto::BridgeAuthorityKeyPair;
use crate::error::BridgeError;
use crate::eth_client::EthClient;
use crate::metered_eth_provider::new_metered_eth_provider;
use crate::metered_eth_provider::MeteredEthHttpProvier;
use crate::metrics::BridgeMetrics;
use crate::starcoin_bridge_client::StarcoinClient;
use crate::types::{is_route_valid, BridgeAction};
use crate::utils::get_eth_contract_addresses;
use anyhow::anyhow;
use ethers::providers::Middleware;
use ethers::types::Address as EthAddress;
use fastcrypto::ed25519::Ed25519KeyPair;
use fastcrypto::traits::KeyPair;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use starcoin_bridge_config::Config;
use starcoin_bridge_json_rpc_types::Coin;
use starcoin_bridge_keys::keypair_file::read_key;
use starcoin_bridge_sdk::apis::CoinReadApi;
use starcoin_bridge_sdk::{StarcoinClient as StarcoinSdkClient, StarcoinClientBuilder};
use starcoin_bridge_types::base_types::{ObjectID, ObjectRef, StarcoinAddress};
use starcoin_bridge_types::bridge::BridgeChainId;
use starcoin_bridge_types::crypto::{NetworkKeyPair, StarcoinKeyPair};
use starcoin_bridge_types::digests::{get_mainnet_chain_identifier, get_testnet_chain_identifier};
use starcoin_bridge_types::event::EventID;
use starcoin_bridge_types::object::Owner;
use tracing::info;

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EthConfig {
    // Rpc url for Eth fullnode, used for query stuff.
    pub eth_rpc_url: String,
    // The proxy address of StarcoinBridge
    pub eth_bridge_proxy_address: String,
    // The expected BridgeChainId on Eth side.
    pub eth_bridge_chain_id: u8,
    // The starting block for EthSyncer to monitor eth contracts.
    // It is required when `run_client` is true. Usually this is
    // the block number when the bridge contracts are deployed.
    // When BridgeNode starts, it reads the contract watermark from storage.
    // If the watermark is not found, it will start from this fallback block number.
    // If the watermark is found, it will start from the watermark.
    // this v.s.`eth_contracts_start_block_override`:
    pub eth_contracts_start_block_fallback: Option<u64>,
    // The starting block for EthSyncer to monitor eth contracts. It overrides
    // the watermark in storage. This is useful when we want to reprocess the events
    // from a specific block number.
    // Note: this field has to be reset after starting the BridgeNode, otherwise it will
    // reprocess the events from this block number every time it starts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eth_contracts_start_block_override: Option<u64>,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StarcoinConfig {
    // Rpc url for Starcoin fullnode, used for query stuff and submit transactions.
    pub starcoin_bridge_rpc_url: String,
    // The expected BridgeChainId on Starcoin side.
    pub starcoin_bridge_chain_id: u8,
    // Path of the file where bridge client key (any StarcoinKeyPair) is stored.
    // If `run_client` is true, and this is None, then use `bridge_authority_key_path` as client key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge_client_key_path: Option<PathBuf>,
    // The gas object to use for paying for gas fees for the client. It needs to
    // be owned by the address associated with bridge client key. If not set
    // and `run_client` is true, it will query and use the gas object with highest
    // amount for the account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge_client_gas_object: Option<ObjectID>,
    // Override the last processed EventID for bridge module `bridge`.
    // When set, StarcoinSyncer will start from this cursor (exclusively) instead of the one in storage.
    // If the cursor is not found in storage or override, the query will start from genesis.
    // Key: starcoin module, Value: last processed EventID (tx_digest, event_seq).
    // Note 1: This field should be rarely used. Only use it when you understand how to follow up.
    // Note 2: the EventID needs to be valid, namely it must exist and matches the filter.
    // Otherwise, it will miss one event because of fullnode Event query semantics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starcoin_bridge_module_last_processed_event_id_override: Option<EventID>,
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct BridgeNodeConfig {
    // The port that the server listens on.
    pub server_listen_port: u16,
    // The port that for metrics server.
    pub metrics_port: u16,
    // Path of the file where bridge authority key (Secp256k1) is stored.
    pub bridge_authority_key_path: PathBuf,
    // Whether to run client. If true, `starcoin.bridge_client_key_path`
    // and `db_path` needs to be provided.
    pub run_client: bool,
    // Path of the client storage. Required when `run_client` is true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_path: Option<PathBuf>,
    // A list of approved governance actions. Action in this list will be signed when requested by client.
    pub approved_governance_actions: Vec<BridgeAction>,
    // Starcoin configuration
    pub starcoin: StarcoinConfig,
    // Eth configuration
    pub eth: EthConfig,
    // Network key used for metrics pushing
    #[serde(default = "default_ed25519_key_pair")]
    pub metrics_key_pair: NetworkKeyPair,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<MetricsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub watchdog_config: Option<WatchdogConfig>,
}

pub fn default_ed25519_key_pair() -> NetworkKeyPair {
    use fastcrypto::traits::ToFromBytes;
    // Use a fixed test key - in production this should be generated securely
    let test_key_bytes: [u8; 32] = [0; 32]; // Fixed seed for testing
    Ed25519KeyPair::from_bytes(&test_key_bytes).expect("Failed to create default Ed25519 keypair")
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct MetricsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_interval_seconds: Option<u64>,
    pub push_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct WatchdogConfig {
    // Total supplies to watch on Starcoin. Mapping from coin name to coin type tag
    pub total_supplies: BTreeMap<String, String>,
}

impl Config for BridgeNodeConfig {}

impl BridgeNodeConfig {
    pub async fn validate(
        &self,
        metrics: Arc<BridgeMetrics>,
    ) -> anyhow::Result<(BridgeServerConfig, Option<BridgeClientConfig>)> {
        info!("Starting config validation");
        if !is_route_valid(
            BridgeChainId::try_from(self.starcoin.starcoin_bridge_chain_id)?,
            BridgeChainId::try_from(self.eth.eth_bridge_chain_id)?,
        ) {
            return Err(anyhow!(
                "Route between Starcoin chain id {} and Eth chain id {} is not valid",
                self.starcoin.starcoin_bridge_chain_id,
                self.eth.eth_bridge_chain_id,
            ));
        };

        // Load bridge authority key from file
        // The key must be a Secp256k1 key for bridge operations
        let bridge_authority_key = match read_key(&self.bridge_authority_key_path, true) {
            Ok(StarcoinKeyPair::Secp256k1(key)) => {
                info!(
                    "Successfully loaded Secp256k1 bridge authority key from {:?}",
                    self.bridge_authority_key_path
                );
                key
            }
            Ok(_) => {
                return Err(anyhow!(
                    "Bridge authority key at {:?} is not a Secp256k1 key. \
                    Bridge requires Secp256k1 keys for compatibility with Ethereum signatures.",
                    self.bridge_authority_key_path
                ));
            }
            Err(e) => {
                return Err(anyhow!(
                    "Failed to read bridge authority key from {:?}: {}. \
                    Please ensure the key file exists and contains a valid Base64-encoded Secp256k1 private key. \
                    You can generate a new key using: starcoin-bridge-keys generate --output <path>",
                    self.bridge_authority_key_path,
                    e
                ));
            }
        };

        // we do this check here instead of `prepare_for_starcoin` below because
        // that is only called when `run_client` is true.
        let starcoin_bridge_client =
            Arc::new(StarcoinClient::<StarcoinSdkClient>::new(&self.starcoin.starcoin_bridge_rpc_url, metrics.clone()).await?);
        
        // Validate that bridge authority key is part of the committee
        let bridge_committee = starcoin_bridge_client
            .get_bridge_committee()
            .await
            .map_err(|e| anyhow!("Error getting bridge committee: {:?}", e))?;
        if !bridge_committee.is_active_member(&bridge_authority_key.public().into()) {
            return Err(anyhow!(
                "Bridge authority key is not part of bridge committee"
            ));
        }
        tracing::info!("Bridge committee validation passed");

        let (eth_client, eth_contracts) = self.prepare_for_eth(metrics.clone()).await?;
        let bridge_summary = starcoin_bridge_client
            .get_bridge_summary()
            .await
            .map_err(|e| anyhow!("Error getting bridge summary: {:?}", e))?;
        if bridge_summary.chain_id != self.starcoin.starcoin_bridge_chain_id {
            anyhow::bail!(
                "Bridge chain id mismatch: expected {}, but connected to {}",
                self.starcoin.starcoin_bridge_chain_id,
                bridge_summary.chain_id
            );
        }

        // Validate approved actions that must be governace actions
        for action in &self.approved_governance_actions {
            if !action.is_governace_action() {
                anyhow::bail!(format!(
                    "{:?}",
                    BridgeError::ActionIsNotGovernanceAction(action.clone())
                ));
            }
        }
        let approved_governance_actions = self.approved_governance_actions.clone();

        let bridge_server_config = BridgeServerConfig {
            key: bridge_authority_key,
            metrics_port: self.metrics_port,
            eth_bridge_proxy_address: eth_contracts[0], // the first contract is bridge proxy
            server_listen_port: self.server_listen_port,
            starcoin_bridge_client: starcoin_bridge_client.clone(),
            eth_client: eth_client.clone(),
            approved_governance_actions,
        };
        if !self.run_client {
            return Ok((bridge_server_config, None));
        }

        // If client is enabled, prepare client config
        let (bridge_client_key, client_starcoin_bridge_address, gas_object_ref) =
            self.prepare_for_starcoin(starcoin_bridge_client.clone(), metrics).await?;

        let db_path = self
            .db_path
            .clone()
            .ok_or(anyhow!("`db_path` is required when `run_client` is true"))?;

        let bridge_client_config = BridgeClientConfig {
            starcoin_bridge_address: client_starcoin_bridge_address,
            key: bridge_client_key,
            gas_object_ref,
            metrics_port: self.metrics_port,
            starcoin_bridge_client: starcoin_bridge_client.clone(),
            eth_client: eth_client.clone(),
            db_path,
            eth_contracts,
            // in `prepare_for_eth` we check if this is None when `run_client` is true. Safe to unwrap here.
            eth_contracts_start_block_fallback: self
                .eth
                .eth_contracts_start_block_fallback
                .unwrap(),
            eth_contracts_start_block_override: self.eth.eth_contracts_start_block_override,
            starcoin_bridge_module_last_processed_event_id_override: self
                .starcoin
                .starcoin_bridge_module_last_processed_event_id_override,
        };

        info!("Config validation complete");
        Ok((bridge_server_config, Some(bridge_client_config)))
    }

    async fn prepare_for_eth(
        &self,
        metrics: Arc<BridgeMetrics>,
    ) -> anyhow::Result<(Arc<EthClient<MeteredEthHttpProvier>>, Vec<EthAddress>)> {
        info!("Creating Ethereum client provider");
        let bridge_proxy_address = EthAddress::from_str(&self.eth.eth_bridge_proxy_address)?;
        let provider = Arc::new(
            new_metered_eth_provider(&self.eth.eth_rpc_url, metrics.clone())
                .unwrap()
                .interval(std::time::Duration::from_millis(2000)),
        );
        let chain_id = provider.get_chainid().await?;
        let (
            committee_address,
            limiter_address,
            vault_address,
            config_address,
            _weth_address,
            _usdt_address,
            _wbtc_address,
            _lbtc_address,
        ) = get_eth_contract_addresses(bridge_proxy_address, &provider).await?;
        let config = EthBridgeConfig::new(config_address, provider.clone());

        if self.run_client && self.eth.eth_contracts_start_block_fallback.is_none() {
            return Err(anyhow!(
                "eth_contracts_start_block_fallback is required when run_client is true"
            ));
        }

        // If bridge chain id is Eth Mainent or Sepolia, we expect to see chain
        // identifier to match accordingly.
        let bridge_chain_id: u8 = config.chain_id().call().await?;
        if self.eth.eth_bridge_chain_id != bridge_chain_id {
            return Err(anyhow!(
                "Bridge chain id mismatch: expected {}, but connected to {}",
                self.eth.eth_bridge_chain_id,
                bridge_chain_id
            ));
        }
        if bridge_chain_id == BridgeChainId::EthMainnet as u8 && chain_id.as_u64() != 1 {
            anyhow::bail!(
                "Expected Eth chain id 1, but connected to {}",
                chain_id.as_u64()
            );
        }
        if bridge_chain_id == BridgeChainId::EthSepolia as u8 && chain_id.as_u64() != 11155111 {
            anyhow::bail!(
                "Expected Eth chain id 11155111, but connected to {}",
                chain_id.as_u64()
            );
        }
        info!(
            "Connected to Eth chain: {}, Bridge chain id: {}",
            chain_id.as_u64(),
            bridge_chain_id,
        );

        let eth_client = Arc::new(
            EthClient::<MeteredEthHttpProvier>::new(
                &self.eth.eth_rpc_url,
                HashSet::from_iter(vec![
                    bridge_proxy_address,
                    committee_address,
                    config_address,
                    limiter_address,
                    vault_address,
                ]),
                metrics,
            )
            .await?,
        );
        let contract_addresses = vec![
            bridge_proxy_address,
            committee_address,
            config_address,
            limiter_address,
            vault_address,
        ];
        info!("Ethereum client setup complete");
        Ok((eth_client, contract_addresses))
    }

    async fn prepare_for_starcoin(
        &self,
        starcoin_bridge_client: Arc<StarcoinClient<StarcoinSdkClient>>,
        metrics: Arc<BridgeMetrics>,
    ) -> anyhow::Result<(StarcoinKeyPair, StarcoinAddress, ObjectRef)> {
        let bridge_client_key = match &self.starcoin.bridge_client_key_path {
            None => read_key(&self.bridge_authority_key_path, true),
            Some(path) => read_key(path, false),
        }?;

        // If bridge chain id is Starcoin Mainent or Testnet, we expect to see chain
        // identifier to match accordingly.
        let starcoin_bridge_identifier = starcoin_bridge_client
            .get_chain_identifier()
            .await
            .map_err(|e| anyhow!("Error getting chain identifier from Starcoin: {:?}", e))?;
        if self.starcoin.starcoin_bridge_chain_id == BridgeChainId::StarcoinMainnet as u8
            && starcoin_bridge_identifier != get_mainnet_chain_identifier().to_string()
        {
            anyhow::bail!(
                "Expected starcoin chain identifier {}, but connected to {}",
                self.starcoin.starcoin_bridge_chain_id,
                starcoin_bridge_identifier
            );
        }
        if self.starcoin.starcoin_bridge_chain_id == BridgeChainId::StarcoinTestnet as u8
            && starcoin_bridge_identifier != get_testnet_chain_identifier().to_string()
        {
            anyhow::bail!(
                "Expected starcoin chain identifier {}, but connected to {}",
                self.starcoin.starcoin_bridge_chain_id,
                starcoin_bridge_identifier
            );
        }
        info!(
            "Connected to Starcoin chain: {}, Bridge chain id: {}",
            starcoin_bridge_identifier, self.starcoin.starcoin_bridge_chain_id,
        );

        let public_bytes = bridge_client_key.public();
        let mut addr_bytes = [0u8; 32];
        addr_bytes[..public_bytes.len().min(32)]
            .copy_from_slice(&public_bytes[..public_bytes.len().min(32)]);
        let client_starcoin_bridge_address = starcoin_bridge_types::base_types::starcoin_bridge_address_from_bytes(addr_bytes);

        let gas_object_id = match self.starcoin.bridge_client_gas_object {
            Some(id) => id,
            None => {
                info!("No gas object configured, finding gas object with highest balance");
                let starcoin_bridge_client = StarcoinClientBuilder::default()
                    .url(&self.starcoin.starcoin_bridge_rpc_url)
                    .build()?;
                let coin =
                    // Minimum balance for gas object is 10 STARCOIN
                    pick_highest_balance_coin(&starcoin_bridge_client.coin_read_api(), client_starcoin_bridge_address, 10_000_000_000)
                        .await?;
                coin.coin_object_id
            }
        };
        let (gas_coin, gas_object_ref, owner) = starcoin_bridge_client
            .get_gas_data_panic_if_not_gas(gas_object_id)
            .await;
        if owner != Owner::AddressOwner(client_starcoin_bridge_address) {
            return Err(anyhow!("Gas object {:?} is not owned by bridge client key's associated starcoin address {:?}, but {:?}", gas_object_id, client_starcoin_bridge_address, owner));
        }
        let balance = gas_coin.value();
        info!("Gas object balance: {}", balance);
        metrics.gas_coin_balance.set(balance as i64);

        info!("Starcoin client setup complete");
        Ok((bridge_client_key, client_starcoin_bridge_address, gas_object_ref))
    }
}

pub struct BridgeServerConfig {
    pub key: BridgeAuthorityKeyPair,
    pub server_listen_port: u16,
    pub eth_bridge_proxy_address: EthAddress,
    pub metrics_port: u16,
    pub starcoin_bridge_client: Arc<StarcoinClient<StarcoinSdkClient>>,
    pub eth_client: Arc<EthClient<MeteredEthHttpProvier>>,
    // A list of approved governance actions. Action in this list will be signed when requested by client.
    pub approved_governance_actions: Vec<BridgeAction>,
}

pub struct BridgeClientConfig {
    pub starcoin_bridge_address: StarcoinAddress,
    pub key: StarcoinKeyPair,
    pub gas_object_ref: ObjectRef,
    pub metrics_port: u16,
    pub starcoin_bridge_client: Arc<StarcoinClient<StarcoinSdkClient>>,
    pub eth_client: Arc<EthClient<MeteredEthHttpProvier>>,
    pub db_path: PathBuf,
    pub eth_contracts: Vec<EthAddress>,
    // See `BridgeNodeConfig` for the explanation of following two fields.
    pub eth_contracts_start_block_fallback: u64,
    pub eth_contracts_start_block_override: Option<u64>,
    pub starcoin_bridge_module_last_processed_event_id_override: Option<EventID>,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct BridgeCommitteeConfig {
    pub bridge_authority_port_and_key_path: Vec<(u64, PathBuf)>,
}

impl Config for BridgeCommitteeConfig {}

pub async fn pick_highest_balance_coin(
    coin_read_api: &CoinReadApi,
    address: StarcoinAddress,
    minimal_amount: u64,
) -> anyhow::Result<Coin> {
    info!("Looking for a suitable gas coin for address {:?}", address);

    let address_bytes = starcoin_bridge_types::base_types::starcoin_bridge_address_to_bytes(address);
    // Only look at STARCOIN coins specifically
    let mut stream = coin_read_api
        .get_coins_stream(address_bytes, Some("0x2::starcoin::STARCOIN".to_string()))
        .boxed();

    let mut coins_checked = 0;

    while let Some(coin_result) = stream.next().await {
        let coin = coin_result?; // Unwrap the Result
        info!(
            "Checking coin: {:?}, balance: {}",
            coin.coin_object_id, coin.balance
        );
        coins_checked += 1;

        // Take the first coin with a sufficient balance
        if coin.balance >= minimal_amount {
            info!(
                "Found suitable gas coin with {} mist (object ID: {:?})",
                coin.balance, coin.coin_object_id
            );
            return Ok(coin);
        }

        // Only check a small number of coins before giving up
        if coins_checked >= 1000 {
            break;
        }
    }

    Err(anyhow!(
        "No suitable gas coin with >= {} mist found for address {:?} after checking {} coins",
        minimal_amount,
        address,
        coins_checked
    ))
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct EthContractAddresses {
    pub starcoin_bridge: EthAddress,
    pub bridge_committee: EthAddress,
    pub bridge_config: EthAddress,
    pub bridge_limiter: EthAddress,
    pub bridge_vault: EthAddress,
}
