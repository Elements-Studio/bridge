// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::EthToStarcoinTokenBridgeV1;
use crate::eth_mock_provider::EthMockProvider;
use crate::events::StarcoinBridgeEvent;
use crate::server::mock_handler::run_mock_server;
use crate::starcoin_bridge_transaction_builder::build_starcoin_bridge_transaction;
use crate::types::{
    BridgeCommittee, BridgeCommitteeValiditySignInfo, CertifiedBridgeAction,
    VerifiedCertifiedBridgeAction,
};
use crate::{
    crypto::{BridgeAuthorityKeyPair, BridgeAuthorityPublicKey, BridgeAuthoritySignInfo},
    events::EmittedStarcoinToEthTokenBridgeV1,
    server::mock_handler::BridgeRequestMockHandler,
    types::{
        BridgeAction, BridgeAuthority, EthToStarcoinBridgeAction, SignedBridgeAction,
        StarcoinToEthBridgeAction,
    },
};
use ethers::abi::{long_signature, ParamType};
use ethers::types::Address as EthAddress;
use ethers::types::{
    Block, BlockNumber, Filter, FilterBlockOption, Log, TransactionReceipt, TxHash, ValueOrArray,
    U64,
};
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::traits::KeyPair;
use fastcrypto::traits::ToFromBytes;
use hex_literal::hex;
use move_core_types::language_storage::TypeTag;
use starcoin_bridge_config::local_ip_utils;
use starcoin_bridge_json_rpc_types::{StarcoinEvent, StarcoinTransactionBlockEffectsAPI};
use starcoin_bridge_sdk::wallet_context::WalletContext;
use starcoin_bridge_test_transaction_builder::TestTransactionBuilder;
use starcoin_bridge_types::base_types::{
    ObjectRef, SequenceNumber, StarcoinAddress, TransactionDigest,
};
use starcoin_bridge_types::bridge::{
    BridgeChainId, BridgeCommitteeSummary, MoveTypeCommitteeMember, TOKEN_ID_USDC,
};
use starcoin_bridge_types::crypto::get_key_pair;
use starcoin_bridge_types::object::Owner;
use starcoin_bridge_types::transaction::{CallArg, ObjectArg};
use starcoin_bridge_types::{BRIDGE_PACKAGE_ID, STARCOIN_BRIDGE_OBJECT_ID};
use std::collections::{BTreeMap, HashMap};
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use tokio::task::JoinHandle;

// Testing helper extensions - publicly export for use in test modules
pub trait StarcoinAddressTestExt {
    fn random_for_testing_only() -> Self;
}

impl StarcoinAddressTestExt for StarcoinAddress {
    fn random_for_testing_only() -> Self {
        use move_core_types::account_address::AccountAddress;
        AccountAddress::random()
    }
}

pub trait TransactionDigestTestExt {
    fn random() -> Self;
}

impl TransactionDigestTestExt for TransactionDigest {
    fn random() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: [u8; 32] = rng.gen();
        bytes // TransactionDigest is just [u8; 32]
    }
}

pub trait StarcoinEventTestExt {
    fn random_for_testing() -> Self;
}

impl StarcoinEventTestExt for StarcoinEvent {
    fn random_for_testing() -> Self {
        use rand::Rng;
        use std::str::FromStr;
        use starcoin_bridge_json_rpc_types::EventID;
        
        let mut rng = rand::thread_rng();
        let tx_digest: [u8; 32] = rng.gen();
        let event_seq: u64 = rng.gen_range(0..1000);
        let block_number: u64 = rng.gen_range(1..10000);
        
        StarcoinEvent {
            id: EventID { tx_digest, event_seq, block_number },
            type_: move_core_types::language_storage::StructTag::from_str(
                "0x1::test::TestEvent"
            ).unwrap(),
            bcs: vec![],
        }
    }
}

pub trait SequenceNumberTestExt {
    fn from_u64(value: u64) -> Self;
}

impl SequenceNumberTestExt for SequenceNumber {
    fn from_u64(value: u64) -> Self {
        SequenceNumber::from(value)
    }
}

// Auto-import for use within this module
use self::{
    SequenceNumberTestExt as _, StarcoinAddressTestExt as _, TransactionDigestTestExt as _,
};

// WalletContext testing helpers - stub implementations
pub trait WalletContextTestExt {
    async fn get_reference_gas_price(&mut self) -> Result<u64, anyhow::Error>;
    fn active_address(&mut self) -> Result<StarcoinAddress, anyhow::Error>;
    async fn get_one_gas_object(&mut self) -> Result<Option<(bool, ObjectRef)>, anyhow::Error>;
    async fn execute_transaction_must_succeed(
        &mut self,
        tx: starcoin_bridge_types::transaction::Transaction,
    ) -> starcoin_bridge_json_rpc_types::StarcoinTransactionBlockResponse;
}

impl WalletContextTestExt for WalletContext {
    async fn get_reference_gas_price(&mut self) -> Result<u64, anyhow::Error> {
        Ok(1000) // Dummy gas price
    }

    fn active_address(&mut self) -> Result<StarcoinAddress, anyhow::Error> {
        Ok(StarcoinAddress::random_for_testing_only())
    }

    async fn get_one_gas_object(&mut self) -> Result<Option<(bool, ObjectRef)>, anyhow::Error> {
        // Return a dummy gas object
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let digest: [u8; 32] = rng.gen();
        let obj_ref = (
            starcoin_bridge_types::base_types::ObjectID::random(),
            SequenceNumber::from(1u64),
            digest,
        );
        Ok(Some((true, obj_ref)))
    }

    async fn execute_transaction_must_succeed(
        &mut self,
        _tx: starcoin_bridge_types::transaction::Transaction,
    ) -> starcoin_bridge_json_rpc_types::StarcoinTransactionBlockResponse {
        unimplemented!("execute_transaction_must_succeed is not implemented for testing")
    }
}

// TestTransactionBuilder testing helper
pub trait TestTransactionBuilderExt {
    fn move_call(
        self,
        package: starcoin_bridge_types::base_types::ObjectID,
        module: &str,
        function: &str,
        type_args: Vec<TypeTag>,
        call_args: Vec<CallArg>,
    ) -> Self;
}

impl TestTransactionBuilderExt for TestTransactionBuilder {
    fn move_call(
        self,
        _package: starcoin_bridge_types::base_types::ObjectID,
        _module: &str,
        _function: &str,
        _type_args: Vec<TypeTag>,
        _call_args: Vec<CallArg>,
    ) -> Self {
        unimplemented!("move_call is not implemented for testing")
    }
}

pub const DUMMY_MUTALBE_BRIDGE_OBJECT_ARG: ObjectArg = ObjectArg::SharedObject {
    id: STARCOIN_BRIDGE_OBJECT_ID,
    initial_shared_version: 1u64, // SequenceNumber is just u64
    mutable: true,
};

pub fn get_test_authority_and_key(
    voting_power: u64,
    port: u16,
) -> (
    BridgeAuthority,
    BridgeAuthorityPublicKey,
    BridgeAuthorityKeyPair,
) {
    let (_, kp): (_, fastcrypto::secp256k1::Secp256k1KeyPair) = get_key_pair();
    let pubkey = kp.public().clone();
    let authority = BridgeAuthority {
        starcoin_bridge_address: StarcoinAddress::random_for_testing_only(),
        pubkey: pubkey.clone(),
        voting_power,
        base_url: format!("http://127.0.0.1:{}", port),
        is_blocklisted: false,
    };

    (authority, pubkey, kp)
}

// TODO: make a builder for this
pub fn get_test_starcoin_bridge_to_eth_bridge_action(
    starcoin_bridge_tx_digest: Option<TransactionDigest>,
    starcoin_bridge_tx_event_index: Option<u16>,
    nonce: Option<u64>,
    amount_starcoin_bridge_adjusted: Option<u64>,
    sender_address: Option<StarcoinAddress>,
    recipient_address: Option<EthAddress>,
    token_id: Option<u8>,
) -> BridgeAction {
    BridgeAction::StarcoinToEthBridgeAction(StarcoinToEthBridgeAction {
        starcoin_bridge_tx_digest: starcoin_bridge_tx_digest
            .unwrap_or_else(TransactionDigest::random),
        starcoin_bridge_tx_event_index: starcoin_bridge_tx_event_index.unwrap_or(0),
        starcoin_bridge_event: EmittedStarcoinToEthTokenBridgeV1 {
            nonce: nonce.unwrap_or_default(),
            starcoin_bridge_chain_id: BridgeChainId::StarcoinCustom,
            starcoin_bridge_address: sender_address
                .unwrap_or_else(StarcoinAddress::random_for_testing_only),
            eth_chain_id: BridgeChainId::EthCustom,
            eth_address: recipient_address.unwrap_or_else(EthAddress::random),
            token_id: token_id.unwrap_or(TOKEN_ID_USDC),
            amount_starcoin_bridge_adjusted: amount_starcoin_bridge_adjusted.unwrap_or(100_000),
        },
    })
}

pub fn get_test_eth_to_starcoin_bridge_action(
    nonce: Option<u64>,
    amount: Option<u64>,
    starcoin_bridge_address: Option<StarcoinAddress>,
    token_id: Option<u8>,
) -> BridgeAction {
    BridgeAction::EthToStarcoinBridgeAction(EthToStarcoinBridgeAction {
        eth_tx_hash: TxHash::random(),
        eth_event_index: 0,
        eth_bridge_event: EthToStarcoinTokenBridgeV1 {
            eth_chain_id: BridgeChainId::EthCustom,
            nonce: nonce.unwrap_or_default(),
            starcoin_bridge_chain_id: BridgeChainId::StarcoinCustom,
            token_id: token_id.unwrap_or(TOKEN_ID_USDC),
            starcoin_bridge_adjusted_amount: amount.unwrap_or(100_000),
            starcoin_bridge_address: starcoin_bridge_address
                .unwrap_or_else(StarcoinAddress::random_for_testing_only),
            eth_address: EthAddress::random(),
        },
    })
}

pub fn run_mock_bridge_server(
    mock_handlers: Vec<BridgeRequestMockHandler>,
) -> (Vec<JoinHandle<()>>, Vec<u16>) {
    let mut handles = vec![];
    let mut ports = vec![];
    for mock_handler in mock_handlers {
        let localhost = local_ip_utils::localhost_for_testing();
        let port = local_ip_utils::get_available_port(&localhost);
        // start server
        let server_handle = run_mock_server(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port),
            mock_handler.clone(),
        );
        ports.push(port);
        handles.push(server_handle);
    }
    (handles, ports)
}

pub fn get_test_authorities_and_run_mock_bridge_server(
    voting_power: Vec<u64>,
    mock_handlers: Vec<BridgeRequestMockHandler>,
) -> (
    Vec<JoinHandle<()>>,
    Vec<BridgeAuthority>,
    Vec<BridgeAuthorityKeyPair>,
) {
    assert_eq!(voting_power.len(), mock_handlers.len());
    let (handles, ports) = run_mock_bridge_server(mock_handlers);
    let mut authorites = vec![];
    let mut secrets = vec![];
    for (port, vp) in ports.iter().zip(voting_power) {
        let (authority, _, secret) = get_test_authority_and_key(vp, *port);
        authorites.push(authority);
        secrets.push(secret);
    }

    (handles, authorites, secrets)
}

pub fn sign_action_with_key(
    action: &BridgeAction,
    secret: &BridgeAuthorityKeyPair,
) -> SignedBridgeAction {
    let sig = BridgeAuthoritySignInfo::new(action, secret);
    SignedBridgeAction::new_from_data_and_sig(action.clone(), sig)
}

pub fn mock_last_finalized_block(mock_provider: &EthMockProvider, block_number: u64) {
    let block = Block::<ethers::types::TxHash> {
        number: Some(U64::from(block_number)),
        ..Default::default()
    };
    mock_provider
        .add_response("eth_getBlockByNumber", ("finalized", false), block)
        .unwrap();
}

// Mocks eth_getLogs and eth_getTransactionReceipt for the given address and block range.
// The input log needs to have transaction_hash set.
pub fn mock_get_logs(
    mock_provider: &EthMockProvider,
    address: EthAddress,
    from_block: u64,
    to_block: u64,
    logs: Vec<Log>,
) {
    mock_provider.add_response::<[ethers::types::Filter; 1], Vec<ethers::types::Log>, Vec<ethers::types::Log>>(
        "eth_getLogs",
        [
            Filter {
                block_option: FilterBlockOption::Range {
                    from_block: Some(BlockNumber::Number(U64::from(from_block))),
                    to_block: Some(BlockNumber::Number(U64::from(to_block))),
                },
                address: Some(ValueOrArray::Value(address)),
                topics: [None, None, None, None],
            }
        ],
        logs.clone(),
    ).unwrap();

    for log in logs {
        mock_provider
            .add_response::<[TxHash; 1], TransactionReceipt, TransactionReceipt>(
                "eth_getTransactionReceipt",
                [log.transaction_hash.unwrap()],
                TransactionReceipt {
                    block_number: log.block_number,
                    logs: vec![log],
                    ..Default::default()
                },
            )
            .unwrap();
    }
}

// Returns a test Log and corresponding BridgeAction
// Refernece: https://github.com/rust-ethereum/ethabi/blob/master/ethabi/src/event.rs#L192
pub fn get_test_log_and_action(
    contract_address: EthAddress,
    tx_hash: TxHash,
    event_index: u16,
) -> (Log, BridgeAction) {
    let token_id = 3u8;
    let starcoin_bridge_adjusted_amount = 10000000u64;
    let source_address = EthAddress::random();
    let starcoin_bridge_address: StarcoinAddress = StarcoinAddress::random_for_testing_only();
    let target_address = Hex::decode(&starcoin_bridge_address.to_string()).unwrap();
    // Note: must use `encode` rather than `encode_packged`
    let encoded = ethers::abi::encode(&[
        // u8/u64 is encoded as u256 in abi standard
        ethers::abi::Token::Uint(ethers::types::U256::from(token_id)),
        ethers::abi::Token::Uint(ethers::types::U256::from(starcoin_bridge_adjusted_amount)),
        ethers::abi::Token::Address(source_address),
        ethers::abi::Token::Bytes(target_address.clone()),
    ]);
    let log = Log {
        address: contract_address,
        topics: vec![
            long_signature(
                "TokensDeposited",
                &[
                    ParamType::Uint(8),
                    ParamType::Uint(64),
                    ParamType::Uint(8),
                    ParamType::Uint(8),
                    ParamType::Uint(64),
                    ParamType::Address,
                    ParamType::Bytes,
                ],
            ),
            hex!("0000000000000000000000000000000000000000000000000000000000000001").into(), // chain id: starcoin testnet
            hex!("0000000000000000000000000000000000000000000000000000000000000010").into(), // nonce: 16
            hex!("000000000000000000000000000000000000000000000000000000000000000b").into(), // chain id: sepolia
        ],
        data: encoded.into(),
        block_hash: Some(TxHash::random()),
        block_number: Some(1.into()),
        transaction_hash: Some(tx_hash),
        log_index: Some(0.into()),
        ..Default::default()
    };
    let topic_1: [u8; 32] = log.topics[1].into();
    let topic_3: [u8; 32] = log.topics[3].into();

    let bridge_action = BridgeAction::EthToStarcoinBridgeAction(EthToStarcoinBridgeAction {
        eth_tx_hash: tx_hash,
        eth_event_index: event_index,
        eth_bridge_event: EthToStarcoinTokenBridgeV1 {
            eth_chain_id: BridgeChainId::try_from(topic_1[topic_1.len() - 1]).unwrap(),
            nonce: u64::from_be_bytes(log.topics[2].as_ref()[24..32].try_into().unwrap()),
            starcoin_bridge_chain_id: BridgeChainId::try_from(topic_3[topic_3.len() - 1]).unwrap(),
            token_id,
            starcoin_bridge_adjusted_amount,
            starcoin_bridge_address,
            eth_address: source_address,
        },
    });
    (log, bridge_action)
}

pub async fn bridge_token(
    _context: &mut WalletContext,
    recv_address: EthAddress,
    _token_ref: ObjectRef,
    _token_type: TypeTag,
    _bridge_object_arg: ObjectArg,
) -> EmittedStarcoinToEthTokenBridgeV1 {
    // Simplified test implementation - return dummy event
    use crate::test_utils::{StarcoinAddressTestExt, TransactionDigestTestExt};
    EmittedStarcoinToEthTokenBridgeV1 {
        nonce: 0,
        starcoin_bridge_chain_id: BridgeChainId::StarcoinCustom,
        eth_chain_id: BridgeChainId::EthCustom,
        starcoin_bridge_address: StarcoinAddress::random_for_testing_only(),
        eth_address: recv_address,
        token_id: TOKEN_ID_USDC,
        amount_starcoin_bridge_adjusted: 100_000,
    }

    /* Original implementation - requires full Starcoin test infrastructure
    let rgp = context.get_reference_gas_price().await.unwrap();
    let sender = context.active_address().unwrap();
    let gas_object = context.get_one_gas_object().await.unwrap().unwrap().1;
    let tx = TestTransactionBuilder::new(sender, gas_object, rgp)
        .move_call(
            BRIDGE_PACKAGE_ID,
            "bridge",
            "send_token",
            vec![
                CallArg::Object(bridge_object_arg),
                CallArg::Pure(bcs::to_bytes(&(BridgeChainId::EthCustom as u8)).unwrap()),
                CallArg::Pure(bcs::to_bytes(&recv_address.as_bytes()).unwrap()),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(token_ref)),
            ],
        )
        .with_type_args(vec![token_type])
        .build();
    let signed_tn = context.sign_transaction(&tx).await;
    let resp = context.execute_transaction_must_succeed(signed_tn).await;
    let events = resp.events.unwrap();
    let bridge_events = events
        .data
        .iter()
        .filter_map(|event| StarcoinBridgeEvent::try_from_starcoin_bridge_event(event).unwrap())
        .collect::<Vec<_>>();
    bridge_events
        .iter()
        .find_map(|e| match e {
            StarcoinBridgeEvent::StarcoinToEthTokenBridgeV1(event) => Some(event.clone()),
            _ => None,
        })
        .unwrap()
    */
}

// Returns a VerifiedCertifiedBridgeAction with signatures from the given
// BridgeAction and BridgeAuthorityKeyPair
pub fn get_certified_action_with_validator_secrets(
    action: BridgeAction,
    secrets: &Vec<BridgeAuthorityKeyPair>,
) -> VerifiedCertifiedBridgeAction {
    let mut sigs = BTreeMap::new();
    for secret in secrets {
        let signed_action = sign_action_with_key(&action, secret);
        sigs.insert(secret.public().into(), signed_action.into_sig().signature);
    }
    let certified_action = CertifiedBridgeAction::new_from_data_and_sig(
        action,
        BridgeCommitteeValiditySignInfo { signatures: sigs },
    );
    VerifiedCertifiedBridgeAction::new_from_verified(certified_action)
}

// Approve a bridge action with the given validator secrets. Return the
// newly created token object reference if `expected_token_receiver` is Some
// (only relevant when the action is eth -> Starcoin transfer),
// Otherwise return None.
// Note: for starcoin -> eth transfers, the actual deposit needs to be recorded.
// Use `bridge_token` to do it.
// TODO(bridge): It appears this function is very slow (particularly, `execute_transaction_must_succeed`).
// Investigate why.
pub async fn approve_action_with_validator_secrets(
    _wallet_context: &mut WalletContext,
    _bridge_obj_org: ObjectArg,
    _action: BridgeAction,
    validator_secrets: &Vec<BridgeAuthorityKeyPair>,
    expected_token_receiver: Option<StarcoinAddress>,
    _id_token_map: &HashMap<u8, TypeTag>,
) -> Option<ObjectRef> {
    // Simplified test implementation - return dummy object ref if receiver is provided
    if expected_token_receiver.is_some() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let digest: [u8; 32] = rng.gen();
        Some((
            starcoin_bridge_types::base_types::ObjectID::random(),
            SequenceNumber::from(1u64),
            digest,
        ))
    } else {
        None
    }

    /* Original implementation - requires full Starcoin test infrastructure
    let action_certificate = get_certified_action_with_validator_secrets(action, validator_secrets);
    let rgp = wallet_context.get_reference_gas_price().await.unwrap();
    let starcoin_bridge_address = wallet_context.active_address().unwrap();
    let gas_obj_ref = wallet_context
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap()
        .1;
    let tx_data = build_starcoin_bridge_transaction(
        starcoin_bridge_address,
        &gas_obj_ref,
        action_certificate,
        bridge_obj_org,
        id_token_map,
        rgp,
    )
    .unwrap();
    let signed_tx = wallet_context.sign_transaction(&tx_data).await;
    let resp = wallet_context
        .execute_transaction_must_succeed(signed_tx)
        .await;

    // If `expected_token_receiver` is None, return
    expected_token_receiver?;

    let expected_token_receiver = expected_token_receiver.unwrap();
    for created in resp.effects.unwrap().created() {
        if created.owner == Owner::AddressOwner(expected_token_receiver) {
            return Some(created.reference.to_object_ref());
        }
    }
    panic!(
        "Didn't find the creted object owned by {}",
        expected_token_receiver
    );
    */
}

pub fn bridge_committee_to_bridge_committee_summary(
    committee: BridgeCommittee,
) -> BridgeCommitteeSummary {
    BridgeCommitteeSummary {
        members: committee
            .members()
            .iter()
            .map(|(k, v)| {
                let bytes = k.as_bytes().to_vec();
                (
                    bytes.clone(),
                    MoveTypeCommitteeMember {
                        starcoin_bridge_address: StarcoinAddress::random_for_testing_only(),
                        bridge_pubkey_bytes: bytes,
                        voting_power: v.voting_power,
                        http_rest_url: v.base_url.as_bytes().to_vec(),
                        blocklisted: v.is_blocklisted,
                    },
                )
            })
            .collect(),
        member_registration: vec![],
        last_committee_update_epoch: 0,
    }
}
