// Object types for Starcoin Bridge
// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

//! Object and ownership types for the Starcoin Bridge.
//!
//! # Starcoin vs Sui Ownership Model
//!
//! - **Sui**: Has 4 ownership types: AddressOwner, ObjectOwner, Shared, Immutable
//! - **Starcoin**: Uses account-based model where resources are owned by accounts
//!
//! For backward compatibility, we keep the Owner enum but adapt it to Starcoin semantics:
//! - `AddressOwner`: Maps directly to Starcoin account ownership
//! - `Shared`: Maps to module-level storage (like a resource under a module address)
//! - `Immutable`: Maps to published modules or frozen resources
//! - `ObjectOwner`: Not supported in Starcoin (we convert to AddressOwner)

use super::base_types::{ObjectID, SequenceNumber, StarcoinAddress};
use serde::{Deserialize, Serialize};

/// Owner type for bridge resources
///
/// In Starcoin, all resources are owned by accounts. This enum provides
/// backward compatibility with Sui-style ownership while mapping to
/// Starcoin's account-based model.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum Owner {
    /// Resource owned by an account address
    /// This is the primary ownership type in Starcoin
    AddressOwner(StarcoinAddress),

    /// For backward compatibility with Sui's object-owning-object pattern
    /// In Starcoin, this is converted to AddressOwner using the ObjectID's
    /// embedded address (last 16 bytes)
    #[deprecated(note = "Starcoin doesn't support object ownership; use AddressOwner")]
    ObjectOwner(ObjectID),

    /// Shared resource - in Starcoin context, this represents a resource
    /// that is stored at a well-known address (like the bridge module address)
    /// and can be accessed by multiple transactions.
    ///
    /// The `initial_shared_version` is kept for compatibility but isn't used
    /// in the same way as Sui - Starcoin doesn't have object versioning.
    Shared {
        /// Kept for serialization compatibility; in Starcoin this is typically 0
        initial_shared_version: SequenceNumber,
    },

    /// Immutable resource - in Starcoin context, this represents:
    /// - Published modules (code is immutable once published)
    /// - Frozen resources that cannot be modified
    Immutable,
}

impl Owner {
    /// Create an AddressOwner
    pub fn address_owner(addr: StarcoinAddress) -> Self {
        Owner::AddressOwner(addr)
    }

    /// Create a Shared owner with initial version 0 (Starcoin default)
    pub fn shared() -> Self {
        Owner::Shared {
            initial_shared_version: 0,
        }
    }

    /// Create a Shared owner with specified version (for compatibility)
    pub fn shared_with_version(version: SequenceNumber) -> Self {
        Owner::Shared {
            initial_shared_version: version,
        }
    }

    /// Create an Immutable owner
    pub fn immutable() -> Self {
        Owner::Immutable
    }

    /// Check if owned by a specific address
    pub fn is_owned_by(&self, addr: &StarcoinAddress) -> bool {
        match self {
            Owner::AddressOwner(owner) => owner == addr,
            #[allow(deprecated)]
            Owner::ObjectOwner(obj_id) => {
                // Check if the ObjectID's embedded address matches
                use super::base_types::ObjectIDExt;
                obj_id.to_starcoin_address().as_ref() == Some(addr)
            }
            _ => false,
        }
    }

    /// Check if this is a shared resource
    pub fn is_shared(&self) -> bool {
        matches!(self, Owner::Shared { .. })
    }

    /// Check if this is immutable
    pub fn is_immutable(&self) -> bool {
        matches!(self, Owner::Immutable)
    }

    /// Get the owner address if this is AddressOwner
    pub fn get_address(&self) -> Option<StarcoinAddress> {
        match self {
            Owner::AddressOwner(addr) => Some(*addr),
            #[allow(deprecated)]
            Owner::ObjectOwner(obj_id) => {
                use super::base_types::ObjectIDExt;
                obj_id.to_starcoin_address()
            }
            _ => None,
        }
    }

    /// Get the initial shared version if this is Shared
    pub fn get_shared_version(&self) -> Option<SequenceNumber> {
        match self {
            Owner::Shared {
                initial_shared_version,
            } => Some(*initial_shared_version),
            _ => None,
        }
    }
}

impl Default for Owner {
    fn default() -> Self {
        Owner::AddressOwner(StarcoinAddress::ZERO)
    }
}

/// Object representation for bridge compatibility
///
/// In Starcoin, this represents a resource stored under an account.
/// The `data` field contains the BCS-serialized resource data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Object {
    /// Object ID (32 bytes for compatibility with Sui-style IDs)
    #[serde(default)]
    pub object_id: ObjectID,
    /// The owner of this resource
    pub owner: Owner,
    /// BCS-serialized data of the resource
    pub data: Vec<u8>,
}

impl Object {
    /// Get the object ID (32 bytes)
    pub fn id(&self) -> ObjectID {
        self.object_id
    }

    /// Create a new Object with the given owner and data
    pub fn new(owner: Owner, data: Vec<u8>) -> Self {
        Self {
            object_id: ObjectID::default(),
            owner,
            data,
        }
    }

    /// Create a new Object with explicit ID
    pub fn with_id(id: ObjectID, owner: Owner, data: Vec<u8>) -> Self {
        Self {
            object_id: id,
            owner,
            data,
        }
    }

    /// Create an Object owned by an address
    pub fn owned_by(addr: StarcoinAddress, data: Vec<u8>) -> Self {
        Self {
            object_id: ObjectID::default(),
            owner: Owner::AddressOwner(addr),
            data,
        }
    }

    /// Create a shared Object
    pub fn shared(data: Vec<u8>) -> Self {
        Self {
            object_id: ObjectID::default(),
            owner: Owner::shared(),
            data,
        }
    }

    /// Create an immutable Object
    pub fn immutable(data: Vec<u8>) -> Self {
        Self {
            object_id: ObjectID::default(),
            owner: Owner::Immutable,
            data,
        }
    }

    /// Try to deserialize the data as a specific type
    pub fn try_as<T: serde::de::DeserializeOwned>(&self) -> Result<T, bcs::Error> {
        bcs::from_bytes(&self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_owner_address() {
        let addr = StarcoinAddress::new([1u8; 16]);
        let owner = Owner::address_owner(addr);

        assert!(owner.is_owned_by(&addr));
        assert!(!owner.is_shared());
        assert!(!owner.is_immutable());
        assert_eq!(owner.get_address(), Some(addr));
    }

    #[test]
    fn test_owner_shared() {
        let owner = Owner::shared();

        assert!(owner.is_shared());
        assert!(!owner.is_immutable());
        assert_eq!(owner.get_shared_version(), Some(0));
    }

    #[test]
    fn test_owner_immutable() {
        let owner = Owner::immutable();

        assert!(owner.is_immutable());
        assert!(!owner.is_shared());
        assert_eq!(owner.get_address(), None);
    }

    #[test]
    fn test_object_deserialization() {
        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct TestData {
            value: u64,
        }

        let test_data = TestData { value: 42 };
        let data = bcs::to_bytes(&test_data).unwrap();
        let obj = Object::owned_by(StarcoinAddress::ZERO, data);

        let recovered: TestData = obj.try_as().unwrap();
        assert_eq!(recovered, test_data);
    }
}
