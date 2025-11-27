// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transaction builder for Starcoin Bridge.
//!
//! This module provides two sets of transaction building functions:
//!
//! 1. **Starcoin Native (recommended)**: Uses `RawUserTransaction` + `ScriptFunction`
//!    - Functions prefixed with `starcoin_` or in the `starcoin_native` module
//!    - Direct mapping to Starcoin's transaction model
//!
//! 2. **Legacy Compatibility**: Uses `ProgrammableTransactionBuilder` pattern
//!    - Kept for backward compatibility with existing code
//!    - Will be deprecated in future versions

use fastcrypto::traits::ToFromBytes;
use move_core_types::ident_str;
use move_core_types::language_storage::ModuleId;
use starcoin_bridge_types::bridge::{
    BRIDGE_CREATE_ADD_TOKEN_ON_STARCOIN_MESSAGE_FUNCTION_NAME,
    BRIDGE_EXECUTE_SYSTEM_MESSAGE_FUNCTION_NAME, BRIDGE_MESSAGE_MODULE_NAME, BRIDGE_MODULE_NAME,
};
use starcoin_bridge_types::transaction::CallArg;
use starcoin_bridge_types::{
    base_types::{ObjectRef, StarcoinAddress},
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{ChainId, ObjectArg, RawUserTransaction, ScriptFunction, TransactionData},
    TypeTag,
};
use starcoin_bridge_types::{Identifier, BRIDGE_PACKAGE_ID};
use std::{collections::HashMap, str::FromStr};

use crate::{
    error::{BridgeError, BridgeResult},
    types::{BridgeAction, VerifiedCertifiedBridgeAction},
};

// =============================================================================
// Starcoin Native Transaction Builders
// =============================================================================

/// Bridge module address as StarcoinAddress (16 bytes)
pub fn bridge_module_address() -> StarcoinAddress {
    StarcoinAddress::new([
        0x24, 0x6b, 0x23, 0x7c, 0x16, 0xc7, 0x61, 0xe9,
        0x47, 0x87, 0x83, 0xdd, 0x83, 0xf7, 0x00, 0x4a,
    ])
}

/// Create token bridge message bytes for Starcoin approve_token_transfer
/// This creates the BCS-serialized message that the Move contract expects
pub fn create_token_bridge_message_bytes(
    source_chain: u8,
    seq_num: u64,
    sender: Vec<u8>,
    target_chain: u8,
    target: Vec<u8>,
    token_type: u8,
    amount: u64,
) -> Vec<u8> {
    // The message format expected by Move:
    // struct TokenTransferMessage {
    //     message_version: u8,  // always 1
    //     seq_num: u64,
    //     source_chain: u8,
    //     sender: vector<u8>,
    //     target_chain: u8,
    //     target: vector<u8>,
    //     token_type: u8,
    //     amount: u64,
    // }
    let mut msg = Vec::new();
    msg.push(1u8); // message_version
    msg.extend_from_slice(&seq_num.to_le_bytes());
    msg.push(source_chain);
    // sender as length-prefixed bytes
    msg.push(sender.len() as u8);
    msg.extend_from_slice(&sender);
    msg.push(target_chain);
    // target as length-prefixed bytes
    msg.push(target.len() as u8);
    msg.extend_from_slice(&target);
    msg.push(token_type);
    msg.extend_from_slice(&amount.to_le_bytes());
    msg
}

/// Transaction builder for Starcoin bridge operations
pub struct StarcoinBridgeTransactionBuilder;

impl StarcoinBridgeTransactionBuilder {
    /// Build a claim token transaction using native Starcoin transaction format
    pub fn build_claim_token(
        sender: StarcoinAddress,
        sequence_number: u64,
        chain_id: u8,
        message_bytes: Vec<u8>,
        signatures: Vec<Vec<u8>>,
    ) -> BridgeResult<starcoin_bridge_types::transaction::RawUserTransaction> {
        starcoin_native::build_approve_token_transfer(
            sender,
            sequence_number,
            chain_id,
            message_bytes,
            signatures,
        )
    }
}

/// Build a Starcoin native transaction for bridge operations
pub mod starcoin_native {
    use super::*;

    /// Build a RawUserTransaction for approving token transfer
    pub fn build_approve_token_transfer(
        sender: StarcoinAddress,
        sequence_number: u64,
        chain_id: u8,
        // Message parameters (serialized as BCS)
        message_bytes: Vec<u8>,
        signatures: Vec<Vec<u8>>,
    ) -> BridgeResult<RawUserTransaction> {
        let module_id = ModuleId::new(
            bridge_module_address(),
            Identifier::new("bridge").map_err(|e| BridgeError::Generic(e.to_string()))?,
        );

        let script_function = ScriptFunction::new(
            module_id,
            Identifier::new("approve_token_transfer").map_err(|e| BridgeError::Generic(e.to_string()))?,
            vec![],
            vec![
                bcs::to_bytes(&message_bytes).map_err(|e| BridgeError::BridgeSerializationError(e.to_string()))?,
                bcs::to_bytes(&signatures).map_err(|e| BridgeError::BridgeSerializationError(e.to_string()))?,
            ],
        );

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| BridgeError::Generic(e.to_string()))?
            .as_secs();

        Ok(RawUserTransaction::new_script_function(
            sender,
            sequence_number,
            script_function,
            10_000_000,  // max_gas_amount
            1,           // gas_unit_price
            now + 3600,  // expiration (1 hour)
            ChainId::new(chain_id),
        ))
    }

    /// Build a RawUserTransaction for claiming and transferring tokens
    pub fn build_claim_and_transfer(
        sender: StarcoinAddress,
        sequence_number: u64,
        chain_id: u8,
        source_chain: u8,
        seq_num: u64,
        token_type: TypeTag,
    ) -> BridgeResult<RawUserTransaction> {
        let module_id = ModuleId::new(
            bridge_module_address(),
            Identifier::new("bridge").map_err(|e| BridgeError::Generic(e.to_string()))?,
        );

        let script_function = ScriptFunction::new(
            module_id,
            Identifier::new("claim_and_transfer_token").map_err(|e| BridgeError::Generic(e.to_string()))?,
            vec![token_type],
            vec![
                bcs::to_bytes(&source_chain).map_err(|e| BridgeError::BridgeSerializationError(e.to_string()))?,
                bcs::to_bytes(&seq_num).map_err(|e| BridgeError::BridgeSerializationError(e.to_string()))?,
            ],
        );

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| BridgeError::Generic(e.to_string()))?
            .as_secs();

        Ok(RawUserTransaction::new_script_function(
            sender,
            sequence_number,
            script_function,
            10_000_000,
            1,
            now + 3600,
            ChainId::new(chain_id),
        ))
    }

    /// Build a RawUserTransaction for executing system messages (emergency, blocklist, etc.)
    pub fn build_execute_system_message(
        sender: StarcoinAddress,
        sequence_number: u64,
        chain_id: u8,
        message_bytes: Vec<u8>,
        signatures: Vec<Vec<u8>>,
    ) -> BridgeResult<RawUserTransaction> {
        let module_id = ModuleId::new(
            bridge_module_address(),
            Identifier::new("bridge").map_err(|e| BridgeError::Generic(e.to_string()))?,
        );

        let script_function = ScriptFunction::new(
            module_id,
            Identifier::new("execute_system_message").map_err(|e| BridgeError::Generic(e.to_string()))?,
            vec![],
            vec![
                bcs::to_bytes(&message_bytes).map_err(|e| BridgeError::BridgeSerializationError(e.to_string()))?,
                bcs::to_bytes(&signatures).map_err(|e| BridgeError::BridgeSerializationError(e.to_string()))?,
            ],
        );

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| BridgeError::Generic(e.to_string()))?
            .as_secs();

        Ok(RawUserTransaction::new_script_function(
            sender,
            sequence_number,
            script_function,
            10_000_000,
            1,
            now + 3600,
            ChainId::new(chain_id),
        ))
    }

    /// Build a RawUserTransaction for sending tokens to another chain
    pub fn build_send_token(
        sender: StarcoinAddress,
        sequence_number: u64,
        chain_id: u8,
        target_chain: u8,
        target_address: Vec<u8>,
        amount: u128,
        token_type: TypeTag,
    ) -> BridgeResult<RawUserTransaction> {
        let module_id = ModuleId::new(
            bridge_module_address(),
            Identifier::new("bridge").map_err(|e| BridgeError::Generic(e.to_string()))?,
        );

        let script_function = ScriptFunction::new(
            module_id,
            Identifier::new("send_token").map_err(|e| BridgeError::Generic(e.to_string()))?,
            vec![token_type],
            vec![
                bcs::to_bytes(&target_chain).map_err(|e| BridgeError::BridgeSerializationError(e.to_string()))?,
                bcs::to_bytes(&target_address).map_err(|e| BridgeError::BridgeSerializationError(e.to_string()))?,
                bcs::to_bytes(&amount).map_err(|e| BridgeError::BridgeSerializationError(e.to_string()))?,
            ],
        );

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| BridgeError::Generic(e.to_string()))?
            .as_secs();

        Ok(RawUserTransaction::new_script_function(
            sender,
            sequence_number,
            script_function,
            10_000_000,
            1,
            now + 3600,
            ChainId::new(chain_id),
        ))
    }

    /// Build a RawUserTransaction for committee registration
    pub fn build_committee_registration(
        sender: StarcoinAddress,
        sequence_number: u64,
        chain_id: u8,
        bridge_pubkey_bytes: Vec<u8>,
        url: Vec<u8>,
    ) -> BridgeResult<RawUserTransaction> {
        let module_id = ModuleId::new(
            bridge_module_address(),
            Identifier::new("bridge").map_err(|e| BridgeError::Generic(e.to_string()))?,
        );

        let script_function = ScriptFunction::new(
            module_id,
            Identifier::new("committee_registration").map_err(|e| BridgeError::Generic(e.to_string()))?,
            vec![],
            vec![
                bcs::to_bytes(&bridge_pubkey_bytes).map_err(|e| BridgeError::BridgeSerializationError(e.to_string()))?,
                bcs::to_bytes(&url).map_err(|e| BridgeError::BridgeSerializationError(e.to_string()))?,
            ],
        );

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| BridgeError::Generic(e.to_string()))?
            .as_secs();

        Ok(RawUserTransaction::new_script_function(
            sender,
            sequence_number,
            script_function,
            10_000_000,
            1,
            now + 3600,
            ChainId::new(chain_id),
        ))
    }
}

// =============================================================================
// Legacy Compatibility Layer (Sui-style ProgrammableTransactionBuilder)
// =============================================================================

pub fn build_starcoin_bridge_transaction(
    client_address: StarcoinAddress,
    gas_object_ref: &ObjectRef,
    action: VerifiedCertifiedBridgeAction,
    bridge_object_arg: ObjectArg,
    starcoin_bridge_token_type_tags: &HashMap<u8, TypeTag>,
    rgp: u64,
) -> BridgeResult<TransactionData> {
    // TODO: Check chain id?
    match action.data() {
        BridgeAction::EthToStarcoinBridgeAction(_) => build_token_bridge_approve_transaction(
            client_address,
            gas_object_ref,
            action,
            true,
            bridge_object_arg,
            starcoin_bridge_token_type_tags,
            rgp,
        ),
        BridgeAction::StarcoinToEthBridgeAction(_) => build_token_bridge_approve_transaction(
            client_address,
            gas_object_ref,
            action,
            false,
            bridge_object_arg,
            starcoin_bridge_token_type_tags,
            rgp,
        ),
        BridgeAction::BlocklistCommitteeAction(_) => build_committee_blocklist_approve_transaction(
            client_address,
            gas_object_ref,
            action,
            bridge_object_arg,
            rgp,
        ),
        BridgeAction::EmergencyAction(_) => build_emergency_op_approve_transaction(
            client_address,
            gas_object_ref,
            action,
            bridge_object_arg,
            rgp,
        ),
        BridgeAction::LimitUpdateAction(_) => build_limit_update_approve_transaction(
            client_address,
            gas_object_ref,
            action,
            bridge_object_arg,
            rgp,
        ),
        BridgeAction::AssetPriceUpdateAction(_) => build_asset_price_update_approve_transaction(
            client_address,
            gas_object_ref,
            action,
            bridge_object_arg,
            rgp,
        ),
        BridgeAction::EvmContractUpgradeAction(_) => {
            // It does not need a Starcoin tranaction to execute EVM contract upgrade
            unreachable!()
        }
        BridgeAction::AddTokensOnStarcoinAction(_) => {
            build_add_tokens_on_starcoin_bridge_transaction(
                client_address,
                gas_object_ref,
                action,
                bridge_object_arg,
                rgp,
            )
        }
        BridgeAction::AddTokensOnEvmAction(_) => {
            // It does not need a Starcoin tranaction to add tokens on EVM
            unreachable!()
        }
    }
}

fn build_token_bridge_approve_transaction(
    client_address: StarcoinAddress,
    gas_object_ref: &ObjectRef,
    action: VerifiedCertifiedBridgeAction,
    claim: bool,
    bridge_object_arg: ObjectArg,
    starcoin_bridge_token_type_tags: &HashMap<u8, TypeTag>,
    rgp: u64,
) -> BridgeResult<TransactionData> {
    let (bridge_action, sigs) = action.into_inner().into_data_and_sig();
    let mut builder = ProgrammableTransactionBuilder::new();

    let (source_chain, seq_num, sender, target_chain, target, token_type, amount) =
        match bridge_action {
            BridgeAction::StarcoinToEthBridgeAction(a) => {
                let bridge_event = a.starcoin_bridge_event;
                (
                    bridge_event.starcoin_bridge_chain_id,
                    bridge_event.nonce,
                    bridge_event.starcoin_bridge_address.to_vec(),
                    bridge_event.eth_chain_id,
                    bridge_event.eth_address.to_fixed_bytes().to_vec(),
                    bridge_event.token_id,
                    bridge_event.amount_starcoin_bridge_adjusted,
                )
            }
            BridgeAction::EthToStarcoinBridgeAction(a) => {
                let bridge_event = a.eth_bridge_event;
                (
                    bridge_event.eth_chain_id,
                    bridge_event.nonce,
                    bridge_event.eth_address.to_fixed_bytes().to_vec(),
                    bridge_event.starcoin_bridge_chain_id,
                    bridge_event.starcoin_bridge_address.to_vec(),
                    bridge_event.token_id,
                    bridge_event.starcoin_bridge_adjusted_amount,
                )
            }
            _ => unreachable!(),
        };

    let source_chain = builder.pure(source_chain as u8).unwrap();
    let seq_num = builder.pure(seq_num).unwrap();
    let sender = builder.pure(sender.clone()).map_err(|e| {
        BridgeError::BridgeSerializationError(format!(
            "Failed to serialize sender: {:?}. Err: {:?}",
            sender, e
        ))
    })?;
    let target_chain = builder.pure(target_chain as u8).unwrap();
    let target = builder.pure(target.clone()).map_err(|e| {
        BridgeError::BridgeSerializationError(format!(
            "Failed to serialize target: {:?}. Err: {:?}",
            target, e
        ))
    })?;
    let arg_token_type = builder.pure(token_type).unwrap();
    let amount = builder.pure(amount).unwrap();

    let arg_msg = builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("message").to_owned(),
        ident_str!("create_token_bridge_message").to_owned(),
        vec![],
        vec![
            source_chain,
            seq_num,
            sender,
            target_chain,
            target,
            arg_token_type,
            amount,
        ],
    );

    // Unwrap: these should not fail
    let arg_bridge = builder.obj(bridge_object_arg).unwrap();
    let arg_clock = builder.input(CallArg::CLOCK_IMM).unwrap();

    let mut sig_bytes = vec![];
    for (_, sig) in sigs.signatures {
        sig_bytes.push(sig.as_bytes().to_vec());
    }
    let arg_signatures = builder.pure(sig_bytes.clone()).map_err(|e| {
        BridgeError::BridgeSerializationError(format!(
            "Failed to serialize signatures: {:?}. Err: {:?}",
            sig_bytes, e
        ))
    })?;

    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        starcoin_bridge_types::bridge::BRIDGE_MODULE_NAME.to_owned(),
        ident_str!("approve_token_transfer").to_owned(),
        vec![],
        vec![arg_bridge, arg_msg, arg_signatures],
    );

    if claim {
        builder.programmable_move_call(
            BRIDGE_PACKAGE_ID,
            starcoin_bridge_types::bridge::BRIDGE_MODULE_NAME.to_owned(),
            ident_str!("claim_and_transfer_token").to_owned(),
            vec![starcoin_bridge_token_type_tags
                .get(&token_type)
                .ok_or(BridgeError::UnknownTokenId(token_type))?
                .clone()],
            vec![arg_bridge, arg_clock, source_chain, seq_num],
        );
    }

    let pt = builder.finish();

    Ok(TransactionData::new_programmable(
        client_address,
        vec![*gas_object_ref],
        pt,
        100_000_000,
        rgp,
    ))
}

fn build_emergency_op_approve_transaction(
    client_address: StarcoinAddress,
    gas_object_ref: &ObjectRef,
    action: VerifiedCertifiedBridgeAction,
    bridge_object_arg: ObjectArg,
    rgp: u64,
) -> BridgeResult<TransactionData> {
    let (bridge_action, sigs) = action.into_inner().into_data_and_sig();

    let mut builder = ProgrammableTransactionBuilder::new();

    let (source_chain, seq_num, action_type) = match bridge_action {
        BridgeAction::EmergencyAction(a) => (a.chain_id, a.nonce, a.action_type),
        _ => unreachable!(),
    };

    // Unwrap: these should not fail
    let source_chain = builder.pure(source_chain as u8).unwrap();
    let seq_num = builder.pure(seq_num).unwrap();
    let action_type = builder.pure(action_type as u8).unwrap();
    let arg_bridge = builder.obj(bridge_object_arg).unwrap();

    let arg_msg = builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("message").to_owned(),
        ident_str!("create_emergency_op_message").to_owned(),
        vec![],
        vec![source_chain, seq_num, action_type],
    );

    let mut sig_bytes = vec![];
    for (_, sig) in sigs.signatures {
        sig_bytes.push(sig.as_bytes().to_vec());
    }
    let arg_signatures = builder.pure(sig_bytes.clone()).map_err(|e| {
        BridgeError::BridgeSerializationError(format!(
            "Failed to serialize signatures: {:?}. Err: {:?}",
            sig_bytes, e
        ))
    })?;

    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("bridge").to_owned(),
        ident_str!("execute_system_message").to_owned(),
        vec![],
        vec![arg_bridge, arg_msg, arg_signatures],
    );

    let pt = builder.finish();

    Ok(TransactionData::new_programmable(
        client_address,
        vec![*gas_object_ref],
        pt,
        100_000_000,
        rgp,
    ))
}

fn build_committee_blocklist_approve_transaction(
    client_address: StarcoinAddress,
    gas_object_ref: &ObjectRef,
    action: VerifiedCertifiedBridgeAction,
    bridge_object_arg: ObjectArg,
    rgp: u64,
) -> BridgeResult<TransactionData> {
    let (bridge_action, sigs) = action.into_inner().into_data_and_sig();

    let mut builder = ProgrammableTransactionBuilder::new();

    let (source_chain, seq_num, blocklist_type, members_to_update) = match bridge_action {
        BridgeAction::BlocklistCommitteeAction(a) => {
            (a.chain_id, a.nonce, a.blocklist_type, a.members_to_update)
        }
        _ => unreachable!(),
    };

    // Unwrap: these should not fail
    let source_chain = builder.pure(source_chain as u8).unwrap();
    let seq_num = builder.pure(seq_num).unwrap();
    let blocklist_type = builder.pure(blocklist_type as u8).unwrap();
    let members_to_update = members_to_update
        .into_iter()
        .map(|m| m.to_eth_address().as_bytes().to_vec())
        .collect::<Vec<_>>();
    let members_to_update = builder.pure(members_to_update).unwrap();
    let arg_bridge = builder.obj(bridge_object_arg).unwrap();

    let arg_msg = builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("message").to_owned(),
        ident_str!("create_blocklist_message").to_owned(),
        vec![],
        vec![source_chain, seq_num, blocklist_type, members_to_update],
    );

    let mut sig_bytes = vec![];
    for (_, sig) in sigs.signatures {
        sig_bytes.push(sig.as_bytes().to_vec());
    }
    let arg_signatures = builder.pure(sig_bytes.clone()).map_err(|e| {
        BridgeError::BridgeSerializationError(format!(
            "Failed to serialize signatures: {:?}. Err: {:?}",
            sig_bytes, e
        ))
    })?;

    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("bridge").to_owned(),
        ident_str!("execute_system_message").to_owned(),
        vec![],
        vec![arg_bridge, arg_msg, arg_signatures],
    );

    let pt = builder.finish();

    Ok(TransactionData::new_programmable(
        client_address,
        vec![*gas_object_ref],
        pt,
        100_000_000,
        rgp,
    ))
}

fn build_limit_update_approve_transaction(
    client_address: StarcoinAddress,
    gas_object_ref: &ObjectRef,
    action: VerifiedCertifiedBridgeAction,
    bridge_object_arg: ObjectArg,
    rgp: u64,
) -> BridgeResult<TransactionData> {
    let (bridge_action, sigs) = action.into_inner().into_data_and_sig();

    let mut builder = ProgrammableTransactionBuilder::new();

    let (receiving_chain_id, seq_num, sending_chain_id, new_usd_limit) = match bridge_action {
        BridgeAction::LimitUpdateAction(a) => {
            (a.chain_id, a.nonce, a.sending_chain_id, a.new_usd_limit)
        }
        _ => unreachable!(),
    };

    // Unwrap: these should not fail
    let receiving_chain_id = builder.pure(receiving_chain_id as u8).unwrap();
    let seq_num = builder.pure(seq_num).unwrap();
    let sending_chain_id = builder.pure(sending_chain_id as u8).unwrap();
    let new_usd_limit = builder.pure(new_usd_limit).unwrap();
    let arg_bridge = builder.obj(bridge_object_arg).unwrap();

    let arg_msg = builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("message").to_owned(),
        ident_str!("create_update_bridge_limit_message").to_owned(),
        vec![],
        vec![receiving_chain_id, seq_num, sending_chain_id, new_usd_limit],
    );

    let mut sig_bytes = vec![];
    for (_, sig) in sigs.signatures {
        sig_bytes.push(sig.as_bytes().to_vec());
    }
    let arg_signatures = builder.pure(sig_bytes.clone()).map_err(|e| {
        BridgeError::BridgeSerializationError(format!(
            "Failed to serialize signatures: {:?}. Err: {:?}",
            sig_bytes, e
        ))
    })?;

    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("bridge").to_owned(),
        ident_str!("execute_system_message").to_owned(),
        vec![],
        vec![arg_bridge, arg_msg, arg_signatures],
    );

    let pt = builder.finish();

    Ok(TransactionData::new_programmable(
        client_address,
        vec![*gas_object_ref],
        pt,
        100_000_000,
        rgp,
    ))
}

fn build_asset_price_update_approve_transaction(
    client_address: StarcoinAddress,
    gas_object_ref: &ObjectRef,
    action: VerifiedCertifiedBridgeAction,
    bridge_object_arg: ObjectArg,
    rgp: u64,
) -> BridgeResult<TransactionData> {
    let (bridge_action, sigs) = action.into_inner().into_data_and_sig();

    let mut builder = ProgrammableTransactionBuilder::new();

    let (source_chain, seq_num, token_id, new_usd_price) = match bridge_action {
        BridgeAction::AssetPriceUpdateAction(a) => {
            (a.chain_id, a.nonce, a.token_id, a.new_usd_price)
        }
        _ => unreachable!(),
    };

    // Unwrap: these should not fail
    let source_chain = builder.pure(source_chain as u8).unwrap();
    let token_id = builder.pure(token_id).unwrap();
    let seq_num = builder.pure(seq_num).unwrap();
    let new_price = builder.pure(new_usd_price).unwrap();
    let arg_bridge = builder.obj(bridge_object_arg).unwrap();

    let arg_msg = builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("message").to_owned(),
        ident_str!("create_update_asset_price_message").to_owned(),
        vec![],
        vec![token_id, source_chain, seq_num, new_price],
    );

    let mut sig_bytes = vec![];
    for (_, sig) in sigs.signatures {
        sig_bytes.push(sig.as_bytes().to_vec());
    }
    let arg_signatures = builder.pure(sig_bytes.clone()).map_err(|e| {
        BridgeError::BridgeSerializationError(format!(
            "Failed to serialize signatures: {:?}. Err: {:?}",
            sig_bytes, e
        ))
    })?;

    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        ident_str!("bridge").to_owned(),
        ident_str!("execute_system_message").to_owned(),
        vec![],
        vec![arg_bridge, arg_msg, arg_signatures],
    );

    let pt = builder.finish();

    Ok(TransactionData::new_programmable(
        client_address,
        vec![*gas_object_ref],
        pt,
        100_000_000,
        rgp,
    ))
}

pub fn build_add_tokens_on_starcoin_bridge_transaction(
    client_address: StarcoinAddress,
    gas_object_ref: &ObjectRef,
    action: VerifiedCertifiedBridgeAction,
    bridge_object_arg: ObjectArg,
    rgp: u64,
) -> BridgeResult<TransactionData> {
    let (bridge_action, sigs) = action.into_inner().into_data_and_sig();

    let mut builder = ProgrammableTransactionBuilder::new();

    let (source_chain, seq_num, native, token_ids, token_type_names, token_prices) =
        match bridge_action {
            BridgeAction::AddTokensOnStarcoinAction(a) => (
                a.chain_id,
                a.nonce,
                a.native,
                a.token_ids,
                a.token_type_names,
                a.token_prices,
            ),
            _ => unreachable!(),
        };
    let token_type_names = token_type_names
        .iter()
        .map(|type_name| type_name.to_canonical_string())
        .collect::<Vec<_>>();
    let source_chain = builder.pure(source_chain as u8).unwrap();
    let seq_num = builder.pure(seq_num).unwrap();
    let native_token = builder.pure(native).unwrap();
    let token_ids = builder.pure(token_ids).unwrap();
    let token_type_names = builder.pure(token_type_names).unwrap();
    let token_prices = builder.pure(token_prices).unwrap();

    let message_arg = builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        BRIDGE_MESSAGE_MODULE_NAME.into(),
        BRIDGE_CREATE_ADD_TOKEN_ON_STARCOIN_MESSAGE_FUNCTION_NAME.into(),
        vec![],
        vec![
            source_chain,
            seq_num,
            native_token,
            token_ids,
            token_type_names,
            token_prices,
        ],
    );

    let bridge_arg = builder.obj(bridge_object_arg).unwrap();

    let mut sig_bytes = vec![];
    for (_, sig) in sigs.signatures {
        sig_bytes.push(sig.as_bytes().to_vec());
    }
    let sigs_arg = builder.pure(sig_bytes.clone()).unwrap();

    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        BRIDGE_MODULE_NAME.into(),
        BRIDGE_EXECUTE_SYSTEM_MESSAGE_FUNCTION_NAME.into(),
        vec![],
        vec![bridge_arg, message_arg, sigs_arg],
    );

    let pt = builder.finish();

    Ok(TransactionData::new_programmable(
        client_address,
        vec![*gas_object_ref],
        pt,
        100_000_000,
        rgp,
    ))
}

pub fn build_committee_register_transaction(
    validator_address: StarcoinAddress,
    gas_object_ref: &ObjectRef,
    bridge_object_arg: ObjectArg,
    bridge_authority_pub_key_bytes: Vec<u8>,
    bridge_url: &str,
    ref_gas_price: u64,
    gas_budget: u64,
) -> BridgeResult<TransactionData> {
    let mut builder = ProgrammableTransactionBuilder::new();
    let system_state = builder.obj(ObjectArg::STARCOIN_SYSTEM_MUT).unwrap();
    let bridge = builder.obj(bridge_object_arg).unwrap();
    let bridge_pubkey = builder
        .input(CallArg::Pure(
            bcs::to_bytes(&bridge_authority_pub_key_bytes).unwrap(),
        ))
        .unwrap();
    let url = builder
        .input(CallArg::Pure(bcs::to_bytes(bridge_url.as_bytes()).unwrap()))
        .unwrap();
    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        BRIDGE_MODULE_NAME.into(),
        Identifier::from_str("committee_registration").unwrap(),
        vec![],
        vec![bridge, system_state, bridge_pubkey, url],
    );
    let data = TransactionData::new_programmable(
        validator_address,
        vec![*gas_object_ref],
        builder.finish(),
        gas_budget,
        ref_gas_price,
    );
    Ok(data)
}

pub fn build_committee_update_url_transaction(
    validator_address: StarcoinAddress,
    gas_object_ref: &ObjectRef,
    bridge_object_arg: ObjectArg,
    bridge_url: &str,
    ref_gas_price: u64,
    gas_budget: u64,
) -> BridgeResult<TransactionData> {
    let mut builder = ProgrammableTransactionBuilder::new();
    let bridge = builder.obj(bridge_object_arg).unwrap();
    let url = builder
        .input(CallArg::Pure(bcs::to_bytes(bridge_url.as_bytes()).unwrap()))
        .unwrap();
    builder.programmable_move_call(
        BRIDGE_PACKAGE_ID,
        BRIDGE_MODULE_NAME.into(),
        Identifier::from_str("update_node_url").unwrap(),
        vec![],
        vec![bridge, url],
    );
    let data = TransactionData::new_programmable(
        validator_address,
        vec![*gas_object_ref],
        builder.finish(),
        gas_budget,
        ref_gas_price,
    );
    Ok(data)
}

/*#[cfg(test)]
mod tests {
    use crate::crypto::BridgeAuthorityKeyPair;
    // use crate::e2e_tests::test_utils::TestClusterWrapperBuilder;  // e2e_tests is disabled
    use crate::metrics::BridgeMetrics;
    use crate::starcoin_bridge_client::StarcoinClient;
    use crate::types::BridgeAction;
    use crate::types::EmergencyAction;
    use crate::types::EmergencyActionType;
    use crate::types::*;
    use crate::{
        crypto::BridgeAuthorityPublicKeyBytes,
        test_utils::{
            approve_action_with_validator_secrets, bridge_token, get_test_eth_to_starcoin_bridge_action,
            get_test_starcoin_bridge_to_eth_bridge_action,
        },
    };
    use ethers::types::Address as EthAddress;
    use std::collections::HashMap;
    use std::sync::Arc;
    use starcoin_bridge_types::bridge::{BridgeChainId, TOKEN_ID_BTC, TOKEN_ID_USDC};
    use starcoin_bridge_types::crypto::get_key_pair;
    use fastcrypto::traits::ToFromBytes;

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_build_starcoin_bridge_transaction_for_token_transfer() {
        telemetry_subscribers::init_for_testing();
        let mut bridge_keys = vec![];
        for _ in 0..=3 {
            let (_, kp): (_, BridgeAuthorityKeyPair) = get_key_pair();
            bridge_keys.push(kp);
        }
        let mut test_cluster = TestClusterWrapperBuilder::new()
            .with_bridge_authority_keys(bridge_keys)
            .with_deploy_tokens(true)
            .build()
            .await;

        let metrics = Arc::new(BridgeMetrics::new_for_testing());
        let starcoin_bridge_client = StarcoinClient::new(&test_cluster.inner.fullnode_handle.rpc_url, metrics)
            .await
            .unwrap();
        let bridge_authority_keys = test_cluster.authority_keys_clone();

        // Note: We don't call `starcoin_bridge_client.get_bridge_committee` here because it will err if the committee
        // is not initialized during the construction of `BridgeCommittee`.
        test_cluster
            .trigger_reconfiguration_if_not_yet_and_assert_bridge_committee_initialized()
            .await;
        let context = &mut test_cluster.inner.wallet;
        let sender = context.active_address().unwrap();
        let usdc_amount = 5000000;
        let bridge_object_arg = starcoin_bridge_client
            .get_mutable_bridge_object_arg_must_succeed()
            .await;
        let id_token_map = starcoin_bridge_client.get_token_id_map().await.unwrap();

        // 1. Test Eth -> Starcoin Transfer approval
        let action = get_test_eth_to_starcoin_bridge_action(None, Some(usdc_amount), Some(sender), None);
        // `approve_action_with_validator_secrets` covers transaction building
        let usdc_object_ref = approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            Some(sender),
            &id_token_map,
        )
        .await
        .unwrap();

        // 2. Test Starcoin -> Eth Transfer approval
        let bridge_event = bridge_token(
            context,
            EthAddress::random(),
            usdc_object_ref,
            id_token_map.get(&TOKEN_ID_USDC).unwrap().clone(),
            bridge_object_arg,
        )
        .await;

        let action = get_test_starcoin_bridge_to_eth_bridge_action(
            None,
            None,
            Some(bridge_event.nonce),
            Some(bridge_event.amount_starcoin_bridge_adjusted),
            Some(bridge_event.starcoin_bridge_address),
            Some(bridge_event.eth_address),
            Some(TOKEN_ID_USDC),
        );
        // `approve_action_with_validator_secrets` covers transaction building
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_build_starcoin_bridge_transaction_for_emergency_op() {
        telemetry_subscribers::init_for_testing();
        let num_valdiator = 2;
        let mut bridge_keys = vec![];
        for _ in 0..num_valdiator {
            let (_, kp): (_, BridgeAuthorityKeyPair) = get_key_pair();
            bridge_keys.push(kp);
        }
        let mut test_cluster = TestClusterWrapperBuilder::new()
            .with_bridge_authority_keys(bridge_keys)
            .with_deploy_tokens(true)
            .build()
            .await;
        let metrics = Arc::new(BridgeMetrics::new_for_testing());
        let starcoin_bridge_client = StarcoinClient::new(&test_cluster.inner.fullnode_handle.rpc_url, metrics)
            .await
            .unwrap();
        let bridge_authority_keys = test_cluster.authority_keys_clone();

        // Wait until committee is set up
        test_cluster
            .trigger_reconfiguration_if_not_yet_and_assert_bridge_committee_initialized()
            .await;
        let summary = starcoin_bridge_client.get_bridge_summary().await.unwrap();
        assert!(!summary.is_frozen);

        let context = &mut test_cluster.inner.wallet;
        let bridge_object_arg = starcoin_bridge_client
            .get_mutable_bridge_object_arg_must_succeed()
            .await;
        let id_token_map = starcoin_bridge_client.get_token_id_map().await.unwrap();

        // 1. Pause
        let action = BridgeAction::EmergencyAction(EmergencyAction {
            nonce: 0,
            chain_id: BridgeChainId::StarcoinCustom,
            action_type: EmergencyActionType::Pause,
        });
        // `approve_action_with_validator_secrets` covers transaction building
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;
        let summary = starcoin_bridge_client.get_bridge_summary().await.unwrap();
        assert!(summary.is_frozen);

        // 2. Unpause
        let action = BridgeAction::EmergencyAction(EmergencyAction {
            nonce: 1,
            chain_id: BridgeChainId::StarcoinCustom,
            action_type: EmergencyActionType::Unpause,
        });
        // `approve_action_with_validator_secrets` covers transaction building
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;
        let summary = starcoin_bridge_client.get_bridge_summary().await.unwrap();
        assert!(!summary.is_frozen);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_build_starcoin_bridge_transaction_for_committee_blocklist() {
        telemetry_subscribers::init_for_testing();
        let mut bridge_keys = vec![];
        for _ in 0..=3 {
            let (_, kp): (_, BridgeAuthorityKeyPair) = get_key_pair();
            bridge_keys.push(kp);
        }
        let mut test_cluster = TestClusterWrapperBuilder::new()
            .with_bridge_authority_keys(bridge_keys)
            .with_deploy_tokens(true)
            .build()
            .await;
        let metrics = Arc::new(BridgeMetrics::new_for_testing());
        let starcoin_bridge_client = StarcoinClient::new(&test_cluster.inner.fullnode_handle.rpc_url, metrics)
            .await
            .unwrap();
        let bridge_authority_keys = test_cluster.authority_keys_clone();

        // Wait until committee is set up
        test_cluster
            .trigger_reconfiguration_if_not_yet_and_assert_bridge_committee_initialized()
            .await;
        let committee = starcoin_bridge_client.get_bridge_summary().await.unwrap().committee;
        let victim = committee.members.first().unwrap().clone().1;
        for member in committee.members {
            assert!(!member.1.blocklisted);
        }

        let context = &mut test_cluster.inner.wallet;
        let bridge_object_arg = starcoin_bridge_client
            .get_mutable_bridge_object_arg_must_succeed()
            .await;
        let id_token_map = starcoin_bridge_client.get_token_id_map().await.unwrap();

        // 1. blocklist The victim
        let action = BridgeAction::BlocklistCommitteeAction(BlocklistCommitteeAction {
            nonce: 0,
            chain_id: BridgeChainId::StarcoinCustom,
            blocklist_type: BlocklistType::Blocklist,
            members_to_update: vec![BridgeAuthorityPublicKeyBytes::from_bytes(
                &victim.bridge_pubkey_bytes,
            )
            .unwrap()],
        });
        // `approve_action_with_validator_secrets` covers transaction building
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;
        let committee = starcoin_bridge_client.get_bridge_summary().await.unwrap().committee;
        for member in committee.members {
            if member.1.bridge_pubkey_bytes == victim.bridge_pubkey_bytes {
                assert!(member.1.blocklisted);
            } else {
                assert!(!member.1.blocklisted);
            }
        }

        // 2. unblocklist the victim
        let action = BridgeAction::BlocklistCommitteeAction(BlocklistCommitteeAction {
            nonce: 1,
            chain_id: BridgeChainId::StarcoinCustom,
            blocklist_type: BlocklistType::Unblocklist,
            members_to_update: vec![BridgeAuthorityPublicKeyBytes::from_bytes(
                &victim.bridge_pubkey_bytes,
            )
            .unwrap()],
        });
        // `approve_action_with_validator_secrets` covers transaction building
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;
        let committee = starcoin_bridge_client.get_bridge_summary().await.unwrap().committee;
        for member in committee.members {
            assert!(!member.1.blocklisted);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_build_starcoin_bridge_transaction_for_limit_update() {
        telemetry_subscribers::init_for_testing();
        let mut bridge_keys = vec![];
        for _ in 0..=3 {
            let (_, kp): (_, BridgeAuthorityKeyPair) = get_key_pair();
            bridge_keys.push(kp);
        }
        let mut test_cluster = TestClusterWrapperBuilder::new()
            .with_bridge_authority_keys(bridge_keys)
            .with_deploy_tokens(true)
            .build()
            .await;
        let metrics = Arc::new(BridgeMetrics::new_for_testing());
        let starcoin_bridge_client = StarcoinClient::new(&test_cluster.inner.fullnode_handle.rpc_url, metrics)
            .await
            .unwrap();
        let bridge_authority_keys = test_cluster.authority_keys_clone();

        // Wait until committee is set up
        test_cluster
            .trigger_reconfiguration_if_not_yet_and_assert_bridge_committee_initialized()
            .await;
        let transfer_limit = starcoin_bridge_client
            .get_bridge_summary()
            .await
            .unwrap()
            .limiter
            .transfer_limit
            .into_iter()
            .map(|(s, d, l)| ((s, d), l))
            .collect::<HashMap<_, _>>();

        let context = &mut test_cluster.inner.wallet;
        let bridge_object_arg = starcoin_bridge_client
            .get_mutable_bridge_object_arg_must_succeed()
            .await;
        let id_token_map = starcoin_bridge_client.get_token_id_map().await.unwrap();

        // update limit
        let action = BridgeAction::LimitUpdateAction(LimitUpdateAction {
            nonce: 0,
            chain_id: BridgeChainId::StarcoinCustom,
            sending_chain_id: BridgeChainId::EthCustom,
            new_usd_limit: 6_666_666 * USD_MULTIPLIER, // $1M USD
        });
        // `approve_action_with_validator_secrets` covers transaction building
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;
        let new_transfer_limit = starcoin_bridge_client
            .get_bridge_summary()
            .await
            .unwrap()
            .limiter
            .transfer_limit;
        for limit in new_transfer_limit {
            if limit.0 == BridgeChainId::EthCustom && limit.1 == BridgeChainId::StarcoinCustom {
                assert_eq!(limit.2, 6_666_666 * USD_MULTIPLIER);
            } else {
                assert_eq!(limit.2, *transfer_limit.get(&(limit.0, limit.1)).unwrap());
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_build_starcoin_bridge_transaction_for_price_update() {
        telemetry_subscribers::init_for_testing();
        let mut bridge_keys = vec![];
        for _ in 0..=3 {
            let (_, kp): (_, BridgeAuthorityKeyPair) = get_key_pair();
            bridge_keys.push(kp);
        }
        let mut test_cluster = TestClusterWrapperBuilder::new()
            .with_bridge_authority_keys(bridge_keys)
            .with_deploy_tokens(true)
            .build()
            .await;
        let metrics = Arc::new(BridgeMetrics::new_for_testing());
        let starcoin_bridge_client = StarcoinClient::new(&test_cluster.inner.fullnode_handle.rpc_url, metrics)
            .await
            .unwrap();
        let bridge_authority_keys = test_cluster.authority_keys_clone();

        // Note: We don't call `starcoin_bridge_client.get_bridge_committee` here because it will err if the committee
        // is not initialized during the construction of `BridgeCommittee`.
        test_cluster
            .trigger_reconfiguration_if_not_yet_and_assert_bridge_committee_initialized()
            .await;
        let notional_values = starcoin_bridge_client.get_notional_values().await.unwrap();
        assert_ne!(notional_values[&TOKEN_ID_USDC], 69_000 * USD_MULTIPLIER);

        let context = &mut test_cluster.inner.wallet;
        let bridge_object_arg = starcoin_bridge_client
            .get_mutable_bridge_object_arg_must_succeed()
            .await;
        let id_token_map = starcoin_bridge_client.get_token_id_map().await.unwrap();

        // update price
        let action = BridgeAction::AssetPriceUpdateAction(AssetPriceUpdateAction {
            nonce: 0,
            chain_id: BridgeChainId::StarcoinCustom,
            token_id: TOKEN_ID_BTC,
            new_usd_price: 69_000 * USD_MULTIPLIER, // $69k USD
        });
        // `approve_action_with_validator_secrets` covers transaction building
        approve_action_with_validator_secrets(
            context,
            bridge_object_arg,
            action.clone(),
            &bridge_authority_keys,
            None,
            &id_token_map,
        )
        .await;
        let new_notional_values = starcoin_bridge_client.get_notional_values().await.unwrap();
        for (token_id, price) in new_notional_values {
            if token_id == TOKEN_ID_BTC {
                assert_eq!(price, 69_000 * USD_MULTIPLIER);
            } else {
                assert_eq!(price, *notional_values.get(&token_id).unwrap());
            }
        }
    }
}*/
