// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

//! Bridge test utilities - provides missing Starcoin test APIs for Starcoin

#[cfg(test)]
pub use starcoin_bridge_types::base_types::{StarcoinAddress, TransactionDigest};

#[cfg(test)]
pub trait TestingExt {
    fn random_for_testing_only() -> Self;
}

#[cfg(test)]
impl TestingExt for StarcoinAddress {
    fn random_for_testing_only() -> Self {
        use move_core_types::account_address::AccountAddress;
        AccountAddress::random().into()
    }
}

#[cfg(test)]
pub trait DigestTestingExt {
    fn random() -> Self;
}

#[cfg(test)]
impl DigestTestingExt for TransactionDigest {
    fn random() -> Self {
        use starcoin_bridge_types::digests::Digest;
        TransactionDigest::from_digest(Digest::random())
    }
}

// Re-export for tests
#[cfg(test)]
pub use TestingExt as _; 
#[cfg(test)]
pub use DigestTestingExt as _;
