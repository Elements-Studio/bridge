// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Starcoin RPC client for ingesting blockchain data.
//!
//! This module provides a client that fetches block and event data from
//! Starcoin HTTP RPC and converts it to the CheckpointData format expected
//! by the indexer framework.

use crate::ingestion::client::{FetchData, FetchError, FetchResult, IngestionClientTrait};
use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use starcoin_bridge_types::effects::{GasCostSummary, TransactionEffects};
use starcoin_bridge_types::event::Event;
use starcoin_bridge_types::execution_status::ExecutionStatus;
use starcoin_bridge_types::full_checkpoint_content::{
    CheckpointData, CheckpointSummary, CheckpointTransaction, TransactionEvents,
};
use starcoin_bridge_types::transaction::TransactionDataAPI;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};
use url::Url;

/// Internal error type for block fetching
enum BlockFetchError {
    NotFound,
    Other(anyhow::Error),
}

/// JSON-RPC request
#[derive(Serialize)]
struct JsonRpcRequest<T: Serialize> {
    jsonrpc: &'static str,
    method: &'static str,
    params: T,
    id: u64,
}

/// JSON-RPC response
#[derive(Deserialize)]
struct JsonRpcResponse<T> {
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Deserialize, Debug)]
struct JsonRpcError {
    code: i64,
    message: String,
}

/// Starcoin event from RPC
#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct RpcEvent {
    type_tag: String,
    data: String,
    block_number: String,
    transaction_hash: String,
}

/// Chain info response
#[derive(Deserialize, Debug)]
struct ChainInfo {
    head: ChainHead,
}

#[derive(Deserialize, Debug)]
struct ChainHead {
    number: String,
}

/// Starcoin HTTP RPC client that implements IngestionClientTrait
pub struct StarcoinRpcClient {
    http_client: HttpClient,
    rpc_url: String,
    bridge_address: String,
    /// Semaphore to limit concurrent requests
    semaphore: Arc<Semaphore>,
    /// Cache the current chain height to avoid excessive RPC calls
    cached_height: Arc<tokio::sync::RwLock<(u64, std::time::Instant)>>,
}

impl StarcoinRpcClient {
    /// Create a new Starcoin HTTP RPC client
    pub async fn new(rpc_url: Url, bridge_address: String) -> anyhow::Result<Self> {
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(5)  // Limit connection pool
            .no_proxy()  // Disable proxy to avoid routing through local proxy
            .build()?;

        let rpc_url_str = rpc_url.to_string();

        // Limit concurrent requests to avoid rate limiting
        let semaphore = Arc::new(Semaphore::new(3));

        let client = Self {
            http_client,
            rpc_url: rpc_url_str.clone(),
            bridge_address: bridge_address.clone(),
            semaphore,
            cached_height: Arc::new(tokio::sync::RwLock::new((0, std::time::Instant::now()))),
        };

        let chain_info = client.get_chain_height().await?;
        info!(
            "Connected to Starcoin RPC at {}, current block: {}, bridge address: {}",
            rpc_url_str, chain_info, bridge_address
        );

        Ok(client)
    }

    /// Call a JSON-RPC method with rate limiting
    async fn call_rpc<P: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        method: &'static str,
        params: P,
    ) -> anyhow::Result<R> {
        // Acquire semaphore permit to limit concurrent requests
        let _permit = self.semaphore.acquire().await?;
        
        // Add small delay between requests to avoid rate limiting
        tokio::time::sleep(Duration::from_millis(50)).await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            method,
            params,
            id: 1,
        };

        let response = self
            .http_client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await?;

        let rpc_response: JsonRpcResponse<R> = response.json().await?;

        if let Some(error) = rpc_response.error {
            // Check for rate limiting
            if error.code == -10000 || error.message.contains("rate-limited") {
                // Wait and retry once
                tokio::time::sleep(Duration::from_secs(1)).await;
                
                let retry_response = self
                    .http_client
                    .post(&self.rpc_url)
                    .json(&request)
                    .send()
                    .await?;
                    
                let retry_rpc_response: JsonRpcResponse<R> = retry_response.json().await?;
                
                if let Some(retry_error) = retry_rpc_response.error {
                    anyhow::bail!("RPC error {}: {}", retry_error.code, retry_error.message);
                }
                
                return retry_rpc_response
                    .result
                    .ok_or_else(|| anyhow::anyhow!("No result in RPC response"));
            }
            anyhow::bail!("RPC error {}: {}", error.code, error.message);
        }

        rpc_response
            .result
            .ok_or_else(|| anyhow::anyhow!("No result in RPC response"))
    }

    /// Get current chain info
    async fn get_chain_height(&self) -> anyhow::Result<u64> {
        // Check cache first (valid for 1 second)
        {
            let cache = self.cached_height.read().await;
            if cache.1.elapsed() < Duration::from_secs(1) && cache.0 > 0 {
                return Ok(cache.0);
            }
        }
        
        let info: ChainInfo = self.call_rpc("chain.info", Vec::<()>::new()).await?;
        let height = info.head
            .number
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse block number: {}", e))?;
        
        // Update cache
        {
            let mut cache = self.cached_height.write().await;
            *cache = (height, std::time::Instant::now());
        }
        
        Ok(height)
    }

    /// Fetch block data at a specific height and convert to CheckpointData
    async fn fetch_block_data(&self, block_height: u64) -> Result<CheckpointData, BlockFetchError> {
        // Check if block exists
        let current_height = self.get_chain_height().await
            .map_err(|e| BlockFetchError::Other(e))?;
        
        if block_height > current_height {
            debug!(
                "Block {} not yet available (current height: {})",
                block_height, current_height
            );
            return Err(BlockFetchError::NotFound);
        }
        
        // Get events for this block
        let bridge_events = self.fetch_bridge_events_for_block(block_height).await
            .map_err(|e| BlockFetchError::Other(e))?;

        // Create checkpoint summary
        let checkpoint_summary = CheckpointSummary {
            epoch: 0,
            sequence_number: block_height,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            network_total_transactions: block_height,
        };

        let mut transactions = Vec::new();

        if !bridge_events.is_empty() {
            debug!(
                "Found {} bridge events in block {}",
                bridge_events.len(),
                block_height
            );

            let checkpoint_tx = CheckpointTransaction {
                transaction: TransactionDataAPI {
                    transaction: vec![],
                    digest: [0u8; 32],
                    sender: [0u8; 32],
                },
                input_objects: vec![],
                output_objects: vec![],
                events: Some(TransactionEvents { data: bridge_events }),
                effects: TransactionEffects {
                    gas_used: GasCostSummary {
                        computation_cost: 0,
                        storage_cost: 0,
                        storage_rebate: 0,
                        non_refundable_storage_fee: 0,
                    },
                    execution_status: ExecutionStatus::Success,
                },
            };
            transactions.push(checkpoint_tx);
        }

        Ok(CheckpointData {
            checkpoint_summary,
            transactions,
        })
    }

    /// Fetch bridge events for a specific block using a single RPC call with all type tags
    async fn fetch_bridge_events_for_block(&self, block_height: u64) -> anyhow::Result<Vec<Event>> {
        // Ensure bridge address has 0x prefix for RPC calls
        let bridge_addr = if self.bridge_address.starts_with("0x") {
            self.bridge_address.clone()
        } else {
            format!("0x{}", self.bridge_address)
        };
        
        // Query all bridge events in a single call using multiple type tags
        let type_tags: Vec<String> = [
            "TokenDepositedEvent",
            "TokenTransferApproved",
            "TokenTransferClaimed",
        ]
        .iter()
        .map(|event_type| format!("{}::Bridge::{}", bridge_addr, event_type))
        .collect();

        let filter = serde_json::json!({
            "type_tags": type_tags,
            "from_block": block_height,
            "to_block": block_height + 1,
        });

        let events: Vec<RpcEvent> = match self.call_rpc("chain.get_events", vec![filter]).await {
            Ok(events) => events,
            Err(e) => {
                // Log error but don't fail - just return empty events
                warn!(
                    "Failed to get bridge events for block {}: {}",
                    block_height, e
                );
                return Ok(Vec::new());
            }
        };

        let mut all_events = Vec::new();
        for rpc_event in events {
            if let Some(event) = self.parse_rpc_event(&rpc_event) {
                all_events.push(event);
            }
        }

        Ok(all_events)
    }

    /// Parse an RPC event into our Event type
    fn parse_rpc_event(&self, rpc_event: &RpcEvent) -> Option<Event> {
        let struct_tag = self.parse_type_tag(&rpc_event.type_tag)?;

        let data_hex = rpc_event.data.strip_prefix("0x").unwrap_or(&rpc_event.data);
        let contents = hex::decode(data_hex).ok()?;

        Some(Event {
            type_: struct_tag,
            contents,
        })
    }

    /// Parse a type tag string into a StructTag
    fn parse_type_tag(&self, type_tag: &str) -> Option<StructTag> {
        let parts: Vec<&str> = type_tag.split("::").collect();
        if parts.len() < 3 {
            return None;
        }

        let addr_str = parts[0].strip_prefix("0x").unwrap_or(parts[0]);
        let addr_bytes = hex::decode(addr_str).ok()?;

        let mut addr_array = [0u8; 16];
        let len = addr_bytes.len().min(16);
        addr_array[16 - len..].copy_from_slice(&addr_bytes[..len]);

        let address = AccountAddress::new(addr_array);
        let module = Identifier::new(parts[1]).ok()?;
        let name = Identifier::new(parts[2]).ok()?;

        Some(StructTag {
            address,
            module,
            name,
            type_params: vec![],
        })
    }
}

#[async_trait::async_trait]
impl IngestionClientTrait for StarcoinRpcClient {
    async fn fetch(&self, checkpoint: u64) -> FetchResult {
        match self.fetch_block_data(checkpoint).await {
            Ok(data) => Ok(FetchData::CheckpointData(data)),
            Err(BlockFetchError::NotFound) => Err(FetchError::NotFound),
            Err(BlockFetchError::Other(e)) => Err(FetchError::Transient {
                reason: "rpc_error",
                error: e,
            }),
        }
    }
}
