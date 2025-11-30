// Simple async JSON-RPC client for Starcoin
// Replaces the heavy starcoin-rpc-client to avoid tokio runtime conflicts
// Uses HTTP JSON-RPC (default port 9850)

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug)]
pub struct SimpleStarcoinRpcClient {
    http_client: reqwest::Client,
    rpc_url: String,
    request_id: std::sync::Arc<AtomicU64>,
    bridge_address: String,
}

#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    params: Vec<Value>,
    id: u64,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    result: Option<Value>,
    error: Option<JsonRpcError>,
    #[allow(dead_code)]
    id: u64,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

impl SimpleStarcoinRpcClient {
    pub fn new(rpc_url: impl Into<String>, bridge_address: impl Into<String>) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            rpc_url: rpc_url.into(),
            request_id: std::sync::Arc::new(AtomicU64::new(1)),
            bridge_address: bridge_address.into(),
        }
    }

    /// Get the bridge contract address
    pub fn bridge_address(&self) -> &str {
        &self.bridge_address
    }

    async fn call(&self, method: &str, params: Vec<Value>) -> Result<Value> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id,
        };

        let response = self
            .http_client
            .post(&self.rpc_url)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP error: {} - {}",
                response.status(),
                response.text().await?
            ));
        }

        let response_text = response.text().await?;
        let rpc_response: JsonRpcResponse = serde_json::from_str(&response_text)?;

        if let Some(error) = rpc_response.error {
            // Log request and response only on error
            tracing::warn!(
                "RPC error - Request: {} | Response: {}",
                serde_json::to_string(&request).unwrap_or_default(),
                &response_text
            );
            return Err(anyhow!(
                "RPC error {}: {}",
                error.code,
                error.message
            ));
        }

        // Return the result, which may be null (valid for queries that return Option)
        Ok(rpc_response.result.unwrap_or(Value::Null))
    }

    // Chain info
    pub async fn chain_info(&self) -> Result<Value> {
        self.call("chain.info", vec![]).await
    }

    // Node info
    pub async fn node_info(&self) -> Result<Value> {
        self.call("node.info", vec![]).await
    }

    /// Get the Starcoin network chain ID from node.info
    /// This is the transaction chain_id (e.g., 254 for dev, 251 for halley, 1 for main)
    pub async fn get_chain_id(&self) -> Result<u8> {
        let node_info = self.node_info().await?;
        
        // Parse net from node_info response, format is like "dev" or chain id number
        // Try to get from self_address which contains chain id, or from net field
        let chain_id = node_info
            .get("net")
            .and_then(|n| n.as_str())
            .and_then(|net| {
                match net.to_lowercase().as_str() {
                    "dev" => Some(254u8),
                    "halley" => Some(253u8),
                    "proxima" => Some(252u8),
                    "barnard" => Some(251u8),
                    "main" => Some(1u8),
                    _ => net.parse::<u8>().ok(),
                }
            })
            .ok_or_else(|| anyhow!("Failed to parse chain_id from node info"))?;
        
        Ok(chain_id)
    }

    /// Get the current block time in seconds from genesis
    /// Uses node.info.now_seconds which is what Starcoin uses for transaction expiration
    pub async fn get_block_timestamp(&self) -> Result<u64> {
        let node_info = self.node_info().await?;
        
        // Parse now_seconds from node_info response
        let now_seconds = node_info
            .get("now_seconds")
            .and_then(|t| t.as_u64())
            .or_else(|| {
                node_info
                    .get("now_seconds")
                    .and_then(|t| t.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .ok_or_else(|| anyhow!("Failed to parse now_seconds from node info"))?;
        
        // Return in milliseconds for compatibility with existing code
        Ok(now_seconds * 1000)
    }

    // Get resource at address (with decode option for json format)
    pub async fn get_resource(
        &self,
        address: &str,
        resource_type: &str,
    ) -> Result<Option<Value>> {
        let result = self
            .call("state.get_resource", vec![json!(address), json!(resource_type), json!({"decode": true})])
            .await?;
        
        if result.is_null() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    // Get account state
    pub async fn get_account(&self, address: &str) -> Result<Option<Value>> {
        let result = self
            .call("state.get_account", vec![json!(address)])
            .await?;
        
        if result.is_null() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    // Get account sequence number
    // First try txpool.next_sequence_number, if null then query state.get_resource
    pub async fn get_sequence_number(&self, address: &str) -> Result<u64> {
        // Try txpool first - returns the next sequence number including pending txs
        let result = self
            .call("txpool.next_sequence_number", vec![json!(address)])
            .await?;
        
        // If txpool returns a number, use it
        if let Some(seq) = result.as_u64() {
            return Ok(seq);
        }
        
        // Otherwise, query the on-chain account resource for sequence_number
        // Starcoin uses full module path: 0x00000000000000000000000000000001::Account::Account
        let resource = self.get_resource(address, "0x00000000000000000000000000000001::Account::Account").await?;
        
        match resource {
            Some(res) => {
                // The resource has a "json" field with the decoded struct
                // Format: {"json": {"sequence_number": 123, ...}, "raw": "0x..."}
                let seq = res.get("json")
                    .and_then(|j| j.get("sequence_number"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                Ok(seq)
            }
            None => Ok(0), // Account doesn't exist, start from 0
        }
    }

    // Query events by transaction hash
    pub async fn get_events_by_txn_hash(&self, txn_hash: &str) -> Result<Vec<Value>> {
        let result = self
            .call("chain.get_events_by_txn_hash", vec![json!(txn_hash)])
            .await?;
        
        Ok(serde_json::from_value(result)?)
    }

    // Query events with filter
    // Starcoin RPC format: chain.get_events(filter)
    // filter: { from_block, to_block, event_keys, addrs, type_tags, limit }
    pub async fn get_events(&self, filter: Value) -> Result<Vec<Value>> {
        let result = self
            .call("chain.get_events", vec![filter])
            .await?;
        
        Ok(serde_json::from_value(result)?)
    }

    // Get transaction
    pub async fn get_transaction(&self, txn_hash: &str) -> Result<Value> {
        self.call("chain.get_transaction", vec![json!(txn_hash)])
            .await
    }

    // Submit transaction
    pub async fn submit_transaction(&self, signed_txn: &str) -> Result<Value> {
        self.call("txpool.submit_hex_transaction", vec![json!(signed_txn)])
            .await
    }

    /// Sign a RawUserTransaction and submit it to the network
    /// Uses Starcoin native types for correct BCS serialization
    pub async fn sign_and_submit_transaction(
        &self,
        key: &starcoin_bridge_types::crypto::StarcoinKeyPair,
        raw_txn: starcoin_bridge_types::transaction::RawUserTransaction,
    ) -> Result<String> {
        use starcoin_bridge_types::crypto::StarcoinKeyPair;
        use starcoin_crypto::ed25519::{Ed25519PrivateKey, Ed25519PublicKey};
        use starcoin_vm_types::account_address::AccountAddress;
        use starcoin_vm_types::transaction::{
            RawUserTransaction as NativeRawUserTransaction,
            TransactionPayload as NativeTransactionPayload,
            ScriptFunction,
        };
        use starcoin_vm_types::genesis_config::ChainId as NativeChainId;
        use starcoin_vm_types::identifier::Identifier;
        use starcoin_vm_types::language_storage::{ModuleId, TypeTag};
        
        // Convert our RawUserTransaction to Starcoin native RawUserTransaction
        // StarcoinAddress is [u8; 16], AccountAddress::new expects [u8; 16]
        let sender = AccountAddress::new(*raw_txn.sender);
        
        // Convert payload - need to rebuild with starcoin_vm_types types
        let native_payload = match &raw_txn.payload {
            starcoin_bridge_types::transaction::TransactionPayload::ScriptFunction(sf) => {
                // Rebuild ModuleId with starcoin_vm_types types
                let module_addr = AccountAddress::new(**sf.module.address());
                let module_name = Identifier::new(sf.module.name().as_str())
                    .map_err(|e| anyhow!("Invalid module name: {:?}", e))?;
                let native_module = ModuleId::new(module_addr, module_name);
                
                let function_name = Identifier::new(sf.function.as_str())
                    .map_err(|e| anyhow!("Invalid function name: {:?}", e))?;
                
                // Convert type args - they should be compatible via BCS
                let native_ty_args: Vec<TypeTag> = sf.ty_args.iter()
                    .map(|t| {
                        // Serialize and deserialize to convert between move_core_types versions
                        let bytes = bcs::to_bytes(t).unwrap();
                        bcs_ext::from_bytes(&bytes).unwrap()
                    })
                    .collect();
                
                NativeTransactionPayload::ScriptFunction(ScriptFunction::new(
                    native_module,
                    function_name,
                    native_ty_args,
                    sf.args.clone(),
                ))
            }
            _ => return Err(anyhow!("Only ScriptFunction payload is supported")),
        };
        
        let native_raw_txn = NativeRawUserTransaction::new_with_default_gas_token(
            sender,
            raw_txn.sequence_number,
            native_payload,
            raw_txn.max_gas_amount,
            raw_txn.gas_unit_price,
            raw_txn.expiration_timestamp_secs,
            NativeChainId::new(raw_txn.chain_id.0),
        );
        
        // Get Ed25519 private key bytes and create Starcoin Ed25519PrivateKey
        let (public_key_bytes, private_key_bytes) = match key {
            StarcoinKeyPair::Ed25519(kp) => {
                use fastcrypto::traits::{KeyPair as FastcryptoKeyPair, ToFromBytes};
                let priv_bytes = kp.as_bytes()[..32].to_vec(); // Ed25519 private key is first 32 bytes
                let pub_bytes = kp.public().as_bytes().to_vec();
                (pub_bytes, priv_bytes)
            }
            _ => return Err(anyhow!("Only Ed25519 keys are supported for Starcoin")),
        };
        
        // Create Starcoin native Ed25519 keys
        let private_key = Ed25519PrivateKey::try_from(private_key_bytes.as_slice())
            .map_err(|e| anyhow!("Invalid Ed25519 private key: {:?}", e))?;
        let public_key = Ed25519PublicKey::try_from(public_key_bytes.as_slice())
            .map_err(|e| anyhow!("Invalid Ed25519 public key: {:?}", e))?;
        
        // Sign using Starcoin's native signing
        let signed_txn = native_raw_txn.sign(&private_key, public_key)
            .map_err(|e| anyhow!("Failed to sign transaction: {:?}", e))?
            .into_inner();
        
        // Serialize using BCS
        let signed_txn_bytes = bcs_ext::to_bytes(&signed_txn)
            .map_err(|e| anyhow!("Failed to serialize signed transaction: {}", e))?;
        
        // Convert to hex and submit
        let signed_txn_hex = hex::encode(&signed_txn_bytes);
        
        tracing::debug!("Submitting transaction hex (len={}): {}...", 
            signed_txn_hex.len(), 
            &signed_txn_hex[..std::cmp::min(100, signed_txn_hex.len())]);
        
        let result = self.submit_transaction(&signed_txn_hex).await?;
        
        // Return transaction hash
        let txn_hash_str = result.as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{:?}", result));
        
        Ok(txn_hash_str)
    }

    /// Sign, submit and wait for transaction confirmation
    pub async fn sign_and_submit_and_wait_transaction(
        &self,
        key: &starcoin_bridge_types::crypto::StarcoinKeyPair,
        raw_txn: starcoin_bridge_types::transaction::RawUserTransaction,
    ) -> Result<String> {
        let txn_hash = self.sign_and_submit_transaction(key, raw_txn).await?;
        
        // Poll for transaction confirmation (max 30 seconds)
        for _ in 0..60 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if let Ok(txn_info) = self.get_transaction_info(&txn_hash).await {
                if !txn_info.is_null() {
                    tracing::info!(?txn_hash, "Transaction confirmed on chain");
                    return Ok(txn_hash);
                }
            }
        }
        
        Err(anyhow!("Transaction {} not confirmed after 30 seconds timeout", txn_hash))
    }

    // Dry run transaction
    pub async fn dry_run_transaction(&self, signed_txn: &str) -> Result<Value> {
        self.call("contract.dry_run", vec![json!(signed_txn)])
            .await
    }
    
    // Get gas price (estimate from recent blocks)
    pub async fn get_gas_price(&self) -> Result<u64> {
        // Starcoin doesn't have dynamic gas price, return default
        Ok(1)
    }

    /// Get the Bridge resource from chain state
    /// Uses state.get_resource RPC to read the Bridge struct directly
    pub async fn get_latest_bridge(&self) -> Result<Value> {
        // Resource type: {bridge_address}::Bridge::Bridge
        let resource_type = format!("{}::Bridge::Bridge", self.bridge_address);
        
        // Call state.get_resource to read the Bridge struct
        self.call("state.get_resource", vec![
            json!(&self.bridge_address),
            json!(resource_type),
            json!({"decode": true})
        ]).await
    }

    /// Call a Move contract function (read-only)
    /// function_id format: "0xADDRESS::MODULE::FUNCTION"
    /// type_args: vector of type tag strings like "0x1::STC::STC"
    /// args: vector of TransactionArgument hex strings
    pub async fn call_contract(
        &self,
        function_id: &str,
        type_args: Vec<String>,
        args: Vec<String>,
    ) -> Result<Value> {
        let contract_call = json!({
            "function_id": function_id,
            "type_args": type_args,
            "args": args
        });
        self.call("contract.call_v2", vec![contract_call]).await
    }

    /// Execute transaction and return the result
    pub async fn submit_and_wait_transaction(&self, signed_txn_hex: &str) -> Result<Value> {
        // Submit transaction
        let txn_hash = self.submit_transaction(signed_txn_hex).await?;
        let txn_hash_str = txn_hash.as_str()
            .ok_or_else(|| anyhow!("Invalid transaction hash response"))?;
        
        // Poll for transaction info (simple polling with retries)
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            if let Ok(txn_info) = self.get_transaction_info(txn_hash_str).await {
                if !txn_info.is_null() {
                    return Ok(txn_info);
                }
            }
        }
        
        Err(anyhow!("Transaction not confirmed after timeout"))
    }

    /// Get transaction info
    pub async fn get_transaction_info(&self, txn_hash: &str) -> Result<Value> {
        self.call("chain.get_transaction_info", vec![json!(txn_hash)])
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_chain_info() {
        let client = SimpleStarcoinRpcClient::new(
            "http://127.0.0.1:9850",
            "0x0000000000000000000000000000dead", // dummy address for test
        );
        let result = client.chain_info().await;
        println!("{:?}", result);
    }
}
