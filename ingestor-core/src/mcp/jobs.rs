use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use serde::Serialize;
use crate::mcp::tools::IndexStatsOutput;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Complete { index_id: String, stats: IndexStatsOutput, warnings: usize },
    Error { message: String },
}

#[derive(Debug, Clone)]
pub struct JobState {
    pub status: JobStatus,
    pub started_at: Instant,
}

impl JobState {
    pub fn queued() -> Self {
        Self { status: JobStatus::Queued, started_at: Instant::now() }
    }
}

/// Maximum concurrent indexing jobs to prevent resource exhaustion.
pub const MAX_CONCURRENT_JOBS: usize = 4;

pub type JobStore = Arc<Mutex<HashMap<String, JobState>>>;

pub fn new_job_store() -> JobStore {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Count jobs that are still queued or running.
///
/// Returns 0 on mutex poison (saturating safe: refuses new jobs only when healthy).
pub fn active_job_count(store: &JobStore) -> usize {
    store.lock().map(|g| g.values().filter(|s| {
        matches!(s.status, JobStatus::Queued | JobStatus::Running)
    }).count()).unwrap_or(0)
}

pub fn new_job_id() -> String {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).expect("getrandom failed");
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}
