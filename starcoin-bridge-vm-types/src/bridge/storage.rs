// Storage trait for Starcoin Bridge
// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

//! Storage abstraction for bridge resources.
//!
//! In Starcoin, resources are stored under accounts. This module provides
//! an abstraction layer that can work with both the Sui-style ObjectID
//! lookups (for compatibility) and Starcoin-style ResourcePath lookups.

use super::base_types::{ObjectID, ResourcePath, StarcoinAddress};
use super::object::Object;
use move_core_types::language_storage::StructTag;

/// Object store trait for backward compatibility with Sui-style lookups
pub trait ObjectStore: Send + Sync {
    /// Get an object by its ObjectID
    /// In Starcoin context, this extracts the address from the ObjectID
    /// and looks up the resource at that address
    fn get_object(&self, id: &ObjectID) -> Option<Object>;
}

/// Resource store trait for Starcoin-native lookups
pub trait ResourceStore: Send + Sync {
    /// Get a resource by its path (address + type)
    fn get_resource(&self, path: &ResourcePath) -> Option<Vec<u8>>;

    /// Get a resource directly by address and type
    fn get_resource_by_type(
        &self,
        address: &StarcoinAddress,
        resource_type: &StructTag,
    ) -> Option<Vec<u8>> {
        let path = ResourcePath::new(*address, resource_type.clone());
        self.get_resource(&path)
    }

    /// Check if a resource exists
    fn has_resource(&self, path: &ResourcePath) -> bool {
        self.get_resource(path).is_some()
    }
}

/// Combined store that supports both ObjectID and ResourcePath lookups
pub trait BridgeStore: ObjectStore + ResourceStore {}

// Blanket impl for types that implement both traits
impl<T: ObjectStore + ResourceStore> BridgeStore for T {}

/// In-memory store for testing
#[derive(Default)]
pub struct InMemoryStore {
    objects: std::collections::HashMap<ObjectID, Object>,
    resources: std::collections::HashMap<ResourcePath, Vec<u8>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_object(&mut self, id: ObjectID, obj: Object) {
        self.objects.insert(id, obj);
    }

    pub fn insert_resource(&mut self, path: ResourcePath, data: Vec<u8>) {
        self.resources.insert(path, data);
    }
}

impl ObjectStore for InMemoryStore {
    fn get_object(&self, id: &ObjectID) -> Option<Object> {
        self.objects.get(id).cloned()
    }
}

impl ResourceStore for InMemoryStore {
    fn get_resource(&self, path: &ResourcePath) -> Option<Vec<u8>> {
        self.resources.get(path).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_core_types::identifier::Identifier;

    #[test]
    fn test_in_memory_store() {
        let mut store = InMemoryStore::new();

        // Test object store
        let obj_id = [1u8; 32];
        let obj = Object::new(super::super::object::Owner::shared(), vec![1, 2, 3]);
        store.insert_object(obj_id, obj.clone());

        let retrieved = store.get_object(&obj_id).unwrap();
        assert_eq!(retrieved.data, vec![1, 2, 3]);

        // Test resource store
        let addr = StarcoinAddress::ZERO;
        let struct_tag = StructTag {
            address: addr,
            module: Identifier::new("test").unwrap(),
            name: Identifier::new("Test").unwrap(),
            type_params: vec![],
        };
        let path = ResourcePath::new(addr, struct_tag);
        store.insert_resource(path.clone(), vec![4, 5, 6]);

        let data = store.get_resource(&path).unwrap();
        assert_eq!(data, vec![4, 5, 6]);
    }
}
