// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::handlers::{
    is_bridge_txn, BRIDGE, TOKEN_DEPOSITED_EVENT, TOKEN_TRANSFER_APPROVED, TOKEN_TRANSFER_CLAIMED,
};
use crate::metrics::BridgeIndexerMetrics;
use crate::struct_tag;
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use move_core_types::language_storage::StructTag;
use std::sync::Arc;
use starcoin_bridge::events::{
    MoveTokenDepositedEvent, MoveTokenTransferApproved, MoveTokenTransferClaimed,
};
use starcoin_bridge_schema::models::{BridgeDataSource, TokenTransfer, TokenTransferStatus};
use starcoin_bridge_schema::schema::token_transfer;
use starcoin_bridge_indexer_alt_framework::pipeline::concurrent::Handler;
use starcoin_bridge_indexer_alt_framework::pipeline::Processor;
use starcoin_bridge_indexer_alt_framework::postgres::Db;
use starcoin_bridge_indexer_alt_framework::store::Store;
use starcoin_bridge_indexer_alt_framework::types::full_checkpoint_content::CheckpointData;
use move_core_types::account_address::AccountAddress;
use tracing::info;

pub struct TokenTransferHandler {
    deposited_event_type: StructTag,
    approved_event_type: StructTag,
    claimed_event_type: StructTag,
    metrics: Arc<BridgeIndexerMetrics>,
}

impl TokenTransferHandler {
    /// Create a new TokenTransferHandler with the given bridge address
    pub fn new(metrics: Arc<BridgeIndexerMetrics>, bridge_address: AccountAddress) -> Self {
        Self {
            deposited_event_type: struct_tag!(bridge_address, BRIDGE, TOKEN_DEPOSITED_EVENT),
            approved_event_type: struct_tag!(bridge_address, BRIDGE, TOKEN_TRANSFER_APPROVED),
            claimed_event_type: struct_tag!(bridge_address, BRIDGE, TOKEN_TRANSFER_CLAIMED),
            metrics,
        }
    }
}

impl Default for TokenTransferHandler {
    fn default() -> Self {
        // For compatibility with existing code that doesn't pass metrics
        use prometheus::Registry;
        use starcoin_bridge_indexer_alt_framework::types::BRIDGE_ADDRESS;
        let registry = Registry::new();
        let metrics = BridgeIndexerMetrics::new(&registry);
        Self::new(metrics, BRIDGE_ADDRESS)
    }
}

impl Processor for TokenTransferHandler {
    const NAME: &'static str = "token_transfer";
    type Value = TokenTransfer;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>, anyhow::Error> {
        let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms as i64;
        let block_height = checkpoint.checkpoint_summary.sequence_number as i64;

        let mut results = vec![];

        for tx in &checkpoint.transactions {
            if !is_bridge_txn(tx) {
                continue;
            }
            info!(
                "Processing bridge txn at block {}, events count: {:?}",
                block_height,
                tx.events.as_ref().map(|e| e.data.len())
            );
            for ev in tx.events.iter().flat_map(|e| &e.data) {
                info!(
                    "Event type: {:?}, expected deposited: {:?}, expected approved: {:?}",
                    ev.type_, self.deposited_event_type, self.approved_event_type
                );
                
                if self.deposited_event_type == ev.type_ {
                    info!("Observed Starcoin Deposit {:?}", ev);
                    let event: MoveTokenDepositedEvent = bcs::from_bytes(&ev.contents)?;

                    // Bridge-specific metrics for token deposits
                    self.metrics
                        .bridge_events_total
                        .with_label_values(&["token_deposited", "starcoin"])
                        .inc();
                    self.metrics
                        .token_transfers_total
                        .with_label_values(&[
                            "starcoin_bridge_to_eth",
                            "deposited",
                            &event.token_type.to_string(),
                        ])
                        .inc();
                    self.metrics
                        .token_transfer_gas_used
                        .with_label_values(&["starcoin_bridge_to_eth", "true"])
                        .inc_by(tx.effects.gas_cost_summary().net_gas_usage() as u64);

                    results.push(TokenTransfer {
                        chain_id: event.source_chain as i32,
                        nonce: event.seq_num as i64,
                        block_height,
                        timestamp_ms,
                        status: TokenTransferStatus::Deposited,
                        data_source: BridgeDataSource::STARCOIN,
                        is_finalized: true,
                        txn_hash: tx.transaction.digest().inner().to_vec(),
                        txn_sender: tx.transaction.sender_address().to_vec(),
                        gas_usage: tx.effects.gas_cost_summary().net_gas_usage(),
                    });
                } else if self.approved_event_type == ev.type_ {
                    info!("Observed Starcoin Approval {:?}", ev);
                    let event: MoveTokenTransferApproved = bcs::from_bytes(&ev.contents)?;

                    // Bridge committee approval metrics
                    self.metrics
                        .bridge_events_total
                        .with_label_values(&["transfer_approved", "starcoin"])
                        .inc();
                    self.metrics
                        .token_transfers_total
                        .with_label_values(&["eth_to_starcoin", "approved", "unknown"])
                        .inc();

                    results.push(TokenTransfer {
                        chain_id: event.message_key.source_chain as i32,
                        nonce: event.message_key.bridge_seq_num as i64,
                        block_height,
                        timestamp_ms,
                        status: TokenTransferStatus::Approved,
                        data_source: BridgeDataSource::STARCOIN,
                        is_finalized: true,
                        txn_hash: tx.transaction.digest().inner().to_vec(),
                        txn_sender: tx.transaction.sender_address().to_vec(),
                        gas_usage: tx.effects.gas_cost_summary().net_gas_usage(),
                    });
                } else if self.claimed_event_type == ev.type_ {
                    info!("Observed Starcoin Claim {:?}", ev);
                    let event: MoveTokenTransferClaimed = bcs::from_bytes(&ev.contents)?;

                    // Bridge transfer completion metrics
                    self.metrics
                        .bridge_events_total
                        .with_label_values(&["transfer_claimed", "starcoin"])
                        .inc();
                    self.metrics
                        .token_transfers_total
                        .with_label_values(&["eth_to_starcoin", "claimed", "unknown"])
                        .inc();

                    results.push(TokenTransfer {
                        chain_id: event.message_key.source_chain as i32,
                        nonce: event.message_key.bridge_seq_num as i64,
                        block_height,
                        timestamp_ms,
                        status: TokenTransferStatus::Claimed,
                        data_source: BridgeDataSource::STARCOIN,
                        is_finalized: true,
                        txn_hash: tx.transaction.digest().inner().to_vec(),
                        txn_sender: tx.transaction.sender_address().to_vec(),
                        gas_usage: tx.effects.gas_cost_summary().net_gas_usage(),
                    });
                }
                // Ignore other event types
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Handler for TokenTransferHandler {
    type Store = Db;
    async fn commit<'a>(
        values: &[Self::Value],
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        Ok(diesel::insert_into(token_transfer::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
