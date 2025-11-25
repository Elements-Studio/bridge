// Stub for starcoin-bridge-types - Remaining types that haven't been migrated to starcoin_bridge_vm_types
#![allow(dead_code, unused_variables)]

use serde::{Deserialize, Serialize};

// Re-export types that have been migrated to starcoin_bridge_vm_types
pub mod base_types {
    pub use starcoin_bridge_vm_types::bridge::base_types::*;

    // Type aliases for compatibility
    pub type TransactionDigest = [u8; 32];

    // STARCOIN_ADDRESS_LENGTH constant (if needed)
    pub const STARCOIN_ADDRESS_LENGTH: usize = 32;

    // ZERO ObjectID constant
    pub const ZERO_OBJECT_ID: ObjectID = [0u8; 32];

    // Helper function for testing
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

    // Extension trait for concise display of numeric types
    pub trait ConciseDisplay {
        fn concise(&self) -> String;
    }

    impl ConciseDisplay for usize {
        fn concise(&self) -> String {
            self.to_string()
        }
    }

    // Extension trait for hex display of byte arrays
    pub trait ToHex {
        fn to_hex(&self) -> String;
    }

    impl ToHex for [u8; 32] {
        fn to_hex(&self) -> String {
            hex::encode(self)
        }
    }

    // Helper functions for conversion (cannot implement From due to orphan rules)
    pub fn starcoin_bridge_address_from_bytes(bytes: [u8; 32]) -> StarcoinAddress {
        use move_core_types::account_address::AccountAddress;
        // AccountAddress in Move is 16 bytes, take first 16
        AccountAddress::from_bytes(&bytes[..16]).unwrap_or(AccountAddress::ZERO)
    }

    pub fn starcoin_bridge_address_to_bytes(addr: StarcoinAddress) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[..16].copy_from_slice(addr.as_ref());
        bytes
    }
}

pub mod bridge {
    pub use starcoin_bridge_vm_types::bridge::bridge::*;
}

pub mod committee {
    pub use starcoin_bridge_vm_types::bridge::committee::*;
}

pub mod crypto {
    // Re-export what we have in starcoin_bridge_vm_types
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
    }

    // Implement Signer trait for Signature compatibility
    impl fastcrypto::traits::Signer<Signature> for StarcoinKeyPair {
        fn sign(&self, _msg: &[u8]) -> Signature {
            // Stub implementation - returns empty signature
            Signature(vec![])
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

    // Placeholder for Starcoin transaction type
    pub type StarcoinTransaction = Vec<u8>;

    // Wrapper type for Transaction with Starcoin-compatible interface
    #[derive(Clone, Debug)]
    pub struct Transaction(pub StarcoinTransaction);

    impl Transaction {
        pub fn from_data(
            _data: TransactionData,
            _signatures: Vec<super::crypto::Signature>,
        ) -> Self {
            // Stub implementation
            unimplemented!("Transaction::from_data not implemented for Starcoin")
        }

        pub fn digest(&self) -> &super::base_types::TransactionDigest {
            // Return a static digest reference - TransactionDigest is [u8; 32]
            static DIGEST: super::base_types::TransactionDigest = [0u8; 32];
            &DIGEST
        }
    }

    // Starcoin-specific types that still need stubs
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct TransactionData;

    impl TransactionData {
        pub fn new_programmable(
            _sender: super::base_types::StarcoinAddress,
            _gas_payment: Vec<super::base_types::ObjectRef>,
            _pt: ProgrammableTransaction,
            _gas_budget: u64,
            _gas_price: u64,
        ) -> Self {
            TransactionData
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
        // Clock object IMM variant
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
        // STARCOIN system object MUT variant
        pub const STARCOIN_SYSTEM_MUT: Self = ObjectArg::SharedObject {
            id: super::base_types::ZERO_OBJECT_ID,
            initial_shared_version: 1,
            mutable: true,
        };
    }

    // Starcoin-specific transaction types (stubs)
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
            module: move_core_types::identifier::Identifier,
            function: move_core_types::identifier::Identifier,
            type_arguments: Vec<move_core_types::language_storage::TypeTag>,
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
        pub module: move_core_types::identifier::Identifier,
        pub function: move_core_types::identifier::Identifier,
        pub type_arguments: Vec<move_core_types::language_storage::TypeTag>,
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
    use super::*;

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
