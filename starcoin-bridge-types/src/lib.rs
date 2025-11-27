// Starcoin Bridge Types
// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

//! Bridge type definitions for Starcoin.
//!
//! This crate provides type definitions that bridge between the Sui-originated
//! bridge code and Starcoin's native types. It maintains backward compatibility
//! while adapting to Starcoin's account-based model.

#![allow(dead_code, unused_variables)]

use serde::{Deserialize, Serialize};

// =============================================================================
// Starcoin Native Transaction Builder
// =============================================================================

pub mod starcoin_transaction_builder;
pub use starcoin_transaction_builder::*;

// =============================================================================
// Re-exports from starcoin_bridge_vm_types
// =============================================================================

pub mod base_types {
    pub use starcoin_bridge_vm_types::bridge::base_types::*;

    // Re-export the ZERO constant
    pub use starcoin_bridge_vm_types::bridge::base_types::ZERO_OBJECT_ID;

    // STARCOIN_ADDRESS_LENGTH - Starcoin uses 16-byte addresses
    pub const STARCOIN_ADDRESS_LENGTH: usize = 16;

    // For compatibility, also define a 32-byte length
    pub const OBJECT_ID_LENGTH: usize = 32;

    /// Create a random ObjectRef for testing
    pub fn random_object_ref() -> ObjectRef {
        use rand::{RngCore, SeedableRng};
        let mut rng = rand::rngs::StdRng::from_entropy();
        let mut id = [0u8; 32];
        rng.fill_bytes(&mut id);
        let version = rng.next_u64();
        let mut digest = [0u8; 32];
        rng.fill_bytes(&mut digest);
        (id, version, digest)
    }

    /// Extension trait for concise display
    pub trait ConciseDisplay {
        fn concise(&self) -> String;
    }

    impl ConciseDisplay for usize {
        fn concise(&self) -> String {
            self.to_string()
        }
    }

    impl ConciseDisplay for u64 {
        fn concise(&self) -> String {
            self.to_string()
        }
    }

    // ==========================================================================
    // Backward compatibility functions (for migration from Sui)
    // ==========================================================================

    /// Convert a 32-byte array to StarcoinAddress (takes last 16 bytes)
    /// This is for backward compatibility with code that uses 32-byte identifiers
    #[inline]
    pub fn starcoin_bridge_address_from_bytes(bytes: [u8; 32]) -> StarcoinAddress {
        bytes32_to_starcoin_address(&bytes)
    }

    /// Convert StarcoinAddress to a 32-byte array (left-padded with zeros)
    /// This is for backward compatibility with code that expects 32-byte identifiers
    #[inline]
    pub fn starcoin_bridge_address_to_bytes(addr: StarcoinAddress) -> [u8; 32] {
        starcoin_address_to_bytes32(&addr)
    }
}

pub mod bridge {
    pub use starcoin_bridge_vm_types::bridge::bridge::*;
}

pub mod committee {
    pub use starcoin_bridge_vm_types::bridge::committee::*;
}

pub mod crypto {
    pub use starcoin_bridge_vm_types::bridge::crypto::*;

    use fastcrypto::{
        ed25519::Ed25519KeyPair,
        error::FastCryptoError,
        secp256k1::Secp256k1KeyPair,
        traits::{EncodeDecodeBase64, KeyPair as KeypairTraits, ToFromBytes},
    };
    use serde::{Deserialize, Serialize};

    // Re-export Signature from starcoin_bridge_vm_types
    pub use starcoin_bridge_vm_types::bridge::crypto::Signature;

    // NetworkKeyPair is just an alias for Ed25519KeyPair in Starcoin
    pub type NetworkKeyPair = Ed25519KeyPair;

    // Extension trait to add copy() method to NetworkKeyPair
    pub trait NetworkKeyPairExt {
        fn copy(&self) -> Self;
    }

    impl NetworkKeyPairExt for NetworkKeyPair {
        fn copy(&self) -> Self {
            // Create a copy by serializing and deserializing
            use fastcrypto::traits::ToFromBytes;
            let bytes = self.as_bytes();
            Ed25519KeyPair::from_bytes(&bytes).expect("Failed to copy keypair")
        }
    }

    // Generic key pair generation function
    pub fn get_key_pair<KP: KeypairTraits>() -> ((), KP) {
        let mut rng = rand::thread_rng();
        ((), KP::generate(&mut rng))
    }

    // Re-export Secp256k1PublicKey for convenience
    pub use fastcrypto::secp256k1::Secp256k1PublicKey;

    // Define StarcoinKeyPair enum (simplified - only Ed25519 and Secp256k1)
    #[derive(Debug, Serialize, Deserialize)]
    #[serde(tag = "type")]
    pub enum StarcoinKeyPair {
        Ed25519(Ed25519KeyPair),
        Secp256k1(Secp256k1KeyPair),
        // TODO: Add Secp256r1 support when fastcrypto adds it
        // Secp256r1(Secp256r1KeyPair),
    }

    impl StarcoinKeyPair {
        pub fn public(&self) -> Vec<u8> {
            use fastcrypto::traits::KeyPair;
            match self {
                StarcoinKeyPair::Ed25519(kp) => kp.public().as_bytes().to_vec(),
                StarcoinKeyPair::Secp256k1(kp) => kp.public().as_bytes().to_vec(),
            }
        }

        /// Sign a message and return (public_key, signature) bytes
        pub fn sign_message(&self, msg: &[u8]) -> (Vec<u8>, Vec<u8>) {
            use fastcrypto::traits::KeyPair;
            match self {
                StarcoinKeyPair::Ed25519(kp) => {
                    let sig = fastcrypto::traits::Signer::<fastcrypto::ed25519::Ed25519Signature>::sign(kp, msg);
                    (kp.public().as_bytes().to_vec(), sig.as_bytes().to_vec())
                }
                StarcoinKeyPair::Secp256k1(kp) => {
                    let sig = fastcrypto::traits::Signer::<fastcrypto::secp256k1::Secp256k1Signature>::sign(kp, msg);
                    (kp.public().as_bytes().to_vec(), sig.as_bytes().to_vec())
                }
            }
        }

        /// Get the private key bytes (for Ed25519 signing)
        pub fn private_key_bytes(&self) -> Vec<u8> {
            match self {
                StarcoinKeyPair::Ed25519(kp) => kp.as_bytes()[..32].to_vec(), // Ed25519 private key is first 32 bytes
                StarcoinKeyPair::Secp256k1(kp) => kp.as_bytes().to_vec(),
            }
        }
    }

    // Implement Signer trait for Signature compatibility
    impl fastcrypto::traits::Signer<Signature> for StarcoinKeyPair {
        fn sign(&self, msg: &[u8]) -> Signature {
            let (_, sig_bytes) = self.sign_message(msg);
            Signature(sig_bytes)
        }
    }

    // Implement starcoin Signer for StarcoinKeyPair
    impl
        starcoin_bridge_vm_types::bridge::crypto::Signer<
            starcoin_bridge_vm_types::bridge::crypto::AuthoritySignature,
        > for StarcoinKeyPair
    {
        fn sign(
            &self,
            _msg: &[u8],
        ) -> starcoin_bridge_vm_types::bridge::crypto::AuthoritySignature {
            // Stub implementation - returns placeholder signature
            use fastcrypto::traits::ToFromBytes;
            // Create a placeholder Ed25519Signature with zeros
            starcoin_bridge_vm_types::bridge::crypto::AuthoritySignature::from_bytes(&[0u8; 64])
                .expect("Failed to create placeholder signature")
        }
    }

    impl EncodeDecodeBase64 for StarcoinKeyPair {
        /// Encode keypair as base64 string with scheme flag prefix (flag || privkey)
        fn encode_base64(&self) -> String {
            use base64ct::{Base64, Encoding};
            Base64::encode_string(&self.to_bytes())
        }

        /// Decode base64 string with scheme flag prefix to keypair
        fn decode_base64(value: &str) -> Result<Self, FastCryptoError> {
            use base64ct::{Base64, Encoding};
            let bytes = Base64::decode_vec(value).map_err(|_| FastCryptoError::InvalidInput)?;
            Self::from_bytes(&bytes).map_err(|_| FastCryptoError::InvalidInput)
        }
    }

    /// Signature scheme flags matching Starcoin's implementation
    const ED25519_FLAG: u8 = 0x00;
    const SECP256K1_FLAG: u8 = 0x01;

    impl StarcoinKeyPair {
        /// Get the scheme flag for this keypair
        fn scheme_flag(&self) -> u8 {
            match self {
                StarcoinKeyPair::Ed25519(_) => ED25519_FLAG,
                StarcoinKeyPair::Secp256k1(_) => SECP256K1_FLAG,
            }
        }

        /// Convert keypair to bytes with scheme flag prefix (flag || privkey)
        pub fn to_bytes(&self) -> Vec<u8> {
            let mut bytes: Vec<u8> = Vec::new();
            // Add scheme flag as first byte
            bytes.push(self.scheme_flag());

            // Add private key bytes
            match self {
                StarcoinKeyPair::Ed25519(kp) => {
                    bytes.extend_from_slice(kp.as_bytes());
                }
                StarcoinKeyPair::Secp256k1(kp) => {
                    bytes.extend_from_slice(kp.as_bytes());
                }
            }
            bytes
        }

        /// Parse keypair from bytes with scheme flag prefix (flag || privkey)
        pub fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
            let flag = bytes.first().ok_or(FastCryptoError::InvalidInput)?;

            match *flag {
                ED25519_FLAG => {
                    let kp = Ed25519KeyPair::from_bytes(&bytes[1..])
                        .map_err(|_| FastCryptoError::InvalidInput)?;
                    Ok(StarcoinKeyPair::Ed25519(kp))
                }
                SECP256K1_FLAG => {
                    let kp = Secp256k1KeyPair::from_bytes(&bytes[1..])
                        .map_err(|_| FastCryptoError::InvalidInput)?;
                    Ok(StarcoinKeyPair::Secp256k1(kp))
                }
                _ => Err(FastCryptoError::InvalidInput),
            }
        }
    }
}

pub mod message_envelope {
    pub use starcoin_bridge_vm_types::bridge::message_envelope::*;
}

pub mod messages_checkpoint {
    pub use starcoin_bridge_vm_types::bridge::messages_checkpoint::*;
}

pub mod object {
    pub use starcoin_bridge_vm_types::bridge::object::*;
}

pub mod collection_types {
    pub use starcoin_bridge_vm_types::bridge::collection_types::*;
}

// ============= Types still needing stubs =============

// Add quorum_driver_types module
pub mod quorum_driver_types {
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct ExecuteTransactionRequestV3 {
        pub transaction: Vec<u8>,
        pub include_events: bool,
        pub include_input_objects: bool,
        pub include_output_objects: bool,
        pub include_auxiliary_data: bool,
    }

    impl Default for ExecuteTransactionRequestV3 {
        fn default() -> Self {
            Self {
                transaction: Vec::new(),
                include_events: false,
                include_input_objects: false,
                include_output_objects: false,
                include_auxiliary_data: false,
            }
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub enum ExecuteTransactionRequestType {
        WaitForEffectsCert,
        WaitForLocalExecution,
    }
}

pub mod digests {
    pub use starcoin_bridge_vm_types::bridge::base_types::TransactionDigest;

    // Digest trait placeholder
    pub trait Digest: Clone + std::fmt::Debug {}
    impl Digest for TransactionDigest {}

    // Chain identifier functions (stubs)
    pub fn get_mainnet_chain_identifier() -> String {
        "mainnet".to_string()
    }

    pub fn get_testnet_chain_identifier() -> String {
        "testnet".to_string()
    }
}

pub mod transaction {
    use super::*;
    use move_core_types::identifier::{IdentStr, Identifier};
    use move_core_types::language_storage::{ModuleId, TypeTag};

    // ==========================================================================
    // Starcoin Native Transaction Types
    // ==========================================================================

    /// ScriptFunction - calls a Move function on-chain
    /// This is the primary way to interact with Move contracts on Starcoin
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct ScriptFunction {
        pub module: ModuleId,
        pub function: Identifier,
        pub ty_args: Vec<TypeTag>,
        pub args: Vec<Vec<u8>>,
    }

    impl ScriptFunction {
        pub fn new(
            module: ModuleId,
            function: Identifier,
            ty_args: Vec<TypeTag>,
            args: Vec<Vec<u8>>,
        ) -> Self {
            Self {
                module,
                function,
                ty_args,
                args,
            }
        }

        pub fn module(&self) -> &ModuleId {
            &self.module
        }

        pub fn function(&self) -> &IdentStr {
            &self.function
        }

        pub fn ty_args(&self) -> &[TypeTag] {
            &self.ty_args
        }

        pub fn args(&self) -> &[Vec<u8>] {
            &self.args
        }
    }

    /// Transaction payload - what the transaction does
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub enum TransactionPayload {
        /// Call a script function
        ScriptFunction(ScriptFunction),
        /// Package deployment (not used in bridge)
        Package(Vec<u8>),
    }

    /// Chain ID for replay protection
    #[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
    pub struct ChainId(pub u8);

    impl ChainId {
        pub fn new(id: u8) -> Self {
            ChainId(id)
        }

        pub fn id(&self) -> u8 {
            self.0
        }
    }

    /// RawUserTransaction - the core transaction structure in Starcoin
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct RawUserTransaction {
        pub sender: super::base_types::StarcoinAddress,
        pub sequence_number: u64,
        pub payload: TransactionPayload,
        pub max_gas_amount: u64,
        pub gas_unit_price: u64,
        pub gas_token_code: String,
        pub expiration_timestamp_secs: u64,
        pub chain_id: ChainId,
    }

    impl RawUserTransaction {
        /// Create a new RawUserTransaction with a script function
        pub fn new_script_function(
            sender: super::base_types::StarcoinAddress,
            sequence_number: u64,
            script_function: ScriptFunction,
            max_gas_amount: u64,
            gas_unit_price: u64,
            expiration_timestamp_secs: u64,
            chain_id: ChainId,
        ) -> Self {
            Self {
                sender,
                sequence_number,
                payload: TransactionPayload::ScriptFunction(script_function),
                max_gas_amount,
                gas_unit_price,
                gas_token_code: "0x1::STC::STC".to_string(),
                expiration_timestamp_secs,
                chain_id,
            }
        }

        pub fn sender(&self) -> super::base_types::StarcoinAddress {
            self.sender
        }

        pub fn sequence_number(&self) -> u64 {
            self.sequence_number
        }

        pub fn payload(&self) -> &TransactionPayload {
            &self.payload
        }

        pub fn max_gas_amount(&self) -> u64 {
            self.max_gas_amount
        }

        pub fn gas_unit_price(&self) -> u64 {
            self.gas_unit_price
        }

        pub fn expiration_timestamp_secs(&self) -> u64 {
            self.expiration_timestamp_secs
        }

        pub fn chain_id(&self) -> ChainId {
            self.chain_id
        }

        /// Serialize for signing
        pub fn to_bytes(&self) -> Vec<u8> {
            bcs::to_bytes(self).expect("RawUserTransaction serialization should not fail")
        }
    }

    /// Signed transaction ready for submission
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct SignedUserTransaction {
        pub raw_txn: RawUserTransaction,
        pub authenticator: TransactionAuthenticator,
    }

    impl SignedUserTransaction {
        pub fn new(raw_txn: RawUserTransaction, authenticator: TransactionAuthenticator) -> Self {
            Self {
                raw_txn,
                authenticator,
            }
        }

        /// Compute transaction hash
        pub fn hash(&self) -> super::base_types::TransactionDigest {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let bytes = bcs::to_bytes(self).unwrap_or_default();
            let mut hasher = DefaultHasher::new();
            bytes.hash(&mut hasher);
            let hash = hasher.finish();

            let mut digest = [0u8; 32];
            digest[..8].copy_from_slice(&hash.to_le_bytes());
            digest[8..16].copy_from_slice(&hash.to_be_bytes());
            digest
        }

        /// Encode as hex string for RPC submission
        pub fn to_hex(&self) -> String {
            hex::encode(bcs::to_bytes(self).unwrap_or_default())
        }
    }

    /// Transaction authenticator (signature)
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub enum TransactionAuthenticator {
        /// Ed25519 signature
        Ed25519 {
            public_key: Vec<u8>,
            signature: Vec<u8>,
        },
        /// Multi-ed25519 (not commonly used)
        MultiEd25519 {
            public_key: Vec<u8>,
            signature: Vec<u8>,
        },
    }

    // ==========================================================================
    // Backward Compatibility Layer (for migration from Sui)
    // These types are kept for code that still uses Sui patterns
    // ==========================================================================

    /// Legacy: Placeholder for Starcoin transaction type
    pub type StarcoinTransaction = Vec<u8>;

    /// Legacy: Wrapper type for Transaction with backward-compatible interface
    #[derive(Clone, Debug)]
    pub struct Transaction(pub StarcoinTransaction);

    impl Transaction {
        pub fn from_data(
            _data: TransactionData,
            _signatures: Vec<super::crypto::Signature>,
        ) -> Self {
            // Stub implementation - use SignedUserTransaction instead
            Transaction(vec![])
        }

        pub fn digest(&self) -> &super::base_types::TransactionDigest {
            static DIGEST: super::base_types::TransactionDigest = [0u8; 32];
            &DIGEST
        }
    }

    /// Legacy: TransactionData for backward compatibility
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct TransactionData {
        /// The actual Starcoin transaction (if built)
        #[serde(skip)]
        pub inner: Option<RawUserTransaction>,
    }

    impl TransactionData {
        /// Legacy constructor - kept for compatibility but does nothing useful
        pub fn new_programmable(
            _sender: super::base_types::StarcoinAddress,
            _gas_payment: Vec<super::base_types::ObjectRef>,
            _pt: ProgrammableTransaction,
            _gas_budget: u64,
            _gas_price: u64,
        ) -> Self {
            TransactionData { inner: None }
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct TransactionDataAPI {
        pub transaction: StarcoinTransaction,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub enum CallArg {
        Pure(Vec<u8>),
        Object(ObjectArg),
    }

    impl CallArg {
        pub const CLOCK_IMM: Self = CallArg::Object(ObjectArg::SharedObject {
            id: super::base_types::ZERO_OBJECT_ID,
            initial_shared_version: 1,
            mutable: false,
        });
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub enum ObjectArg {
        ImmOrOwnedObject((super::base_types::ObjectID, u64, [u8; 32])),
        SharedObject {
            id: super::base_types::ObjectID,
            initial_shared_version: u64,
            mutable: bool,
        },
    }

    impl ObjectArg {
        pub const STARCOIN_SYSTEM_MUT: Self = ObjectArg::SharedObject {
            id: super::base_types::ZERO_OBJECT_ID,
            initial_shared_version: 1,
            mutable: true,
        };
    }

    #[derive(Copy, Clone, Debug, Serialize, Deserialize)]
    pub enum Argument {
        Input(u16),
        Result(u16),
        NestedResult(u16, u16),
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub enum Command {
        MoveCall(Box<ProgrammableMoveCall>),
        TransferObjects(Vec<Argument>, Argument),
        SplitCoins(Argument, Vec<Argument>),
        MergeCoins(Argument, Vec<Argument>),
    }

    impl Command {
        pub fn move_call(
            package: super::base_types::ObjectID,
            module: Identifier,
            function: Identifier,
            type_arguments: Vec<TypeTag>,
            arguments: Vec<Argument>,
        ) -> Self {
            Command::MoveCall(Box::new(ProgrammableMoveCall {
                package,
                module,
                function,
                type_arguments,
                arguments,
            }))
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct ProgrammableMoveCall {
        pub package: super::base_types::ObjectID,
        pub module: Identifier,
        pub function: Identifier,
        pub type_arguments: Vec<TypeTag>,
        pub arguments: Vec<Argument>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct ProgrammableTransaction {
        pub inputs: Vec<CallArg>,
        pub commands: Vec<Command>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub enum TransactionKind {
        ProgrammableTransaction(ProgrammableTransaction),
    }

    impl TransactionKind {
        pub fn programmable(pt: ProgrammableTransaction) -> Self {
            TransactionKind::ProgrammableTransaction(pt)
        }
    }
}

pub mod event {
    // Use a simple tuple for EventID (checkpoint_sequence, event_index)
    pub type EventID = (u64, u64);

    // Placeholder for contract event
    pub type Event = Vec<u8>;
}

pub mod programmable_transaction_builder {
    use super::transaction::*;

    pub struct ProgrammableTransactionBuilder {
        inputs: Vec<CallArg>,
        commands: Vec<Command>,
        next_input: u16,
    }

    impl ProgrammableTransactionBuilder {
        pub fn new() -> Self {
            Self {
                inputs: Vec::new(),
                commands: Vec::new(),
                next_input: 0,
            }
        }

        pub fn pure<T: serde::Serialize>(&mut self, value: T) -> Result<Argument, String> {
            let bytes = bcs::to_bytes(&value).map_err(|e| e.to_string())?;
            let input_idx = self.next_input;
            self.next_input += 1;
            self.inputs.push(CallArg::Pure(bytes));
            Ok(Argument::Input(input_idx))
        }

        pub fn input(&mut self, call_arg: CallArg) -> Result<Argument, String> {
            let input_idx = self.next_input;
            self.next_input += 1;
            self.inputs.push(call_arg);
            Ok(Argument::Input(input_idx))
        }

        pub fn obj(&mut self, obj_arg: ObjectArg) -> Result<Argument, String> {
            let input_idx = self.next_input;
            self.next_input += 1;
            self.inputs.push(CallArg::Object(obj_arg));
            Ok(Argument::Input(input_idx))
        }

        pub fn programmable_move_call(
            &mut self,
            package: super::base_types::ObjectID,
            module: move_core_types::identifier::Identifier,
            function: move_core_types::identifier::Identifier,
            type_arguments: Vec<move_core_types::language_storage::TypeTag>,
            call_args: Vec<Argument>,
        ) -> Argument {
            let command_idx = self.commands.len() as u16;
            self.commands.push(Command::move_call(
                package,
                module,
                function,
                type_arguments,
                call_args,
            ));
            Argument::Result(command_idx)
        }

        pub fn finish(self) -> ProgrammableTransaction {
            ProgrammableTransaction {
                inputs: self.inputs,
                commands: self.commands,
            }
        }
    }
}

pub mod gas_coin {
    #[derive(Clone, Debug)]
    pub struct GasCoin {
        pub value: u64,
    }

    impl GasCoin {
        pub fn value(&self) -> u64 {
            self.value
        }
    }
}

pub mod full_checkpoint_content {
    use super::*;

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct CheckpointData {
        pub checkpoint_summary: CheckpointSummary,
        pub transactions: Vec<CheckpointTransaction>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct CheckpointTransaction {
        pub transaction: transaction::TransactionDataAPI,
        pub input_objects: Vec<object::Object>,
        pub output_objects: Vec<object::Object>,
        pub events: Option<TransactionEvents>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct TransactionEvents {
        pub data: Vec<event::Event>,
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct CheckpointSummary {
        pub epoch: u64,
        pub sequence_number: u64,
        pub timestamp_ms: u64,
        pub network_total_transactions: u64,
    }
}

pub mod starcoin_bridge_system_state {
    use super::*;

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct StarcoinSystemState {
        pub epoch: u64,
    }

    pub mod starcoin_bridge_system_state_summary {
        use super::*;

        #[derive(Clone, Debug, Serialize, Deserialize)]
        pub struct StarcoinSystemStateSummary {
            pub epoch: u64,
            pub active_validators: Vec<StarcoinValidatorSummary>,
        }

        #[derive(Clone, Debug, Serialize, Deserialize)]
        pub struct StarcoinValidatorSummary {
            pub starcoin_bridge_address: super::base_types::StarcoinAddress,
            pub name: String,
        }
    }
}

// ============= Constants =============
// Starcoin bridge package address (32 bytes for compatibility, but Starcoin uses 16 bytes)
// From Move.toml: Bridge = "0x246b237c16c761e9478783dd83f7004a"
// Padded with zeros in front to maintain compatibility with existing code expecting 32 bytes
pub const BRIDGE_PACKAGE_ID: [u8; 32] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 16 zero bytes padding
    0x24, 0x6b, 0x23, 0x7c, 0x16, 0xc7, 0x61, 0xe9, // Actual Starcoin address
    0x47, 0x87, 0x83, 0xdd, 0x83, 0xf7, 0x00, 0x4a,
];
// Note: Starcoin doesn't have a separate bridge object like Starcoin
pub const STARCOIN_BRIDGE_OBJECT_ID: [u8; 32] = [0; 32];

// Use Starcoin/Move types instead of stubs
pub use move_core_types::identifier::Identifier;
pub use move_core_types::language_storage::TypeTag;

// Parse function stub
pub fn parse_starcoin_bridge_type_tag(_s: &str) -> Result<TypeTag, String> {
    Err("parse_starcoin_bridge_type_tag not implemented".to_string())
}
