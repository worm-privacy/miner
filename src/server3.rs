use crate::logic::compute_proof;
use axum::{
    Json, Router,
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use uuid::Uuid;
// use crate::fp::{Fp, FpRepr};
use crate::constants::{
    poseidon_burn_address_prefix, poseidon_coin_prefix, poseidon_nullifier_prefix,
};
use crate::utils::{RapidsnarkOutput, find_burn_key, generate_burn_address};
use alloy::{
    eips::BlockId,
    hex::ToHexExt,
    network::TransactionBuilder,
    primitives::{
        Address, U256,
        utils::{format_ether, parse_ether},
    },
    providers::Provider,
    rpc::types::TransactionRequest,
};
use serde_json::Value;
use tokio::task::JoinHandle;

use anyhow::anyhow;
use std::str::FromStr;
//  use super::CommonOpt;
// use crate::cli::utils::{
//     append_new_entry, burn_file, check_required_files, coins_file, init_coins_file, next_id,
// };
// use crate::constants::{
//     poseidon_burn_address_prefix, poseidon_coin_prefix, poseidon_nullifier_prefix,
// };
use crate::fp::{Fp, FpRepr};
use crate::poseidon::{poseidon2, poseidon3};
// use crate::utils::{RapidsnarkOutput, find_burn_key, generate_burn_address};
use alloy::rlp::Encodable;
// use alloy::{
//     eips::BlockId,
//     hex::ToHexExt,
//     network::TransactionBuilder,
//     primitives::{
//         U256,
//         utils::{format_ether, parse_ether},
//     },
//     providers::Provider,
//     rpc::types::TransactionRequest,
// };
// use anyhow::anyhow;
// #[derive(Deserialize)]
// struct BurnRequest {
//     amount: String,
//     fee: String,
//     spend: String,
//     wallet_address: String,
// }
// async fn run_burn_job(
//     amount: String,
//     fee: String,
//     spend: String,
//     wallet_address: String,
//     _params_dir: &std::path::Path,
// ) -> Result<String, anyhow::Error> {
//     let fee = parse_ether(&fee)?;
//     let spend = parse_ether(&spend)?;
//     let amount = parse_ether(&amount)?;

//     let wallet_address = Address::from_str(&wallet_address.trim())?;

//     if amount > parse_ether("1")? {
//         return Err(anyhow!("Can't burn more than 1 ETH in a single call!"));
//     }

//     if fee + spend > amount {
//         return Err(anyhow!("Sum of fee and spend must be <= amount"));
//     }

//     // Generate burn_key
//     println!("Generating burn key...");
//     let burn_key = find_burn_key(3, wallet_address, fee);
//     let burn_key_str = U256::from_le_bytes(burn_key.to_repr().0).to_string();
//     println!("finished,{}", burn_key_str);

//     Ok(burn_key_str)
// }


// GET /poll_burn/:job_id
// async fn poll_burn(Path(job_id): Path<String>, State(state): State<AppState>) -> impl IntoResponse {
//     match Uuid::parse_str(&job_id) {
//         Ok(uuid) => match state.jobs.get(&uuid) {
//             Some(job) => match job.value() {
//                 JobStatus::Pending => Json(PollResponse::Pending),
//                 JobStatus::CompletedBurn { burn_address } => Json(PollResponse::Completed {
//                     burn_address: burn_address.clone(),
//                 }),
//                 JobStatus::Failed(error) => Json(PollResponse::Failed {
//                     error: error.clone(),
//                 }),
//                 _ => Json(PollResponse::Failed {
//                     error: "Job is not a burn job".to_string(),
//                 }),
//             },
//             None => Json(PollResponse::Failed {
//                 error: "Job not found".to_string(),
//             }),
//         },
//         Err(_) => Json(PollResponse::Failed {
//             error: "Invalid job_id".to_string(),
//         }),
//     }
// }


// // POST /start_burn
// async fn start_burn(
//     State(state): State<AppState>,
//     Json(payload): Json<BurnRequest>,
// ) -> impl IntoResponse {
//     let job_id = Uuid::new_v4();
//     state.jobs.insert(job_id, JobStatus::Pending);

//     let jobs = state.jobs.clone();
//     let params_dir = state.params_dir.clone();

//     tokio::spawn(async move {
//         let result = run_burn_job(
//             payload.amount,
//             payload.fee,
//             payload.spend,
//             payload.wallet_address,
//             &params_dir,
//         )
//         .await;

//         match result {
//             Ok(burn_address) => {
//                 jobs.insert(job_id, JobStatus::CompletedBurn { burn_address });
//             }
//             Err(e) => {
//                 jobs.insert(job_id, JobStatus::Failed(e.to_string()));
//             }
//         }
//     });

//     Json(JobResponse {
//         job_id: job_id.to_string(),
//     })
// }
use ff::PrimeField;
use std::process::Command;
use structopt::StructOpt;

// #[derive(Debug, Clone)]
// enum JobStatus {
//     Pending,
//     CompletedBurn { burn_address: String },
//     CompletedProof { result: ProofOutput },
//     Failed(String),
// }
type JobMap = Arc<DashMap<Uuid, JobStatus>>;

// #[derive(Clone)]
// struct AppState {
//     jobs: JobMap,
//     params_dir: std::path::PathBuf,
// }



#[derive(Serialize)]
struct JobResponse {
    job_id: String,
}

// #[derive(Serialize)]
// #[serde(rename_all = "snake_case")]
// enum PollResponse {
//     Pending,
//     Completed { burn_address: String },
//     ProofResult(ProofOutput),
//     Failed { error: String },
// }

#[derive(Deserialize, Debug, Clone)]
pub struct ProofInput {
    pub network: String,
    pub amount: String,
    pub fee: String,
    pub spend: String,
    pub burn_key: String,
    pub wallet_address: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct ProofOutput {
    pub burn_address: String,
    pub proof: Value,
    pub block_number: u64,
    pub nullifier_u256: String,
    pub remaining_coin: String,
    pub fee: String,
    pub spend: String,
    pub wallet_address: String,
}






// async fn start_proof(
//     State(state): State<AppState>,
//     Json(payload): Json<ProofInput>,
// ) -> impl IntoResponse {
//     let job_id = Uuid::new_v4();
//     state.jobs.insert(job_id, JobStatus::Pending);

//     let jobs = state.jobs.clone();

//     tokio::spawn(async move {
//         let result = compute_proof(payload).await;

//         match result {
//             Ok(proof_output) => {
//                 jobs.insert(
//                     job_id,
//                     JobStatus::CompletedProof {
//                         result: proof_output,
//                     },
//                 );
//             }
//             Err(e) => {
//                 jobs.insert(job_id, JobStatus::Failed(e.to_string()));
//             }
//         }
//     });

//     Json(JobResponse {
//         job_id: job_id.to_string(),
//     })
// }

// GET /poll_proof/:job_id
// async fn poll_proof(
//     Path(job_id): Path<String>,
//     State(state): State<AppState>,
// ) -> impl IntoResponse {
//     match Uuid::parse_str(&job_id) {
//         Ok(uuid) => match state.jobs.get(&uuid) {
//             Some(job) => match job.value() {
//                 JobStatus::Pending => Json(PollResponse::Pending),
//                 JobStatus::CompletedProof { result } => {
//                     Json(PollResponse::ProofResult(result.clone()))
//                 }
//                 JobStatus::Failed(error) => Json(PollResponse::Failed {
//                     error: error.clone(),
//                 }),
//                 _ => Json(PollResponse::Failed {
//                     error: "Job is not a proof job".to_string(),
//                 }),
//             },
//             None => Json(PollResponse::Failed {
//                 error: "Job not found".to_string(),
//             }),
//         },
//         Err(_) => Json(PollResponse::Failed {
//             error: "Invalid job_id".to_string(),
//         }),
//     }
// }
use anyhow::Result;
use tokio::net::TcpListener;
// use axum::{http::uri, routing::get, Router};
use tracing::{debug, error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
// use axum::{response::{Response, IntoResponse}, Json, http::StatusCode};
// use serde::{Deserialize, Serialize};
use axum::http::{Method, StatusCode};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{EnvFilter, fmt};

pub async fn run_server32() -> Result<()> {
    // let state = AppState {
    //     jobs: Arc::new(DashMap::new()),
    //     params_dir: std::path::PathBuf::from("./params"), // adjust this
    // };

    let (job_queue, receiver) = JobQueue::new();
    let jobs = Arc::new(DashMap::new());

    let state = AppState {
        jobs: jobs.clone(),
        job_queue: job_queue.clone(),
        params_dir: std::path::PathBuf::from("./params"),
    };

    spawn_job_worker(receiver, jobs.clone());
    // Build router
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_origin(Any)
        .allow_headers(Any);
    let trace = TraceLayer::new_for_http();

    let app = Router::new()
        .route("/proof", post(start_proof))
        .route("/proof/{job_id}", get(poll_proof))
        .with_state(state)
        .layer(cors)
        .layer(trace);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);
    let addr: SocketAddr = format!("0.0.0.0:{port}").parse().unwrap();
    tracing::info!("Axum burn API (Redis-backed) listening on http://{addr}");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

use axum::{

    response::{Response},

};
#[derive(Serialize)]
struct ApiResponse<T> {
    status: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<T>,
}

pub struct AppJson<T>(StatusCode, ApiResponse<T>);

impl<T: Serialize> IntoResponse for AppJson<T> {
    fn into_response(self) -> Response {
        let (status, body) = (self.0, Json(self.1));
        (status, body).into_response()
    }
}


// async fn poll_proof(
//     Path(job_id): Path<String>,
//     State(state): State<AppState>,
// ) -> impl IntoResponse {
//     match Uuid::parse_str(&job_id) {
//         Ok(uuid) => match state.jobs.get(&uuid) {
//             Some(job) => match job.value() {
//                 JobStatus::Pending => AppJson(
//                     StatusCode::OK,
//                     ApiResponse {
//                         status: "success".into(),
//                         message: "Proof job is still pending".into(),
//                         result: Some(PollResponse::Pending),
//                     },
//                 ),
//                 JobStatus::CompletedProof { result } => AppJson(
//                     StatusCode::OK,
//                     ApiResponse {
//                         status: "success".into(),
//                         message: "Proof completed".into(),
//                         result: Some(PollResponse::ProofResult(result.clone())),
//                     },
//                 ),
//                 JobStatus::Failed(error) => AppJson(
//                     StatusCode::OK, // You could also make this a 500
//                     ApiResponse {
//                         status: "error".into(),
//                         message: format!("Job failed: {}", error),
//                         result: Some(PollResponse::Failed {
//                             error: error.clone(),
//                         }),
//                     },
//                 ),
//                 _ => AppJson(
//                     StatusCode::INTERNAL_SERVER_ERROR,
//                     ApiResponse {
//                         status: "error".into(),
//                         message: "Unexpected job type".into(),
//                         result: None,
//                     },
//                 ),
//             },
//             None => AppJson(
//                 StatusCode::NOT_FOUND,
//                 ApiResponse {
//                     status: "error".into(),
//                     message: "Job not found".into(),
//                     result: None,
//                 },
//             ),
//         },
//         Err(_) => AppJson(
//             StatusCode::BAD_REQUEST,
//             ApiResponse {
//                 status: "error".into(),
//                 message: "Invalid job_id format".into(),
//                 result: None,
//             },
//         ),
//     }
// }

use tokio::time::{timeout, Duration};
use futures::FutureExt;
use std::panic::AssertUnwindSafe;

// async fn start_proof(
//     State(state): State<AppState>,
//     Json(payload): Json<ProofInput>,
// ) -> impl IntoResponse {
//     let job_id = Uuid::new_v4();
//     state.jobs.insert(job_id, JobStatus::Pending);
//     let jobs = state.jobs.clone();

//     tokio::spawn(async move {
//         info!("Starting proof computation for job: {job_id}");

//         let result = std::panic::AssertUnwindSafe(compute_proof(payload))
//             .catch_unwind() // Catch panics to prevent silent failures
//             .await;

//         match result {
//             Ok(Ok(proof_output)) => {
//                 info!("Proof computation succeeded for job: {job_id}");
//                 jobs.insert(
//                     job_id,
//                     JobStatus::CompletedProof {
//                         result: proof_output,
//                     },
//                 );
//             }
//             Ok(Err(e)) => {
//                 error!("Proof computation failed for job {job_id}: {:?}", e);
//                 jobs.insert(job_id, JobStatus::Failed(e.to_string()));
//             }
//             Err(panic_err) => {
//                 error!("Proof computation panicked for job {job_id}: {:?}", panic_err);
//                 jobs.insert(
//                     job_id,
//                     JobStatus::Failed("Internal server error during proof".into()),
//                 );
//             }
//         }
//     });

//     Json(JobResponse {
//         job_id: job_id.to_string(),
//     })
// }




use tokio::sync::mpsc::{UnboundedSender, UnboundedReceiver, unbounded_channel};

#[derive(Clone)]
struct JobQueue {
    sender: UnboundedSender<(Uuid, ProofInput)>,
}

impl JobQueue {
    fn new() -> (Self, UnboundedReceiver<(Uuid, ProofInput)>) {
        let (tx, rx) = unbounded_channel();
        (JobQueue { sender: tx }, rx)
    }

    fn submit(&self, job_id: Uuid, input: ProofInput) {
        let _ = self.sender.send((job_id, input));
    }
}

#[derive(Clone)]
pub struct AppState {
    pub jobs: Arc<DashMap<Uuid, JobStatus>>,
    pub job_queue: JobQueue,
    pub params_dir: std::path::PathBuf,
}

#[derive(Clone)]
enum JobStatus {
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
async fn start_proof(
    State(state): State<AppState>,
    Json(payload): Json<ProofInput>,
) -> impl IntoResponse {
    let job_id = Uuid::new_v4();

    // Insert job with pending status
    state.jobs.insert(job_id, JobStatus::Pending);

    // Enqueue it for processing (non-blocking)
    state.job_queue.submit(job_id, payload);

    // Return consistent envelope
    (
        StatusCode::OK, // you can change to 202 ACCEPTED if you prefer
        Json(ApiResponse {
            status: "queued".into(),
            message: "Job enqueued".into(),
            result: Some(JobResponse {
                job_id: job_id.to_string(),
            }),
        }),
    )
}

fn spawn_job_worker(
    mut receiver: UnboundedReceiver<(Uuid, ProofInput)>,
    jobs: Arc<DashMap<Uuid, JobStatus>>,
) {
    // One worker task
    tokio::spawn(async move {
        // Process jobs STRICTLY sequentially
        while let Some((job_id, input)) = receiver.recv().await {
            println!("[worker] picked job {}", job_id);
            jobs.insert(job_id, JobStatus::InProgress);

            // Run compute_proof on the blocking pool, and WAIT for it to finish
            // before pulling the next job. This enforces single-concurrency.
            let handle = tokio::runtime::Handle::current();
            let res = tokio::task::spawn_blocking(move || {
                // drive the async future to completion on this blocking thread
                handle.block_on(compute_proof(input))
            })
            .await; // <- await here makes the loop wait until this job is done

            match res {
                Ok(Ok(output)) => {
                    println!("[worker] job {} completed", job_id);
                    jobs.insert(job_id, JobStatus::CompletedProof { result: output });
                }
                Ok(Err(e)) => {
                    println!("[worker] job {} failed: {}", job_id, e);
                    jobs.insert(job_id, JobStatus::Failed(e.to_string()));
                }
                Err(join_err) => {
                    println!("[worker] job {} join error: {}", job_id, join_err);
                    jobs.insert(job_id, JobStatus::Failed(format!("join error: {join_err}")));
                }
            }
            println!("------------------------------------------------------");
        }
    });
}


pub async fn poll_proof(
    Path(job_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match Uuid::parse_str(&job_id) {
        Err(_) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<ProofOutput> {
                status: "error".into(),
                message: "Invalid job ID".into(),
                result: None,
            }),
        ),
        Ok(uuid) => match state.jobs.get(&uuid) {
            None => (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<ProofOutput> {
                    status: "error".into(),
                    message: "Job not found".into(),
                    result: None,
                }),
            ),
            Some(status) => match status.value() {
                JobStatus::Pending => (
                    StatusCode::OK,
                    Json(ApiResponse::<ProofOutput> {
                        status: "pending".into(),
                        message: "Job is pending".into(),
                        result: None,
                    }),
                ),
                JobStatus::InProgress => (
                    StatusCode::OK,
                    Json(ApiResponse::<ProofOutput> {
                        status: "in_progress".into(),
                        message: "Job is in progress".into(),
                        result: None,
                    }),
                ),
                JobStatus::CompletedProof { result } => (
                    StatusCode::OK,
                    Json(ApiResponse::<ProofOutput> {
                        status: "completed".into(),
                        message: "Job completed".into(),
                        // 👇 Directly return the actual output here (no extra nesting)
                        result: Some(result.clone()),
                    }),
                ),
                JobStatus::Failed(err) => (
                    StatusCode::OK, // or INTERNAL_SERVER_ERROR if you prefer
                    Json(ApiResponse::<ProofOutput> {
                        status: "error".into(),
                        message: format!("Job failed: {}", err),
                        result: None,
                    }),
                ),
            },
        },
    }
}