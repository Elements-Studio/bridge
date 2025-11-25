// Crypto types for Starcoin Bridge
// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::ed25519::{Ed25519KeyPair, Ed25519PublicKey, Ed25519Signature};
use fastcrypto::secp256k1::Secp256k1KeyPair;
use fastcrypto::traits::{Signer as SignerTrait, ToFromBytes};
use serde::{Deserialize, Serialize};

pub use fastcrypto::traits::KeyPair;

// Authority key types (Ed25519 for consensus)
pub type AuthorityKeyPair = Ed25519KeyPair;
pub type AuthorityPublicKey = Ed25519PublicKey;
pub type AuthoritySignature = Ed25519Signature;

// Network key types
pub type NetworkKeyPair = Ed25519KeyPair;
pub type NetworkPublicKey = Ed25519PublicKey;

// General Starcoin key pair (Secp256k1 for user keys)
pub type StarcoinKeyPair = Secp256k1KeyPair;

// Authority public key bytes
pub type AuthorityPublicKeyBytes = [u8; 32];

/// Convert Ed25519PublicKey to bytes
pub fn authority_public_key_bytes(pk: &Ed25519PublicKey) -> AuthorityPublicKeyBytes {
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&pk.as_bytes()[..32]);
    bytes
}

/// Signature wrapper
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Signature(pub Vec<u8>);

impl Signature {
    // Create a new secure signature - compatible with starcoin_bridge_types::crypto::Signature
    pub fn new_secure<T, S>(_intent_msg: &T, _signer: &S) -> Self
    where
        T: Serialize,
        S: ?Sized + Signer<AuthoritySignature>,
    {
        // For now, return an empty signature (stub implementation)
        // In real implementation, this would:
        // 1. Serialize the intent message
        // 2. Hash it
        // 3. Sign with the signer
        Signature(Vec::new())
    }
}

/// Signer trait
pub trait Signer<Sig> {
    fn sign(&self, msg: &[u8]) -> Sig;
}

impl Signer<AuthoritySignature> for AuthorityKeyPair {
    fn sign(&self, msg: &[u8]) -> AuthoritySignature {
        SignerTrait::sign(self, msg)
    }
}

// Authority sign info
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthoritySignInfo {
    pub epoch: u64,
    pub authority: AuthorityPublicKeyBytes,
    pub signature: AuthoritySignature,
}

impl AuthoritySignInfo {
    pub fn new<S>(
        epoch: u64,
        data: &impl Serialize,
        intent: &shared_crypto::intent::Intent,
        authority: AuthorityPublicKeyBytes,
        secret: &S,
    ) -> Self
    where
        S: Signer<AuthoritySignature>,
    {
        let mut message = bcs::to_bytes(intent).expect("intent serialization should not fail");
        message.extend(bcs::to_bytes(data).expect("data serialization should not fail"));

        let signature = secret.sign(&message);

        Self {
            epoch,
            authority,
            signature,
        }
    }

    pub fn verify_secure(
        &self,
        _data: &impl Serialize,
        _intent: shared_crypto::intent::Intent,
        _committee: &super::committee::Committee,
    ) -> super::error::StarcoinResult<()> {
        // TODO: Implement actual verification
        Ok(())
    }
}

pub trait AuthoritySignInfoTrait {
    fn epoch(&self) -> u64;
}

impl AuthoritySignInfoTrait for AuthoritySignInfo {
    fn epoch(&self) -> u64 {
        self.epoch
    }
}

/// Quorum signature info
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthorityQuorumSignInfo {
    pub epoch: u64,
    pub signatures: Vec<(AuthorityPublicKeyBytes, AuthoritySignature)>,
}

impl AuthorityQuorumSignInfo {
    pub fn new_from_auth_sign_infos(
        sign_infos: Vec<AuthoritySignInfo>,
        _committee: &super::committee::Committee,
    ) -> super::error::StarcoinResult<Self> {
        let epoch = sign_infos.first().map(|s| s.epoch).unwrap_or(0);
        let signatures = sign_infos
            .into_iter()
            .map(|s| (s.authority, s.signature))
            .collect();
        Ok(Self { epoch, signatures })
    }
}

pub type AuthorityStrongQuorumSignInfo = AuthorityQuorumSignInfo;

/// Empty sign info
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmptySignInfo {}

// Note: These utility functions are not currently usable due to rand version conflicts
// between different crates in the dependency tree. For production use, generate keys
// using the appropriate RNG from the fastcrypto crate directly.
//
// TODO: Resolve rand version conflicts or use a compatible RNG

/// Generate random committee keypairs
/// WARNING: Currently disabled due to rand version conflicts
#[allow(dead_code)]
pub fn random_committee_key_pairs_of_size(_size: usize) -> Vec<AuthorityKeyPair> {
    unimplemented!("random_committee_key_pairs_of_size disabled due to rand version conflicts")
}

/// Get a key pair for testing  
/// WARNING: Currently disabled due to rand version conflicts
#[allow(dead_code)]
pub fn get_key_pair() -> (AuthorityPublicKeyBytes, StarcoinKeyPair) {
    unimplemented!("get_key_pair disabled due to rand version conflicts")
}

/// Encode/Decode Base64 trait
pub trait EncodeDecodeBase64 {
    fn encode_base64(&self) -> String;
    fn decode_base64(s: &str) -> Result<Self, anyhow::Error>
    where
        Self: Sized;
}
