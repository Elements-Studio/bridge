// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! ETH Bridge Indexer module
//!
//! This module provides functionality to index Ethereum bridge events
//! and store them in the database.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::AsyncPgConnection;
use diesel_async::RunQueryDsl;
use ethers::types::Address as EthAddress;
use starcoin_bridge::abi::{EthBridgeEvent, EthStarcoinBridgeEvents, EthToStarcoinTokenBridgeV1};
use starcoin_bridge::eth_client::EthClient;
use starcoin_bridge::eth_syncer::EthSyncer;
use starcoin_bridge::metrics::BridgeMetrics;
use starcoin_bridge::types::EthLog;
use starcoin_bridge_schema::models::{
    BridgeDataSource, TokenTransfer, TokenTransferData, TokenTransferStatus,
};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// Start the ETH indexer
pub async fn start_eth_indexer(
    eth_rpc_url: String,
    eth_bridge_address: String,
    eth_start_block: u64,
    pool: Pool<AsyncPgConnection>,
    bridge_metrics: Arc<BridgeMetrics>,
) -> Result<Vec<JoinHandle<()>>> {
    info!("Starting ETH indexer...");
    info!("  ETH RPC URL: {}", eth_rpc_url);
    info!("  Bridge Address: {}", eth_bridge_address);
    info!("  Start Block: {}", eth_start_block);

    // Parse bridge address
    let bridge_address: EthAddress = eth_bridge_address
        .parse()
        .context("Failed to parse eth_bridge_address")?;

    // Create ETH client
    let eth_client = Arc::new(
        EthClient::new(
            &eth_rpc_url,
            HashSet::from([bridge_address]),
            bridge_metrics.clone(),
            true, // use_latest_block for local testing
        )
        .await
        .map_err(|e| anyhow!("Failed to create ETH client: {:?}", e))?,
    );

    // Try to get contract addresses from the proxy, but use only bridge address if it fails
    let provider = eth_client.provider();
    let addresses_to_watch: HashMap<EthAddress, u64> = match starcoin_bridge::utils::get_eth_contract_addresses(bridge_address, &provider).await {
        Ok(contract_addresses) => {
            info!("Found ETH contract addresses:");
            info!("  Committee: {:?}", contract_addresses.0);
            info!("  Limiter: {:?}", contract_addresses.1);
            info!("  Vault: {:?}", contract_addresses.2);
            info!("  Config: {:?}", contract_addresses.3);
            
            HashMap::from([
                (bridge_address, eth_start_block),
                (contract_addresses.0, eth_start_block), // committee
                (contract_addresses.1, eth_start_block), // limiter
                (contract_addresses.3, eth_start_block), // config
            ])
        }
        Err(e) => {
            warn!("Failed to get ETH contract addresses (using bridge address only): {:?}", e);
            HashMap::from([(bridge_address, eth_start_block)])
        }
    };

    // Start ETH syncer
    let (mut handles, eth_events_rx, _finalized_rx) = EthSyncer::new(eth_client.clone(), addresses_to_watch)
        .run(bridge_metrics.clone())
        .await
        .map_err(|e| anyhow!("Failed to start ETH syncer: {:?}", e))?;

    info!("ETH syncer started, waiting for events...");

    // Spawn event processing task
    let process_handle = tokio::spawn(process_eth_events(eth_events_rx, pool));

    handles.push(process_handle);
    Ok(handles)
}

/// Process ETH events from the syncer
async fn process_eth_events(
    mut eth_events_rx: starcoin_metrics::metered_channel::Receiver<(EthAddress, u64, Vec<EthLog>)>,
    pool: Pool<AsyncPgConnection>,
) {
    while let Some((contract_addr, block_num, logs)) = eth_events_rx.recv().await {
        if logs.is_empty() {
            continue;
        }

        info!(
            "Received {} logs from contract {:?} at block {}",
            logs.len(),
            contract_addr,
            block_num
        );

        for log in logs {
            if let Err(e) = process_eth_log(&log, &pool).await {
                error!("Failed to process ETH log: {:?}", e);
            }
        }
    }
}

async fn process_eth_log(log: &EthLog, pool: &Pool<AsyncPgConnection>) -> Result<()> {
    // Try to parse the log as a bridge event
    let event = match EthBridgeEvent::try_from_eth_log(log) {
        Some(e) => e,
        None => {
            warn!("Could not parse ETH log as bridge event: {:?}", log.tx_hash);
            return Ok(());
        }
    };

    // Get connection from pool
    let mut conn = pool.get().await.context("Failed to get database connection")?;

    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    match event {
        EthBridgeEvent::EthStarcoinBridgeEvents(bridge_event) => {
            process_bridge_event(bridge_event, log, timestamp_ms, &mut conn).await?;
        }
        _ => {
            // Committee, Limiter, Config events - for now just log them
            info!("Received non-bridge ETH event (committee/limiter/config)");
        }
    }

    Ok(())
}

async fn process_bridge_event(
    bridge_event: EthStarcoinBridgeEvents,
    log: &EthLog,
    timestamp_ms: i64,
    conn: &mut diesel_async::pooled_connection::deadpool::Object<AsyncPgConnection>,
) -> Result<()> {
    match bridge_event {
        EthStarcoinBridgeEvents::TokensDepositedFilter(deposit) => {
            info!(
                "Processing ETH deposit: nonce={}, block={}",
                deposit.nonce, log.block_number
            );

            // Convert to our bridge event type
            let bridge_event = EthToStarcoinTokenBridgeV1::try_from(&deposit)
                .map_err(|e| anyhow!("Failed to convert deposit event: {:?}", e))?;

            // Create token transfer record
            let transfer = TokenTransfer {
                chain_id: bridge_event.eth_chain_id as i32,
                nonce: bridge_event.nonce as i64,
                block_height: log.block_number as i64,
                timestamp_ms,
                txn_hash: log.tx_hash.as_bytes().to_vec(),
                txn_sender: bridge_event.eth_address.as_bytes().to_vec(),
                status: TokenTransferStatus::Deposited,
                gas_usage: 0,
                data_source: BridgeDataSource::ETH,
                is_finalized: true,
            };

            // Create token transfer data
            let transfer_data = TokenTransferData {
                chain_id: bridge_event.eth_chain_id as i32,
                nonce: bridge_event.nonce as i64,
                block_height: log.block_number as i64,
                timestamp_ms,
                txn_hash: log.tx_hash.as_bytes().to_vec(),
                sender_address: bridge_event.eth_address.as_bytes().to_vec(),
                destination_chain: bridge_event.starcoin_bridge_chain_id as i32,
                recipient_address: bridge_event.starcoin_bridge_address.to_vec(),
                token_id: bridge_event.token_id as i32,
                amount: bridge_event.starcoin_bridge_adjusted_amount as i64,
                is_finalized: true,
            };

            // Insert into database
            use starcoin_bridge_schema::schema::{token_transfer, token_transfer_data};

            diesel::insert_into(token_transfer::table)
                .values(&transfer)
                .on_conflict_do_nothing()
                .execute(conn)
                .await
                .context("Failed to insert token transfer")?;

            diesel::insert_into(token_transfer_data::table)
                .values(&transfer_data)
                .on_conflict_do_nothing()
                .execute(conn)
                .await
                .context("Failed to insert token transfer data")?;

            info!(
                "Inserted ETH deposit: chain_id={}, nonce={}, amount={}",
                bridge_event.eth_chain_id,
                bridge_event.nonce,
                bridge_event.starcoin_bridge_adjusted_amount
            );
        }
        EthStarcoinBridgeEvents::TokensClaimedFilter(claim) => {
            info!(
                "Processing ETH claim: nonce={}, block={}",
                claim.nonce, log.block_number
            );

            // Create token transfer record for claim
            let transfer = TokenTransfer {
                chain_id: claim.source_chain_id as i32,
                nonce: claim.nonce as i64,
                block_height: log.block_number as i64,
                timestamp_ms,
                txn_hash: log.tx_hash.as_bytes().to_vec(),
                txn_sender: claim.recipient_address.as_bytes().to_vec(),
                status: TokenTransferStatus::Claimed,
                gas_usage: 0,
                data_source: BridgeDataSource::ETH,
                is_finalized: true,
            };

            use starcoin_bridge_schema::schema::token_transfer;

            diesel::insert_into(token_transfer::table)
                .values(&transfer)
                .on_conflict_do_nothing()
                .execute(conn)
                .await
                .context("Failed to insert token transfer claim")?;

            info!(
                "Inserted ETH claim: chain_id={}, nonce={}",
                claim.source_chain_id, claim.nonce
            );
        }
        _ => {
            // Other events (Paused, Unpaused, etc.)
            info!("Ignoring ETH bridge event: {:?}", bridge_event);
        }
    }

    Ok(())
}
