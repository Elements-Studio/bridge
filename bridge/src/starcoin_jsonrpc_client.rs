// StarcoinClientInner implementation using simple JSON-RPC
// This completely replaces the starcoin-rpc-client SDK

use crate::error::BridgeError;
use crate::simple_starcoin_rpc::SimpleStarcoinRpcClient;
use crate::starcoin_bridge_client::StarcoinClientInner;
use async_trait::async_trait;
use starcoin_bridge_json_rpc_types::StarcoinTransactionBlockResponse;
use starcoin_bridge_json_rpc_types::{EventFilter, EventPage, StarcoinEvent};
// Use the tuple EventID type from starcoin_bridge_types
use starcoin_bridge_types::event::EventID;
use starcoin_bridge_types::base_types::{ObjectID, ObjectRef, TransactionDigest};
use starcoin_bridge_types::bridge::{BridgeSummary, MoveTypeParsedTokenTransferMessage};
use starcoin_bridge_types::gas_coin::GasCoin;
use starcoin_bridge_types::object::Owner;
use starcoin_bridge_types::transaction::{ObjectArg, Transaction};

use crate::types::BridgeActionStatus;

#[allow(dead_code)]
const BRIDGE_ADDRESS: &str = "0x246b237c16c761e9478783dd83f7004a";
#[allow(dead_code)]
const BRIDGE_MODULE: &str = "Bridge";
#[allow(dead_code)]
const BRIDGE_RESOURCE: &str = "Bridge";

#[derive(Clone, Debug)]
pub struct StarcoinJsonRpcClient {
    rpc: SimpleStarcoinRpcClient,
}

impl StarcoinJsonRpcClient {
    pub fn new(rpc_url: &str) -> Self {
        Self {
            rpc: SimpleStarcoinRpcClient::new(rpc_url),
        }
    }

    /// Parse RPC bridge summary response into BridgeSummary
    fn parse_rpc_bridge_summary(rpc_response: &serde_json::Value) -> Result<BridgeSummary, JsonRpcError> {
        use starcoin_bridge_types::bridge::{
            BridgeCommitteeSummary, BridgeLimiterSummary,
            BridgeTreasurySummary, MoveTypeCommitteeMember, BridgeTokenMetadata,
        };
        use starcoin_bridge_types::base_types::StarcoinAddress;

        // Parse committee
        let committee = rpc_response.get("committee").unwrap_or(&serde_json::Value::Null);
        let mut committee_members = vec![];
        
        if let Some(members) = committee.get("members").and_then(|v| v.as_array()) {
            for member in members {
                let pubkey_hex = member.get("public_key").and_then(|v| v.as_str()).unwrap_or("");
                let voting_power = member.get("stake").and_then(|v| v.as_u64()).unwrap_or(0);
                let address_hex = member.get("address").and_then(|v| v.as_str()).unwrap_or("");
                
                if let Ok(pubkey_bytes) = hex::decode(pubkey_hex) {
                    let starcoin_addr = StarcoinAddress::from_hex_literal(address_hex)
                        .unwrap_or(StarcoinAddress::ZERO);
                    committee_members.push((pubkey_bytes.clone(), MoveTypeCommitteeMember {
                        starcoin_bridge_address: starcoin_addr,
                        bridge_pubkey_bytes: pubkey_bytes,
                        voting_power,
                        http_rest_url: vec![],
                        blocklisted: false,
                    }));
                }
            }
        }

        let committee_summary = BridgeCommitteeSummary {
            members: committee_members,
            member_registration: vec![],
            last_committee_update_epoch: 0,
        };

        // Parse treasury
        let treasury = rpc_response.get("treasury").unwrap_or(&serde_json::Value::Null);
        let mut supported_tokens = vec![];
        
        if let Some(tokens) = treasury.get("tokens").and_then(|v| v.as_array()) {
            for token in tokens {
                let token_type = token.get("token_type").and_then(|v| v.as_str()).unwrap_or("");
                supported_tokens.push((token_type.to_string(), BridgeTokenMetadata::default()));
            }
        }

        let treasury_summary = BridgeTreasurySummary {
            supported_tokens,
            id_token_type_map: vec![],
        };

        // Parse config
        let config = rpc_response.get("config").unwrap_or(&serde_json::Value::Null);
        let is_frozen = !config.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);

        Ok(BridgeSummary {
            bridge_version: 1,
            message_version: 1,
            chain_id: 1, // TODO: Get from chain info
            sequence_nums: vec![],
            committee: committee_summary,
            treasury: treasury_summary,
            bridge_records_id: [0u8; 32], // Default to zero, will be queried from chain
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

#[async_trait]
impl StarcoinClientInner for StarcoinJsonRpcClient {
    type Error = JsonRpcError;

    async fn query_events(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
    ) -> Result<EventPage, Self::Error> {
        // Apply cursor as from_block if provided
        // EventID is (block_number, event_index) tuple
        let mut filter = query.clone();
        if let Some((block_num, _event_idx)) = cursor {
            filter.from_block = Some(block_num);
        }

        let raw_events = self.rpc.get_events(filter.to_rpc_filter()).await?;
        
        // Parse events
        let mut events = Vec::new();
        let mut last_block_num = 0u64;
        for (_idx, event_value) in raw_events.iter().enumerate() {
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

            // Extract block number for cursor
            if let Some(block_num) = event_value.get("block_number").and_then(|v| v.as_u64()) {
                last_block_num = block_num;
            }

            if let Ok(event) = StarcoinEvent::try_from_rpc_event(event_value, tx_hash) {
                events.push(event);
            }
        }

        // Determine if there are more pages
        let has_next_page = !raw_events.is_empty() && raw_events.len() >= filter.limit.unwrap_or(1000);
        // Cursor is (block_number, event_count) for next query
        let next_cursor: Option<EventID> = if has_next_page {
            Some((last_block_num, events.len() as u64))
        } else {
            None
        };

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
        let block_number = chain_info
            .get("head")
            .and_then(|h| h.get("number"))
            .and_then(|n| n.as_u64())
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
        _tx: Transaction,
    ) -> Result<StarcoinTransactionBlockResponse, BridgeError> {
        // TODO: Implement transaction execution via RPC
        Err(BridgeError::Generic("Transaction execution not yet implemented via JSON-RPC".into()))
    }

    async fn get_token_transfer_action_onchain_status(
        &self,
        _bridge_object_arg: ObjectArg,
        _source_chain_id: u8,
        _seq_number: u64,
    ) -> Result<BridgeActionStatus, BridgeError> {
        // TODO: Query on-chain status via RPC
        Ok(BridgeActionStatus::Pending)
    }

    async fn get_token_transfer_action_onchain_signatures(
        &self,
        _bridge_object_arg: ObjectArg,
        _source_chain_id: u8,
        _seq_number: u64,
    ) -> Result<Option<Vec<Vec<u8>>>, BridgeError> {
        // TODO: Query signatures via RPC
        Ok(None)
    }

    async fn get_parsed_token_transfer_message(
        &self,
        _bridge_object_arg: ObjectArg,
        _source_chain_id: u8,
        _seq_number: u64,
    ) -> Result<Option<MoveTypeParsedTokenTransferMessage>, BridgeError> {
        // TODO: Query and parse message via RPC
        Ok(None)
    }

    async fn get_gas_data_panic_if_not_gas(
        &self,
        _gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner) {
        // TODO: Query gas coin data via RPC
        panic!("get_gas_data_panic_if_not_gas not yet implemented via JSON-RPC")
    }
}
