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

    // Get resource at address
    pub async fn get_resource(
        &self,
        address: &str,
        resource_type: &str,
    ) -> Result<Option<Value>> {
        let result = self
            .call("state.get_resource", vec![json!(address), json!(resource_type)])
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
        // Resource type: 0x1::account::Account
        let resource = self.get_resource(address, "0x1::account::Account").await?;
        
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
    pub async fn sign_and_submit_transaction(
        &self,
        key: &starcoin_bridge_types::crypto::StarcoinKeyPair,
        raw_txn: starcoin_bridge_types::transaction::RawUserTransaction,
    ) -> Result<String> {
        use starcoin_bridge_types::crypto::StarcoinKeyPair;
        use fastcrypto::hash::{HashFunction, Sha3_256};
        use fastcrypto::traits::{KeyPair as FastcryptoKeyPair, ToFromBytes, Signer};
        
        // Serialize the raw transaction
        let raw_txn_bytes = bcs::to_bytes(&raw_txn)
            .map_err(|e| anyhow!("Failed to serialize raw transaction: {}", e))?;
        
        // Hash the raw transaction for signing
        // Starcoin uses a specific prefix for transaction signing
        let prefix = Sha3_256::digest(b"STARCOIN::RawUserTransaction");
        let mut to_sign = prefix.digest.to_vec();
        to_sign.extend_from_slice(&raw_txn_bytes);
        let txn_hash = Sha3_256::digest(&to_sign);
        
        // Sign and get public key based on key type
        let (public_key_bytes, signature_bytes) = match key {
            StarcoinKeyPair::Ed25519(kp) => {
                // Use the Signer trait to sign
                let sig: fastcrypto::ed25519::Ed25519Signature = kp.sign(&txn_hash.digest);
                (kp.public().as_bytes().to_vec(), sig.as_ref().to_vec())
            }
            _ => return Err(anyhow!("Only Ed25519 keys are supported")),
        };
        
        // Build SignedUserTransaction
        // Format: raw_txn + authenticator
        // Authenticator format for Ed25519: scheme(1 byte) + pubkey(32 bytes) + signature(64 bytes)
        let mut signed_txn_bytes = raw_txn_bytes.clone();
        signed_txn_bytes.push(0u8); // Ed25519 scheme = 0
        signed_txn_bytes.extend_from_slice(&public_key_bytes);
        signed_txn_bytes.extend_from_slice(&signature_bytes);
        
        // Convert to hex and submit
        let signed_txn_hex = hex::encode(&signed_txn_bytes);
        
        let result = self.submit_transaction(&signed_txn_hex).await?;
        
        // Return transaction hash
        let txn_hash_str = result.as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{:?}", result));
        
        Ok(txn_hash_str)
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
