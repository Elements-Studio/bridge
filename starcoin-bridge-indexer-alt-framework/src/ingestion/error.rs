// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub type Result<T> = std::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Block {0} not found")]
    NotFound(u64),

    #[error("Failed to deserialize block {0}: {1}")]
    DeserializationError(u64, #[source] anyhow::Error),

    #[error("Failed to fetch block {0}: {1}")]
    FetchError(u64, #[source] anyhow::Error),

    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),

    #[error("No subscribers for ingestion service")]
    NoSubscribers,

    #[error("Shutdown signal received, stopping ingestion service")]
    Cancelled,

    #[error("RPC client error: {0}")]
    RpcError(String),
}
