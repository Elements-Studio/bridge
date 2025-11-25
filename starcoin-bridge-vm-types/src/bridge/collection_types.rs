// Collection types for Bridge (matching Starcoin's Move types structure)
// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

use super::id::UID;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkedTableNode<K, V> {
    pub prev: Option<K>,
    pub next: Option<K>,
    pub value: V,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bag {
    pub id: UID,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkedTable<K> {
    pub id: UID,
    pub size: u64,
    pub head: Option<K>,
    pub tail: Option<K>,
}

/// VecMap matches Starcoin's Move VecMap structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VecMap<K, V> {
    pub contents: Vec<Entry<K, V>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry<K, V> {
    pub key: K,
    pub value: V,
}

impl<K, V> VecMap<K, V> {
    pub fn new() -> Self {
        Self {
            contents: Vec::new(),
        }
    }

    pub fn insert(&mut self, key: K, value: V)
    where
        K: PartialEq,
    {
        if let Some(entry) = self.contents.iter_mut().find(|e| e.key == key) {
            entry.value = value;
        } else {
            self.contents.push(Entry { key, value });
        }
    }

    pub fn get(&self, key: &K) -> Option<&V>
    where
        K: PartialEq,
    {
        self.contents
            .iter()
            .find(|e| &e.key == key)
            .map(|e| &e.value)
    }

    pub fn size(&self) -> u64 {
        self.contents.len() as u64
    }
}

impl<K, V> Default for VecMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}
