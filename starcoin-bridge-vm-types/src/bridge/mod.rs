// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

//! Bridge-related modules organized in a submodule

#![allow(clippy::module_inception)]

// Infrastructure modules
pub mod base_types;
pub mod crypto;
pub mod error;
pub mod id;
pub mod multiaddr;
pub mod object;
pub mod starcoin_serde;
pub mod storage;

// Bridge business logic modules
pub mod bridge;
pub mod collection_types;
pub mod committee;
pub mod dynamic_field;
pub mod executable_transaction;
pub mod message_envelope;
pub mod messages_checkpoint;
pub mod versioned;

// Re-export main types for convenience
pub use bridge::{Bridge, BridgeInnerV1, BridgeSummary};
pub use committee::Committee;
pub use message_envelope::{Envelope, VerifiedEnvelope};
