use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

use axum::{
    extract::{Path, Query, State},
    http::{Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::{cors::{Any, CorsLayer}, trace::TraceLayer};
use tracing::{error, info, Level};
use tracing_subscriber::{fmt, EnvFilter};
use uuid::Uuid;

use tokio::time::Instant;

// Redis
use redis::{aio::ConnectionManager, AsyncCommands};
use tokio::task::JoinHandle;
// Alloy
use alloy::primitives::Address;

// ⬇️ Replace with your real crate name:
// use my_crate::{compute_burn_address, BurnInput}; // <-- CHANGE crate name

use redis::{Client,};

// #[derive(Debug,Clone,Serialize,Deserialize)]
// #[serde(rename_all="snake_case")]
// enum JobStatus {
//     Queued,
//     Running,
//     Completed,
//     Failed
// }

// #[derive(Debug,Clone,Serialize,Deserialize)]
// struct BurnRequest {
//     fee:String,
//     spend:String,
//     wallet_address:String
// }

// #[derive(Debug,Clone,Serialize,Deserialize)]
// struct BurnResponse{
//     burn_addres:String,
//     burn_key_u256_le:String,
//     nullifier_u256_le:String,
//     fee_wei:String,
//     spend_wei:String
// }


// #[derive(Debug,Clone,Serialize,Deserialize)]
// struct JobRecord{
//     status:JobStatus,
//     #[serde(default)]
//     input:Option<BurnRequest>,
//     #[serde(default)]
//     result:Option<BurnResponse>,
//     #[serde(default)]
//     error:Option<String>,
//     #[serde(default)]
//     created_at_ms:Option<u64>,
//     #[serde(default)]
//     updated_at_ms:Option<u64>

// }

// #[derive(Debug,Clone,Serialize)]
// struct EnqueueResponse{
//     job_id:Uuid,
//     status:JobStatus,
//     queued:bool
// }

// #[derive(Debug,Clone,Serialize)]
// struct JobQueryResponse{
//     job_id:Uuid,
//     status:JobStatus,
//     result:Option<BurnResponse>,
//     error:Option<String>,
// }

// #[derive(Debug,Clone,Serialize)]
// struct ErrorMsg{
//     error:String,
// }
// #[derive(Clone)]
// struct AppState{
//     redis:Arc<ConnectionManager>,
//     queue_key:String,
//     job_key_prefix:String,
//     job_ttl:Duration
// }



// pub async fn init_state_and_workers() -> anyhow::Result<(AppState,Vec<JoinHandle<()>>)>{
//     let redis_url = std::env::var("REDIS_URL")
//         .unwrap_or_else(|_| "redis:///127.0.0.1:6379".to_string());
//     let queue_key = std::env::var("BURN_QUEUE_KEY")
//         .unwrap_or_else(|_| "burn:queue".to_string());
//     let job_key_prefix = std::env::var("BURN_JOB_PREFIX")
//         .unwrap_or_else(|_| "burn:job".to_string());
//     let job_ttl_secs:u64 = std::env::var("BURN_JOB_TTL_SECS")
//         .ok()
//         .and_then(|s| s.parse().ok())
//         .unwrap_or(15*60);

//     let client = Client::open(redis_url)?;
//     let manager = ConnectionManager::new(client).await?;
//     let redis = Arc::new(manager);
//     let default_workers = std::thread::available_parallelism()
//         .map(|n| n.get().min(4))
//         .unwrap_or(2);

//     let worker_count:usize = std::env::var("BURN_WORKERS")
//         .ok()
//         .and_then(|s| s.parse().ok())
//         .unwrap_or(default_workers);

//     let state = AppState {
//         redis,
//         queue_key,
//         job_key_prefix,
//         job_ttl:Duration::from_secs(job_ttl_secs)
//     };
//     println!("Starting {} redis workers...",worker_count);
//     let mut handles = Vec::with_capacity(worker_count);
//     for idx in 0..worker_count{
//         handles.push(spawn_worker(idx,state.clone()));
//     }
//     Ok((state,handles))


// }