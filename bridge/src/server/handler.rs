// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::type_complexity)]

use crate::crypto::{BridgeAuthorityKeyPair, BridgeAuthoritySignInfo};
use crate::error::{BridgeError, BridgeResult};
use crate::eth_client::EthClient;
use crate::metrics::BridgeMetrics;
use crate::starcoin_bridge_client::{StarcoinClient, StarcoinClientInner};
use crate::types::{BridgeAction, SignedBridgeAction};
use async_trait::async_trait;
use axum::Json;
use ethers::providers::JsonRpcClient;
use ethers::types::TxHash;
use lru::LruCache;
use starcoin_bridge_types::base_types::TransactionDigest;
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::sync::Arc;
use tap::TapFallible;
use tokio::sync::{oneshot, Mutex};
use tracing::info;

use super::governance_verifier::GovernanceVerifier;

#[async_trait]
pub trait BridgeRequestHandlerTrait {
    // Handles a request to sign a BridgeAction that bridges assets
    // from Ethereum to Starcoin. The inputs are a transaction hash on Ethereum
    // that emitted the bridge event and the Event index in that transaction
    async fn handle_eth_tx_hash(
        &self,
        tx_hash_hex: String,
        event_idx: u16,
    ) -> Result<Json<SignedBridgeAction>, BridgeError>;
    // Handles a request to sign a BridgeAction that bridges assets
    // from Starcoin to Ethereum. The inputs are a transaction digest on Starcoin
    // that emitted the bridge event and the Event index in that transaction
    async fn handle_starcoin_bridge_tx_digest(
        &self,
        tx_digest_base58: String,
        event_idx: u16,
    ) -> Result<Json<SignedBridgeAction>, BridgeError>;

    // Handles a request to sign a governance action.
    async fn handle_governance_action(
        &self,
        action: BridgeAction,
    ) -> Result<Json<SignedBridgeAction>, BridgeError>;
}

#[async_trait::async_trait]
pub trait ActionVerifier<K>: Send + Sync {
    // Name of the verifier, used for metrics
    fn name(&self) -> &'static str;
    async fn verify(&self, key: K) -> BridgeResult<BridgeAction>;
}

struct StarcoinActionVerifier<C> {
    starcoin_bridge_client: Arc<StarcoinClient<C>>,
}

struct EthActionVerifier<P> {
    eth_client: Arc<EthClient<P>>,
}

#[async_trait::async_trait]
impl<C> ActionVerifier<(TransactionDigest, u16)> for StarcoinActionVerifier<C>
where
    C: StarcoinClientInner + Send + Sync + 'static,
{
    fn name(&self) -> &'static str {
        "StarcoinActionVerifier"
    }

    async fn verify(&self, key: (TransactionDigest, u16)) -> BridgeResult<BridgeAction> {
        let (tx_digest, event_idx) = key;
        self.starcoin_bridge_client
            .get_bridge_action_by_tx_digest_and_event_idx_maybe(&tx_digest, event_idx)
            .await
            .tap_ok(|action| info!("Starcoin action found: {:?}", action))
    }
}

#[async_trait::async_trait]
impl<C> ActionVerifier<(TxHash, u16)> for EthActionVerifier<C>
where
    C: JsonRpcClient + Send + Sync + 'static,
{
    fn name(&self) -> &'static str {
        "EthActionVerifier"
    }

    async fn verify(&self, key: (TxHash, u16)) -> BridgeResult<BridgeAction> {
        let (tx_hash, event_idx) = key;
        self.eth_client
            .get_finalized_bridge_action_maybe(tx_hash, event_idx)
            .await
            .tap_ok(|action| info!("Eth action found: {:?}", action))
    }
}

struct SignerWithCache<K> {
    signer: Arc<BridgeAuthorityKeyPair>,
    verifier: Arc<dyn ActionVerifier<K>>,
    mutex: Arc<Mutex<()>>,
    cache: LruCache<K, Arc<Mutex<Option<BridgeResult<SignedBridgeAction>>>>>,
    metrics: Arc<BridgeMetrics>,
}

impl<K> SignerWithCache<K>
where
    K: std::hash::Hash + Eq + Clone + Send + Sync + 'static,
{
    fn new(
        signer: Arc<BridgeAuthorityKeyPair>,
        verifier: impl ActionVerifier<K> + 'static,
        metrics: Arc<BridgeMetrics>,
    ) -> Self {
        Self {
            signer,
            verifier: Arc::new(verifier),
            mutex: Arc::new(Mutex::new(())),
            cache: LruCache::new(NonZeroUsize::new(1000).unwrap()),
            metrics,
        }
    }

    fn spawn(
        mut self,
        mut rx: mysten_metrics::metered_channel::Receiver<(
            K,
            oneshot::Sender<BridgeResult<SignedBridgeAction>>,
        )>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let (key, tx) = rx
                    .recv()
                    .await
                    .unwrap_or_else(|| panic!("Server signer's channel is closed"));
                let result = self.sign(key).await;
                // The receiver may be dropped before the sender (client connection was dropped for example),
                // we ignore the error in that case.
                let _ = tx.send(result);
            }
        })
    }

    async fn get_cache_entry(
        &mut self,
        key: K,
    ) -> Arc<Mutex<Option<BridgeResult<SignedBridgeAction>>>> {
        // This mutex exists to make sure everyone gets the same entry, namely no double insert
        let _ = self.mutex.lock().await;
        self.cache
            .get_or_insert(key, || Arc::new(Mutex::new(None)))
            .clone()
    }

    async fn sign(&mut self, key: K) -> BridgeResult<SignedBridgeAction> {
        let signer = self.signer.clone();
        let verifier = self.verifier.clone();
        let verifier_name = verifier.name();
        let entry = self.get_cache_entry(key.clone()).await;
        let mut guard = entry.lock().await;
        if let Some(result) = &*guard {
            self.metrics
                .signer_with_cache_hit
                .with_label_values(&[verifier_name])
                .inc();
            return result.clone();
        }
        self.metrics
            .signer_with_cache_miss
            .with_label_values(&[verifier_name])
            .inc();
        match verifier.verify(key.clone()).await {
            Ok(bridge_action) => {
                let sig = BridgeAuthoritySignInfo::new(&bridge_action, &signer);
                let result = SignedBridgeAction::new_from_data_and_sig(bridge_action, sig);
                // Cache result if Ok
                *guard = Some(Ok(result.clone()));
                Ok(result)
            }
            Err(e) => {
                match e {
                    // Only cache non-transient errors
                    BridgeError::GovernanceActionIsNotApproved { .. }
                    | BridgeError::ActionIsNotGovernanceAction(..)
                    | BridgeError::BridgeEventInUnrecognizedStarcoinPackage
                    | BridgeError::BridgeEventInUnrecognizedEthContract
                    | BridgeError::BridgeEventNotActionable
                    | BridgeError::NoBridgeEventsInTxPosition => {
                        *guard = Some(Err(e.clone()));
                    }
                    _ => (),
                }
                Err(e)
            }
        }
    }

    /*#[cfg(test)]
    async fn get_testing_only(
        &mut self,
        key: K,
    ) -> Option<&Arc<Mutex<Option<BridgeResult<SignedBridgeAction>>>>> {
        let _ = self.mutex.lock().await;
        self.cache.get(&key)
    }*/
}

pub struct BridgeRequestHandler {
    starcoin_bridge_signer_tx: mysten_metrics::metered_channel::Sender<(
        (TransactionDigest, u16),
        oneshot::Sender<BridgeResult<SignedBridgeAction>>,
    )>,
    eth_signer_tx: mysten_metrics::metered_channel::Sender<(
        (TxHash, u16),
        oneshot::Sender<BridgeResult<SignedBridgeAction>>,
    )>,
    governance_signer_tx: mysten_metrics::metered_channel::Sender<(
        BridgeAction,
        oneshot::Sender<BridgeResult<SignedBridgeAction>>,
    )>,
}

impl BridgeRequestHandler {
    pub fn new<
        SC: StarcoinClientInner + Send + Sync + 'static,
        EP: JsonRpcClient + Send + Sync + 'static,
    >(
        signer: BridgeAuthorityKeyPair,
        starcoin_bridge_client: Arc<StarcoinClient<SC>>,
        eth_client: Arc<EthClient<EP>>,
        approved_governance_actions: Vec<BridgeAction>,
        metrics: Arc<BridgeMetrics>,
    ) -> Self {
        let (starcoin_bridge_signer_tx, starcoin_bridge_rx) =
            mysten_metrics::metered_channel::channel(
                1000,
                &mysten_metrics::get_metrics()
                    .unwrap()
                    .channel_inflight
                    .with_label_values(&["server_starcoin_bridge_action_signing_queue"]),
            );
        let (eth_signer_tx, eth_rx) = mysten_metrics::metered_channel::channel(
            1000,
            &mysten_metrics::get_metrics()
                .unwrap()
                .channel_inflight
                .with_label_values(&["server_eth_action_signing_queue"]),
        );
        let (governance_signer_tx, governance_rx) = mysten_metrics::metered_channel::channel(
            1000,
            &mysten_metrics::get_metrics()
                .unwrap()
                .channel_inflight
                .with_label_values(&["server_governance_action_signing_queue"]),
        );
        let signer = Arc::new(signer);

        SignerWithCache::new(
            signer.clone(),
            StarcoinActionVerifier {
                starcoin_bridge_client,
            },
            metrics.clone(),
        )
        .spawn(starcoin_bridge_rx);
        SignerWithCache::new(
            signer.clone(),
            EthActionVerifier { eth_client },
            metrics.clone(),
        )
        .spawn(eth_rx);
        SignerWithCache::new(
            signer.clone(),
            GovernanceVerifier::new(approved_governance_actions).unwrap(),
            metrics.clone(),
        )
        .spawn(governance_rx);

        Self {
            starcoin_bridge_signer_tx,
            eth_signer_tx,
            governance_signer_tx,
        }
    }
}

#[async_trait]
impl BridgeRequestHandlerTrait for BridgeRequestHandler {
    async fn handle_eth_tx_hash(
        &self,
        tx_hash_hex: String,
        event_idx: u16,
    ) -> Result<Json<SignedBridgeAction>, BridgeError> {
        let tx_hash = TxHash::from_str(&tx_hash_hex).map_err(|_| BridgeError::InvalidTxHash)?;

        let (tx, rx) = oneshot::channel();
        self.eth_signer_tx
            .send(((tx_hash, event_idx), tx))
            .await
            .unwrap_or_else(|_| panic!("Server eth signing channel is closed"));
        let signed_action = rx
            .await
            .unwrap_or_else(|_| panic!("Server signing task's oneshot channel is dropped"))?;
        Ok(Json(signed_action))
    }

    async fn handle_starcoin_bridge_tx_digest(
        &self,
        tx_digest_base58: String,
        event_idx: u16,
    ) -> Result<Json<SignedBridgeAction>, BridgeError> {
        let tx_digest: TransactionDigest = bs58::decode(&tx_digest_base58)
            .into_vec()
            .ok()
            .and_then(|v| v.try_into().ok())
            .ok_or(BridgeError::InvalidTxHash)?;
        let (tx, rx) = oneshot::channel();
        self.starcoin_bridge_signer_tx
            .send(((tx_digest, event_idx), tx))
            .await
            .unwrap_or_else(|_| panic!("Server starcoin signing channel is closed"));
        let signed_action = rx
            .await
            .unwrap_or_else(|_| panic!("Server signing task's oneshot channel is dropped"))?;
        Ok(Json(signed_action))
    }

    async fn handle_governance_action(
        &self,
        action: BridgeAction,
    ) -> Result<Json<SignedBridgeAction>, BridgeError> {
        if !action.is_governace_action() {
            return Err(BridgeError::ActionIsNotGovernanceAction(action));
        }
        let (tx, rx) = oneshot::channel();
        self.governance_signer_tx
            .send((action, tx))
            .await
            .unwrap_or_else(|_| panic!("Server governance action signing channel is closed"));
        let signed_action = rx.await.unwrap_or_else(|_| {
            panic!("Server governance action task's oneshot channel is dropped")
        })?;
        Ok(Json(signed_action))
    }
}

/*#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{
        eth_mock_provider::EthMockProvider,
        events::{init_all_struct_tags, MoveTokenDepositedEvent, StarcoinToEthTokenBridgeV1},
        starcoin_bridge_mock_client::StarcoinMockClient,
        test_utils::{
            get_test_log_and_action, get_test_starcoin_bridge_to_eth_bridge_action, mock_last_finalized_block,
        },
        types::{EmergencyAction, EmergencyActionType, LimitUpdateAction},
    };
    use ethers::types::{Address as EthAddress, TransactionReceipt};
    use starcoin_bridge_json_rpc_types::{BcsEvent, StarcoinEvent};
    use starcoin_bridge_types::bridge::{BridgeChainId, TOKEN_ID_USDC};
    use starcoin_bridge_types::{base_types::StarcoinAddress, crypto::get_key_pair};

    #[tokio::test]
    async fn test_starcoin_bridge_signer_with_cache() {
        let (_, kp): (_, BridgeAuthorityKeyPair) = get_key_pair();
        let signer = Arc::new(kp);
        let starcoin_bridge_client_mock = StarcoinMockClient::default();
        let starcoin_bridge_verifier = StarcoinActionVerifier {
            starcoin_bridge_client: Arc::new(StarcoinClient::new_for_testing(starcoin_bridge_client_mock.clone())),
        };
        let metrics = Arc::new(BridgeMetrics::new_for_testing());
        let mut starcoin_bridge_signer_with_cache = SignerWithCache::new(signer.clone(), starcoin_bridge_verifier, metrics);

        // Test `get_cache_entry` creates a new entry if not exist
        let starcoin_bridge_tx_digest = TransactionDigest::random();
        let starcoin_bridge_event_idx = 42;
        assert!(starcoin_bridge_signer_with_cache
            .get_testing_only((starcoin_bridge_tx_digest, starcoin_bridge_event_idx))
            .await
            .is_none());
        let entry = starcoin_bridge_signer_with_cache
            .get_cache_entry((starcoin_bridge_tx_digest, starcoin_bridge_event_idx))
            .await;
        let entry_ = starcoin_bridge_signer_with_cache
            .get_testing_only((starcoin_bridge_tx_digest, starcoin_bridge_event_idx))
            .await;
        assert!(entry_.unwrap().lock().await.is_none());

        let action = get_test_starcoin_bridge_to_eth_bridge_action(
            Some(starcoin_bridge_tx_digest),
            Some(starcoin_bridge_event_idx),
            None,
            None,
            None,
            None,
            None,
        );
        let sig = BridgeAuthoritySignInfo::new(&action, &signer);
        let signed_action = SignedBridgeAction::new_from_data_and_sig(action.clone(), sig);
        entry.lock().await.replace(Ok(signed_action));
        let entry_ = starcoin_bridge_signer_with_cache
            .get_testing_only((starcoin_bridge_tx_digest, starcoin_bridge_event_idx))
            .await;
        assert!(entry_.unwrap().lock().await.is_some());

        // Test `sign` caches Err result
        let starcoin_bridge_tx_digest = TransactionDigest::random();
        let starcoin_bridge_event_idx = 0;

        // Mock an non-cacheable error such as rpc error
        starcoin_bridge_client_mock.add_events_by_tx_digest_error(starcoin_bridge_tx_digest);
        starcoin_bridge_signer_with_cache
            .sign((starcoin_bridge_tx_digest, starcoin_bridge_event_idx))
            .await
            .unwrap_err();
        let entry_ = starcoin_bridge_signer_with_cache
            .get_testing_only((starcoin_bridge_tx_digest, starcoin_bridge_event_idx))
            .await;
        assert!(entry_.unwrap().lock().await.is_none());

        // Mock a cacheable error such as no bridge events in tx position (empty event list)
        starcoin_bridge_client_mock.add_events_by_tx_digest(starcoin_bridge_tx_digest, vec![]);
        assert!(matches!(
            starcoin_bridge_signer_with_cache
                .sign((starcoin_bridge_tx_digest, starcoin_bridge_event_idx))
                .await,
            Err(BridgeError::NoBridgeEventsInTxPosition)
        ));
        let entry_ = starcoin_bridge_signer_with_cache
            .get_testing_only((starcoin_bridge_tx_digest, starcoin_bridge_event_idx))
            .await;
        assert_eq!(
            entry_.unwrap().lock().await.clone().unwrap().unwrap_err(),
            BridgeError::NoBridgeEventsInTxPosition,
        );

        // TODO: test BridgeEventInUnrecognizedStarcoinPackage, StarcoinBridgeEvent::try_from_starcoin_bridge_event
        // and BridgeEventNotActionable to be cached

        // Test `sign` caches Ok result
        let emitted_event_1 = MoveTokenDepositedEvent {
            seq_num: 1,
            source_chain: BridgeChainId::StarcoinCustom as u8,
            sender_address: StarcoinAddress::random_for_testing_only().to_vec(),
            target_chain: BridgeChainId::EthCustom as u8,
            target_address: EthAddress::random().as_bytes().to_vec(),
            token_type: TOKEN_ID_USDC,
            amount_starcoin_bridge_adjusted: 12345,
        };

        init_all_struct_tags();

        let mut starcoin_bridge_event_1 = StarcoinEvent::random_for_testing();
        starcoin_bridge_event_1.type_ = StarcoinToEthTokenBridgeV1.get().unwrap().clone();
        starcoin_bridge_event_1.bcs = BcsEvent::new(bcs::to_bytes(&emitted_event_1).unwrap());
        let starcoin_bridge_tx_digest = starcoin_bridge_event_1.id.tx_digest;

        let mut starcoin_bridge_event_2 = StarcoinEvent::random_for_testing();
        starcoin_bridge_event_2.type_ = StarcoinToEthTokenBridgeV1.get().unwrap().clone();
        starcoin_bridge_event_2.bcs = BcsEvent::new(bcs::to_bytes(&emitted_event_1).unwrap());
        let starcoin_bridge_event_idx_2 = 1;
        starcoin_bridge_client_mock.add_events_by_tx_digest(starcoin_bridge_tx_digest, vec![starcoin_bridge_event_2.clone()]);

        starcoin_bridge_client_mock.add_events_by_tx_digest(
            starcoin_bridge_tx_digest,
            vec![starcoin_bridge_event_1.clone(), starcoin_bridge_event_2.clone()],
        );
        let signed_1 = starcoin_bridge_signer_with_cache
            .sign((starcoin_bridge_tx_digest, starcoin_bridge_event_idx))
            .await
            .unwrap();
        let signed_2 = starcoin_bridge_signer_with_cache
            .sign((starcoin_bridge_tx_digest, starcoin_bridge_event_idx_2))
            .await
            .unwrap();

        // Because the result is cached now, the verifier should not be called again.
        // Even though we remove the `add_events_by_tx_digest` mock, we will still get the same result.
        starcoin_bridge_client_mock.add_events_by_tx_digest(starcoin_bridge_tx_digest, vec![]);
        assert_eq!(
            starcoin_bridge_signer_with_cache
                .sign((starcoin_bridge_tx_digest, starcoin_bridge_event_idx))
                .await
                .unwrap(),
            signed_1
        );
        assert_eq!(
            starcoin_bridge_signer_with_cache
                .sign((starcoin_bridge_tx_digest, starcoin_bridge_event_idx_2))
                .await
                .unwrap(),
            signed_2
        );
    }

    #[tokio::test]
    async fn test_eth_signer_with_cache() {
        let (_, kp): (_, BridgeAuthorityKeyPair) = get_key_pair();
        let signer = Arc::new(kp);
        let eth_mock_provider = EthMockProvider::default();
        let contract_address = EthAddress::random();
        let eth_client = EthClient::new_mocked(
            eth_mock_provider.clone(),
            HashSet::from_iter(vec![contract_address]),
        );
        let eth_verifier = EthActionVerifier {
            eth_client: Arc::new(eth_client),
        };
        let metrics = Arc::new(BridgeMetrics::new_for_testing());
        let mut eth_signer_with_cache =
            SignerWithCache::new(signer.clone(), eth_verifier, metrics.clone());

        // Test `get_cache_entry` creates a new entry if not exist
        let eth_tx_hash = TxHash::random();
        let eth_event_idx = 42;
        assert!(eth_signer_with_cache
            .get_testing_only((eth_tx_hash, eth_event_idx))
            .await
            .is_none());
        let entry = eth_signer_with_cache
            .get_cache_entry((eth_tx_hash, eth_event_idx))
            .await;
        let entry_ = eth_signer_with_cache
            .get_testing_only((eth_tx_hash, eth_event_idx))
            .await;
        // first unwrap should not pacic because the entry should have been inserted by `get_cache_entry`
        assert!(entry_.unwrap().lock().await.is_none());

        let (_, action) = get_test_log_and_action(contract_address, eth_tx_hash, eth_event_idx);
        let sig = BridgeAuthoritySignInfo::new(&action, &signer);
        let signed_action = SignedBridgeAction::new_from_data_and_sig(action.clone(), sig);
        entry.lock().await.replace(Ok(signed_action.clone()));
        let entry_ = eth_signer_with_cache
            .get_testing_only((eth_tx_hash, eth_event_idx))
            .await;
        assert_eq!(
            entry_.unwrap().lock().await.clone().unwrap().unwrap(),
            signed_action
        );

        // Test `sign` caches Ok result
        let eth_tx_hash = TxHash::random();
        let eth_event_idx = 0;
        let (log, _action) = get_test_log_and_action(contract_address, eth_tx_hash, eth_event_idx);
        eth_mock_provider
            .add_response::<[TxHash; 1], TransactionReceipt, TransactionReceipt>(
                "eth_getTransactionReceipt",
                [log.transaction_hash.unwrap()],
                TransactionReceipt {
                    block_number: log.block_number,
                    logs: vec![log.clone()],
                    ..Default::default()
                },
            )
            .unwrap();
        mock_last_finalized_block(&eth_mock_provider, log.block_number.unwrap().as_u64());

        eth_signer_with_cache
            .sign((eth_tx_hash, eth_event_idx))
            .await
            .unwrap();
        let entry_ = eth_signer_with_cache
            .get_testing_only((eth_tx_hash, eth_event_idx))
            .await;
        entry_.unwrap().lock().await.clone().unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_signer_with_governace_verifier() {
        let action_1 = BridgeAction::EmergencyAction(EmergencyAction {
            chain_id: BridgeChainId::EthCustom,
            nonce: 1,
            action_type: EmergencyActionType::Pause,
        });
        let action_2 = BridgeAction::LimitUpdateAction(LimitUpdateAction {
            chain_id: BridgeChainId::EthCustom,
            sending_chain_id: BridgeChainId::StarcoinCustom,
            nonce: 1,
            new_usd_limit: 10000,
        });

        let verifier = GovernanceVerifier::new(vec![action_1.clone(), action_2.clone()]).unwrap();
        assert_eq!(
            verifier.verify(action_1.clone()).await.unwrap(),
            action_1.clone()
        );
        assert_eq!(
            verifier.verify(action_2.clone()).await.unwrap(),
            action_2.clone()
        );

        let (_, kp): (_, BridgeAuthorityKeyPair) = get_key_pair();
        let signer = Arc::new(kp);
        let metrics = Arc::new(BridgeMetrics::new_for_testing());
        let mut signer_with_cache = SignerWithCache::new(signer.clone(), verifier, metrics.clone());

        // action_1 is signable
        signer_with_cache.sign(action_1.clone()).await.unwrap();
        // signed action is cached
        let entry_ = signer_with_cache.get_testing_only(action_1.clone()).await;
        assert_eq!(
            entry_
                .unwrap()
                .lock()
                .await
                .clone()
                .unwrap()
                .unwrap()
                .data(),
            &action_1
        );

        // alter action_1 to action_3
        let action_3 = BridgeAction::EmergencyAction(EmergencyAction {
            chain_id: BridgeChainId::EthCustom,
            nonce: 1,
            action_type: EmergencyActionType::Unpause,
        });
        // action_3 is not signable
        assert!(matches!(
            signer_with_cache.sign(action_3.clone()).await.unwrap_err(),
            BridgeError::GovernanceActionIsNotApproved { .. }
        ));
        // error is cached
        let entry_ = signer_with_cache.get_testing_only(action_3.clone()).await;
        assert!(matches!(
            entry_.unwrap().lock().await.clone().unwrap().unwrap_err(),
            BridgeError::GovernanceActionIsNotApproved { .. }
        ));

        // Non governace action is not signable
        let action_4 = get_test_starcoin_bridge_to_eth_bridge_action(None, None, None, None, None, None, None);
        assert!(matches!(
            signer_with_cache.sign(action_4.clone()).await.unwrap_err(),
            BridgeError::ActionIsNotGovernanceAction(..)
        ));
        // error is cached
        let entry_ = signer_with_cache.get_testing_only(action_4.clone()).await;
        assert!(matches!(
            entry_.unwrap().lock().await.clone().unwrap().unwrap_err(),
            BridgeError::ActionIsNotGovernanceAction { .. }
        ));
    }
    // TODO: add tests for BridgeRequestHandler (need to hook up local eth node)
}*/
