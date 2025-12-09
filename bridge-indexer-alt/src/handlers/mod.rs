// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use starcoin_bridge_indexer_alt_framework::types::full_checkpoint_content::CheckpointTransaction;

pub mod error_handler;
pub mod governance_action_handler;
pub mod token_transfer_data_handler;
pub mod token_transfer_handler;

const LIMITER: &IdentStr = ident_str!("limiter");
const BRIDGE: &IdentStr = ident_str!("Bridge");
const COMMITTEE: &IdentStr = ident_str!("Committee");
const TREASURY: &IdentStr = ident_str!("Treasury");

const TOKEN_DEPOSITED_EVENT: &IdentStr = ident_str!("TokenDepositedEvent");
const TOKEN_TRANSFER_APPROVED: &IdentStr = ident_str!("TokenTransferApproved");
const TOKEN_TRANSFER_CLAIMED: &IdentStr = ident_str!("TokenTransferClaimed");

#[macro_export]
macro_rules! struct_tag {
    ($address:ident, $module:ident, $name:ident) => {{
        StructTag {
            address: $address,
            module: $module.into(),
            name: $name.into(),
            type_params: vec![],
        }
    }};
}

/// Check if a transaction is a bridge transaction.
/// For Starcoin RPC mode, we only query bridge-related events, so if a transaction
/// has any events, it's considered a bridge transaction.
/// For checkpoint/remote mode, we check input_objects as before.
pub fn is_bridge_txn(txn: &CheckpointTransaction) -> bool {
    // If transaction has events, it's from our bridge event query
    if txn.events.as_ref().map_or(false, |e| !e.data.is_empty()) {
        return true;
    }
    
    // Fallback: check input_objects (for remote/checkpoint mode)
    // Note: STARCOIN_BRIDGE_OBJECT_ID is all zeros in Starcoin, so this
    // won't match anything meaningful, but kept for compatibility
    false
}
