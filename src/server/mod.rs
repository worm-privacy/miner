pub mod types;
pub mod proof_logic;
pub mod handlers;
pub mod worker;
pub mod queue;



use anyhow::Result;

use axum::http::{Method};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
pub use queue::JobQueue;
pub use worker::spawn_job_worker;
pub use types::{AppState,};

use axum::{Router, routing::{get, post}};
use std::net::SocketAddr;
use std::{sync::Arc};
pub use handlers::{poll_proof,start_proof};
fn load_env_files() {
    let _ = dotenvy::dotenv();

    let _ = dotenvy::from_filename_override("settings.env");

    if let Ok(p) = std::env::var("ENV_FILE") {
        let _ = dotenvy::from_filename_override(p);
    }
}



fn socket_addr_from_env() -> SocketAddr {
    if let Ok(s) = std::env::var("SOCKET_ADDR")
        .or_else(|_| std::env::var("BIND_ADDR"))
        .or_else(|_| std::env::var("ADDR"))
    {
        if let Ok(addr) = s.parse::<SocketAddr>() {
            return addr;
        }
        let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
        let host = if s.contains(':') && !(s.starts_with('[') && s.ends_with(']')) {
            format!("[{s}]")
        } else { s };
        return format!("{host}:{port}").parse().expect("valid socket addr from env");
    }

    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
    let host = if host.contains(':') && !(host.starts_with('[') && host.ends_with(']')) {
        format!("[{host}]")
    } else { host };
    format!("{host}:{port}").parse().expect("valid socket addr")
}

pub async fn run_server() -> Result<()> {
    load_env_files();
    let queue_cap = std::env::var("PROOF_QUEUE_CAP")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10);
    println!();

    let (job_queue, receiver) = JobQueue::with_capacity(queue_cap);
    let jobs = Arc::new(dashmap::DashMap::new());

    let state = AppState {
        jobs: jobs.clone(),
        job_queue: job_queue.clone(),
        params_dir: std::path::PathBuf::from("./params"),
    };

    spawn_job_worker(receiver, jobs.clone(),job_queue.clone());
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

    
    let addr = socket_addr_from_env();
    println!("Axum API listening on http://{addr}");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}
