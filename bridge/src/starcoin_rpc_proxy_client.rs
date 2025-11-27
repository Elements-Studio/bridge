// Client wrapper for communicating with starcoin-rpc-proxy subprocess
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
enum RpcRequest {
    Connect { url: String },
    GetChainIdentifier,
    GetBridgeCommittee,
    GetBridgeSummary,
    GetLatestCheckpointSequenceNumber,
    Ping,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum RpcResponse {
    Success { result: serde_json::Value },
    Error { error: String },
}

pub struct StarcoinRpcProxyClient {
    process: Mutex<Option<Child>>,
    stdin: Mutex<Option<ChildStdin>>,
    stdout: Mutex<Option<BufReader<ChildStdout>>>,
}

impl StarcoinRpcProxyClient {
    pub fn spawn(proxy_bin_path: &str) -> Result<Self> {
        let mut child = Command::new(proxy_bin_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn starcoin-rpc-proxy: {}", e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to get stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to get stdout"))?;

        tracing::info!("Spawned starcoin-rpc-proxy subprocess (pid: {:?})", child.id());

        Ok(Self {
            process: Mutex::new(Some(child)),
            stdin: Mutex::new(Some(stdin)),
            stdout: Mutex::new(Some(BufReader::new(stdout))),
        })
    }

    fn send_request(&self, req: RpcRequest) -> Result<serde_json::Value> {
        let request_json = serde_json::to_string(&req)?;

        // Write request
        {
            let mut stdin_guard = self.stdin.lock().unwrap();
            let stdin = stdin_guard
                .as_mut()
                .ok_or_else(|| anyhow!("Stdin not available"))?;
            writeln!(stdin, "{}", request_json)?;
            stdin.flush()?;
        }

        // Read response
        let mut stdout_guard = self.stdout.lock().unwrap();
        let stdout = stdout_guard
            .as_mut()
            .ok_or_else(|| anyhow!("Stdout not available"))?;

        let mut response_line = String::new();
        stdout
            .read_line(&mut response_line)
            .map_err(|e| anyhow!("Failed to read response: {}", e))?;

        let response: RpcResponse = serde_json::from_str(&response_line)?;

        match response {
            RpcResponse::Success { result } => Ok(result),
            RpcResponse::Error { error } => Err(anyhow!("Proxy error: {}", error)),
        }
    }

    pub fn connect(&self, url: &str) -> Result<()> {
        let req = RpcRequest::Connect {
            url: url.to_string(),
        };
        self.send_request(req)?;
        Ok(())
    }

    pub fn get_chain_identifier(&self) -> Result<String> {
        let result = self.send_request(RpcRequest::GetChainIdentifier)?;
        Ok(serde_json::from_value(result)?)
    }

    pub fn get_bridge_committee(&self) -> Result<serde_json::Value> {
        self.send_request(RpcRequest::GetBridgeCommittee)
    }

    pub fn get_bridge_summary(&self) -> Result<serde_json::Value> {
        self.send_request(RpcRequest::GetBridgeSummary)
    }

    pub fn get_latest_checkpoint_sequence_number(&self) -> Result<u64> {
        let result = self.send_request(RpcRequest::GetLatestCheckpointSequenceNumber)?;
        Ok(serde_json::from_value(result)?)
    }

    pub fn ping(&self) -> Result<()> {
        self.send_request(RpcRequest::Ping)?;
        Ok(())
    }
}

impl Drop for StarcoinRpcProxyClient {
    fn drop(&mut self) {
        // Kill the child process when this client is dropped
        if let Ok(mut guard) = self.process.lock() {
            if let Some(mut child) = guard.take() {
                tracing::info!("Killing starcoin-rpc-proxy subprocess");
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}
