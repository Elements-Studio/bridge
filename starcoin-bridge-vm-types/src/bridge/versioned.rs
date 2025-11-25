// Versioned wrapper (matching Starcoin's versioned.move structure)
// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

use super::id::UID;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Versioned {
    pub id: UID,
    pub version: u64,
}
