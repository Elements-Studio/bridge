// Dynamic field support for Starcoin Bridge
// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

//! Dynamic field abstraction for bridge compatibility.
//!
//! # Sui vs Starcoin Dynamic Fields
//!
//! - **Sui**: Uses dynamic fields attached to objects, identified by parent ObjectID + name
//! - **Starcoin**: Uses Table<K, V> or SimpleMap for similar functionality
//!
//! This module provides a compatibility layer that maps Sui-style dynamic field
//! lookups to Starcoin's Table-based storage pattern.

use super::base_types::ObjectID;
use super::error::StarcoinResult;
use super::storage::ObjectStore;
use serde::{Deserialize, Serialize};

/// Field struct representing a dynamic field entry
///
/// In Sui, this is stored as a Move object with UID.
/// In Starcoin, this maps to a Table entry or SimpleMap entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Field<K, V> {
    /// In Sui: the UID of the field object
    /// In Starcoin: can be derived from parent + name hash
    pub id: ObjectID,
    /// The key/name of the field
    pub name: K,
    /// The value stored in the field
    pub value: V,
}

impl<K, V> Field<K, V> {
    /// Create a new Field
    pub fn new(id: ObjectID, name: K, value: V) -> Self {
        Self { id, name, value }
    }

    /// Get a reference to the name
    pub fn name(&self) -> &K {
        &self.name
    }

    /// Get a reference to the value
    pub fn value(&self) -> &V {
        &self.value
    }

    /// Consume and return the value
    pub fn into_value(self) -> V {
        self.value
    }
}

/// Get a dynamic field from an object store
///
/// This is a compatibility function for Sui-style dynamic field lookups.
/// In Starcoin, this would typically be implemented by:
/// 1. Computing the Table key from the parent ID and name
/// 2. Looking up the value in the Table
///
/// For now, this returns None as the actual implementation requires
/// integration with Starcoin's Table module.
pub fn get_dynamic_field_from_store<K, V>(
    _store: &dyn ObjectStore,
    _parent: ObjectID,
    _name: &K,
) -> StarcoinResult<Option<Field<K, V>>>
where
    K: Clone + Serialize,
    V: Clone + for<'de> Deserialize<'de>,
{
    // TODO: Implement actual Table lookup when integrated with Starcoin state
    //
    // In a full implementation, this would:
    // 1. Compute the table entry key: hash(parent_id || bcs::to_bytes(name))
    // 2. Look up the entry in the parent's Table resource
    // 3. Deserialize and return the value
    //
    // For now, return None to indicate field not found
    Ok(None)
}

/// Compute a deterministic field ID from parent and name
///
/// This creates a unique ID for a dynamic field based on its parent and key.
/// Used for compatibility with Sui's dynamic field addressing.
pub fn compute_field_id<K: Serialize>(parent: &ObjectID, name: &K) -> ObjectID {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let name_bytes = bcs::to_bytes(name).unwrap_or_default();

    let mut hasher = DefaultHasher::new();
    parent.hash(&mut hasher);
    name_bytes.hash(&mut hasher);

    let hash = hasher.finish();

    // Create ObjectID from hash (deterministic but unique per parent+name)
    let mut id = [0u8; 32];
    id[..8].copy_from_slice(&hash.to_le_bytes());
    id[8..16].copy_from_slice(&hash.to_be_bytes()); // Use both orderings for more uniqueness
    // Copy parent ID suffix for traceability
    id[16..32].copy_from_slice(&parent[16..32]);

    id
}

/// Table-based dynamic field accessor for Starcoin
///
/// This provides a more Starcoin-native interface for dynamic field access.
#[derive(Clone, Debug)]
pub struct TableAccessor<K, V> {
    /// The table's handle/address
    pub table_handle: ObjectID,
    _phantom: std::marker::PhantomData<(K, V)>,
}

impl<K, V> TableAccessor<K, V>
where
    K: Serialize + Clone,
    V: for<'de> Deserialize<'de> + Clone,
{
    /// Create a new TableAccessor
    pub fn new(table_handle: ObjectID) -> Self {
        Self {
            table_handle,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get a value from the table
    ///
    /// In a full implementation, this would query Starcoin state.
    pub fn get(&self, _store: &dyn ObjectStore, _key: &K) -> StarcoinResult<Option<V>> {
        // TODO: Implement actual Table::borrow lookup
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_creation() {
        let id = [1u8; 32];
        let field: Field<String, u64> = Field::new(id, "test_key".to_string(), 42);

        assert_eq!(field.name(), "test_key");
        assert_eq!(*field.value(), 42);
        assert_eq!(field.into_value(), 42);
    }

    #[test]
    fn test_compute_field_id() {
        let parent = [1u8; 32];
        let name1 = "field1";
        let name2 = "field2";

        let id1 = compute_field_id(&parent, &name1);
        let id2 = compute_field_id(&parent, &name2);

        // Different names should produce different IDs
        assert_ne!(id1, id2);

        // Same parent+name should produce same ID (deterministic)
        let id1_again = compute_field_id(&parent, &name1);
        assert_eq!(id1, id1_again);
    }
}
