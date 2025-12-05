// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::WatchdogConfig;
use crate::crypto::BridgeAuthorityPublicKeyBytes;
use crate::metered_eth_provider::MeteredEthHttpProvier;
use crate::starcoin_bridge_client::StarcoinBridgeClient;
use crate::starcoin_bridge_watchdog::eth_bridge_status::EthBridgeStatus;
use crate::starcoin_bridge_watchdog::eth_vault_balance::{EthereumVaultBalance, VaultAsset};
use crate::starcoin_bridge_watchdog::metrics::WatchdogMetrics;
use crate::starcoin_bridge_watchdog::starcoin_bridge_status::StarcoinBridgeStatus;
use crate::starcoin_bridge_watchdog::{BridgeWatchDog, Observable};
use crate::types::BridgeCommittee;
use crate::utils::{
    get_committee_voting_power_by_name, get_eth_contract_addresses, get_validator_names_by_pub_keys,
};
use crate::{
    action_executor::BridgeActionExecutor,
    client::bridge_authority_aggregator::BridgeAuthorityAggregator,
    config::{BridgeClientConfig, BridgeNodeConfig},
    eth_syncer::EthSyncer,
    events::init_all_struct_tags,
    metrics::BridgeMetrics,
    monitor::BridgeMonitor,
    orchestrator::BridgeOrchestrator,
    server::{handler::BridgeRequestHandler, run_server, BridgeNodePublicMetadata},
    starcoin_bridge_syncer::StarcoinSyncer,
    storage::BridgeOrchestratorTables,
};
use arc_swap::ArcSwap;
use ethers::providers::Provider;
use ethers::types::Address as EthAddress;
use starcoin_bridge_types::{
    bridge::{
        BRIDGE_COMMITTEE_MODULE_NAME, BRIDGE_LIMITER_MODULE_NAME, BRIDGE_MODULE_NAME,
        BRIDGE_TREASURY_MODULE_NAME,
    },
    event::EventID,
    Identifier,
};
use starcoin_metrics::spawn_logged_monitored_task;
use std::collections::BTreeMap;
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::task::JoinHandle;
use tracing::info;

pub async fn run_bridge_node(
    config: BridgeNodeConfig,
    metadata: BridgeNodePublicMetadata,
    prometheus_registry: prometheus::Registry,
) -> anyhow::Result<JoinHandle<()>> {
    init_all_struct_tags();
    let metrics = Arc::new(BridgeMetrics::new(&prometheus_registry));
    let watchdog_config = config.watchdog_config.clone();
    let (server_config, client_config) = config.validate(metrics.clone()).await?;
    let starcoin_bridge_chain_identifier = server_config
        .starcoin_bridge_client
        .get_chain_identifier()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get starcoin chain identifier: {:?}", e))?;
    let eth_chain_identifier = server_config
        .eth_client
        .get_chain_id()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get eth chain identifier: {:?}", e))?;
    prometheus_registry
        .register(starcoin_metrics::bridge_uptime_metric(
            "bridge",
            metadata.version,
            &starcoin_bridge_chain_identifier,
            &eth_chain_identifier.to_string(),
            client_config.is_some(),
        ))
        .unwrap();

    let committee = Arc::new(
        server_config
            .starcoin_bridge_client
            .get_bridge_committee()
            .await
            .expect("Failed to get committee"),
    );
    let mut handles = vec![];

    // Start watchdog
    let eth_provider = server_config.eth_client.provider();
    let eth_bridge_proxy_address = server_config.eth_bridge_proxy_address;
    let starcoin_bridge_client = server_config.starcoin_bridge_client.clone();
    handles.push(spawn_logged_monitored_task!(start_watchdog(
        watchdog_config,
        &prometheus_registry,
        eth_provider,
        eth_bridge_proxy_address,
        starcoin_bridge_client
    )));

    // Update voting right metrics
    // Before reconfiguration happens we only set it once when the node starts
    // TODO: Implement get_latest_starcoin_bridge_system_state via JSON-RPC
    // For now skip this metric initialization
    let starcoin_bridge_system: Option<starcoin_bridge_json_rpc_types::StarcoinSystemStateSummary> =
        None;

    // Start Client
    if let Some(client_config) = client_config {
        let committee_keys_to_names = if let Some(ref system_state) = starcoin_bridge_system {
            Arc::new(get_validator_names_by_pub_keys(&committee, system_state).await)
        } else {
            // Use base URL as fallback when system state is not available
            Arc::new(
                committee
                    .members()
                    .iter()
                    .map(|(name, validator)| (name.clone(), validator.base_url.clone()))
                    .collect(),
            )
        };
        let client_components = start_client_components(
            client_config,
            committee.clone(),
            committee_keys_to_names,
            metrics.clone(),
        )
        .await?;
        handles.extend(client_components);
    }

    if let Some(ref system_state) = starcoin_bridge_system {
        let committee_name_mapping =
            get_committee_voting_power_by_name(&committee, system_state).await;
        for (name, voting_power) in committee_name_mapping.into_iter() {
            metrics
                .current_bridge_voting_rights
                .with_label_values(&[name.as_str()])
                .set(voting_power as i64);
        }
    }

    // Start Server
    let socket_address = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        server_config.server_listen_port,
    );
    Ok(run_server(
        &socket_address,
        BridgeRequestHandler::new(
            server_config.key,
            server_config.starcoin_bridge_client,
            server_config.eth_client,
            server_config.approved_governance_actions,
            metrics.clone(),
        ),
        metrics,
        Arc::new(metadata),
    ))
}

async fn start_watchdog(
    watchdog_config: Option<WatchdogConfig>,
    registry: &prometheus::Registry,
    eth_provider: Arc<Provider<MeteredEthHttpProvier>>,
    eth_bridge_proxy_address: EthAddress,
    starcoin_bridge_client: Arc<StarcoinBridgeClient>,
) {
    let watchdog_metrics = WatchdogMetrics::new(registry);
    let (
        _committee_address,
        _limiter_address,
        vault_address,
        _config_address,
        weth_address,
        usdt_address,
        wbtc_address,
        lbtc_address,
    ) = get_eth_contract_addresses(eth_bridge_proxy_address, &eth_provider)
        .await
        .unwrap_or_else(|e| panic!("get_eth_contract_addresses should not fail: {}", e));

    let eth_vault_balance = EthereumVaultBalance::new(
        eth_provider.clone(),
        vault_address,
        weth_address,
        VaultAsset::WETH,
        watchdog_metrics.eth_vault_balance.clone(),
    )
    .await
    .unwrap_or_else(|e| panic!("Failed to create eth vault balance: {}", e));

    let usdt_vault_balance = EthereumVaultBalance::new(
        eth_provider.clone(),
        vault_address,
        usdt_address,
        VaultAsset::USDT,
        watchdog_metrics.usdt_vault_balance.clone(),
    )
    .await
    .unwrap_or_else(|e| panic!("Failed to create usdt vault balance: {}", e));

    let wbtc_vault_balance = EthereumVaultBalance::new(
        eth_provider.clone(),
        vault_address,
        wbtc_address,
        VaultAsset::WBTC,
        watchdog_metrics.wbtc_vault_balance.clone(),
    )
    .await
    .unwrap_or_else(|e| panic!("Failed to create wbtc vault balance: {}", e));

    let lbtc_vault_balance = if !lbtc_address.is_zero() {
        Some(
            EthereumVaultBalance::new(
                eth_provider.clone(),
                vault_address,
                lbtc_address,
                VaultAsset::LBTC,
                watchdog_metrics.lbtc_vault_balance.clone(),
            )
            .await
            .unwrap_or_else(|e| panic!("Failed to create lbtc vault balance: {}", e)),
        )
    } else {
        None
    };

    let eth_bridge_status = EthBridgeStatus::new(
        eth_provider,
        eth_bridge_proxy_address,
        watchdog_metrics.eth_bridge_paused.clone(),
    );

    let starcoin_bridge_status = StarcoinBridgeStatus::new(
        starcoin_bridge_client.clone(),
        watchdog_metrics.starcoin_bridge_paused.clone(),
    );

    let mut observables: Vec<Box<dyn Observable + Send + Sync>> = vec![
        Box::new(eth_vault_balance),
        Box::new(usdt_vault_balance),
        Box::new(wbtc_vault_balance),
        Box::new(eth_bridge_status),
        Box::new(starcoin_bridge_status),
    ];

    // Add lbtc_vault_balance if it's available
    if let Some(balance) = lbtc_vault_balance {
        observables.push(Box::new(balance));
    }

    // TODO: Re-enable TotalSupplies when JSON-RPC client implements coin_read_api
    // if let Some(watchdog_config) = watchdog_config {
    //     if !watchdog_config.total_supplies.is_empty() {
    //         let total_supplies = TotalSupplies::new(
    //             starcoin_bridge_client.clone(),
    //             watchdog_config.total_supplies,
    //             watchdog_metrics.total_supplies.clone(),
    //         );
    //         observables.push(Box::new(total_supplies));
    //     }
    // }
    let _ = watchdog_config; // Silence unused warning

    BridgeWatchDog::new(observables).run().await
}

// TODO: is there a way to clean up the overrides after it's stored in DB?
async fn start_client_components(
    client_config: BridgeClientConfig,
    committee: Arc<BridgeCommittee>,
    committee_keys_to_names: Arc<BTreeMap<BridgeAuthorityPublicKeyBytes, String>>,
    metrics: Arc<BridgeMetrics>,
) -> anyhow::Result<Vec<JoinHandle<()>>> {
    let store: std::sync::Arc<BridgeOrchestratorTables> =
        BridgeOrchestratorTables::new(&client_config.db_path.join("client"));
    let starcoin_bridge_modules_to_watch = get_starcoin_bridge_modules_to_watch(
        &store,
        client_config.starcoin_bridge_module_last_processed_event_id_override,
    );
    let eth_contracts_to_watch = get_eth_contracts_to_watch(
        &store,
        &client_config.eth_contracts,
        client_config.eth_contracts_start_block_fallback,
        client_config.eth_contracts_start_block_override,
    );

    let starcoin_bridge_client = client_config.starcoin_bridge_client.clone();

    // Get bridge package ID from client (parsed from config's starcoin_bridge_proxy_address)
    let bridge_address_str = starcoin_bridge_client.bridge_address();
    let bridge_package_id = {
        let addr_str = bridge_address_str.trim_start_matches("0x");
        let addr_bytes = hex::decode(addr_str).expect("Invalid bridge address hex");
        let mut id = [0u8; 32];
        // Starcoin uses 16-byte addresses, left-pad with zeros for ObjectID (32 bytes)
        id[16..32].copy_from_slice(&addr_bytes);
        id
    };
    tracing::info!(
        "Using bridge package ID from config: {}",
        bridge_address_str
    );

    let mut all_handles = vec![];
    let (task_handles, eth_events_rx, _) =
        EthSyncer::new(client_config.eth_client.clone(), eth_contracts_to_watch)
            .run(metrics.clone())
            .await
            .expect("Failed to start eth syncer");
    all_handles.extend(task_handles);

    let (task_handles, starcoin_bridge_events_rx) = StarcoinSyncer::new(
        client_config.starcoin_bridge_client,
        bridge_package_id,
        starcoin_bridge_modules_to_watch,
        metrics.clone(),
    )
    .run(Duration::from_secs(2))
    .await
    .expect("Failed to start starcoin syncer");
    all_handles.extend(task_handles);

    let bridge_auth_agg = Arc::new(ArcSwap::from(Arc::new(BridgeAuthorityAggregator::new(
        committee,
        metrics.clone(),
        committee_keys_to_names,
    ))));
    // TODO: should we use one query instead of two?
    let starcoin_bridge_token_type_tags = starcoin_bridge_client.get_token_id_map().await.unwrap();
    let is_bridge_paused = starcoin_bridge_client.is_bridge_paused().await.unwrap();

    let (bridge_pause_tx, bridge_pause_rx) = tokio::sync::watch::channel(is_bridge_paused);

    let (starcoin_bridge_monitor_tx, starcoin_bridge_monitor_rx) =
        starcoin_metrics::metered_channel::channel(
            10000,
            &starcoin_metrics::get_metrics()
                .unwrap()
                .channel_inflight
                .with_label_values(&["starcoin_bridge_monitor_queue"]),
        );
    let (eth_monitor_tx, eth_monitor_rx) = starcoin_metrics::metered_channel::channel(
        10000,
        &starcoin_metrics::get_metrics()
            .unwrap()
            .channel_inflight
            .with_label_values(&["eth_monitor_queue"]),
    );

    let starcoin_bridge_token_type_tags =
        Arc::new(ArcSwap::from(Arc::new(starcoin_bridge_token_type_tags)));
    let bridge_action_executor = BridgeActionExecutor::new(
        starcoin_bridge_client.clone(),
        bridge_auth_agg.clone(),
        store.clone(),
        client_config.key,
        client_config.starcoin_bridge_address,
        client_config.gas_object_ref.0,
        starcoin_bridge_token_type_tags.clone(),
        bridge_pause_rx,
        metrics.clone(),
    )
    .await;

    let monitor = BridgeMonitor::new(
        starcoin_bridge_client.clone(),
        starcoin_bridge_monitor_rx,
        eth_monitor_rx,
        bridge_auth_agg.clone(),
        bridge_pause_tx,
        starcoin_bridge_token_type_tags,
        metrics.clone(),
    );
    all_handles.push(spawn_logged_monitored_task!(monitor.run()));

    let orchestrator = BridgeOrchestrator::new(
        starcoin_bridge_client,
        starcoin_bridge_events_rx,
        eth_events_rx,
        store.clone(),
        starcoin_bridge_monitor_tx,
        eth_monitor_tx,
        metrics,
    );

    all_handles.extend(orchestrator.run(bridge_action_executor).await);
    Ok(all_handles)
}

fn get_starcoin_bridge_modules_to_watch(
    store: &std::sync::Arc<BridgeOrchestratorTables>,
    starcoin_bridge_module_last_processed_event_id_override: Option<EventID>,
) -> HashMap<Identifier, Option<EventID>> {
    let starcoin_bridge_modules = vec![
        BRIDGE_MODULE_NAME.to_owned(),
        BRIDGE_COMMITTEE_MODULE_NAME.to_owned(),
        BRIDGE_TREASURY_MODULE_NAME.to_owned(),
        BRIDGE_LIMITER_MODULE_NAME.to_owned(),
    ];
    if let Some(cursor) = starcoin_bridge_module_last_processed_event_id_override {
        info!(
            "Overriding cursor for starcoin bridge modules to {:?}",
            cursor
        );
        return HashMap::from_iter(
            starcoin_bridge_modules
                .iter()
                .map(|module| (module.clone(), Some(cursor))),
        );
    }

    let starcoin_bridge_module_stored_cursor = store
        .get_starcoin_bridge_event_cursors(&starcoin_bridge_modules)
        .expect("Failed to get eth starcoin event cursors from storage");
    let mut starcoin_bridge_modules_to_watch = HashMap::new();
    for (module_identifier, cursor) in starcoin_bridge_modules
        .iter()
        .zip(starcoin_bridge_module_stored_cursor)
    {
        if cursor.is_none() {
            info!(
                "No cursor found for starcoin bridge module {} in storage or config override, query start from the beginning.",
                module_identifier
            );
        }
        starcoin_bridge_modules_to_watch.insert(module_identifier.clone(), cursor);
    }
    starcoin_bridge_modules_to_watch
}

fn get_eth_contracts_to_watch(
    store: &std::sync::Arc<BridgeOrchestratorTables>,
    eth_contracts: &[EthAddress],
    eth_contracts_start_block_fallback: u64,
    eth_contracts_start_block_override: Option<u64>,
) -> HashMap<EthAddress, u64> {
    let stored_eth_cursors = store
        .get_eth_event_cursors(eth_contracts)
        .expect("Failed to get eth event cursors from storage");
    let mut eth_contracts_to_watch = HashMap::new();
    for (contract, stored_cursor) in eth_contracts.iter().zip(stored_eth_cursors) {
        // start block precedence:
        // eth_contracts_start_block_override > stored cursor > eth_contracts_start_block_fallback
        match (eth_contracts_start_block_override, stored_cursor) {
            (Some(override_), _) => {
                eth_contracts_to_watch.insert(*contract, override_);
                info!(
                    "Overriding cursor for eth bridge contract {} to {}. Stored cursor: {:?}",
                    contract, override_, stored_cursor
                );
            }
            (None, Some(stored_cursor)) => {
                // +1: The stored value is the last block that was processed, so we start from the next block.
                eth_contracts_to_watch.insert(*contract, stored_cursor + 1);
            }
            (None, None) => {
                // If no cursor is found, start from the fallback block.
                eth_contracts_to_watch.insert(*contract, eth_contracts_start_block_fallback);
            }
        }
    }
    eth_contracts_to_watch
}

#[cfg(test)]
mod tests {
    use ethers::types::Address as EthAddress;

    use super::*;

    #[tokio::test]
    async fn test_get_eth_contracts_to_watch() {
        telemetry_subscribers::init_for_testing();
        let temp_dir = tempfile::tempdir().unwrap();
        let eth_contracts = vec![
            EthAddress::from_low_u64_be(1),
            EthAddress::from_low_u64_be(2),
        ];
        let store = BridgeOrchestratorTables::new(temp_dir.path());

        // No override, no watermark found in DB, use fallback
        let contracts = get_eth_contracts_to_watch(&store, &eth_contracts, 10, None);
        assert_eq!(
            contracts,
            vec![(eth_contracts[0], 10), (eth_contracts[1], 10)]
                .into_iter()
                .collect::<HashMap<_, _>>()
        );

        // no watermark found in DB, use override
        let contracts = get_eth_contracts_to_watch(&store, &eth_contracts, 10, Some(420));
        assert_eq!(
            contracts,
            vec![(eth_contracts[0], 420), (eth_contracts[1], 420)]
                .into_iter()
                .collect::<HashMap<_, _>>()
        );

        store
            .update_eth_event_cursor(eth_contracts[0], 100)
            .unwrap();
        store
            .update_eth_event_cursor(eth_contracts[1], 102)
            .unwrap();

        // No override, found watermarks in DB, use +1
        let contracts = get_eth_contracts_to_watch(&store, &eth_contracts, 10, None);
        assert_eq!(
            contracts,
            vec![(eth_contracts[0], 101), (eth_contracts[1], 103)]
                .into_iter()
                .collect::<HashMap<_, _>>()
        );

        // use override
        let contracts = get_eth_contracts_to_watch(&store, &eth_contracts, 10, Some(200));
        assert_eq!(
            contracts,
            vec![(eth_contracts[0], 200), (eth_contracts[1], 200)]
                .into_iter()
                .collect::<HashMap<_, _>>()
        );
    }

    #[tokio::test]
    async fn test_get_starcoin_bridge_modules_to_watch() {
        telemetry_subscribers::init_for_testing();
        let temp_dir = tempfile::tempdir().unwrap();

        let store = BridgeOrchestratorTables::new(temp_dir.path());
        let bridge_module = BRIDGE_MODULE_NAME.to_owned();
        let committee_module = BRIDGE_COMMITTEE_MODULE_NAME.to_owned();
        let treasury_module = BRIDGE_TREASURY_MODULE_NAME.to_owned();
        let limiter_module = BRIDGE_LIMITER_MODULE_NAME.to_owned();
        // No override, no stored watermark, use None
        let starcoin_bridge_modules_to_watch = get_starcoin_bridge_modules_to_watch(&store, None);
        assert_eq!(
            starcoin_bridge_modules_to_watch,
            vec![
                (bridge_module.clone(), None),
                (committee_module.clone(), None),
                (treasury_module.clone(), None),
                (limiter_module.clone(), None)
            ]
            .into_iter()
            .collect::<HashMap<_, _>>()
        );

        // no stored watermark, use override
        // EventID is now (u64, u64) - (block_number, event_seq)
        let override_cursor: EventID = (100, 42);
        let starcoin_bridge_modules_to_watch = get_starcoin_bridge_modules_to_watch(&store, Some(override_cursor));
        assert_eq!(
            starcoin_bridge_modules_to_watch,
            vec![
                (bridge_module.clone(), Some(override_cursor)),
                (committee_module.clone(), Some(override_cursor)),
                (treasury_module.clone(), Some(override_cursor)),
                (limiter_module.clone(), Some(override_cursor))
            ]
            .into_iter()
            .collect::<HashMap<_, _>>()
        );

        // No override, found stored watermark for `bridge` module, use stored watermark for `bridge`
        // and None for `committee`
        // EventID is now (u64, u64)
        let stored_cursor: EventID = (200, 100);
        store
            .update_starcoin_bridge_event_cursor(bridge_module.clone(), stored_cursor)
            .unwrap();
        let starcoin_bridge_modules_to_watch = get_starcoin_bridge_modules_to_watch(&store, None);
        assert_eq!(
            starcoin_bridge_modules_to_watch,
            vec![
                (bridge_module.clone(), Some(stored_cursor)),
                (committee_module.clone(), None),
                (treasury_module.clone(), None),
                (limiter_module.clone(), None)
            ]
            .into_iter()
            .collect::<HashMap<_, _>>()
        );

        // found stored watermark, use override
        let stored_cursor2: EventID = (300, 100);
        store
            .update_starcoin_bridge_event_cursor(committee_module.clone(), stored_cursor2)
            .unwrap();
        let starcoin_bridge_modules_to_watch = get_starcoin_bridge_modules_to_watch(&store, Some(override_cursor));
        assert_eq!(
            starcoin_bridge_modules_to_watch,
            vec![
                (bridge_module.clone(), Some(override_cursor)),
                (committee_module.clone(), Some(override_cursor)),
                (treasury_module.clone(), Some(override_cursor)),
                (limiter_module.clone(), Some(override_cursor))
            ]
            .into_iter()
            .collect::<HashMap<_, _>>()
        );
    }

    // NOTE: The following tests are disabled because they require e2e test infrastructure
    // (BridgeTestCluster, BridgeTestClusterBuilder) which depends on Sui test cluster.
    // They should be enabled once we have proper Starcoin test infrastructure.
    /*
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_starting_bridge_node() { ... }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_starting_bridge_node_with_client() { ... }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_starting_bridge_node_with_client_and_separate_client_key() { ... }

    async fn setup() -> BridgeTestCluster { ... }
    */
}
