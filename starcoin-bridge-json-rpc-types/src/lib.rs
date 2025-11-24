// Wrapper around Starcoin RPC types for bridge compatibility
#![allow(dead_code, unused_imports)]

use serde::{Deserialize, Serialize};

// Re-export Starcoin RPC types
pub use starcoin_rpc_api::types::*;

// Add Starcoin-specific types that Bridge needs

// Placeholder for Starcoin event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarcoinEvent {
    pub id: EventID,
    pub type_: move_core_types::language_storage::StructTag,
    pub bcs: Vec<u8>,
}

// Event ID contains transaction digest and event sequence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventID {
    pub tx_digest: [u8; 32],
    pub event_seq: u64,
}

impl From<EventID> for (u64, u64) {
    fn from(id: EventID) -> (u64, u64) {
        // For cursor, use first 8 bytes of tx_digest as tx_seq
        let tx_seq = u64::from_le_bytes([
            id.tx_digest[0],
            id.tx_digest[1],
            id.tx_digest[2],
            id.tx_digest[3],
            id.tx_digest[4],
            id.tx_digest[5],
            id.tx_digest[6],
            id.tx_digest[7],
        ]);
        (tx_seq, id.event_seq)
    }
}

// Placeholder for Starcoin execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StarcoinExecutionStatus {
    Success,
    Failure { error: String },
}

// Placeholder for Starcoin transaction block effects API
pub trait StarcoinTransactionBlockEffectsAPI {
    fn status(&self) -> &StarcoinExecutionStatus;
}

// Placeholder for Starcoin transaction block response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarcoinTransactionBlockResponse {
    pub digest: Option<[u8; 32]>,
    pub effects: Option<StarcoinTransactionBlockEffects>,
    pub events: Option<Vec<StarcoinEvent>>,
    pub object_changes: Option<Vec<ObjectChange>>,
}

impl StarcoinTransactionBlockResponse {
    pub fn status_ok(&self) -> Option<bool> {
        self.effects
            .as_ref()
            .map(|e| matches!(e.status(), StarcoinExecutionStatus::Success))
    }
}

impl StarcoinTransactionBlockEffectsAPI for StarcoinTransactionBlockResponse {
    fn status(&self) -> &StarcoinExecutionStatus {
        unimplemented!("TODO: Implement for Starcoin")
    }
}

// Placeholder for transaction effects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarcoinTransactionBlockEffects {
    pub status: StarcoinExecutionStatus,
}

impl StarcoinTransactionBlockEffects {
    pub fn status(&self) -> &StarcoinExecutionStatus {
        &self.status
    }
}

// Placeholder for Starcoin transaction block response options
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StarcoinTransactionBlockResponseOptions {
    pub show_input: bool,
    pub show_raw_input: bool,
    pub show_effects: bool,
    pub show_events: bool,
    pub show_object_changes: bool,
    pub show_balance_changes: bool,
}

impl StarcoinTransactionBlockResponseOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_input(mut self) -> Self {
        self.show_input = true;
        self
    }

    pub fn with_effects(mut self) -> Self {
        self.show_effects = true;
        self
    }

    pub fn with_events(mut self) -> Self {
        self.show_events = true;
        self
    }

    pub fn with_object_changes(mut self) -> Self {
        self.show_object_changes = true;
        self
    }

    pub fn with_balance_changes(mut self) -> Self {
        self.show_balance_changes = true;
        self
    }
}

// Placeholder for event filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventFilter {
    MoveEventModule { package: [u8; 32], module: String },
    MoveEventType(String),
    Transaction([u8; 32]),
}

// Placeholder for generic page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_next_page: bool,
}

// EventPage is an alias for Page<StarcoinEvent>
pub type EventPage = Page<StarcoinEvent>;

// Placeholder for StarcoinObjectDataOptions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StarcoinObjectDataOptions {
    pub show_type: bool,
    pub show_owner: bool,
    pub show_previous_transaction: bool,
    pub show_display: bool,
    pub show_content: bool,
    pub show_bcs: bool,
    pub show_storage_rebate: bool,
}

impl StarcoinObjectDataOptions {
    pub fn with_owner(mut self) -> Self {
        self.show_owner = true;
        self
    }

    pub fn with_content(mut self) -> Self {
        self.show_content = true;
        self
    }

    pub fn new() -> Self {
        Self::default()
    }
}

// Placeholder for StarcoinExecutionResult
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarcoinExecutionResult {
    pub return_values: Vec<(Vec<u8>, String)>, // (value_bytes, type_tag)
}

// Placeholder for DevInspectResults
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevInspectResults {
    pub results: Option<Vec<StarcoinExecutionResult>>,
    pub effects: Option<String>, // Simplified - should be StarcoinTransactionBlockEffects
}

// Placeholder for StarcoinObjectResponse
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarcoinObjectResponse {
    pub data: Option<StarcoinObjectData>,
}

// Placeholder for StarcoinObjectData
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarcoinObjectData {
    pub object_id: [u8; 32],
    pub version: u64,
    pub digest: [u8; 32],
    pub owner: Option<Owner>,
}

impl StarcoinObjectData {
    pub fn object_ref(&self) -> ([u8; 32], u64, [u8; 32]) {
        (self.object_id, self.version, self.digest)
    }
}

// Placeholder for Owner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Owner {
    AddressOwner([u8; 32]),
    ObjectOwner([u8; 32]),
    Shared { initial_shared_version: u64 },
    Immutable,
}

// Placeholder for Supply
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Supply {
    pub value: u64,
}

// Placeholder for Coin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coin {
    pub coin_object_id: [u8; 32],
    pub version: u64,
    pub digest: [u8; 32],
    pub balance: u64,
    pub coin_type: String,
    pub previous_transaction: [u8; 32],
}

impl Coin {
    pub fn object_ref(&self) -> ([u8; 32], u64, [u8; 32]) {
        (self.coin_object_id, self.version, self.digest)
    }
}

// Placeholder for ObjectChange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectChange {
    Created {
        sender: [u8; 32],
        owner: String,
        object_type: move_core_types::language_storage::StructTag,
        object_id: [u8; 32],
        version: u64,
        digest: [u8; 32],
    },
    Mutated {
        sender: [u8; 32],
        owner: String,
        object_type: move_core_types::language_storage::StructTag,
        object_id: [u8; 32],
        version: u64,
        previous_version: u64,
        digest: [u8; 32],
    },
    Deleted {
        sender: [u8; 32],
        object_id: [u8; 32],
        version: u64,
    },
}

impl ObjectChange {
    pub fn object_ref(&self) -> ([u8; 32], u64, [u8; 32]) {
        match self {
            ObjectChange::Created {
                object_id,
                version,
                digest,
                ..
            } => (*object_id, *version, *digest),
            ObjectChange::Mutated {
                object_id,
                version,
                digest,
                ..
            } => (*object_id, *version, *digest),
            ObjectChange::Deleted {
                object_id, version, ..
            } => (*object_id, *version, [0u8; 32]),
        }
    }
}

// Placeholder for StarcoinSystemStateSummary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarcoinValidatorSummary {
    pub starcoin_bridge_address: [u8; 32],
    pub protocol_pubkey_bytes: Vec<u8>,
    pub name: String,
    pub voting_power: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarcoinSystemStateSummary {
    pub epoch: u64,
    pub protocol_version: u64,
    pub system_state_version: u64,
    pub active_validators: Vec<StarcoinValidatorSummary>,
}

// Placeholder for StarcoinCommittee
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarcoinCommittee {
    pub epoch: u64,
    pub validators: std::collections::HashMap<[u8; 32], u64>, // address -> voting_power
}

// Coin page for paginated coin queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinPage {
    pub data: Vec<Coin>,
    pub next_cursor: Option<String>,
    pub has_next_page: bool,
}
