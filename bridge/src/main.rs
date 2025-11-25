// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use fastcrypto::traits::KeyPair;
use mysten_metrics::start_prometheus_server;
use starcoin_bridge::config::BridgeNodeConfig;
use starcoin_bridge::node::run_bridge_node;
use starcoin_bridge::server::BridgeNodePublicMetadata;
use starcoin_bridge_config::Config;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};
// use starcoin_bridge_metrics_push_client::start_metrics_push_task;  // Disabled: conflicts with starcoin's subtle version
use tracing::info;

// Define the `GIT_REVISION` and `VERSION` consts
bin_version::bin_version!();

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
#[clap(name = env!("CARGO_BIN_NAME"))]
#[clap(version = VERSION)]
struct Args {
    #[clap(long)]
    pub config_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = BridgeNodeConfig::load(&args.config_path).unwrap();

    // Init metrics server
    let metrics_address =
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), config.metrics_port);
    let registry_service = start_prometheus_server(metrics_address);
    let prometheus_registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&prometheus_registry);
    info!("Metrics server started at port {}", config.metrics_port);

    // Init logging
    let (_guard, _filter_handle) = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .with_prom_registry(&prometheus_registry)
        .init();

    let metadata = BridgeNodePublicMetadata::new(VERSION, config.metrics_key_pair.public().clone());

    // Metrics push functionality disabled due to rustls/subtle version conflicts with starcoin
    /*
    if let Some(metrics_config) = &config.metrics {
        start_metrics_push_task(
            metrics_config.push_interval_seconds,
            metrics_config.push_url.clone(),
            config.metrics_key_pair.copy(),
            registry_service.clone(),
        );
    }
    */

    Ok(run_bridge_node(config, metadata, prometheus_registry)
        .await?
        .await?)
}
