// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
// Simplified for Starcoin Bridge

use super::base_types::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type EpochId = u64;
pub type StakeUnit = u64;
pub type CommitteeDigest = [u8; 32];
pub type ProtocolVersion = u64;

// Voting power constants (Bridge uses these)
pub const TOTAL_VOTING_POWER: StakeUnit = 10_000;
pub const QUORUM_THRESHOLD: StakeUnit = 6_667;
pub const VALIDITY_THRESHOLD: StakeUnit = 3_334;

/// Committee trait for Bridge compatibility
pub trait CommitteeTrait {
    fn epoch(&self) -> EpochId;
    fn num_members(&self) -> usize;
    fn total_votes(&self) -> StakeUnit {
        TOTAL_VOTING_POWER
    }
    fn quorum_threshold(&self) -> StakeUnit {
        QUORUM_THRESHOLD
    }
    fn validity_threshold(&self) -> StakeUnit {
        VALIDITY_THRESHOLD
    }

    // Shuffle committee members by stake with random number generator
    fn shuffle_by_stake_with_rng<R: rand::Rng>(
        &self,
        preferences: &[super::base_types::AuthorityName],
        rng: &mut R,
    ) -> Vec<super::base_types::AuthorityName>;

    // Get weight/stake of an authority
    fn weight(&self, authority: &super::base_types::AuthorityName) -> StakeUnit;
}

/// Minimal Committee implementation for Bridge
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Committee {
    pub epoch: EpochId,
    pub voting_rights: Vec<(AuthorityName, StakeUnit)>,
}

impl Committee {
    pub fn new(epoch: EpochId, voting_rights: BTreeMap<AuthorityName, StakeUnit>) -> Self {
        let voting_rights: Vec<(AuthorityName, StakeUnit)> =
            voting_rights.iter().map(|(a, s)| (*a, *s)).collect();
        Committee {
            epoch,
            voting_rights,
        }
    }

    pub fn epoch(&self) -> EpochId {
        self.epoch
    }
}

impl CommitteeTrait for Committee {
    fn epoch(&self) -> EpochId {
        self.epoch
    }

    fn num_members(&self) -> usize {
        self.voting_rights.len()
    }

    fn shuffle_by_stake_with_rng<R: rand::Rng>(
        &self,
        _preferences: &[super::base_types::AuthorityName],
        _rng: &mut R,
    ) -> Vec<super::base_types::AuthorityName> {
        // Simple stub: return authorities in original order
        self.voting_rights.iter().map(|(name, _)| *name).collect()
    }

    fn weight(&self, authority: &super::base_types::AuthorityName) -> StakeUnit {
        self.voting_rights
            .iter()
            .find(|(name, _)| name == authority)
            .map(|(_, stake)| *stake)
            .unwrap_or(0)
    }
}
