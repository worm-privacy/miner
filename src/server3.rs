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
use ff::PrimeField;
use std::process::Command;
use structopt::StructOpt;

#[derive(Debug, Clone)]
enum JobStatus {
    Pending,
    CompletedBurn { burn_address: String },
    CompletedProof { result: ProofOutput },
    Failed(String),
}
type JobMap = Arc<DashMap<Uuid, JobStatus>>;

#[derive(Clone)]
struct AppState {
    jobs: JobMap,
    params_dir: std::path::PathBuf,
}

#[derive(Deserialize)]
struct BurnRequest {
    amount: String,
    fee: String,
    spend: String,
    wallet_address: String,
}

#[derive(Serialize)]
struct JobResponse {
    job_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum PollResponse {
    Pending,
    Completed { burn_address: String },
    ProofResult(ProofOutput),
    Failed { error: String },
}

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

// POST /start_burn
async fn start_burn(
    State(state): State<AppState>,
    Json(payload): Json<BurnRequest>,
) -> impl IntoResponse {
    let job_id = Uuid::new_v4();
    state.jobs.insert(job_id, JobStatus::Pending);

    let jobs = state.jobs.clone();
    let params_dir = state.params_dir.clone();

    tokio::spawn(async move {
        let result = run_burn_job(
            payload.amount,
            payload.fee,
            payload.spend,
            payload.wallet_address,
            &params_dir,
        )
        .await;

        match result {
            Ok(burn_address) => {
                jobs.insert(job_id, JobStatus::CompletedBurn { burn_address });
            }
            Err(e) => {
                jobs.insert(job_id, JobStatus::Failed(e.to_string()));
            }
        }
    });

    Json(JobResponse {
        job_id: job_id.to_string(),
    })
}

// GET /poll_burn/:job_id
async fn poll_burn(Path(job_id): Path<String>, State(state): State<AppState>) -> impl IntoResponse {
    match Uuid::parse_str(&job_id) {
        Ok(uuid) => match state.jobs.get(&uuid) {
            Some(job) => match job.value() {
                JobStatus::Pending => Json(PollResponse::Pending),
                JobStatus::CompletedBurn { burn_address } => Json(PollResponse::Completed {
                    burn_address: burn_address.clone(),
                }),
                JobStatus::Failed(error) => Json(PollResponse::Failed {
                    error: error.clone(),
                }),
                _ => Json(PollResponse::Failed {
                    error: "Job is not a burn job".to_string(),
                }),
            },
            None => Json(PollResponse::Failed {
                error: "Job not found".to_string(),
            }),
        },
        Err(_) => Json(PollResponse::Failed {
            error: "Invalid job_id".to_string(),
        }),
    }
}

async fn run_burn_job(
    amount: String,
    fee: String,
    spend: String,
    wallet_address: String,
    _params_dir: &std::path::Path,
) -> Result<String, anyhow::Error> {
    let fee = parse_ether(&fee)?;
    let spend = parse_ether(&spend)?;
    let amount = parse_ether(&amount)?;

    let wallet_address = Address::from_str(&wallet_address.trim())?;

    if amount > parse_ether("1")? {
        return Err(anyhow!("Can't burn more than 1 ETH in a single call!"));
    }

    if fee + spend > amount {
        return Err(anyhow!("Sum of fee and spend must be <= amount"));
    }

    // Generate burn_key
    println!("Generating burn key...");
    let burn_key = find_burn_key(3, wallet_address, fee);
    let burn_key_str = U256::from_le_bytes(burn_key.to_repr().0).to_string();
    println!("finished,{}", burn_key_str);

    Ok(burn_key_str)
}
async fn start_proof(
    State(state): State<AppState>,
    Json(payload): Json<ProofInput>,
) -> impl IntoResponse {
    let job_id = Uuid::new_v4();
    state.jobs.insert(job_id, JobStatus::Pending);

    let jobs = state.jobs.clone();

    tokio::spawn(async move {
        let result = compute_proof(payload).await;

        match result {
            Ok(proof_output) => {
                jobs.insert(
                    job_id,
                    JobStatus::CompletedProof {
                        result: proof_output,
                    },
                );
            }
            Err(e) => {
                jobs.insert(job_id, JobStatus::Failed(e.to_string()));
            }
        }
    });

    Json(JobResponse {
        job_id: job_id.to_string(),
    })
}

// GET /poll_proof/:job_id
async fn poll_proof(
    Path(job_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match Uuid::parse_str(&job_id) {
        Ok(uuid) => match state.jobs.get(&uuid) {
            Some(job) => match job.value() {
                JobStatus::Pending => Json(PollResponse::Pending),
                JobStatus::CompletedProof { result } => {
                    Json(PollResponse::ProofResult(result.clone()))
                }
                JobStatus::Failed(error) => Json(PollResponse::Failed {
                    error: error.clone(),
                }),
                _ => Json(PollResponse::Failed {
                    error: "Job is not a proof job".to_string(),
                }),
            },
            None => Json(PollResponse::Failed {
                error: "Job not found".to_string(),
            }),
        },
        Err(_) => Json(PollResponse::Failed {
            error: "Invalid job_id".to_string(),
        }),
    }
}
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
    let state = AppState {
        jobs: Arc::new(DashMap::new()),
        params_dir: std::path::PathBuf::from("./params"), // adjust this
    };

    // Build router
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_origin(Any)
        .allow_headers(Any);
    let trace = TraceLayer::new_for_http();

    let app = Router::new()
        .route("/burn", post(start_burn))
        .route("/burn/{job_id}", get(poll_burn))
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
