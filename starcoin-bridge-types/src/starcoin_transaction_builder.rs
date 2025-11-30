// Starcoin Transaction Builder
// Copyright (c) The Starcoin Core Contributors
// SPDX-License-Identifier: Apache-2.0

//! Transaction builder for Starcoin native transactions.
//!
//! This module provides a builder pattern for constructing Starcoin transactions,
//! replacing the Sui-style ProgrammableTransactionBuilder with Starcoin's
//! RawUserTransaction + ScriptFunction model.

use crate::base_types::StarcoinAddress;
use crate::transaction::{ChainId, RawUserTransaction, ScriptFunction, SignedUserTransaction, TransactionAuthenticator};
use crate::{Identifier, TypeTag};
use move_core_types::language_storage::ModuleId;

/// Bridge package address as StarcoinAddress (16 bytes)
pub fn bridge_module_address() -> StarcoinAddress {
    // Take last 16 bytes of BRIDGE_PACKAGE_ID
    StarcoinAddress::new([
        0x24, 0x6b, 0x23, 0x7c, 0x16, 0xc7, 0x61, 0xe9,
        0x47, 0x87, 0x83, 0xdd, 0x83, 0xf7, 0x00, 0x4a,
    ])
}

/// Default gas configuration for bridge transactions
pub struct GasConfig {
    pub max_gas_amount: u64,
    pub gas_unit_price: u64,
}

impl Default for GasConfig {
    fn default() -> Self {
        Self {
            max_gas_amount: 10_000_000,  // 10M gas units
            gas_unit_price: 1,            // 1 nano STC per gas unit
        }
    }
}

/// Builder for Starcoin bridge transactions
pub struct StarcoinTransactionBuilder {
    sender: StarcoinAddress,
    sequence_number: u64,
    chain_id: ChainId,
    gas_config: GasConfig,
    expiration_secs: u64,
}

impl StarcoinTransactionBuilder {
    /// Create a new transaction builder
    pub fn new(
        sender: StarcoinAddress,
        sequence_number: u64,
        chain_id: u8,
    ) -> Self {
        Self {
            sender,
            sequence_number,
            chain_id: ChainId::new(chain_id),
            gas_config: GasConfig::default(),
            expiration_secs: 3600, // 1 hour default
        }
    }

    /// Set gas configuration
    pub fn with_gas(mut self, max_gas_amount: u64, gas_unit_price: u64) -> Self {
        self.gas_config = GasConfig {
            max_gas_amount,
            gas_unit_price,
        };
        self
    }

    /// Set expiration time in seconds from now
    pub fn with_expiration(mut self, expiration_secs: u64) -> Self {
        self.expiration_secs = expiration_secs;
        self
    }

    /// Build a script function call transaction
    pub fn build_script_function(
        &self,
        module_address: StarcoinAddress,
        module_name: &str,
        function_name: &str,
        type_args: Vec<TypeTag>,
        args: Vec<Vec<u8>>,
    ) -> Result<RawUserTransaction, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();

        let module_id = ModuleId::new(
            module_address,
            Identifier::new(module_name).map_err(|e| e.to_string())?,
        );

        let script_function = ScriptFunction::new(
            module_id,
            Identifier::new(function_name).map_err(|e| e.to_string())?,
            type_args,
            args,
        );

        Ok(RawUserTransaction::new_script_function(
            self.sender,
            self.sequence_number,
            script_function,
            self.gas_config.max_gas_amount,
            self.gas_config.gas_unit_price,
            now + self.expiration_secs,
            self.chain_id,
        ))
    }

    /// Build a bridge module call (convenience method)
    pub fn build_bridge_call(
        &self,
        module_name: &str,
        function_name: &str,
        type_args: Vec<TypeTag>,
        args: Vec<Vec<u8>>,
    ) -> Result<RawUserTransaction, String> {
        self.build_script_function(
            bridge_module_address(),
            module_name,
            function_name,
            type_args,
            args,
        )
    }
}

// =============================================================================
// Bridge-specific transaction builders
// =============================================================================

/// Build transaction for approving a token transfer
pub fn build_approve_token_transfer(
    sender: StarcoinAddress,
    sequence_number: u64,
    chain_id: u8,
    // Message parameters
    source_chain: u8,
    seq_num: u64,
    sender_bytes: Vec<u8>,
    target_chain: u8,
    target_bytes: Vec<u8>,
    token_type: u8,
    amount: u128,
    // Signatures
    signatures: Vec<Vec<u8>>,
) -> Result<RawUserTransaction, String> {
    let builder = StarcoinTransactionBuilder::new(sender, sequence_number, chain_id);

    // Serialize arguments using BCS
    let args = vec![
        bcs::to_bytes(&source_chain).map_err(|e| e.to_string())?,
        bcs::to_bytes(&seq_num).map_err(|e| e.to_string())?,
        bcs::to_bytes(&sender_bytes).map_err(|e| e.to_string())?,
        bcs::to_bytes(&target_chain).map_err(|e| e.to_string())?,
        bcs::to_bytes(&target_bytes).map_err(|e| e.to_string())?,
        bcs::to_bytes(&token_type).map_err(|e| e.to_string())?,
        bcs::to_bytes(&amount).map_err(|e| e.to_string())?,
        bcs::to_bytes(&signatures).map_err(|e| e.to_string())?,
    ];

    builder.build_bridge_call("Bridge", "approve_token_transfer", vec![], args)
}

/// Build transaction for claiming and transferring tokens
pub fn build_claim_and_transfer_token(
    sender: StarcoinAddress,
    sequence_number: u64,
    chain_id: u8,
    source_chain: u8,
    seq_num: u64,
    token_type_tag: TypeTag,
) -> Result<RawUserTransaction, String> {
    let builder = StarcoinTransactionBuilder::new(sender, sequence_number, chain_id);

    let args = vec![
        bcs::to_bytes(&source_chain).map_err(|e| e.to_string())?,
        bcs::to_bytes(&seq_num).map_err(|e| e.to_string())?,
    ];

    builder.build_bridge_call(
        "Bridge",
        "claim_and_transfer_token",
        vec![token_type_tag],
        args,
    )
}

/// Build transaction for executing a system message (emergency, blocklist, etc.)
pub fn build_execute_system_message(
    sender: StarcoinAddress,
    sequence_number: u64,
    chain_id: u8,
    message_bytes: Vec<u8>,
    signatures: Vec<Vec<u8>>,
) -> Result<RawUserTransaction, String> {
    let builder = StarcoinTransactionBuilder::new(sender, sequence_number, chain_id);

    let args = vec![
        bcs::to_bytes(&message_bytes).map_err(|e| e.to_string())?,
        bcs::to_bytes(&signatures).map_err(|e| e.to_string())?,
    ];

    builder.build_bridge_call("Bridge", "execute_system_message", vec![], args)
}

/// Build transaction for sending tokens to another chain
/// This calls the specific send_bridge_* entry functions based on token type
pub fn build_send_token(
    sender: StarcoinAddress,
    sequence_number: u64,
    chain_id: u8,
    target_chain: u8,
    target_address: Vec<u8>,
    amount: u128,
    token_type_tag: TypeTag,
) -> Result<RawUserTransaction, String> {
    let builder = StarcoinTransactionBuilder::new(sender, sequence_number, chain_id);

    let args = vec![
        bcs::to_bytes(&target_chain).map_err(|e| e.to_string())?,
        bcs::to_bytes(&target_address).map_err(|e| e.to_string())?,
        bcs::to_bytes(&amount).map_err(|e| e.to_string())?,
    ];

    // Determine which entry function to call based on token type
    let function_name = match &token_type_tag {
        TypeTag::Struct(s) => {
            match s.name.as_str() {
                "ETH" => "send_bridge_eth",
                "BTC" => "send_bridge_btc",
                "USDC" => "send_bridge_usdc",
                "USDT" => "send_bridge_usdt",
                _ => return Err(format!("Unsupported token type: {}", s.name)),
            }
        }
        _ => return Err("Token type must be a struct".to_string()),
    };

    // The send_bridge_* functions don't take type parameters
    builder.build_bridge_call("Bridge", function_name, vec![], args)
}

// =============================================================================
// Signing utilities
// =============================================================================

/// Sign a raw transaction with Ed25519 using raw bytes
pub fn sign_transaction_ed25519(
    raw_txn: RawUserTransaction,
    private_key: &[u8],
    public_key: &[u8],
) -> Result<SignedUserTransaction, String> {
    use ed25519_dalek::{Signer, SigningKey};

    if private_key.len() != 32 {
        return Err("Invalid private key length".to_string());
    }

    let signing_key = SigningKey::from_bytes(
        private_key.try_into().map_err(|_| "Invalid private key")?,
    );

    let message = raw_txn.to_bytes();
    let signature = signing_key.sign(&message);

    Ok(SignedUserTransaction::new(
        raw_txn,
        TransactionAuthenticator::Ed25519 {
            public_key: public_key.to_vec(),
            signature: signature.to_bytes().to_vec(),
        },
    ))
}

/// Sign a raw transaction using StarcoinKeyPair
pub fn sign_transaction(
    raw_txn: RawUserTransaction,
    keypair: &crate::crypto::StarcoinKeyPair,
) -> Result<SignedUserTransaction, String> {
    let message = raw_txn.to_bytes();
    let (public_key, signature) = keypair.sign_message(&message);

    Ok(SignedUserTransaction::new(
        raw_txn,
        TransactionAuthenticator::Ed25519 {
            public_key,
            signature,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_send_token() {
        let sender = StarcoinAddress::ZERO;
        let result = build_send_token(
            sender,
            0,
            1,
            2,
            vec![0u8; 20], // ETH address
            1000000000,
            TypeTag::Bool, // placeholder
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_transaction_builder() {
        let sender = StarcoinAddress::ZERO;
        let builder = StarcoinTransactionBuilder::new(sender, 0, 1)
            .with_gas(5_000_000, 2)
            .with_expiration(7200);

        let result = builder.build_bridge_call(
            "Bridge",
            "test_function",
            vec![],
            vec![],
        );
        assert!(result.is_ok());
    }
}
