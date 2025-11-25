// Error types for Starcoin Bridge
// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

/// Bridge-specific errors
#[derive(Error, Debug)]
pub enum StarcoinError {
    #[error("Bridge read error: {0}")]
    StarcoinBridgeReadError(String),

    #[error("Bridge write error: {0}")]
    BridgeWriteError(String),

    #[error("Invalid bridge data: {0}")]
    InvalidData(String),

    #[error("Generic bridge error: {0}")]
    GenericBridgeError(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type StarcoinResult<T> = std::result::Result<T, StarcoinError>;
