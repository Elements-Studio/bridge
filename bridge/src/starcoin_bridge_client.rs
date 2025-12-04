// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
use anyhow::anyhow;
use async_trait::async_trait;
use core::panic;
use fastcrypto::traits::ToFromBytes;
#[cfg(test)]
use serde::de::DeserializeOwned;
#[cfg(test)]
use starcoin_bridge_json_rpc_api::BridgeReadApiClient;
#[cfg(test)]
use starcoin_bridge_json_rpc_types::DevInspectResults;
use starcoin_bridge_json_rpc_types::{EventFilter, Page, StarcoinEvent};
use starcoin_bridge_json_rpc_types::{EventPage, StarcoinTransactionBlockResponse};
#[cfg(test)]
use starcoin_bridge_json_rpc_types::{
    StarcoinObjectDataOptions, StarcoinTransactionBlockResponseOptions,
};
#[cfg(test)]
use starcoin_bridge_sdk::{StarcoinClient as StarcoinSdkClient, StarcoinClientBuilder};
use starcoin_bridge_types::base_types::ObjectRef;
#[cfg(test)]
use starcoin_bridge_types::base_types::StarcoinAddress;
use starcoin_bridge_types::base_types::{ObjectID, TransactionDigest};
use starcoin_bridge_types::bridge::{
    BridgeSummary, BridgeTreasurySummary, MoveTypeCommitteeMember,
    MoveTypeParsedTokenTransferMessage,
};
use starcoin_bridge_types::event::EventID;
use starcoin_bridge_types::gas_coin::GasCoin;
use starcoin_bridge_types::object::Owner;
use starcoin_bridge_types::parse_starcoin_bridge_type_tag;
#[cfg(test)]
use starcoin_bridge_types::transaction::{
    Argument, CallArg, Command, ProgrammableTransaction, TransactionKind,
};
use starcoin_bridge_types::transaction::{ObjectArg, Transaction};
use starcoin_bridge_types::Identifier;
use starcoin_bridge_types::TypeTag;
#[cfg(test)]
use starcoin_bridge_types::BRIDGE_PACKAGE_ID;
#[cfg(test)]
use starcoin_bridge_types::STARCOIN_BRIDGE_OBJECT_ID;
use std::collections::HashMap;
use std::str::from_utf8;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OnceCell;
use tracing::{error, warn};

use crate::crypto::BridgeAuthorityPublicKey;
use crate::error::{BridgeError, BridgeResult};
use crate::events::StarcoinBridgeEvent;
use crate::metrics::BridgeMetrics;
use crate::retry_with_max_elapsed_time;
use crate::starcoin_jsonrpc_client::StarcoinJsonRpcClient;
use crate::types::BridgeActionStatus;
use crate::types::ParsedTokenTransferMessage;
use crate::types::{BridgeAction, BridgeAuthority, BridgeCommittee};

pub struct StarcoinClient<P> {
    inner: P,
    bridge_metrics: Arc<BridgeMetrics>,
}

// JSON-RPC based client (default, no runtime conflicts)
pub type StarcoinBridgeClient = StarcoinClient<StarcoinJsonRpcClient>;

// Legacy type alias for backward compatibility
pub type StarcoinBridgeSdkClient = StarcoinBridgeClient;

impl StarcoinBridgeClient {
    pub fn new(rpc_url: &str, bridge_address: &str) -> Self {
        Self {
            inner: StarcoinJsonRpcClient::new(rpc_url, bridge_address),
            bridge_metrics: Arc::new(BridgeMetrics::new_for_testing()),
        }
    }

    pub fn with_metrics(
        rpc_url: &str,
        bridge_address: &str,
        bridge_metrics: Arc<BridgeMetrics>,
    ) -> Self {
        Self {
            inner: StarcoinJsonRpcClient::new(rpc_url, bridge_address),
            bridge_metrics,
        }
    }

    pub fn starcoin_bridge_client(&self) -> &StarcoinJsonRpcClient {
        &self.inner
    }

    /// Get access to the underlying JSON-RPC client
    pub fn json_rpc_client(&self) -> &StarcoinJsonRpcClient {
        &self.inner
    }
}

// SDK-based client (only for tests)
#[cfg(test)]
impl StarcoinClient<StarcoinSdkClient> {
    pub async fn new(rpc_url: &str, bridge_metrics: Arc<BridgeMetrics>) -> anyhow::Result<Self> {
        let inner = StarcoinClientBuilder::default()
            .url(rpc_url)
            .build()
            .map_err(|e| {
                anyhow!("Can't establish connection with Starcoin Rpc {rpc_url}. Error: {e}")
            })?;
        let self_ = Self {
            inner,
            bridge_metrics,
        };
        self_.describe().await?;
        Ok(self_)
    }

    pub fn starcoin_bridge_client(&self) -> &StarcoinSdkClient {
        &self.inner
    }
}

impl<P> StarcoinClient<P>
where
    P: StarcoinClientInner,
{
    pub fn new_for_testing(inner: P) -> Self {
        Self {
            inner,
            bridge_metrics: Arc::new(BridgeMetrics::new_for_testing()),
        }
    }

    /// Get the configured bridge contract address
    pub fn bridge_address(&self) -> &str {
        self.inner.bridge_address()
    }

    async fn describe(&self) -> anyhow::Result<()> {
        let chain_id = self.inner.get_chain_identifier().await?;
        let block_number = self.inner.get_latest_checkpoint_sequence_number().await?;
        tracing::info!(
            "StarcoinClient is connected to chain {chain_id}, current block number: {block_number}"
        );
        // Chain identifier is informational - actual chain ID validation happens in config.rs
        Ok(())
    }

    // Get the mutable bridge object arg on chain.
    // We retry a few times in case of errors. If it fails eventually, we panic.
    // In general it's safe to call in the beginning of the program.
    // After the first call, the result is cached since the value should never change.
    pub async fn get_mutable_bridge_object_arg_must_succeed(&self) -> ObjectArg {
        static ARG: OnceCell<ObjectArg> = OnceCell::const_new();
        ARG.get_or_init(|| async move {
            let Ok(Ok(bridge_object_arg)) = retry_with_max_elapsed_time!(
                self.inner.get_mutable_bridge_object_arg(),
                Duration::from_secs(30)
            ) else {
                panic!("Failed to get bridge object arg after retries");
            };
            bridge_object_arg
        })
        .await
        .clone()
    }

    // Query emitted Events that are defined in the given Move Module.
    pub async fn query_events_by_module(
        &self,
        package: ObjectID,
        module: Identifier,
        // cursor is exclusive
        cursor: Option<EventID>,
    ) -> BridgeResult<Page<StarcoinEvent>> {
        // Starcoin uses 16-byte addresses, extract from last 16 bytes of ObjectID
        // ObjectID is 32 bytes: [16 zero padding bytes][16 byte Starcoin address]
        let starcoin_addr = &package[16..32]; // Take last 16 bytes

        // Starcoin's chain.get_events doesn't support wildcard type_tags
        // We query all events and filter by module on client side
        let filter = EventFilter {
            // Don't filter by type_tags - we'll filter in code
            ..Default::default()
        };
        let events = self.inner.query_events(filter.clone(), cursor).await?;

        // Filter events to only include those from the requested package and module
        // Note: Starcoin events use PascalCase module names (e.g., "Bridge"), while
        // our module identifiers use lowercase (e.g., "bridge"). Use case-insensitive comparison.
        let filtered_data: Vec<_> = events
            .data
            .into_iter()
            .filter(|event| {
                event.type_.address.as_ref() == starcoin_addr
                    && event.type_.module.as_str().to_lowercase() == module.as_str().to_lowercase()
            })
            .collect();

        Ok(Page {
            data: filtered_data,
            next_cursor: events.next_cursor,
            has_next_page: events.has_next_page,
        })
    }

    // Returns BridgeAction from a Starcoin Transaction with transaction hash
    // and the event index. If event is declared in an unrecognized
    // package, return error.
    //
    // Note: event_idx refers to the Nth bridge event in the transaction (0-indexed),
    // not the absolute index in the transaction's event list. This is because
    // Starcoin transactions may emit multiple events (e.g., Account::WithdrawEvent,
    // Token::BurnEvent) before the Bridge event.
    pub async fn get_bridge_action_by_tx_digest_and_event_idx_maybe(
        &self,
        tx_digest: &TransactionDigest,
        event_idx: u16,
    ) -> BridgeResult<BridgeAction> {
        let events = self.inner.get_events_by_tx_digest(*tx_digest).await?;

        // Get expected bridge address from config (16 bytes for Starcoin)
        let expected_addr = hex::decode(self.bridge_address().trim_start_matches("0x"))
            .map_err(|_| BridgeError::BridgeEventInUnrecognizedStarcoinPackage)?;

        // Find all bridge events (events from the bridge module)
        let bridge_events: Vec<_> = events
            .iter()
            .enumerate()
            .filter(|(_, event)| event.type_.address.as_ref() == expected_addr.as_slice())
            .collect();

        tracing::debug!(
            "Found {} bridge events in tx {:?}, looking for event_idx {}",
            bridge_events.len(),
            tx_digest,
            event_idx
        );

        // Get the Nth bridge event (event_idx is relative to bridge events only)
        let (actual_idx, event) = bridge_events.get(event_idx as usize).ok_or_else(|| {
            tracing::warn!(
                "No bridge event at index {} in tx {:?}, total bridge events: {}",
                event_idx,
                tx_digest,
                bridge_events.len()
            );
            BridgeError::NoBridgeEventsInTxPosition
        })?;

        tracing::debug!(
            "Using bridge event at actual index {} (requested bridge event idx {})",
            actual_idx,
            event_idx
        );

        let bridge_event = StarcoinBridgeEvent::try_from_starcoin_bridge_event(event)?
            .ok_or(BridgeError::NoBridgeEventsInTxPosition)?;

        bridge_event
            .try_into_bridge_action(*tx_digest, event_idx)
            .ok_or(BridgeError::BridgeEventNotActionable)
    }

    pub async fn get_bridge_summary(&self) -> BridgeResult<BridgeSummary> {
        self.inner
            .get_bridge_summary()
            .await
            .map_err(|e| BridgeError::InternalError(format!("Can't get bridge committee: {e}")))
    }

    pub async fn is_bridge_paused(&self) -> BridgeResult<bool> {
        self.get_bridge_summary()
            .await
            .map(|summary| summary.is_frozen)
    }

    pub async fn get_treasury_summary(&self) -> BridgeResult<BridgeTreasurySummary> {
        Ok(self.get_bridge_summary().await?.treasury)
    }

    pub async fn get_token_id_map(&self) -> BridgeResult<HashMap<u8, TypeTag>> {
        self.get_bridge_summary()
            .await?
            .treasury
            .id_token_type_map
            .into_iter()
            .map(|(id, name)| {
                parse_starcoin_bridge_type_tag(&format!("0x{name}"))
                    .map(|name| (id, name))
                    .map_err(|e| {
                        BridgeError::InternalError(format!(
                            "Failed to retrieve token id mapping: {e}, type name: {name}"
                        ))
                    })
            })
            .collect()
    }

    pub async fn get_notional_values(&self) -> BridgeResult<HashMap<u8, u64>> {
        let bridge_summary = self.get_bridge_summary().await?;
        bridge_summary
            .treasury
            .id_token_type_map
            .iter()
            .map(|(id, type_name)| {
                bridge_summary
                    .treasury
                    .supported_tokens
                    .iter()
                    .find_map(|(tn, metadata)| {
                        if type_name == tn {
                            Some((*id, metadata.notional_value))
                        } else {
                            None
                        }
                    })
                    .ok_or(BridgeError::InternalError(
                        "Error encountered when retrieving token notional values.".into(),
                    ))
            })
            .collect()
    }

    pub async fn get_bridge_committee(&self) -> BridgeResult<BridgeCommittee> {
        let bridge_summary =
            self.inner.get_bridge_summary().await.map_err(|e| {
                BridgeError::InternalError(format!("Can't get bridge committee: {e}"))
            })?;
        let move_type_bridge_committee = bridge_summary.committee;

        let mut authorities = vec![];
        // Convert MoveTypeBridgeCommittee members to BridgeAuthority
        // This logic is here because BridgeCommittee needs to be constructed from authorities
        for (_, member) in move_type_bridge_committee.members {
            let MoveTypeCommitteeMember {
                starcoin_bridge_address,
                bridge_pubkey_bytes,
                voting_power,
                http_rest_url,
                blocklisted,
            } = member;
            let pubkey = BridgeAuthorityPublicKey::from_bytes(&bridge_pubkey_bytes)?;
            let base_url = from_utf8(&http_rest_url).unwrap_or_else(|_e| {
                warn!(
                    "Bridge authority address: {}, pubkey: {:?} has invalid http url: {:?}",
                    starcoin_bridge_address, bridge_pubkey_bytes, http_rest_url
                );
                ""
            });
            authorities.push(BridgeAuthority {
                starcoin_bridge_address,
                pubkey,
                voting_power,
                base_url: base_url.into(),
                is_blocklisted: blocklisted,
            });
        }
        BridgeCommittee::new(authorities)
    }

    pub async fn get_chain_identifier(&self) -> BridgeResult<String> {
        Ok(self.inner.get_chain_identifier().await?)
    }

    pub async fn get_reference_gas_price_until_success(&self) -> u64 {
        loop {
            let Ok(Ok(rgp)) = retry_with_max_elapsed_time!(
                self.inner.get_reference_gas_price(),
                Duration::from_secs(30)
            ) else {
                self.bridge_metrics
                    .starcoin_bridge_rpc_errors
                    .with_label_values(&["get_reference_gas_price"])
                    .inc();
                error!("Failed to get reference gas price");
                continue;
            };
            return rgp;
        }
    }

    pub async fn get_latest_checkpoint_sequence_number(&self) -> BridgeResult<u64> {
        Ok(self.inner.get_latest_checkpoint_sequence_number().await?)
    }

    pub async fn execute_transaction_block_with_effects(
        &self,
        tx: starcoin_bridge_types::transaction::Transaction,
    ) -> BridgeResult<StarcoinTransactionBlockResponse> {
        self.inner.execute_transaction_block_with_effects(tx).await
    }

    // This function polls until action status is success
    // Performance in tests can be improved by using a mock client
    pub async fn get_token_transfer_action_onchain_status_until_success(
        &self,
        source_chain_id: u8,
        seq_number: u64,
    ) -> BridgeActionStatus {
        loop {
            let bridge_object_arg = self.get_mutable_bridge_object_arg_must_succeed().await;
            let Ok(Ok(status)) = retry_with_max_elapsed_time!(
                self.inner.get_token_transfer_action_onchain_status(
                    bridge_object_arg.clone(),
                    source_chain_id,
                    seq_number
                ),
                Duration::from_secs(30)
            ) else {
                self.bridge_metrics
                    .starcoin_bridge_rpc_errors
                    .with_label_values(&["get_token_transfer_action_onchain_status"])
                    .inc();
                error!(
                    "[QUERY] Failed to get token transfer action onchain status: source_chain={}, seq_num={}",
                    source_chain_id, seq_number
                );
                continue;
            };
            
            return status;
        }
    }

    pub async fn get_token_transfer_action_onchain_signatures_until_success(
        &self,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Option<Vec<Vec<u8>>> {
        loop {
            let bridge_object_arg = self.get_mutable_bridge_object_arg_must_succeed().await;
            let Ok(Ok(sigs)) = retry_with_max_elapsed_time!(
                self.inner.get_token_transfer_action_onchain_signatures(
                    bridge_object_arg.clone(),
                    source_chain_id,
                    seq_number
                ),
                Duration::from_secs(30)
            ) else {
                self.bridge_metrics
                    .starcoin_bridge_rpc_errors
                    .with_label_values(&["get_token_transfer_action_onchain_signatures"])
                    .inc();
                error!(
                    source_chain_id,
                    seq_number, "Failed to get token transfer action onchain signatures"
                );
                continue;
            };
            return sigs;
        }
    }

    pub async fn get_parsed_token_transfer_message(
        &self,
        source_chain_id: u8,
        seq_number: u64,
    ) -> BridgeResult<Option<ParsedTokenTransferMessage>> {
        let bridge_object_arg = self.get_mutable_bridge_object_arg_must_succeed().await;
        let message = self
            .inner
            .get_parsed_token_transfer_message(bridge_object_arg, source_chain_id, seq_number)
            .await?;
        Ok(match message {
            Some(payload) => Some(ParsedTokenTransferMessage::try_from(payload)?),
            None => None,
        })
    }

    pub async fn get_gas_data_panic_if_not_gas(
        &self,
        gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner) {
        self.inner
            .get_gas_data_panic_if_not_gas(gas_object_id)
            .await
    }

    /// Get account sequence number for transaction building
    pub async fn get_sequence_number(&self, address: &str) -> BridgeResult<u64> {
        self.inner.get_sequence_number(address).await.map_err(|e| {
            BridgeError::InternalError(format!("Failed to get sequence number: {:?}", e))
        })
    }

    /// Get the current block timestamp from the Starcoin chain
    /// Returns the timestamp in milliseconds from genesis
    pub async fn get_block_timestamp(&self) -> BridgeResult<u64> {
        self.inner.get_block_timestamp().await.map_err(|e| {
            BridgeError::InternalError(format!("Failed to get block timestamp: {:?}", e))
        })
    }

    /// Sign and submit a transaction to the Starcoin network
    pub async fn sign_and_submit_transaction(
        &self,
        key: &starcoin_bridge_types::crypto::StarcoinKeyPair,
        raw_txn: starcoin_bridge_types::transaction::RawUserTransaction,
    ) -> BridgeResult<String> {
        self.inner
            .sign_and_submit_transaction(key, raw_txn)
            .await
            .map_err(|e| {
                BridgeError::InternalError(format!("Transaction submission failed: {:?}", e))
            })
    }

    /// Sign, submit and wait for transaction confirmation
    /// Polls for up to 30 seconds until the transaction is confirmed on chain
    /// by checking that the account sequence number has incremented
    pub async fn sign_and_submit_and_wait_transaction(
        &self,
        key: &starcoin_bridge_types::crypto::StarcoinKeyPair,
        raw_txn: starcoin_bridge_types::transaction::RawUserTransaction,
    ) -> BridgeResult<String> {
        // Get the expected sequence number after transaction confirms
        let expected_seq = raw_txn.sequence_number() + 1;
        let sender_address = key.starcoin_address().to_hex_literal();

        let txn_hash = self.sign_and_submit_transaction(key, raw_txn).await?;

        tracing::info!(
            ?txn_hash,
            expected_seq,
            "Transaction submitted, waiting for confirmation"
        );

        // Poll for transaction confirmation (max 30 seconds, check every 500ms)
        for i in 0..60 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            // Check if transaction is confirmed by verifying sequence number has incremented
            match self.get_sequence_number(&sender_address).await {
                Ok(current_seq) => {
                    if current_seq >= expected_seq {
                        tracing::info!(
                            ?txn_hash,
                            current_seq,
                            expected_seq,
                            "Transaction confirmed on chain"
                        );
                        return Ok(txn_hash);
                    }
                    if i % 10 == 0 {
                        tracing::debug!(
                            ?txn_hash,
                            current_seq,
                            expected_seq,
                            "Still waiting for confirmation..."
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(?txn_hash, ?e, "Failed to get sequence number, retrying...");
                }
            }
        }

        Err(BridgeError::InternalError(format!(
            "Transaction {} not confirmed after 30 seconds timeout",
            txn_hash
        )))
    }
}

// Use a trait to abstract over the StarcoinSDKClient and StarcoinMockClient for testing.
#[async_trait]
pub trait StarcoinClientInner: Send + Sync {
    type Error: Into<anyhow::Error> + Send + Sync + std::error::Error + 'static;

    /// Get the configured bridge contract address
    fn bridge_address(&self) -> &str;

    async fn query_events(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
    ) -> Result<EventPage, Self::Error>;

    async fn get_events_by_tx_digest(
        &self,
        tx_digest: TransactionDigest,
    ) -> Result<Vec<StarcoinEvent>, Self::Error>;

    async fn get_chain_identifier(&self) -> Result<String, Self::Error>;

    async fn get_reference_gas_price(&self) -> Result<u64, Self::Error>;

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, Self::Error>;

    async fn get_mutable_bridge_object_arg(&self) -> Result<ObjectArg, Self::Error>;

    async fn get_bridge_summary(&self) -> Result<BridgeSummary, Self::Error>;

    async fn execute_transaction_block_with_effects(
        &self,
        tx: Transaction,
    ) -> Result<StarcoinTransactionBlockResponse, BridgeError>;

    async fn get_token_transfer_action_onchain_status(
        &self,
        bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<BridgeActionStatus, BridgeError>;

    async fn get_token_transfer_action_onchain_signatures(
        &self,
        bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<Vec<Vec<u8>>>, BridgeError>;

    async fn get_parsed_token_transfer_message(
        &self,
        bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<MoveTypeParsedTokenTransferMessage>, BridgeError>;

    async fn get_gas_data_panic_if_not_gas(
        &self,
        gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner);

    /// Get account sequence number for transaction building
    async fn get_sequence_number(&self, address: &str) -> Result<u64, BridgeError>;

    /// Get the current block timestamp from the chain
    /// Returns the timestamp in milliseconds from genesis
    async fn get_block_timestamp(&self) -> Result<u64, BridgeError>;

    /// Sign and submit a raw transaction to the network
    async fn sign_and_submit_transaction(
        &self,
        key: &starcoin_bridge_types::crypto::StarcoinKeyPair,
        raw_txn: starcoin_bridge_types::transaction::RawUserTransaction,
    ) -> Result<String, BridgeError>;
}

// SDK-based implementation (only for tests)
#[cfg(test)]
#[async_trait]
impl StarcoinClientInner for StarcoinSdkClient {
    type Error = starcoin_bridge_sdk::error::Error;

    fn bridge_address(&self) -> &str {
        // Return a dummy address for testing
        "0x0000000000000000000000000000000b"
    }

    async fn query_events(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
    ) -> Result<EventPage, Self::Error> {
        self.event_api()
            .query_events(query, cursor, None, false)
            .await
            .map_err(|e| {
                starcoin_bridge_sdk::error::Error::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })
    }

    async fn get_events_by_tx_digest(
        &self,
        tx_digest: TransactionDigest,
    ) -> Result<Vec<StarcoinEvent>, Self::Error> {
        // Query events from Starcoin using the SDK
        // Note: Currently get_events returns Vec<Event> where Event is Vec<u8> (stub)
        // We need to convert these to proper StarcoinEvent objects
        let _events = self.event_api().get_events(&tx_digest).await.map_err(|e| {
            starcoin_bridge_sdk::error::Error::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to get events: {}", e),
            ))
        })?;

        // TODO: Parse the raw event bytes into StarcoinEvent objects
        // This requires understanding the event structure from Starcoin transactions
        Ok(vec![])
    }

    async fn get_chain_identifier(&self) -> Result<String, Self::Error> {
        self.read_api().get_chain_identifier().await.map_err(|e| {
            starcoin_bridge_sdk::error::Error::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })
    }

    async fn get_reference_gas_price(&self) -> Result<u64, Self::Error> {
        self.governance_api()
            .get_reference_gas_price()
            .await
            .map_err(|e| {
                starcoin_bridge_sdk::error::Error::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64, Self::Error> {
        self.read_api()
            .get_latest_checkpoint_sequence_number()
            .await
            .map_err(|e| {
                starcoin_bridge_sdk::error::Error::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })
    }

    async fn get_mutable_bridge_object_arg(&self) -> Result<ObjectArg, Self::Error> {
        let initial_shared_version = self
            .http()
            .get_bridge_object_initial_shared_version()
            .await
            .map_err(|e| {
                starcoin_bridge_sdk::error::Error::from(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })?;
        Ok(ObjectArg::SharedObject {
            id: STARCOIN_BRIDGE_OBJECT_ID,
            initial_shared_version, // SequenceNumber is just u64
            mutable: true,
        })
    }

    async fn get_bridge_summary(&self) -> Result<BridgeSummary, Self::Error> {
        self.http().get_latest_bridge().await.map_err(|e| {
            starcoin_bridge_sdk::error::Error::from(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })
    }

    async fn get_token_transfer_action_onchain_status(
        &self,
        bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<BridgeActionStatus, BridgeError> {
        dev_inspect_bridge::<u8>(
            self,
            bridge_object_arg,
            source_chain_id,
            seq_number,
            "get_token_transfer_action_status",
        )
        .await
        .and_then(|status_byte| BridgeActionStatus::try_from(status_byte).map_err(Into::into))
    }

    async fn get_token_transfer_action_onchain_signatures(
        &self,
        bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<Vec<Vec<u8>>>, BridgeError> {
        dev_inspect_bridge::<Option<Vec<Vec<u8>>>>(
            self,
            bridge_object_arg,
            source_chain_id,
            seq_number,
            "get_token_transfer_action_signatures",
        )
        .await
    }

    async fn execute_transaction_block_with_effects(
        &self,
        tx: Transaction,
    ) -> Result<StarcoinTransactionBlockResponse, BridgeError> {
        match self
            .quorum_driver_api()
            .execute_transaction_block(
                tx,
                StarcoinTransactionBlockResponseOptions::new()
                    .with_effects()
                    .with_events(),
                starcoin_bridge_types::quorum_driver_types::ExecuteTransactionRequestType::WaitForEffectsCert,
            )
            .await
        {
            Ok(response) => Ok(response),
            Err(e) => return Err(BridgeError::StarcoinTxFailureGeneric(e.to_string())),
        }
    }

    async fn get_parsed_token_transfer_message(
        &self,
        bridge_object_arg: ObjectArg,
        source_chain_id: u8,
        seq_number: u64,
    ) -> Result<Option<MoveTypeParsedTokenTransferMessage>, BridgeError> {
        dev_inspect_bridge::<Option<MoveTypeParsedTokenTransferMessage>>(
            self,
            bridge_object_arg,
            source_chain_id,
            seq_number,
            "get_parsed_token_transfer_message",
        )
        .await
    }

    async fn get_gas_data_panic_if_not_gas(
        &self,
        gas_object_id: ObjectID,
    ) -> (GasCoin, ObjectRef, Owner) {
        loop {
            match self
                .read_api()
                .get_object_with_options(
                    gas_object_id,
                    StarcoinObjectDataOptions::default()
                        .with_owner()
                        .with_content(),
                )
                .await
                .map(|resp| resp.data)
            {
                Ok(Some(gas_obj)) => {
                    let owner = gas_obj.owner.clone().expect("Owner is requested");
                    // TODO: Parse gas coin value from object data
                    // For now, use a default value
                    let gas_coin = GasCoin {
                        value: 1_000_000_000,
                    };
                    // Convert Owner manually to avoid cyclic dependency
                    let owner_converted = match owner {
                        starcoin_bridge_json_rpc_types::Owner::AddressOwner(addr) => {
                            starcoin_bridge_types::object::Owner::AddressOwner(
                                starcoin_bridge_types::base_types::starcoin_bridge_address_from_bytes(addr),
                            )
                        }
                        starcoin_bridge_json_rpc_types::Owner::ObjectOwner(id) => {
                            starcoin_bridge_types::object::Owner::ObjectOwner(id.into())
                        }
                        starcoin_bridge_json_rpc_types::Owner::Shared {
                            initial_shared_version,
                        } => starcoin_bridge_types::object::Owner::Shared {
                            initial_shared_version,
                        },
                        starcoin_bridge_json_rpc_types::Owner::Immutable => starcoin_bridge_types::object::Owner::Immutable,
                    };
                    return (gas_coin, gas_obj.object_ref(), owner_converted);
                }
                other => {
                    warn!("Can't get gas object: {:?}: {:?}", gas_object_id, other);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn get_sequence_number(&self, _address: &str) -> Result<u64, BridgeError> {
        // SDK-based implementation for tests
        // TODO: Implement proper sequence number retrieval
        Ok(0)
    }

    async fn get_block_timestamp(&self) -> Result<u64, BridgeError> {
        // SDK-based implementation for tests
        // Return current system time in milliseconds
        Ok(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64)
    }

    async fn sign_and_submit_transaction(
        &self,
        _key: &starcoin_bridge_types::crypto::StarcoinKeyPair,
        _raw_txn: starcoin_bridge_types::transaction::RawUserTransaction,
    ) -> Result<String, BridgeError> {
        // SDK-based implementation for tests
        // This is only used in tests and will use mock transactions
        Err(BridgeError::Generic(
            "SDK-based transaction submission not implemented".into(),
        ))
    }
}

// SDK-based helper function (only for tests)
#[cfg(test)]
// Helper function to dev-inspect `bridge::{function_name}` function
// with bridge object arg, source chain id, seq number as param
// and parse the return value as `T`.
async fn dev_inspect_bridge<T>(
    starcoin_bridge_client: &StarcoinSdkClient,
    bridge_object_arg: ObjectArg,
    source_chain_id: u8,
    seq_number: u64,
    function_name: &str,
) -> Result<T, BridgeError>
where
    T: DeserializeOwned,
{
    let pt = ProgrammableTransaction {
        inputs: vec![
            CallArg::Object(bridge_object_arg),
            CallArg::Pure(bcs::to_bytes(&source_chain_id).unwrap()),
            CallArg::Pure(bcs::to_bytes(&seq_number).unwrap()),
        ],
        commands: vec![Command::move_call(
            BRIDGE_PACKAGE_ID,
            Identifier::new("bridge").unwrap(),
            Identifier::new(function_name).unwrap(),
            vec![],
            vec![Argument::Input(0), Argument::Input(1), Argument::Input(2)],
        )],
    };
    let kind = TransactionKind::programmable(pt);
    let zero_address =
        starcoin_bridge_types::base_types::starcoin_bridge_address_to_bytes(StarcoinAddress::ZERO);
    let resp = starcoin_bridge_client
        .read_api()
        .dev_inspect_transaction_block(zero_address, kind, None, None)
        .await?;
    let DevInspectResults {
        results, effects, ..
    } = resp;
    let Some(results) = results else {
        return Err(BridgeError::Generic(format!(
            "No results returned for '{}', effects: {:?}",
            function_name, effects
        )));
    };
    let return_values = &results
        .first()
        .ok_or(BridgeError::Generic(format!(
            "No return values for '{}', results: {:?}",
            function_name, results
        )))?
        .return_values;
    let (value_bytes, _type_tag) = return_values.first().ok_or(BridgeError::Generic(format!(
        "No first return value for '{}', results: {:?}",
        function_name, results
    )))?;
    bcs::from_bytes::<T>(value_bytes).map_err(|e| {
        BridgeError::Generic(format!(
            "Failed to parse return value for '{}', error: {:?}, results: {:?}",
            function_name, e, results
        ))
    })
}

/*#[cfg(test)]
mod tests {
    use crate::crypto::BridgeAuthorityKeyPair;
    use crate::e2e_tests::test_utils::TestClusterWrapperBuilder;
    use crate::{
        events::{EmittedStarcoinToEthTokenBridgeV1, MoveTokenDepositedEvent},
        starcoin_bridge_mock_client::StarcoinMockClient,
        test_utils::{
            approve_action_with_validator_secrets, bridge_token, get_test_eth_to_starcoin_bridge_action,
            get_test_starcoin_bridge_to_eth_bridge_action,
        },
        types::StarcoinToEthBridgeAction,
    };
    use ethers::types::Address as EthAddress;
    use move_core_types::account_address::AccountAddress;
    use serde::{Deserialize, Serialize};
    use std::str::FromStr;
    use starcoin_bridge_json_rpc_types::BcsEvent;
    use starcoin_bridge_types::bridge::{BridgeChainId, TOKEN_ID_STARCOIN, TOKEN_ID_USDC};
    use starcoin_bridge_types::crypto::get_key_pair;

    use super::*;
    use crate::events::{init_all_struct_tags, StarcoinToEthTokenBridgeV1};

    #[tokio::test]
    async fn get_bridge_action_by_tx_digest_and_event_idx_maybe() {
        // Note: for random events generated in this test, we only care about
        // tx_digest and event_seq, so it's ok that package and module does
        // not match the query parameters.
        telemetry_subscribers::init_for_testing();
        let mock_client = StarcoinMockClient::default();
        let starcoin_bridge_client = StarcoinClient::new_for_testing(mock_client.clone());
        let tx_digest = TransactionDigest::random();

        // Ensure all struct tags are inited
        init_all_struct_tags();

        let sanitized_event_1 = EmittedStarcoinToEthTokenBridgeV1 {
            nonce: 1,
            starcoin_bridge_chain_id: BridgeChainId::StarcoinTestnet,
            starcoin_bridge_address: StarcoinAddress::random_for_testing_only(),
            eth_chain_id: BridgeChainId::EthSepolia,
            eth_address: EthAddress::random(),
            token_id: TOKEN_ID_STARCOIN,
            amount_starcoin_bridge_adjusted: 100,
        };
        let emitted_event_1 = MoveTokenDepositedEvent {
            seq_num: sanitized_event_1.nonce,
            source_chain: sanitized_event_1.starcoin_bridge_chain_id as u8,
            sender_address: sanitized_event_1.starcoin_bridge_address.to_vec(),
            target_chain: sanitized_event_1.eth_chain_id as u8,
            target_address: sanitized_event_1.eth_address.as_bytes().to_vec(),
            token_type: sanitized_event_1.token_id,
            amount_starcoin_bridge_adjusted: sanitized_event_1.amount_starcoin_bridge_adjusted,
        };

        let mut starcoin_bridge_event_1 = StarcoinEvent::random_for_testing();
        starcoin_bridge_event_1.type_ = StarcoinToEthTokenBridgeV1.get().unwrap().clone();
        starcoin_bridge_event_1.bcs = BcsEvent::new(bcs::to_bytes(&emitted_event_1).unwrap());

        #[derive(Serialize, Deserialize)]
        struct RandomStruct {}

        let event_2: RandomStruct = RandomStruct {};
        // undeclared struct tag
        let mut starcoin_bridge_event_2 = StarcoinEvent::random_for_testing();
        starcoin_bridge_event_2.type_ = StarcoinToEthTokenBridgeV1.get().unwrap().clone();
        starcoin_bridge_event_2.type_.module = Identifier::from_str("unrecognized_module").unwrap();
        starcoin_bridge_event_2.bcs = BcsEvent::new(bcs::to_bytes(&event_2).unwrap());

        // Event 3 is defined in non-bridge package
        let mut starcoin_bridge_event_3 = starcoin_bridge_event_1.clone();
        starcoin_bridge_event_3.type_.address = AccountAddress::random();

        mock_client.add_events_by_tx_digest(
            tx_digest,
            vec![
                starcoin_bridge_event_1.clone(),
                starcoin_bridge_event_2.clone(),
                starcoin_bridge_event_1.clone(),
                starcoin_bridge_event_3.clone(),
            ],
        );
        let expected_action_1 = BridgeAction::StarcoinToEthBridgeAction(StarcoinToEthBridgeAction {
            starcoin_bridge_tx_digest: tx_digest,
            starcoin_bridge_tx_event_index: 0,
            starcoin_bridge_event: sanitized_event_1.clone(),
        });
        assert_eq!(
            starcoin_bridge_client
                .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 0)
                .await
                .unwrap(),
            expected_action_1,
        );
        let expected_action_2 = BridgeAction::StarcoinToEthBridgeAction(StarcoinToEthBridgeAction {
            starcoin_bridge_tx_digest: tx_digest,
            starcoin_bridge_tx_event_index: 2,
            starcoin_bridge_event: sanitized_event_1.clone(),
        });
        assert_eq!(
            starcoin_bridge_client
                .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 2)
                .await
                .unwrap(),
            expected_action_2,
        );
        assert!(matches!(
            starcoin_bridge_client
                .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 1)
                .await
                .unwrap_err(),
            BridgeError::NoBridgeEventsInTxPosition
        ),);
        assert!(matches!(
            starcoin_bridge_client
                .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 3)
                .await
                .unwrap_err(),
            BridgeError::BridgeEventInUnrecognizedStarcoinPackage
        ),);
        assert!(matches!(
            starcoin_bridge_client
                .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 4)
                .await
                .unwrap_err(),
            BridgeError::NoBridgeEventsInTxPosition
        ),);

        // if the StructTag matches with unparsable bcs, it returns an error
        starcoin_bridge_event_2.type_ = StarcoinToEthTokenBridgeV1.get().unwrap().clone();
        mock_client.add_events_by_tx_digest(tx_digest, vec![starcoin_bridge_event_2]);
        starcoin_bridge_client
            .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, 2)
            .await
            .unwrap_err();
    }

    // Test get_action_onchain_status.
    // Use validator secrets to bridge USDC from Ethereum initially.
    // TODO: we need an e2e test for this with published solidity contract and committee with BridgeNodes
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_get_action_onchain_status_for_starcoin_bridge_to_eth_transfer() {
        telemetry_subscribers::init_for_testing();
        let mut bridge_keys = vec![];
        for _ in 0..=3 {
            let (_, kp): (_, BridgeAuthorityKeyPair) = get_key_pair();
            bridge_keys.push(kp);
        }
        let mut test_cluster = TestClusterWrapperBuilder::new()
            .with_bridge_authority_keys(bridge_keys)
            .with_deploy_tokens(true)
            .build()
            .await;

        let bridge_metrics = Arc::new(BridgeMetrics::new_for_testing());
        let starcoin_bridge_client =
            StarcoinClient::new(&test_cluster.inner.fullnode_handle.rpc_url, bridge_metrics)
                .await
                .unwrap();
        let bridge_authority_keys = test_cluster.authority_keys_clone();

        // Wait until committee is set up
        test_cluster
            .trigger_reconfiguration_if_not_yet_and_assert_bridge_committee_initialized()
            .await;
        let context = &mut test_cluster.inner.wallet;
        let sender = context.active_address().unwrap();
        let usdc_amount = 5000000;
        let bridge_object_arg = starcoin_bridge_client
            .get_mutable_bridge_object_arg_must_succeed()
            .await;
        let id_token_map = starcoin_bridge_client.get_token_id_map().await.unwrap();

        // 1. Create a Eth -> Starcoin Transfer (recipient is sender address), approve with validator secrets and assert its status to be Claimed
        let action = get_test_eth_to_starcoin_bridge_action(None, Some(usdc_amount), Some(sender), None);
        let usdc_object_ref = approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            Some(sender),
            &id_token_map,
        )
        .await
        .unwrap();

        let status = starcoin_bridge_client
            .inner
            .get_token_transfer_action_onchain_status(
                bridge_object_arg,
                action.chain_id() as u8,
                action.seq_number(),
            )
            .await
            .unwrap();
        assert_eq!(status, BridgeActionStatus::Claimed);

        // 2. Create a Starcoin -> Eth Transfer, approve with validator secrets and assert its status to be Approved
        // We need to actually send tokens to bridge to initialize the record.
        let eth_recv_address = EthAddress::random();
        let bridge_event = bridge_token(
            context,
            eth_recv_address,
            usdc_object_ref,
            id_token_map.get(&TOKEN_ID_USDC).unwrap().clone(),
            bridge_object_arg,
        )
        .await;
        assert_eq!(bridge_event.nonce, 0);
        assert_eq!(bridge_event.starcoin_bridge_chain_id, BridgeChainId::StarcoinCustom);
        assert_eq!(bridge_event.eth_chain_id, BridgeChainId::EthCustom);
        assert_eq!(bridge_event.eth_address, eth_recv_address);
        assert_eq!(bridge_event.starcoin_bridge_address, sender);
        assert_eq!(bridge_event.token_id, TOKEN_ID_USDC);
        assert_eq!(bridge_event.amount_starcoin_bridge_adjusted, usdc_amount);

        let action = get_test_starcoin_bridge_to_eth_bridge_action(
            None,
            None,
            Some(bridge_event.nonce),
            Some(bridge_event.amount_starcoin_bridge_adjusted),
            Some(bridge_event.starcoin_bridge_address),
            Some(bridge_event.eth_address),
            Some(TOKEN_ID_USDC),
        );
        let status = starcoin_bridge_client
            .inner
            .get_token_transfer_action_onchain_status(
                bridge_object_arg,
                action.chain_id() as u8,
                action.seq_number(),
            )
            .await
            .unwrap();
        // At this point, the record is created and the status is Pending
        assert_eq!(status, BridgeActionStatus::Pending);

        // Approve it and assert its status to be Approved
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;

        let status = starcoin_bridge_client
            .inner
            .get_token_transfer_action_onchain_status(
                bridge_object_arg,
                action.chain_id() as u8,
                action.seq_number(),
            )
            .await
            .unwrap();
        assert_eq!(status, BridgeActionStatus::Approved);

        // 3. Create a random action and assert its status as NotFound
        let action =
            get_test_starcoin_bridge_to_eth_bridge_action(None, None, Some(100), None, None, None, None);
        let status = starcoin_bridge_client
            .inner
            .get_token_transfer_action_onchain_status(
                bridge_object_arg,
                action.chain_id() as u8,
                action.seq_number(),
            )
            .await
            .unwrap();
        assert_eq!(status, BridgeActionStatus::NotFound);
    }
}*/
