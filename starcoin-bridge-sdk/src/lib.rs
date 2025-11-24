// Wrapper around Starcoin RPC client for bridge compatibility
#![allow(dead_code, unused_variables, unused_imports)]

use anyhow::Result;
use starcoin_rpc_api::bridge::BridgeCommittee;
use starcoin_rpc_client::{AsyncRpcClient, ConnSource};
use starcoin_bridge_types::bridge::{BridgeSummary, BridgeTreasurySummary};

// Sub-modules
pub mod apis;
pub mod error;

// StarcoinClient wraps Starcoin's AsyncRpcClient
// Note: AsyncRpcClient doesn't implement Clone, so we wrap it in Arc
pub struct StarcoinClient {
    client: std::sync::Arc<AsyncRpcClient>,
}

impl Clone for StarcoinClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
        }
    }
}

impl StarcoinClient {
    // Create a new StarcoinClient from a Starcoin RPC client
    pub fn new(client: AsyncRpcClient) -> Self {
        Self {
            client: std::sync::Arc::new(client),
        }
    }

    // Get read API interface
    pub fn read_api(&self) -> ReadApi {
        ReadApi {
            client: self.client.clone(),
        }
    }

    // Get governance API interface
    pub fn governance_api(&self) -> GovernanceApi {
        GovernanceApi {
            client: self.client.clone(),
        }
    }

    // Get coin read API interface
    pub fn coin_read_api(&self) -> apis::CoinReadApi {
        apis::CoinReadApi::new()
    }

    // Get event API interface
    pub fn event_api(&self) -> EventApi {
        EventApi {
            client: self.client.clone(),
        }
    }

    // Get Bridge Read API interface
    pub fn http(&self) -> BridgeReadApi {
        BridgeReadApi {
            client: self.client.clone(),
        }
    }

    // Get quorum driver API (stub)
    pub fn quorum_driver_api(&self) -> QuorumDriverApi {
        QuorumDriverApi {
            client: self.client.clone(),
        }
    }

    // Get the underlying Starcoin client
    pub fn starcoin_client(&self) -> &AsyncRpcClient {
        &self.client
    }
}

impl std::fmt::Debug for StarcoinClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StarcoinClient").finish()
    }
}

// ReadApi provides read-only access to blockchain data
pub struct ReadApi {
    client: std::sync::Arc<AsyncRpcClient>,
}

impl ReadApi {
    // Get the underlying Starcoin client
    pub fn starcoin_client(&self) -> &AsyncRpcClient {
        &self.client
    }

    // Get the latest checkpoint sequence number (block number in Starcoin)
    // TODO: Query actual latest block number from Starcoin node  
    pub async fn get_latest_checkpoint_sequence_number(&self) -> Result<u64> {
        // Return dummy value for now
        // In production, should call chain.info() to get latest block number
        Ok(0)
    }

    // Get bridge summary
    // TODO: Implement actual bridge committee query from Starcoin
    pub async fn get_bridge_summary(&self) -> Result<starcoin_bridge_types::bridge::BridgeSummary> {
        // Return a minimal bridge summary for testing
        // In production, this should query the actual bridge state from Starcoin
        use starcoin_bridge_types::bridge::*;
        Ok(BridgeSummary {
            committee: BridgeCommitteeSummary {
                members: vec![],
                member_registration: vec![],
                last_committee_update_epoch: 0,
            },
            treasury: BridgeTreasurySummary::default(),
            bridge_version: 1,
            message_version: 1,
            chain_id: 2, // StarcoinCustom as configured
            sequence_nums: Default::default(),
            bridge_records_id: Default::default(),
            limiter: Default::default(),
            is_frozen: false,
        })
    }

    // Dev inspect transaction block
    pub async fn dev_inspect_transaction_block(
        &self,
        sender: [u8; 32],
        tx_kind: starcoin_bridge_types::transaction::TransactionKind,
        gas_price: Option<u64>,
        epoch: Option<u64>,
    ) -> Result<starcoin_bridge_json_rpc_types::DevInspectResults> {
        let sender_hex = hex::encode(sender);
        let tx_data = serde_json::to_value(&tx_kind)?;

        let result = self
            .client
            .bridge_dev_inspect_transaction_block(sender_hex, tx_data, gas_price, epoch)
            .await?;

        // Convert Starcoin DevInspectResult to Starcoin DevInspectResults
        Ok(starcoin_bridge_json_rpc_types::DevInspectResults {
            results: None, // Transaction simulation not fully implemented yet
            effects: Some(result.effects.to_string()),
        })
    }

    // Get chain identifier
    // TODO: Query actual chain info from Starcoin node
    pub async fn get_chain_identifier(&self) -> Result<String> {
        // Return a default chain identifier for now
        // In production, should call node_info() or similar RPC method
        Ok("starcoin-dev".to_string())
    }

    // Get object with options
    pub async fn get_object_with_options(
        &self,
        object_id: [u8; 32],
        options: starcoin_bridge_json_rpc_types::StarcoinObjectDataOptions,
    ) -> Result<starcoin_bridge_json_rpc_types::StarcoinObjectResponse> {
        let object_id_hex = hex::encode(object_id);
        let options_json = serde_json::to_value(&options)?;

        let result = self
            .client
            .bridge_get_object_with_options(object_id_hex, options_json)
            .await?;

        // Convert Starcoin ObjectResponse to Starcoin StarcoinObjectResponse
        // In Starcoin, objects are represented as resources at addresses
        Ok(starcoin_bridge_json_rpc_types::StarcoinObjectResponse { 
            data: result.data.map(|_| {
                // TODO: Properly convert object data when needed
                // For now, return None as this requires full object model conversion
                return None::<starcoin_bridge_json_rpc_types::StarcoinObjectData>;
            }).flatten()
        })
    }
}

// GovernanceApi provides governance-related access
pub struct GovernanceApi {
    client: std::sync::Arc<AsyncRpcClient>,
}

impl GovernanceApi {
    // Get the underlying Starcoin client
    pub fn starcoin_client(&self) -> &AsyncRpcClient {
        &self.client
    }

    // Get latest system state
    pub async fn get_latest_starcoin_bridge_system_state(
        &self,
    ) -> Result<starcoin_bridge_json_rpc_types::StarcoinSystemStateSummary> {
        let result = self.client.bridge_get_latest_system_state().await?;

        // Convert Starcoin SystemStateSummary to Starcoin format
        Ok(starcoin_bridge_json_rpc_types::StarcoinSystemStateSummary {
            epoch: result.epoch,
            protocol_version: 1,       // Starcoin doesn't have protocol versioning like Starcoin
            system_state_version: 1,   // Starcoin doesn't have system state versioning
            active_validators: vec![], // TODO: Properly convert validators when data is available
        })
    }

    // Get committee info
    pub async fn get_committee_info(
        &self,
        epoch: Option<u64>,
    ) -> Result<starcoin_bridge_json_rpc_types::StarcoinCommittee> {
        let result = self.client.bridge_get_committee_info(epoch).await?;

        // Convert Starcoin CommitteeInfo to Starcoin format
        // StarcoinCommittee uses a HashMap of public keys ([u8; 32]) to stake amounts
        use std::collections::HashMap;
        let mut validators: HashMap<[u8; 32], u64> = HashMap::new();
        for member in result.members {
            // TODO: Properly parse public key bytes from member.public_key string
            // For now, use placeholder zero array
            let pk_bytes = [0u8; 32];
            validators.insert(pk_bytes, member.stake);
        }
        
        Ok(starcoin_bridge_json_rpc_types::StarcoinCommittee {
            epoch: result.epoch,
            validators,
        })
    }

    // Get reference gas price
    pub async fn get_reference_gas_price(&self) -> Result<u64> {
        self.client.bridge_get_reference_gas_price().await
    }
}

// EventApi provides event query access
pub struct EventApi {
    client: std::sync::Arc<AsyncRpcClient>,
}

impl EventApi {
    // Get the underlying Starcoin client
    pub fn starcoin_client(&self) -> &AsyncRpcClient {
        &self.client
    }

    // Query events
    pub async fn query_events(
        &self,
        query: starcoin_bridge_json_rpc_types::EventFilter,
        cursor: Option<starcoin_bridge_types::event::EventID>,
        limit: Option<usize>,
        descending: bool,
    ) -> Result<starcoin_bridge_json_rpc_types::EventPage> {
        // Convert Starcoin EventFilter to Starcoin BridgeEventFilter
        let filter = starcoin_rpc_api::bridge::BridgeEventFilter {
            event_type: None, // TODO: map from query
            status: None,
        };

        // Convert EventID (tx_seq, event_seq) to cursor string
        // EventID is (u64, u64) representing (tx_seq, event_seq)
        let cursor_str = cursor.map(|(tx_seq, event_seq)| format!("{}:{}", tx_seq, event_seq));
        let result = self
            .client
            .bridge_query_events(filter, cursor_str, limit, descending)
            .await?;

        // Convert Starcoin BridgeEventPage to Starcoin format
        Ok(starcoin_bridge_json_rpc_types::EventPage {
            data: vec![], // TODO: convert result.events
            next_cursor: result.next_cursor,
            has_next_page: result.has_next_page,
        })
    }

    // Get events by transaction digest
    pub async fn get_events(&self, digest: &[u8; 32]) -> Result<Vec<starcoin_bridge_types::event::Event>> {
        let hash = starcoin_crypto::HashValue::new(*digest);
        let _result = self.client.bridge_get_events(hash).await?;

        // Convert Starcoin BridgeEvent to Starcoin Event
        // TODO: Implement proper type conversion
        Ok(vec![])
    }
}

// CoinReadApi provides coin-related read access
pub struct CoinReadApi {
    client: std::sync::Arc<AsyncRpcClient>,
}

impl CoinReadApi {
    // Get the underlying Starcoin client
    pub fn starcoin_client(&self) -> &AsyncRpcClient {
        &self.client
    }

    // Get total supply for a coin type
    pub async fn get_total_supply(&self, coin_type: &str) -> Result<starcoin_bridge_json_rpc_types::Supply> {
        // Call the Bridge RPC API to get total supply
        let supply = self.client.bridge_get_total_supply(coin_type.to_string()).await?;
        Ok(starcoin_bridge_json_rpc_types::Supply { value: supply.total })
    }
}

// QuorumDriverApi provides quorum driver access
pub struct QuorumDriverApi {
    client: std::sync::Arc<AsyncRpcClient>,
}

impl QuorumDriverApi {
    // Get the underlying Starcoin client
    pub fn starcoin_client(&self) -> &AsyncRpcClient {
        &self.client
    }

    // Execute transaction block
    pub async fn execute_transaction_block(
        &self,
        tx: starcoin_bridge_types::transaction::Transaction,
        options: starcoin_bridge_json_rpc_types::StarcoinTransactionBlockResponseOptions,
        request_type: starcoin_bridge_types::quorum_driver_types::ExecuteTransactionRequestType,
    ) -> Result<starcoin_bridge_json_rpc_types::StarcoinTransactionBlockResponse> {
        // Transaction doesn't implement Serialize, so we'll pass it as debug string
        // TODO: properly serialize transaction to bytes if needed
        let tx_str = format!("{:?}", tx);
        let tx_json = serde_json::Value::String(tx_str);
        let options_json = serde_json::to_value(&options)?;
        let request_type_str = format!("{:?}", request_type);

        let result = self
            .client
            .bridge_execute_transaction_block(tx_json, options_json, request_type_str)
            .await?;

        // Convert Starcoin TransactionBlockResponse to Starcoin format
        let digest_bytes: [u8; 32] = result.digest.to_vec().try_into().unwrap_or([0u8; 32]);
        Ok(starcoin_bridge_json_rpc_types::StarcoinTransactionBlockResponse {
            digest: Some(digest_bytes),
            effects: None,        // TODO: convert result.effects
            events: None,         // TODO: convert result.events
            object_changes: None, // TODO: track object changes
        })
    }
}

// BridgeReadApi provides bridge-specific read access
pub struct BridgeReadApi {
    client: std::sync::Arc<AsyncRpcClient>,
}

impl BridgeReadApi {
    // Get the underlying Starcoin client
    pub fn starcoin_client(&self) -> &AsyncRpcClient {
        &self.client
    }

    // Get chain identifier (returns network name)
    // TODO: This should query the actual Starcoin node for chain info
    pub async fn bridge_get_chain_identifier(&self) -> Result<String> {
        // For now, return a default identifier
        // In production, this should call node_info() and extract the chain ID
        Ok("starcoin-dev".to_string())
    }

    // Get latest checkpoint sequence number (equivalent to block number in Starcoin)
    // TODO: This should query the actual latest block number from Starcoin
    pub async fn bridge_get_latest_checkpoint_sequence_number(&self) -> Result<u64> {
        // For now, return a dummy value
        // In production, this should call chain.info() or similar
        Ok(0)
    }
}

// Implement BridgeReadApiClient for BridgeReadApi
#[async_trait::async_trait]
impl starcoin_bridge_json_rpc_api::BridgeReadApiClient for BridgeReadApi {
    async fn get_bridge_object_initial_shared_version(&self) -> Result<u64, eyre::Error> {
        self.client
            .bridge_get_bridge_object_initial_shared_version()
            .await
            .map_err(|e| eyre::eyre!(e))
    }

    async fn get_latest_bridge(&self) -> Result<starcoin_bridge_types::bridge::BridgeSummary, eyre::Error> {
        let result = self
            .client
            .bridge_get_latest_bridge()
            .await
            .map_err(|e| eyre::eyre!(e))?;

        // Convert Starcoin BridgeSummary to Starcoin format
        use starcoin_vm_types::bridge::bridge::*;
        use starcoin_vm_types::bridge::base_types::StarcoinAddress;
        
        // Convert committee members from RPC format to Move format
        let members: Vec<(Vec<u8>, MoveTypeCommitteeMember)> = result
            .committee
            .members
            .iter()
            .map(|member| {
                // Parse address as hex - Starcoin addresses are 16 bytes, pad to 32 for Starcoin
                let addr_str = member.address.trim_start_matches("0x");
                let starcoin_address: StarcoinAddress = if let Ok(bytes) = hex::decode(addr_str) {
                    // Try to parse as AccountAddress from hex string
                    member.address.parse().unwrap_or(StarcoinAddress::ZERO)
                } else {
                    StarcoinAddress::ZERO
                };
                
                // Parse public key as hex bytes
                let pubkey_bytes = hex::decode(member.public_key.trim_start_matches("0x"))
                    .unwrap_or_else(|_| vec![0u8; 33]);
                
                (
                    vec![0u8], // Member ID (using empty vec as placeholder)
                    MoveTypeCommitteeMember {
                        starcoin_bridge_address: starcoin_address,
                        bridge_pubkey_bytes: pubkey_bytes,
                        voting_power: member.stake,
                        http_rest_url: vec![], // Empty URL
                        blocklisted: false,
                    },
                )
            })
            .collect();
        
        Ok(BridgeSummary {
            committee: BridgeCommitteeSummary {
                members,
                member_registration: vec![],
                last_committee_update_epoch: 0,
            },
            treasury: BridgeTreasurySummary::default(),
            bridge_version: 1,
            message_version: 1,
            chain_id: 2, // StarcoinCustom as configured
            sequence_nums: Default::default(),
            bridge_records_id: Default::default(),
            limiter: Default::default(),
            is_frozen: false,
        })
    }
}

// StarcoinClientBuilder for constructing StarcoinClient instances
pub struct StarcoinClientBuilder {
    url: Option<String>,
}

impl StarcoinClientBuilder {
    // Create a new builder
    pub fn new() -> Self {
        Self { url: None }
    }

    // Set the RPC URL
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    // Build the StarcoinClient from a URL string (static method)
    pub async fn build_from_url(url: impl AsRef<str>) -> Result<StarcoinClient> {
        let url_str = url.as_ref();
        let conn_source = if url_str.starts_with("ws://") || url_str.starts_with("wss://") {
            ConnSource::WebSocket(url_str.to_string())
        } else if url_str.starts_with("http://") || url_str.starts_with("https://") {
            // For HTTP URLs, convert to WebSocket
            let ws_url = url_str
                .replace("http://", "ws://")
                .replace("https://", "wss://");
            ConnSource::WebSocket(ws_url)
        } else {
            // Assume it's an IPC path
            ConnSource::Ipc(std::path::PathBuf::from(url_str))
        };

        let client = AsyncRpcClient::new(conn_source).await?;
        Ok(StarcoinClient::new(client))
    }

    // Build with configured URL (instance method)
    pub async fn build(self) -> Result<StarcoinClient> {
        let url = self.url.ok_or_else(|| anyhow::anyhow!("URL not set"))?;
        Self::build_from_url(&url).await
    }
}

impl Default for StarcoinClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// Wallet context module
pub mod wallet_context {
    use super::*;

    // WalletContext wraps wallet functionality
    pub struct WalletContext {
        client: Option<StarcoinClient>,
        addresses: Vec<[u8; 32]>,
    }

    impl WalletContext {
        // Create a new wallet context
        pub fn new() -> Result<Self> {
            Ok(Self {
                client: None,
                addresses: Vec::new(),
            })
        }

        // Get the client
        pub fn get_client(&self) -> Result<&StarcoinClient> {
            self.client
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Client not set"))
        }

        // Get addresses
        pub fn get_addresses(&self) -> Vec<[u8; 32]> {
            self.addresses.clone()
        }

        // Get one gas object owned by address
        pub async fn get_one_gas_object_owned_by_address(
            &self,
            address: [u8; 32],
        ) -> Result<Option<([u8; 32], u64, [u8; 32])>> {
            // In Starcoin, gas is paid from account balance, not gas objects
            // Return None to indicate gas objects don't exist in Starcoin model
            log::warn!(
                "get_one_gas_object_owned_by_address called for {:?} - Starcoin uses account balance for gas",
                hex::encode(address)
            );
            Ok(None)
        }

        // Sign transaction
        pub async fn sign_transaction(
            &self,
            tx_data: &starcoin_bridge_types::transaction::TransactionData,
        ) -> Result<starcoin_bridge_types::crypto::Signature> {
            // For now, return an empty signature
            // Real implementation would need access to private keys
            log::warn!("sign_transaction called - returning empty signature (needs keystore integration)");
            Ok(starcoin_bridge_types::crypto::Signature(vec![]))
        }
    }

    impl Default for WalletContext {
        fn default() -> Self {
            Self {
                client: None,
                addresses: Vec::new(),
            }
        }
    }
}
