// Base types for Starcoin Bridge compatibility
// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::fmt;

// Re-export from move types
pub use move_core_types::account_address::AccountAddress as StarcoinAddress;

/// Transaction digest (hash)
pub type TransactionDigest = [u8; 32];

/// Authority name (public key bytes)
pub type AuthorityName = [u8; 32];

/// Object ID
pub type ObjectID = [u8; 32];

// Helper for ObjectID
impl ObjectIDExt for ObjectID {
    fn random() -> Self {
        use rand::{RngCore, SeedableRng};
        let mut rng = rand::rngs::StdRng::from_seed([0u8; 32]); // deterministic for now
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        bytes
    }
}

pub trait ObjectIDExt {
    fn random() -> Self;
}

/// Sequence number for versioning
pub type SequenceNumber = u64;

/// Object digest
pub type ObjectDigest = [u8; 32];

/// Object reference: (ID, version, digest)
pub type ObjectRef = (ObjectID, SequenceNumber, ObjectDigest);

/// Trait for concise name display
pub trait ConciseableName<'a> {
    type ConciseTypeRef;
    type ConciseType;

    fn concise(&'a self) -> Self::ConciseTypeRef;
    fn concise_owned(&self) -> Self::ConciseType;
}

impl<'a> ConciseableName<'a> for StarcoinAddress {
    type ConciseTypeRef = &'a StarcoinAddress;
    type ConciseType = String;

    fn concise(&'a self) -> Self::ConciseTypeRef {
        self
    }

    fn concise_owned(&self) -> String {
        format!("{:?}", self)
    }
}

/// Version digest
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Hash)]
pub struct VersionDigest(pub SequenceNumber, pub ObjectDigest);

impl fmt::Display for VersionDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {:?})", self.0, self.1)
    }
}
