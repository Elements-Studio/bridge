// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Ethereum checkpoint data types for the indexer framework.

use ethers::types::{Address as EthAddress, Log, H256};
use serde::{Deserialize, Serialize};

/// Summary information for an Ethereum block/checkpoint
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EthCheckpointSummary {
    /// Block number
    pub sequence_number: u64,
    /// Block timestamp in milliseconds
    pub timestamp_ms: u64,
    /// Block hash
    pub block_hash: H256,
}

/// A transaction with its logs from Ethereum
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EthTransaction {
    /// Transaction hash
    pub tx_hash: H256,
    /// Block number
    pub block_number: u64,
    /// Transaction sender
    pub from: EthAddress,
    /// Logs emitted by this transaction
    pub logs: Vec<Log>,
    /// Block timestamp in milliseconds
    pub timestamp_ms: u64,
}

/// Complete checkpoint data from Ethereum
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EthCheckpointData {
    /// Block summary
    pub checkpoint_summary: EthCheckpointSummary,
    /// Transactions with bridge-related logs
    pub transactions: Vec<EthTransaction>,
}

impl EthCheckpointData {
    /// Get the block number
    pub fn block_number(&self) -> u64 {
        self.checkpoint_summary.sequence_number
    }

    /// Get the block timestamp in milliseconds
    pub fn timestamp_ms(&self) -> u64 {
        self.checkpoint_summary.timestamp_ms
    }

    /// Check if this checkpoint has any transactions
    pub fn has_transactions(&self) -> bool {
        !self.transactions.is_empty()
    }

    /// Get total number of logs across all transactions
    pub fn total_logs(&self) -> usize {
        self.transactions.iter().map(|t| t.logs.len()).sum()
    }
}
