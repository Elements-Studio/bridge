// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::Context;
use clap::Parser;
use prometheus::Registry;
use std::net::SocketAddr;
use starcoin_bridge_indexer_alt::handlers::error_handler::ErrorTransactionHandler;
use starcoin_bridge_indexer_alt::handlers::governance_action_handler::GovernanceActionHandler;
use starcoin_bridge_indexer_alt::handlers::token_transfer_data_handler::TokenTransferDataHandler;
use starcoin_bridge_indexer_alt::handlers::token_transfer_handler::TokenTransferHandler;
use starcoin_bridge_indexer_alt::metrics::BridgeIndexerMetrics;
use starcoin_bridge_schema::MIGRATIONS;
use starcoin_bridge_indexer_alt_framework::ingestion::{ClientArgs, IngestionConfig};
use starcoin_bridge_indexer_alt_framework::postgres::DbArgs;
use starcoin_bridge_indexer_alt_framework::{Indexer, IndexerArgs};
use starcoin_bridge_indexer_alt_metrics::{MetricsArgs, MetricsService};
use tokio_util::sync::CancellationToken;
use url::Url;

#[derive(Parser)]
#[clap(rename_all = "kebab-case", author, version)]
struct Args {
    #[command(flatten)]
    db_args: DbArgs,
    #[command(flatten)]
    indexer_args: IndexerArgs,
    #[clap(env, long, default_value = "0.0.0.0:9184")]
    metrics_address: SocketAddr,
    #[clap(
        env,
        long,
        default_value = "postgres://postgres:postgrespw@localhost:5432/bridge"
    )]
    database_url: Url,
    /// Remote checkpoint store URL (mutually exclusive with --rpc-api-url)
    #[clap(env, long)]
    remote_store_url: Option<Url>,
    /// Starcoin RPC URL to fetch blocks/events from
    #[clap(env, long)]
    rpc_api_url: Option<Url>,
    /// Bridge contract address on Starcoin (used with --rpc-api-url)
    #[clap(env, long, default_value = "0xefa1e687a64f869193f109f75d0432be")]
    bridge_address: String,
}
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let Args {
        db_args,
        indexer_args,
        metrics_address,
        database_url,
        remote_store_url,
        rpc_api_url,
        bridge_address,
    } = Args::parse();

    let cancel = CancellationToken::new();
    let registry = Registry::new_custom(Some("bridge".into()), None)
        .context("Failed to create Prometheus registry.")?;

    // Initialize bridge-specific metrics
    let bridge_metrics = BridgeIndexerMetrics::new(&registry);

    let metrics = MetricsService::new(
        MetricsArgs { metrics_address },
        registry,
        cancel.child_token(),
    );

    let metrics_prefix = None;
    
    // Use lower concurrency when using RPC mode to avoid rate limiting
    let ingestion_config = if rpc_api_url.is_some() {
        IngestionConfig {
            checkpoint_buffer_size: 100,   // Reduced buffer
            ingest_concurrency: 5,         // Low concurrency for RPC
            retry_interval_ms: 500,        // Longer retry interval
        }
    } else {
        IngestionConfig::default()
    };
    
    let mut indexer = Indexer::new_from_pg(
        database_url,
        db_args,
        indexer_args,
        ClientArgs {
            remote_store_url,
            local_ingestion_path: None,
            rpc_api_url,
            bridge_address: Some(bridge_address.clone()),
        },
        ingestion_config,
        Some(&MIGRATIONS),
        metrics_prefix,
        metrics.registry(),
        cancel.clone(),
    )
    .await?;

    // Parse bridge address for handlers
    // bridge_address already includes "0x" prefix
    let bridge_addr = move_core_types::account_address::AccountAddress::from_hex_literal(&bridge_address)
        .context("Failed to parse bridge address")?;

    indexer
        .concurrent_pipeline(
            TokenTransferHandler::new(bridge_metrics.clone(), bridge_addr),
            Default::default(),
        )
        .await?;

    indexer
        .concurrent_pipeline(TokenTransferDataHandler::default(), Default::default())
        .await?;

    indexer
        .concurrent_pipeline(
            GovernanceActionHandler::new(bridge_metrics.clone(), bridge_addr),
            Default::default(),
        )
        .await?;

    indexer
        .concurrent_pipeline(ErrorTransactionHandler, Default::default())
        .await?;

    let h_indexer = indexer.run().await?;
    let h_metrics = metrics.run().await?;

    let _ = h_indexer.await;
    cancel.cancel();
    let _ = h_metrics.await;
    Ok(())
}
