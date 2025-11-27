// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A mock implementation of Starcoin JSON-RPC client.

use crate::error::{BridgeError, BridgeResult};
use async_trait::async_trait;
use starcoin_bridge_json_rpc_types::StarcoinTransactionBlockResponse;
use starcoin_bridge_json_rpc_types::{EventFilter, EventPage, StarcoinEvent};
use starcoin_bridge_types::base_types::{ObjectID, ObjectRef, TransactionDigest};
use starcoin_bridge_types::bridge::{
    BridgeCommitteeSummary, BridgeSummary, MoveTypeParsedTokenTransferMessage,
};
use starcoin_bridge_types::event::EventID;
use starcoin_bridge_types::gas_coin::GasCoin;
use starcoin_bridge_types::object::Owner;
use starcoin_bridge_types::transaction::{ObjectArg, Transaction};
use starcoin_bridge_types::Identifier;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use crate::starcoin_bridge_client::StarcoinClientInner;
use crate::types::{BridgeAction, BridgeActionStatus, IsBridgePaused};

// Dummy bridge object arg function
pub fn dummy_bridge_object_arg() -> ObjectArg {
    ObjectArg::ImmOrOwnedObject((
        [0u8; 32],
        0,
        [0u8; 32],
    ))
}

// Mock client used in test environments.
#[allow(clippy::type_complexity)]
#[derive(Clone, Debug)]
pub struct StarcoinMockClient {
    // the top two fields do not change during tests so we don't need them to be Arc<Mutex>>
    chain_identifier: String,
    latest_checkpoint_sequence_number: Arc<AtomicU64>,
    events: Arc<Mutex<HashMap<(ObjectID, Identifier, Option<EventID>), EventPage>>>,
    past_event_query_params: Arc<Mutex<VecDeque<(ObjectID, Identifier, Option<EventID>)>>>,
    events_by_tx_digest: Arc<
        Mutex<
            HashMap<
                TransactionDigest,
                Result<Vec<StarcoinEvent>, starcoin_bridge_sdk::error::Error>,
            >,
        >,
    >,
    transaction_responses:
        Arc<Mutex<HashMap<TransactionDigest, BridgeResult<StarcoinTransactionBlockResponse>>>>,
    wildcard_transaction_response:
        Arc<Mutex<Option<BridgeResult<StarcoinTransactionBlockResponse>>>>,
    get_object_info: Arc<Mutex<HashMap<ObjectID, (GasCoin, ObjectRef, Owner)>>>,
    onchain_status: Arc<Mutex<HashMap<(u8, u64), BridgeActionStatus>>>,
    bridge_committee_summary: Arc<Mutex<Option<BridgeCommitteeSummary>>>,
    is_paused: Arc<Mutex<Option<IsBridgePaused>>>,
    requested_transactions_tx: tokio::sync::broadcast::Sender<TransactionDigest>,
}

impl StarcoinMockClient {
    pub fn default() -> Self {
        Self {
            chain_identifier: "".to_string(),
            latest_checkpoint_sequence_number: Arc::new(AtomicU64::new(0)),
            events: Default::default(),
            past_event_query_params: Default::default(),
            events_by_tx_digest: Default::default(),
            transaction_responses: Default::default(),
            wildcard_transaction_response: Default::default(),
            get_object_info: Default::default(),
            onchain_status: Default::default(),
            bridge_committee_summary: Default::default(),
            is_paused: Default::default(),
            requested_transactions_tx: tokio::sync::broadcast::channel(10000).0,
        }
    }

    pub fn add_event_response(
        &self,
        package: ObjectID,
        module: Identifier,
        cursor: EventID,
        events: EventPage,
    ) {
        self.events
            .lock()
            .unwrap()
            .insert((package, module, Some(cursor)), events);
    }

    pub fn add_events_by_tx_digest(
        &self,
        tx_digest: TransactionDigest,
        events: Vec<StarcoinEvent>,
    ) {
        self.events_by_tx_digest
            .lock()
            .unwrap()
            .insert(tx_digest, Ok(events));
    }

    pub fn add_events_by_tx_digest_error(&self, tx_digest: TransactionDigest) {
        self.events_by_tx_digest.lock().unwrap().insert(
            tx_digest,
            Err(starcoin_bridge_sdk::error::Error::StarcoinError(
                "".to_string(),
            )),
        );
    }

    pub fn add_transaction_response(
        &self,
        tx_digest: TransactionDigest,
        response: BridgeResult<StarcoinTransactionBlockResponse>,
    ) {
        self.transaction_responses
            .lock()
            .unwrap()
            .insert(tx_digest, response);
    }

    pub fn set_action_onchain_status(&self, action: &BridgeAction, status: BridgeActionStatus) {
        self.onchain_status
            .lock()
            .unwrap()
            .insert((action.chain_id() as u8, action.seq_number()), status);
    }

    pub fn set_bridge_committee(&self, committee: BridgeCommitteeSummary) {
        self.bridge_committee_summary
            .lock()
            .unwrap()
            .replace(committee);
    }

    pub fn set_is_bridge_paused(&self, value: IsBridgePaused) {
        self.is_paused.lock().unwrap().replace(value);
    }

    pub fn set_wildcard_transaction_response(
        &self,
        response: BridgeResult<StarcoinTransactionBlockResponse>,
    ) {
        *self.wildcard_transaction_response.lock().unwrap() = Some(response);
    }

    pub fn set_latest_checkpoint_sequence_number(&self, value: u64) {
        self.latest_checkpoint_sequence_number
            .store(value, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn add_gas_object_info(&self, gas_coin: GasCoin, object_ref: ObjectRef, owner: Owner) {
        self.get_object_info
            .lock()
            .unwrap()
            .insert(object_ref.0, (gas_coin, object_ref, owner));
    }

    pub fn subscribe_to_requested_transactions(
        &self,
    ) -> tokio::sync::broadcast::Receiver<TransactionDigest> {
        self.requested_transactions_tx.subscribe()
    }
}

#[async_trait]
impl StarcoinClientInner for StarcoinMockClient {
    type Error = starcoin_bridge_sdk::error::Error;

    // Unwraps in this function: We assume the responses are pre-populated
    // by the test before calling into this function.
    async fn query_events(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
    ) -> Result<EventPage, Self::Error> {
        let events = self.events.lock().unwrap();
        
        // EventFilter is now a struct with type_tags field
        // Extract module info from type_tags if available
        if let Some(type_tags) = &query.type_tags {
            if let Some(first_tag) = type_tags.first() {
                // Parse module and package from type tag string
                // Format: "0x{address}::{module}::{struct}"
                let parts: Vec<&str> = first_tag.split("::").collect();
                if parts.len() >= 2 {
                    let package_hex = parts[0].trim_start_matches("0x");
                    let mut package = [0u8; 32];
                    if let Ok(bytes) = hex::decode(package_hex) {
                        let len = bytes.len().min(32);
                        package[32 - len..].copy_from_slice(&bytes[..len]);
                    }
                    let module = parts[1].to_string();
                    let module_id = Identifier::new(module.as_str()).unwrap();
                    let key = (package, module_id, cursor);
                    self.past_event_query_params
                        .lock()
                        .unwrap()
                        .push_back(key.clone());
                    return Ok(events.get(&key).cloned().unwrap_or_else(|| {
                        panic!(
                            "No preset events found for type_tag: {:?}, cursor: {:?}",
                            first_tag, cursor
                        )
                    }));
                }
            }
        }
        
        // Default: return empty page
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
        let events = self.events_by_tx_digest.lock().unwrap();

        match events
            .get(&tx_digest)
            .unwrap_or_else(|| panic!("No preset events found for tx_digest: {:?}", tx_digest))
        {
            Ok(events) => Ok(events.clone()),
            // starcoin_bridge_sdk::error::Error is not Clone
            Err(_) => Err(starcoin_bridge_sdk::error::Error::StarcoinError(
                "Mock error".to_string(),
            )),
        }
    }

    async fn get_chain_identifier(&self) -> Result<String, Self::Error> {
        Ok(self.chain_identifier.clone())
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, Self::Error> {
        Ok(self
            .latest_checkpoint_sequence_number
            .load(std::sync::atomic::Ordering::Relaxed))
    }

    async fn get_mutable_bridge_object_arg(&self) -> Result<ObjectArg, Self::Error> {
        Ok(dummy_bridge_object_arg())
    }

    async fn get_reference_gas_price(&self) -> Result<u64, Self::Error> {
        Ok(1000)
    }

    async fn get_bridge_summary(&self) -> Result<BridgeSummary, Self::Error> {
        Ok(BridgeSummary {
            bridge_version: 0,
            message_version: 0,
            chain_id: 0,
            sequence_nums: vec![],
            bridge_records_id: [0u8; 32],
            is_frozen: self.is_paused.lock().unwrap().unwrap_or_default(),
            limiter: Default::default(),
            committee: self
                .bridge_committee_summary
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_default(),
            treasury: Default::default(),
        })
    }

    async fn get_token_transfer_action_onchain_status(
        &self,
        _bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<BridgeActionStatus, BridgeError> {
        Ok(self
            .onchain_status
            .lock()
            .unwrap()
            .get(&(source_chain_id, seq_number))
            .cloned()
            .unwrap_or(BridgeActionStatus::Pending))
    }

    async fn get_token_transfer_action_onchain_signatures(
        &self,
        _bridge_object_arg: ObjectArg,
        _source_chain_id: u8,
        _seq_number: u64,
    ) -> Result<Option<Vec<Vec<u8>>>, BridgeError> {
        unimplemented!()
    }

    async fn get_parsed_token_transfer_message(
        &self,
        _bridge_object_arg: ObjectArg,
        _source_chain_id: u8,
        _seq_number: u64,
    ) -> Result<Option<MoveTypeParsedTokenTransferMessage>, BridgeError> {
        unimplemented!()
    }

    async fn execute_transaction_block_with_effects(
        &self,
        tx: Transaction,
    ) -> Result<StarcoinTransactionBlockResponse, BridgeError> {
        self.requested_transactions_tx.send(*tx.digest()).unwrap();
        match self.transaction_responses.lock().unwrap().get(tx.digest()) {
            Some(response) => response.clone(),
            None => self
                .wildcard_transaction_response
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| panic!("No preset transaction response found for tx: {:?}", tx)),
        }
    }

    async fn get_gas_data_panic_if_not_gas(
        &self,
        gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner) {
        self.get_object_info
            .lock()
            .unwrap()
            .get(&gas_object_id)
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "No preset gas object info found for gas_object_id: {:?}",
                    gas_object_id
                )
            })
    }

    async fn get_sequence_number(&self, _address: &str) -> Result<u64, BridgeError> {
        // Mock implementation for testing
        Ok(0)
    }

    async fn sign_and_submit_transaction(
        &self,
        _key: &starcoin_bridge_types::crypto::StarcoinKeyPair,
        _raw_txn: starcoin_bridge_types::transaction::RawUserTransaction,
    ) -> Result<String, BridgeError> {
        // Mock implementation for testing
        Err(BridgeError::Generic("Mock transaction submission not implemented".into()))
    }
}
