// StarcoinClientInner implementation using simple JSON-RPC
// This completely replaces the starcoin-rpc-client SDK

use crate::error::BridgeError;
use crate::simple_starcoin_rpc::SimpleStarcoinRpcClient;
use crate::starcoin_bridge_client::StarcoinClientInner;
use async_trait::async_trait;
use starcoin_bridge_json_rpc_types::{
    EventFilter, EventPage, StarcoinEvent, StarcoinExecutionStatus,
    StarcoinTransactionBlockEffects, StarcoinTransactionBlockResponse,
};
// Use the tuple EventID type from starcoin_bridge_types
use starcoin_bridge_types::base_types::{ObjectID, ObjectRef, TransactionDigest};
use starcoin_bridge_types::bridge::{
    BridgeSummary, MoveTypeParsedTokenTransferMessage, MoveTypeTokenTransferPayload,
};
use starcoin_bridge_types::event::EventID;
use starcoin_bridge_types::gas_coin::GasCoin;
use starcoin_bridge_types::object::Owner;
use starcoin_bridge_types::transaction::{ObjectArg, Transaction};

use crate::types::BridgeActionStatus;

/// Bridge module name
const BRIDGE_MODULE: &str = "Bridge";

/// Transfer status constants (matching Move contract)
const TRANSFER_STATUS_PENDING: u8 = 0;
const TRANSFER_STATUS_APPROVED: u8 = 1;
const TRANSFER_STATUS_CLAIMED: u8 = 2;
const TRANSFER_STATUS_NOT_FOUND: u8 = 3;

/// Helper trait to parse JSON values that might be strings or numbers
trait JsonValueExt {
    /// Try to get a u64 value, handling both numeric and string representations
    fn as_u64_flex(&self) -> Option<u64>;
}

impl JsonValueExt for serde_json::Value {
    fn as_u64_flex(&self) -> Option<u64> {
        self.as_u64()
            .or_else(|| self.as_str().and_then(|s| s.parse().ok()))
    }
}

#[derive(Clone, Debug)]
pub struct StarcoinJsonRpcClient {
    rpc: SimpleStarcoinRpcClient,
}

impl StarcoinJsonRpcClient {
    pub fn new(rpc_url: &str, bridge_address: &str) -> Self {
        Self {
            rpc: SimpleStarcoinRpcClient::new(rpc_url, bridge_address),
        }
    }

    /// Get the underlying RPC client
    pub fn rpc(&self) -> &SimpleStarcoinRpcClient {
        &self.rpc
    }

    /// Get the bridge contract address
    pub fn bridge_address(&self) -> &str {
        self.rpc.bridge_address()
    }

    /// Call a Move view function on the Bridge module
    async fn call_bridge_function(
        &self,
        function_name: &str,
        type_args: Vec<String>,
        args: Vec<String>,
    ) -> Result<serde_json::Value, JsonRpcError> {
        let function_id = format!(
            "{}::{}::{}",
            self.bridge_address(),
            BRIDGE_MODULE,
            function_name
        );
        self.rpc
            .call_contract(&function_id, type_args, args)
            .await
            .map_err(JsonRpcError::from)
    }

    /// Convert u8 status code from Move contract to BridgeActionStatus
    fn parse_transfer_status(status: u8) -> BridgeActionStatus {
        match status {
            TRANSFER_STATUS_PENDING => BridgeActionStatus::Pending,
            TRANSFER_STATUS_APPROVED => BridgeActionStatus::Approved,
            TRANSFER_STATUS_CLAIMED => BridgeActionStatus::Claimed,
            TRANSFER_STATUS_NOT_FOUND => BridgeActionStatus::NotFound,
            _ => BridgeActionStatus::NotFound,
        }
    }

    /// Parse Move response into signatures
    fn parse_signatures_response(response: &serde_json::Value) -> Option<Vec<Vec<u8>>> {
        // Response format from contract.call_v2:
        // [{"type": "option", "value": {"type": "vector", "value": [...]}}]
        if let Some(arr) = response.as_array() {
            if let Some(first) = arr.first() {
                // Check if it's an Option type with Some value
                if let Some(opt_value) = first.get("value") {
                    if !opt_value.is_null() {
                        // Parse vector of vector<u8>
                        if let Some(inner_arr) = opt_value.get("value").and_then(|v| v.as_array()) {
                            let mut signatures = Vec::new();
                            for item in inner_arr {
                                if let Some(bytes) = item.get("value").and_then(|v| v.as_array()) {
                                    let sig: Vec<u8> = bytes
                                        .iter()
                                        .filter_map(|b| b.as_u64().map(|n| n as u8))
                                        .collect();
                                    signatures.push(sig);
                                } else if let Some(hex_str) = item.as_str() {
                                    if let Ok(bytes) = hex::decode(hex_str.trim_start_matches("0x"))
                                    {
                                        signatures.push(bytes);
                                    }
                                }
                            }
                            if !signatures.is_empty() {
                                return Some(signatures);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Parse RPC bridge summary response into BridgeSummary
    fn parse_rpc_bridge_summary(
        rpc_response: &serde_json::Value,
    ) -> Result<BridgeSummary, JsonRpcError> {
        use starcoin_bridge_types::base_types::StarcoinAddress;
        use starcoin_bridge_types::bridge::{
            BridgeCommitteeSummary, BridgeLimiterSummary, BridgeTokenMetadata,
            BridgeTreasurySummary, MoveTypeCommitteeMember,
        };

        // The RPC response has structure: { "json": { "inner": { ... } }, "raw": "..." }
        // Extract the inner bridge data
        let inner = rpc_response
            .get("json")
            .and_then(|j| j.get("inner"))
            .unwrap_or(rpc_response);

        // Parse bridge version and chain id
        let bridge_version = inner
            .get("bridge_version")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        let message_version = inner
            .get("message_version")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u8;
        let chain_id = inner.get("chain_id").and_then(|v| v.as_u64()).unwrap_or(1) as u8;
        let is_frozen = inner
            .get("paused")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Parse committee
        // Structure: { "committee": { "members": { "data": [ { "key": "...", "value": { ... } } ] } } }
        let committee = inner.get("committee").unwrap_or(&serde_json::Value::Null);
        let mut committee_members = vec![];

        if let Some(members_data) = committee
            .get("members")
            .and_then(|m| m.get("data"))
            .and_then(|d| d.as_array())
        {
            for entry in members_data {
                let key = entry.get("key").and_then(|k| k.as_str()).unwrap_or("");
                let value = entry.get("value").unwrap_or(&serde_json::Value::Null);

                let pubkey_hex = value
                    .get("bridge_pubkey_bytes")
                    .and_then(|v| v.as_str())
                    .unwrap_or(key);
                let voting_power = value
                    .get("voting_power")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let address_hex = value
                    .get("starcoin_address")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let blocklisted = value
                    .get("blocklisted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let http_rest_url_hex = value
                    .get("http_rest_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                // Decode pubkey (strip 0x prefix if present)
                let pubkey_clean = pubkey_hex.trim_start_matches("0x");
                if let Ok(pubkey_bytes) = hex::decode(pubkey_clean) {
                    let starcoin_addr = StarcoinAddress::from_hex_literal(address_hex)
                        .unwrap_or(StarcoinAddress::ZERO);

                    // Decode http_rest_url from hex to bytes
                    let url_clean = http_rest_url_hex.trim_start_matches("0x");
                    let http_rest_url = hex::decode(url_clean).unwrap_or_default();

                    committee_members.push((
                        pubkey_bytes.clone(),
                        MoveTypeCommitteeMember {
                            starcoin_bridge_address: starcoin_addr,
                            bridge_pubkey_bytes: pubkey_bytes,
                            voting_power,
                            http_rest_url,
                            blocklisted,
                        },
                    ));
                }
            }
        }

        let last_committee_update_epoch = committee
            .get("last_committee_update_epoch")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let committee_summary = BridgeCommitteeSummary {
            members: committee_members,
            member_registration: vec![],
            last_committee_update_epoch,
        };

        // Parse treasury
        // Structure: { "treasury": { "supported_tokens": { "data": [...] }, "id_token_type_map": { "data": [...] } } }
        let treasury = inner.get("treasury").unwrap_or(&serde_json::Value::Null);
        let mut supported_tokens = vec![];
        let mut id_token_type_map = vec![];

        if let Some(tokens_data) = treasury
            .get("supported_tokens")
            .and_then(|t| t.get("data"))
            .and_then(|d| d.as_array())
        {
            for entry in tokens_data {
                let token_type = entry.get("key").and_then(|k| k.as_str()).unwrap_or("");
                supported_tokens.push((token_type.to_string(), BridgeTokenMetadata::default()));
            }
        }

        if let Some(map_data) = treasury
            .get("id_token_type_map")
            .and_then(|t| t.get("data"))
            .and_then(|d| d.as_array())
        {
            for entry in map_data {
                let id = entry.get("key").and_then(|k| k.as_u64()).unwrap_or(0) as u8;
                let token_type = entry
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                id_token_type_map.push((id, token_type));
            }
        }

        let treasury_summary = BridgeTreasurySummary {
            supported_tokens,
            id_token_type_map,
        };

        // Parse sequence_nums
        let mut sequence_nums = vec![];
        if let Some(seq_data) = inner
            .get("sequence_nums")
            .and_then(|s| s.get("data"))
            .and_then(|d| d.as_array())
        {
            for entry in seq_data {
                let chain_id = entry.get("key").and_then(|k| k.as_u64()).unwrap_or(0) as u8;
                let seq_num = entry.get("value").and_then(|v| v.as_u64()).unwrap_or(0);
                sequence_nums.push((chain_id, seq_num));
            }
        }

        Ok(BridgeSummary {
            bridge_version,
            message_version,
            chain_id,
            sequence_nums,
            committee: committee_summary,
            treasury: treasury_summary,
            bridge_records_id: [0u8; 32], // Default to zero
            limiter: BridgeLimiterSummary::default(),
            is_frozen,
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct JsonRpcError(String);

impl From<anyhow::Error> for JsonRpcError {
    fn from(e: anyhow::Error) -> Self {
        JsonRpcError(e.to_string())
    }
}

impl From<serde_json::Error> for JsonRpcError {
    fn from(e: serde_json::Error) -> Self {
        JsonRpcError(e.to_string())
    }
}

/// Maximum block range allowed by Starcoin RPC for event queries
const MAX_BLOCK_RANGE: u64 = 32;

#[async_trait]
impl StarcoinClientInner for StarcoinJsonRpcClient {
    type Error = JsonRpcError;

    fn bridge_address(&self) -> &str {
        self.rpc.bridge_address()
    }

    async fn query_events(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
    ) -> Result<EventPage, Self::Error> {
        // Get current block height from chain
        let chain_info = self.rpc.chain_info().await?;
        let current_block = chain_info
            .get("head")
            .and_then(|h| h.get("number"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        // Apply cursor as from_block if provided
        // EventID is (block_number, event_index) tuple
        let mut filter = query.clone();
        let from_block = if let Some((block_num, _event_idx)) = cursor {
            // Start from the next block after cursor (cursor is exclusive)
            block_num.saturating_add(1)
        } else {
            filter.from_block.unwrap_or(0)
        };

        // Ensure from_block doesn't exceed current block
        if from_block > current_block {
            // No new blocks to query
            return Ok(EventPage {
                data: vec![],
                next_cursor: cursor,
                has_next_page: false,
            });
        }

        // Set to_block with max range limit (Starcoin limits to 32 blocks)
        let to_block = std::cmp::min(
            from_block.saturating_add(MAX_BLOCK_RANGE - 1),
            current_block,
        );

        filter.from_block = Some(from_block);
        filter.to_block = Some(to_block);

        // Set a reasonable limit
        if filter.limit.is_none() {
            filter.limit = Some(100);
        }

        tracing::debug!(
            from_block = from_block,
            to_block = to_block,
            current_block = current_block,
            "Querying Starcoin events"
        );

        let raw_events = self.rpc.get_events(filter.to_rpc_filter()).await?;

        // Parse events
        let mut events = Vec::new();
        let mut last_block_num = from_block;
        for event_value in raw_events.iter() {
            // Extract tx_hash for event ID
            let tx_hash = event_value
                .get("transaction_hash")
                .and_then(|v| v.as_str())
                .and_then(|s| hex::decode(s.trim_start_matches("0x")).ok())
                .map(|bytes| {
                    let mut arr = [0u8; 32];
                    let len = bytes.len().min(32);
                    arr[..len].copy_from_slice(&bytes[..len]);
                    arr
                })
                .unwrap_or([0u8; 32]);

            // Extract block number for cursor - handle both string and number formats
            if let Some(block_num) = event_value.get("block_number") {
                let parsed_block = if let Some(s) = block_num.as_str() {
                    s.parse::<u64>().ok()
                } else {
                    block_num.as_u64()
                };
                if let Some(bn) = parsed_block {
                    last_block_num = bn;
                }
            }

            if let Ok(event) = StarcoinEvent::try_from_rpc_event(event_value, tx_hash) {
                events.push(event);
            }
        }

        // Determine if there are more blocks to query
        let has_next_page = to_block < current_block;

        // Next cursor: use last queried block for next iteration
        let next_cursor: Option<EventID> = Some((to_block, 0));

        Ok(EventPage {
            data: events,
            next_cursor,
            has_next_page,
        })
    }

    async fn get_events_by_tx_digest(
        &self,
        tx_digest: TransactionDigest,
    ) -> Result<Vec<StarcoinEvent>, Self::Error> {
        let tx_hash = format!("0x{}", hex::encode(tx_digest));
        let raw_events = self.rpc.get_events_by_txn_hash(&tx_hash).await?;

        // Parse each event from RPC response into StarcoinEvent
        let mut events = Vec::new();
        for event_value in raw_events {
            match StarcoinEvent::try_from_rpc_event(&event_value, tx_digest) {
                Ok(event) => events.push(event),
                Err(e) => {
                    tracing::warn!("Failed to parse event: {:?}, error: {}", event_value, e);
                }
            }
        }
        Ok(events)
    }

    async fn get_chain_identifier(&self) -> Result<String, Self::Error> {
        let chain_info = self.rpc.chain_info().await?;
        let chain_id = chain_info
            .get("chain_id")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| JsonRpcError("Missing chain_id".into()))?;
        Ok(format!("{}", chain_id))
    }

    async fn get_reference_gas_price(&self) -> Result<u64, Self::Error> {
        Ok(self.rpc.get_gas_price().await?)
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, Self::Error> {
        let chain_info = self.rpc.chain_info().await?;
        // Starcoin returns block number as string, so try both as_u64() and as_str().parse()
        let block_number = chain_info
            .get("head")
            .and_then(|h| h.get("number"))
            .and_then(|n| {
                n.as_u64()
                    .or_else(|| n.as_str().and_then(|s| s.parse().ok()))
            })
            .ok_or_else(|| JsonRpcError("Missing block number".into()))?;
        Ok(block_number)
    }

    async fn get_mutable_bridge_object_arg(&self) -> Result<ObjectArg, Self::Error> {
        // Return a dummy object arg for now
        // TODO: Query actual bridge object from chain
        use starcoin_bridge_types::STARCOIN_BRIDGE_OBJECT_ID;
        Ok(ObjectArg::SharedObject {
            id: STARCOIN_BRIDGE_OBJECT_ID,
            initial_shared_version: 1,
            mutable: true,
        })
    }

    async fn get_bridge_summary(&self) -> Result<BridgeSummary, Self::Error> {
        // Call bridge.get_latest_bridge RPC
        let rpc_response = self.rpc.get_latest_bridge().await?;

        // Parse the RPC response and convert to BridgeSummary
        // RPC returns: { committee: {...}, treasury: {...}, config: {...} }
        Self::parse_rpc_bridge_summary(&rpc_response)
    }

    async fn execute_transaction_block_with_effects(
        &self,
        tx: Transaction,
    ) -> Result<StarcoinTransactionBlockResponse, BridgeError> {
        // Transaction wraps serialized signed transaction bytes
        let signed_txn_hex = hex::encode(&tx.0);

        // Submit and wait for transaction confirmation
        let txn_info = self
            .rpc
            .submit_and_wait_transaction(&signed_txn_hex)
            .await
            .map_err(|e| BridgeError::Generic(format!("Transaction execution failed: {}", e)))?;

        // Parse the response into StarcoinTransactionBlockResponse
        let tx_hash = txn_info
            .get("transaction_hash")
            .and_then(|v| v.as_str())
            .and_then(|s| hex::decode(s.trim_start_matches("0x")).ok())
            .map(|bytes| {
                let mut arr = [0u8; 32];
                let len = bytes.len().min(32);
                arr[..len].copy_from_slice(&bytes[..len]);
                arr
            })
            .unwrap_or([0u8; 32]);

        let status = txn_info
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let success = status == "Executed" || status == "executed";

        Ok(StarcoinTransactionBlockResponse {
            digest: Some(tx_hash),
            effects: Some(StarcoinTransactionBlockEffects {
                status: if success {
                    StarcoinExecutionStatus::Success
                } else {
                    StarcoinExecutionStatus::Failure {
                        error: status.to_string(),
                    }
                },
            }),
            events: None,
            object_changes: None,
        })
    }

    async fn get_token_transfer_action_onchain_status(
        &self,
        _bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<BridgeActionStatus, BridgeError> {
        // Call query_token_transfer_status via contract.call_v2
        // Function signature: query_token_transfer_status(source_chain: u8, bridge_seq_num: u64): u8
        // Note: Starcoin contract.call_v2 requires type suffix on arguments (e.g., "12u8", "0u64")
        let args = vec![
            format!("{}u8", source_chain_id), // source_chain as u8
            format!("{}u64", seq_number),     // bridge_seq_num as u64
        ];

        match self
            .call_bridge_function("query_token_transfer_status", vec![], args)
            .await
        {
            Ok(response) => {
                // Parse u8 status from response
                // Response format: [1] (direct array of values)
                let status = response
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u8)
                    .unwrap_or(TRANSFER_STATUS_NOT_FOUND);

                tracing::debug!(
                    "Query transfer status response: {:?}, parsed status: {}",
                    response,
                    status
                );
                Ok(Self::parse_transfer_status(status))
            }
            Err(e) => {
                tracing::warn!("Failed to query transfer status: {:?}", e);
                // If function call fails (e.g., function not found), return NotFound
                Ok(BridgeActionStatus::NotFound)
            }
        }
    }

    async fn get_token_transfer_action_onchain_signatures(
        &self,
        _bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<Vec<Vec<u8>>>, BridgeError> {
        // Call query_token_transfer_signatures via contract.call_v2
        // Note: Starcoin contract.call_v2 requires type suffix on arguments
        let args = vec![
            format!("{}u8", source_chain_id),
            format!("{}u64", seq_number),
        ];

        match self
            .call_bridge_function("query_token_transfer_signatures", vec![], args)
            .await
        {
            Ok(response) => Ok(Self::parse_signatures_response(&response)),
            Err(e) => {
                tracing::warn!("Failed to query transfer signatures: {:?}", e);
                Ok(None)
            }
        }
    }

    async fn get_parsed_token_transfer_message(
        &self,
        _bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<MoveTypeParsedTokenTransferMessage>, BridgeError> {
        // Call get_parsed_token_transfer_message via contract.call_v2
        // Note: Starcoin contract.call_v2 requires type suffix on arguments
        let args = vec![
            format!("{}u8", source_chain_id),
            format!("{}u64", seq_number),
        ];

        match self
            .call_bridge_function("test_get_parsed_token_transfer_message", vec![], args)
            .await
        {
            Ok(response) => {
                // Parse the response into MoveTypeParsedTokenTransferMessage
                // Response format: [{"type": "option", "value": {...}}]
                if let Some(arr) = response.as_array() {
                    if let Some(first) = arr.first() {
                        if let Some(opt_value) = first.get("value") {
                            if !opt_value.is_null() {
                                // Parse the struct fields
                                let message_version = opt_value
                                    .get("message_version")
                                    .and_then(|v| v.as_u64())
                                    .map(|n| n as u8)
                                    .unwrap_or(1);

                                let seq_num = opt_value
                                    .get("seq_num")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(seq_number);

                                let source_chain = opt_value
                                    .get("source_chain")
                                    .and_then(|v| v.as_u64())
                                    .map(|n| n as u8)
                                    .unwrap_or(source_chain_id);

                                let payload = opt_value
                                    .get("payload")
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| hex::decode(s.trim_start_matches("0x")).ok())
                                    .unwrap_or_default();

                                // Parse parsed_payload struct
                                let parsed_payload = opt_value.get("parsed_payload");
                                let sender_address = parsed_payload
                                    .and_then(|p| p.get("sender_address"))
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| hex::decode(s.trim_start_matches("0x")).ok())
                                    .unwrap_or_default();

                                let target_chain = parsed_payload
                                    .and_then(|p| p.get("target_chain"))
                                    .and_then(|v| v.as_u64())
                                    .map(|n| n as u8)
                                    .unwrap_or(0);

                                let target_address = parsed_payload
                                    .and_then(|p| p.get("target_address"))
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| hex::decode(s.trim_start_matches("0x")).ok())
                                    .unwrap_or_default();

                                let token_type = parsed_payload
                                    .and_then(|p| p.get("token_type"))
                                    .and_then(|v| v.as_u64())
                                    .map(|n| n as u8)
                                    .unwrap_or(0);

                                let amount = parsed_payload
                                    .and_then(|p| p.get("amount"))
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);

                                return Ok(Some(MoveTypeParsedTokenTransferMessage {
                                    message_version,
                                    seq_num,
                                    source_chain,
                                    payload,
                                    parsed_payload: MoveTypeTokenTransferPayload {
                                        sender_address,
                                        target_chain,
                                        target_address,
                                        token_type,
                                        amount,
                                    },
                                }));
                            }
                        }
                    }
                }
                Ok(None)
            }
            Err(e) => {
                tracing::warn!("Failed to query parsed token transfer message: {:?}", e);
                Ok(None)
            }
        }
    }

    async fn get_gas_data_panic_if_not_gas(
        &self,
        gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner) {
        // Query account balance for the gas object
        // For Starcoin, gas is STC balance, not a separate object
        // We return a dummy value since Starcoin handles gas differently
        let gas_coin = GasCoin {
            value: 1_000_000_000,
        }; // 1 STC in micros
        let object_ref = (gas_object_id, 1u64, [0u8; 32]);
        let owner = Owner::AddressOwner(starcoin_bridge_types::base_types::StarcoinAddress::ZERO);

        (gas_coin, object_ref, owner)
    }

    async fn get_sequence_number(&self, address: &str) -> Result<u64, BridgeError> {
        self.rpc
            .get_sequence_number(address)
            .await
            .map_err(|e| BridgeError::Generic(format!("Failed to get sequence number: {}", e)))
    }

    async fn get_block_timestamp(&self) -> Result<u64, BridgeError> {
        self.rpc
            .get_block_timestamp()
            .await
            .map_err(|e| BridgeError::Generic(format!("Failed to get block timestamp: {}", e)))
    }

    async fn sign_and_submit_transaction(
        &self,
        key: &starcoin_bridge_types::crypto::StarcoinKeyPair,
        raw_txn: starcoin_bridge_types::transaction::RawUserTransaction,
    ) -> Result<String, BridgeError> {
        self.rpc
            .sign_and_submit_transaction(key, raw_txn)
            .await
            .map_err(|e| {
                BridgeError::Generic(format!("Failed to sign and submit transaction: {}", e))
            })
    }
}
