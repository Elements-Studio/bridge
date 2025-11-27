// Proxy-based Starcoin client that communicates with starcoin-rpc-proxy subprocess
// This avoids nested tokio runtime issues

use crate::error::{BridgeError, BridgeResult};
use crate::starcoin_bridge_client::StarcoinClientInner;
use crate::starcoin_rpc_proxy_client::StarcoinRpcProxyClient;
use async_trait::async_trait;
use once_cell::sync::{Lazy, OnceCell};
use starcoin_bridge_json_rpc_types::StarcoinTransactionBlockResponse;
use starcoin_bridge_json_rpc_types::{EventFilter, EventPage, StarcoinEvent};
use starcoin_bridge_types::base_types::{ObjectID, ObjectRef, TransactionDigest};
use starcoin_bridge_types::bridge::{
    BridgeSummary, MoveTypeParsedTokenTransferMessage,
};
use starcoin_bridge_types::event::EventID;
use starcoin_bridge_types::gas_coin::GasCoin;
use starcoin_bridge_types::object::Owner;
use starcoin_bridge_types::transaction::{ObjectArg, Transaction};
use std::sync::Arc;

use crate::types::BridgeActionStatus;

// Dummy bridge object arg - matches test_utils::DUMMY_MUTALBE_BRIDGE_OBJECT_ARG
static DUMMY_BRIDGE_OBJECT_ARG: Lazy<ObjectArg> = Lazy::new(|| {
    ObjectArg::ImmOrOwnedObject((
        [0u8; 32], // ObjectID::ZERO equivalent
        0,
        [0u8; 32],
    ))
});

// Global proxy client singleton
static PROXY_CLIENT: OnceCell<Arc<StarcoinRpcProxyClient>> = OnceCell::new();

pub fn init_global_proxy(proxy_bin_path: &str, rpc_url: &str) -> anyhow::Result<()> {
    let client = StarcoinRpcProxyClient::spawn(proxy_bin_path)?;
    client.connect(rpc_url)?;
    client.ping()?;
    PROXY_CLIENT
        .set(Arc::new(client))
        .map_err(|_| anyhow::anyhow!("Proxy already initialized"))?;
    Ok(())
}

fn get_proxy() -> anyhow::Result<Arc<StarcoinRpcProxyClient>> {
    PROXY_CLIENT
        .get()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Proxy not initialized"))
}

#[derive(Clone, Debug)]
pub struct StarcoinProxyClient {
    // Empty struct - all state is in the global proxy
}

impl Default for StarcoinProxyClient {
    fn default() -> Self {
        Self::new()
    }
}

impl StarcoinProxyClient {
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ProxyError(String);

impl From<anyhow::Error> for ProxyError {
    fn from(e: anyhow::Error) -> Self {
        ProxyError(e.to_string())
    }
}

impl From<serde_json::Error> for ProxyError {
    fn from(e: serde_json::Error) -> Self {
        ProxyError(e.to_string())
    }
}

#[async_trait]
impl StarcoinClientInner for StarcoinProxyClient {
    type Error = ProxyError;

    async fn query_events(
        &self,
        _query: EventFilter,
        _cursor: Option<EventID>,
    ) -> Result<EventPage, Self::Error> {
        // TODO: Add query_events to proxy protocol
        Ok(EventPage {
            data: vec![],
            next_cursor: None,
            has_next_page: false,
        })
    }

    async fn get_events_by_tx_digest(
        &self,
        _tx_digest: TransactionDigest,
    ) -> Result<Vec<StarcoinEvent>, Self::Error> {
        // TODO: Add get_events_by_tx_digest to proxy protocol
        Ok(vec![])
    }

    async fn get_chain_identifier(&self) -> Result<String, Self::Error> {
        let proxy = get_proxy().map_err(|e| ProxyError(e.to_string()))?;
        proxy.get_chain_identifier().map_err(ProxyError::from)
    }

    async fn get_reference_gas_price(&self) -> Result<u64, Self::Error> {
        // TODO: Add get_reference_gas_price to proxy protocol
        Ok(1000)
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, Self::Error> {
        let proxy = get_proxy().map_err(|e| ProxyError(e.to_string()))?;
        proxy.get_latest_checkpoint_sequence_number().map_err(ProxyError::from)
    }

    async fn get_mutable_bridge_object_arg(&self) -> Result<ObjectArg, Self::Error> {
        // TODO: Add get_mutable_bridge_object_arg to proxy protocol
        // Use dummy value for now
        Ok(DUMMY_BRIDGE_OBJECT_ARG.clone())
    }

    async fn get_bridge_summary(&self) -> Result<BridgeSummary, Self::Error> {
        let proxy = get_proxy()?;
        let value = proxy.get_bridge_summary()?;
        Ok(serde_json::from_value(value)?)
    }

    async fn execute_transaction_block_with_effects(
        &self,
        _tx: Transaction,
    ) -> Result<StarcoinTransactionBlockResponse, BridgeError> {
        // TODO: Add execute_transaction_block_with_effects to proxy protocol
        Err(BridgeError::Generic("Not implemented in proxy client".into()))
    }

    async fn get_token_transfer_action_onchain_status(
        &self,
        _bridge_object_arg: ObjectArg,
        _source_chain_id: u8,
        _seq_number: u64,
    ) -> Result<BridgeActionStatus, BridgeError> {
        // TODO: Add get_token_transfer_action_onchain_status to proxy protocol
        Ok(BridgeActionStatus::Pending)
    }

    async fn get_token_transfer_action_onchain_signatures(
        &self,
        _bridge_object_arg: ObjectArg,
        _source_chain_id: u8,
        _seq_number: u64,
    ) -> Result<Option<Vec<Vec<u8>>>, BridgeError> {
        // TODO: Add get_token_transfer_action_onchain_signatures to proxy protocol
        Ok(None)
    }

    async fn get_parsed_token_transfer_message(
        &self,
        _bridge_object_arg: ObjectArg,
        _source_chain_id: u8,
        _seq_number: u64,
    ) -> Result<Option<MoveTypeParsedTokenTransferMessage>, BridgeError> {
        // TODO: Add get_parsed_token_transfer_message to proxy protocol
        Ok(None)
    }

    async fn get_gas_data_panic_if_not_gas(
        &self,
        _gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner) {
        // TODO: Add get_gas_data_panic_if_not_gas to proxy protocol
        panic!("Not implemented in proxy client")
    }
}
