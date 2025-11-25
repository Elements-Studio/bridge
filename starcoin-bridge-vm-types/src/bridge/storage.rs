// Storage trait (minimal stub for Bridge)
// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

use super::base_types::ObjectID;
use super::object::Object;

pub trait ObjectStore {
    fn get_object(&self, id: &ObjectID) -> Option<Object>;
}
