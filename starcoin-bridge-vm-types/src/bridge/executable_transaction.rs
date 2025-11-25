// Executable transaction (minimal stub)
// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CertificateProof {
    QuorumExecuted(Vec<u8>),
    Checkpoint(u64, u64),
}

impl CertificateProof {
    pub fn new_from_cert_sig(sig: Vec<u8>) -> Self {
        CertificateProof::QuorumExecuted(sig)
    }

    pub fn new_from_checkpoint(seq: u64, index: u64) -> Self {
        CertificateProof::Checkpoint(seq, index)
    }

    pub fn new_from_consensus(_round: u64, _index: u64) -> Self {
        // Consensus is represented as checkpoint for now
        CertificateProof::Checkpoint(_round, _index)
    }

    pub fn new_system() -> Self {
        CertificateProof::Checkpoint(0, 0)
    }

    pub fn epoch(&self) -> u64 {
        // For bridge, epoch is embedded in checkpoint or derived
        match self {
            CertificateProof::Checkpoint(epoch, _) => *epoch,
            CertificateProof::QuorumExecuted(_) => 0,
        }
    }
}
