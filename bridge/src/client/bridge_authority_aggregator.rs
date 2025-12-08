// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! BridgeAuthorityAggregator requests signature from the single committee member.
//!
//! # Starcoin Bridge Simplification
//! For Starcoin deployment, the committee has exactly ONE member with maximum voting power.
//! This removes the need for complex multi-member quorum aggregation logic.

use crate::client::bridge_client::BridgeClient;
use crate::crypto::BridgeAuthorityPublicKeyBytes;
use crate::error::{BridgeError, BridgeResult};
use crate::metrics::BridgeMetrics;
use crate::types::BridgeCommitteeValiditySignInfo;
use crate::types::{
    BridgeAction, BridgeCommittee, CertifiedBridgeAction, VerifiedCertifiedBridgeAction,
};
use starcoin_bridge_types::base_types::ConciseableName;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

const TOTAL_TIMEOUT_MS: u64 = 5_000;
const RETRY_INTERVAL_MS: u64 = 500;

pub struct BridgeAuthorityAggregator {
    pub committee: Arc<BridgeCommittee>,
    pub client: Arc<BridgeClient>,
    pub authority_key: BridgeAuthorityPublicKeyBytes,
    pub metrics: Arc<BridgeMetrics>,
    /// Mapping from committee keys to names for metrics reporting
    pub committee_keys_to_names: Arc<BTreeMap<BridgeAuthorityPublicKeyBytes, String>>,
}

impl BridgeAuthorityAggregator {
    pub fn new(
        committee: Arc<BridgeCommittee>,
        metrics: Arc<BridgeMetrics>,
        committee_keys_to_names: Arc<BTreeMap<BridgeAuthorityPublicKeyBytes, String>>,
    ) -> Self {
        // Starcoin bridge: single member committee
        assert_eq!(
            committee.members().len(),
            1,
            "Starcoin bridge requires exactly one committee member"
        );

        let (authority_key, authority) = committee.members().iter().next().unwrap();
        let authority_key = authority_key.clone();
        assert!(
            !authority.is_blocklisted,
            "The single committee member cannot be blocklisted"
        );

        let client = BridgeClient::new(authority_key.clone(), committee.clone())
            .expect("Failed to create BridgeClient for the single committee member");

        Self {
            committee,
            client: Arc::new(client),
            authority_key,
            metrics,
            committee_keys_to_names,
        }
    }

    #[cfg(test)]
    pub fn new_for_testing(committee: Arc<BridgeCommittee>) -> Self {
        Self::new(
            committee,
            Arc::new(BridgeMetrics::new_for_testing()),
            Arc::new(BTreeMap::new()),
        )
    }

    /// Request signature from the single committee member
    pub async fn request_committee_signatures(
        &self,
        action: BridgeAction,
    ) -> BridgeResult<VerifiedCertifiedBridgeAction> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(TOTAL_TIMEOUT_MS);
        let retry_interval = Duration::from_millis(RETRY_INTERVAL_MS);

        // Retry loop for the single authority
        while start.elapsed() < timeout {
            match self.client.request_sign_bridge_action(action.clone()).await {
                Ok(verified_signed_action) => {
                    info!(
                        "Got signature from single authority {}",
                        self.authority_key.concise()
                    );

                    // Build certified action with the single signature
                    let mut signatures = BTreeMap::new();
                    signatures.insert(
                        self.authority_key.clone(),
                        verified_signed_action.auth_sig().signature.clone(),
                    );
                    let sig_info = BridgeCommitteeValiditySignInfo { signatures };
                    let certified_action = CertifiedBridgeAction::new_from_data_and_sig(
                        verified_signed_action.into_inner().into_data(),
                        sig_info,
                    );
                    let verified_certified =
                        VerifiedCertifiedBridgeAction::new_from_verified(certified_action);

                    self.metrics
                        .auth_agg_ok_responses
                        .with_label_values(&["single_authority"])
                        .inc();

                    return Ok(verified_certified);
                }
                Err(BridgeError::TxNotFinalized) => {
                    warn!(
                        "Bridge authority {} observing transaction not yet finalized, retrying in {:?}",
                        self.authority_key.concise(),
                        retry_interval
                    );
                    tokio::time::sleep(retry_interval).await;
                }
                Err(e) => {
                    self.metrics
                        .auth_agg_bad_responses
                        .with_label_values(&["single_authority"])
                        .inc();
                    return Err(e);
                }
            }
        }

        self.metrics
            .auth_agg_bad_responses
            .with_label_values(&["single_authority"])
            .inc();

        Err(BridgeError::TransientProviderError(format!(
            "Bridge authority {} did not observe finalized transaction after {:?}",
            self.authority_key.concise(),
            timeout
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::mock_handler::BridgeRequestMockHandler;
    use crate::test_utils::{
        get_test_authorities_and_run_mock_bridge_server, get_test_authority_and_key,
        get_test_starcoin_bridge_to_eth_bridge_action, sign_action_with_key,
        TransactionDigestTestExt,
    };
    use crate::types::BridgeCommittee;
    use starcoin_bridge_types::bridge::BRIDGE_COMMITTEE_MAXIMAL_VOTING_POWER;
    use starcoin_bridge_types::digests::TransactionDigest;

    fn create_single_member_committee() -> (BridgeCommittee, crate::crypto::BridgeAuthorityKeyPair)
    {
        let (authority, _, secret) =
            get_test_authority_and_key(BRIDGE_COMMITTEE_MAXIMAL_VOTING_POWER, 12345);
        let committee = BridgeCommittee::new(vec![authority]).unwrap();
        (committee, secret)
    }

    #[tokio::test]
    async fn test_bridge_auth_agg_construction() {
        telemetry_subscribers::init_for_testing();

        let (committee, _secret) = create_single_member_committee();
        let agg = BridgeAuthorityAggregator::new_for_testing(Arc::new(committee.clone()));

        // Verify single client is created
        assert_eq!(committee.members().len(), 1);
    }

    #[tokio::test]
    #[should_panic(expected = "Starcoin bridge requires exactly one committee member")]
    async fn test_bridge_auth_agg_rejects_multi_member() {
        telemetry_subscribers::init_for_testing();

        // This should panic at BridgeCommittee::new - multi-member not allowed
        let (auth1, _, _) = get_test_authority_and_key(5000, 12345);
        let (auth2, _, _) = get_test_authority_and_key(5000, 12346);
        let _committee = BridgeCommittee::new(vec![auth1, auth2]).unwrap();
    }

    #[tokio::test]
    #[should_panic(expected = "Starcoin bridge: the single committee member cannot be blocklisted")]
    async fn test_bridge_auth_agg_rejects_blocklisted() {
        telemetry_subscribers::init_for_testing();

        // This should panic at BridgeCommittee::new - blocklisted not allowed
        let (mut authority, _, _) =
            get_test_authority_and_key(BRIDGE_COMMITTEE_MAXIMAL_VOTING_POWER, 12345);
        authority.is_blocklisted = true;
        let _committee = BridgeCommittee::new(vec![authority]).unwrap();
    }

    #[tokio::test]
    async fn test_bridge_auth_agg_ok() {
        telemetry_subscribers::init_for_testing();

        let mock = BridgeRequestMockHandler::new();

        // start server with single authority
        let (_handles, authorities, secrets) = get_test_authorities_and_run_mock_bridge_server(
            vec![BRIDGE_COMMITTEE_MAXIMAL_VOTING_POWER],
            vec![mock.clone()],
        );

        let committee = BridgeCommittee::new(authorities.clone()).unwrap();
        let agg = BridgeAuthorityAggregator::new_for_testing(Arc::new(committee));

        let starcoin_bridge_tx_digest = TransactionDigest::random();
        let starcoin_bridge_tx_event_index = 0;
        let nonce = 0;
        let amount = 1000;
        let action = get_test_starcoin_bridge_to_eth_bridge_action(
            Some(starcoin_bridge_tx_digest),
            Some(starcoin_bridge_tx_event_index),
            Some(nonce),
            Some(amount),
            None,
            None,
            None,
        );

        // Authority returns signature
        mock.add_starcoin_bridge_event_response(
            starcoin_bridge_tx_digest,
            starcoin_bridge_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[0])),
            None,
        );
        let certified = agg
            .request_committee_signatures(action.clone())
            .await
            .unwrap();

        // Verify the certified action
        assert_eq!(certified.data(), &action);
        assert_eq!(certified.auth_sig().signatures.len(), 1);
        assert!(certified
            .auth_sig()
            .signatures
            .contains_key(&authorities[0].pubkey_bytes()));
    }

    #[tokio::test]
    async fn test_bridge_auth_agg_error() {
        telemetry_subscribers::init_for_testing();

        let mock = BridgeRequestMockHandler::new();

        // start server with single authority
        let (_handles, authorities, _secrets) = get_test_authorities_and_run_mock_bridge_server(
            vec![BRIDGE_COMMITTEE_MAXIMAL_VOTING_POWER],
            vec![mock.clone()],
        );

        let committee = BridgeCommittee::new(authorities.clone()).unwrap();
        let agg = BridgeAuthorityAggregator::new_for_testing(Arc::new(committee));

        let starcoin_bridge_tx_digest = TransactionDigest::random();
        let starcoin_bridge_tx_event_index = 0;
        let nonce = 0;
        let amount = 1000;
        let action = get_test_starcoin_bridge_to_eth_bridge_action(
            Some(starcoin_bridge_tx_digest),
            Some(starcoin_bridge_tx_event_index),
            Some(nonce),
            Some(amount),
            None,
            None,
            None,
        );

        // Authority returns error
        mock.add_starcoin_bridge_event_response(
            starcoin_bridge_tx_digest,
            starcoin_bridge_tx_event_index,
            Err(BridgeError::RestAPIError("test error".into())),
            None,
        );
        let err = agg
            .request_committee_signatures(action.clone())
            .await
            .unwrap_err();
        assert!(matches!(err, BridgeError::RestAPIError(_)));
    }
}
