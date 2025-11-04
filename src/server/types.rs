use alloy::rpc::types::EIP1186AccountProofResponse;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use crate::server::queue::JobQueue;

#[derive(Serialize)]
pub struct JobResponse {
    pub job_id: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ProofInput {
    pub network: String,
    pub amount: String,
    pub broadcaster_fee: String,
    pub prover_fee: String,
    pub spend: String,
    pub burn_key: String,
    pub wallet_address: String,
    pub proof: Option<EIP1186AccountProofResponse>,
    pub block_number: Option<u64>,
}

#[derive(Serialize, Debug, Clone)]
pub struct ProofOutput {
    pub burn_address: String,
    pub proof: Value,
    pub block_number: u64,
    pub nullifier_u256: String,
    pub remaining_coin: String,
    pub broadcaster_fee: String,
    pub prover_fee: String,
    pub prover: String,
    pub reveal_amount: String,
    pub wallet_address: String,
}

#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
}

#[derive(Clone)]
pub struct AppState {
    pub jobs: Arc<DashMap<Uuid, JobStatus>>,
    pub job_queue: JobQueue,
    pub params_dir: std::path::PathBuf,
}

#[derive(Clone)]
pub enum JobStatus {
    Pending,
    InProgress,
    CompletedProof { result: ProofOutput },
    Failed(String),
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PollResponse {
    Pending,
    InProgress,
    Completed { result: ProofOutput },
    Failed { error: String },
}
