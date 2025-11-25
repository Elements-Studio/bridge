// Object types (minimal stub for Bridge)
// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

use super::base_types::{ObjectID, SequenceNumber};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum Owner {
    AddressOwner(super::base_types::StarcoinAddress),
    ObjectOwner(ObjectID),
    Shared {
        initial_shared_version: SequenceNumber,
    },
    Immutable,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Object {
    pub owner: Owner,
    pub data: Vec<u8>, // Add data field
}
