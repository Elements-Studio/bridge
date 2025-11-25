// Starcoin serde utilities (minimal stub)
// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BigInt<T>(#[schemars(with = "String")] pub T);

impl<T: fmt::Display> fmt::Display for BigInt<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub trait Readable {}
