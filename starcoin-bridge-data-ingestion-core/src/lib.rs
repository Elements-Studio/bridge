// Stub for starcoin-bridge-data-ingestion-core - to be replaced with Starcoin data ingestion
#![allow(dead_code, unused_variables, unused_imports)]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// TODO: Replace with Starcoin metrics implementation
#[derive(Clone, Debug)]
pub struct DataIngestionMetrics;

// TODO: Replace with Starcoin indexer executor
pub struct IndexerExecutor {
    // Placeholder fields
}

impl IndexerExecutor {
    pub fn new(
        _progress_store: impl ProgressStore,
        _initial_checkpoint: u64,
        _metrics: DataIngestionMetrics,
    ) -> Self {
        Self {}
    }

    pub async fn register(&mut self, _worker_pool: WorkerPool) -> anyhow::Result<()> {
        unimplemented!("TODO: Implement Starcoin IndexerExecutor::register")
    }

    pub async fn run(
        &mut self,
        _checkpoint_path: std::path::PathBuf,
        _remote_store_url: Option<String>,
        _remote_store_options: Vec<(String, String)>,
        _reader_options: ReaderOptions,
        _exit_receiver: tokio::sync::oneshot::Receiver<()>,
    ) -> anyhow::Result<()> {
        unimplemented!("TODO: Implement Starcoin IndexerExecutor::run")
    }
}

// TODO: Replace with Starcoin progress store
#[async_trait]
pub trait ProgressStore: Send + Sync {
    async fn load(&mut self, task_name: String) -> Result<u64, anyhow::Error>;
    async fn save(&mut self, task_name: String, checkpoint_number: u64) -> anyhow::Result<()>;
}

// TODO: Replace with Starcoin reader options
#[derive(Clone, Debug, Default)]
pub struct ReaderOptions {
    pub batch_size: usize,
}

// TODO: Replace with Starcoin worker trait
#[async_trait]
pub trait Worker: Send + Sync {
    type Result: Send;

    async fn process_checkpoint(
        &self,
        checkpoint: &starcoin_bridge_types::full_checkpoint_content::CheckpointData,
    ) -> anyhow::Result<()>;
}

// TODO: Replace with Starcoin worker pool
pub struct WorkerPool {
    // Placeholder fields
}

impl WorkerPool {
    pub fn new<W: Worker>(_worker: W, _task_name: String, _concurrency: usize) -> Self {
        Self {}
    }
}
