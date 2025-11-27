// StarcoinClientInner implementation using simple JSON-RPC
// This completely replaces the starcoin-rpc-client SDK

use crate::error::BridgeError;
use crate::simple_starcoin_rpc::SimpleStarcoinRpcClient;
use crate::starcoin_bridge_client::StarcoinClientInner;
use async_trait::async_trait;
use starcoin_bridge_json_rpc_types::StarcoinTransactionBlockResponse;
use starcoin_bridge_json_rpc_types::{EventFilter, EventPage, StarcoinEvent};
use starcoin_bridge_types::base_types::{ObjectID, ObjectRef, TransactionDigest};
use starcoin_bridge_types::bridge::{BridgeSummary, MoveTypeParsedTokenTransferMessage};
use starcoin_bridge_types::event::EventID;
use starcoin_bridge_types::gas_coin::GasCoin;
use starcoin_bridge_types::object::Owner;
use starcoin_bridge_types::transaction::{ObjectArg, Transaction};

use crate::types::BridgeActionStatus;

const BRIDGE_ADDRESS: &str = "0x246b237c16c761e9478783dd83f7004a";
const BRIDGE_MODULE: &str = "Bridge";
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
        _query: EventFilter,
        _cursor: Option<EventID>,
    ) -> Result<EventPage, Self::Error> {
        // TODO: Implement event querying via RPC
        Ok(EventPage {
            data: vec![],
            next_cursor: None,
            has_next_page: false,
        })
    }

    async fn get_events_by_tx_digest(
        &self,
        tx_digest: TransactionDigest,
    ) -> Result<Vec<StarcoinEvent>, Self::Error> {
        let tx_hash = format!("0x{}", hex::encode(tx_digest));
        let _events = self.rpc.get_events_by_txn_hash(&tx_hash).await?;
        
        // TODO: Parse events into StarcoinEvent format
        Ok(vec![])
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
        let resource_type = format!("{}::{}::{}", BRIDGE_ADDRESS, BRIDGE_MODULE, BRIDGE_RESOURCE);
        
        let resource = self
            .rpc
            .get_resource(BRIDGE_ADDRESS, &resource_type)
            .await?
            .ok_or_else(|| JsonRpcError(format!("Bridge resource not found at {}", BRIDGE_ADDRESS)))?;

        // Parse the resource into BridgeSummary
        // TODO: Implement proper parsing based on Move resource structure
        let summary = serde_json::from_value(resource)?;
        Ok(summary)
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
