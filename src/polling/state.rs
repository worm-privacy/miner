

use std::{sync::Arc, time::Duration};
use redis::aio::ConnectionManager;
use crate::polling::job::spawn_worker;
use tokio::task::JoinHandle;
#[derive(Clone)]
pub struct AppState{
    pub redis:Arc<ConnectionManager>,
    pub queue_key:String,
    pub job_key_prefix:String,
    pub job_ttl:Duration
}


pub async fn init_state_and_workers() -> anyhow::Result<(AppState,Vec<JoinHandle<()>>)>{
    let redis_url = std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis:///127.0.0.1:6379".to_string());
    let queue_key = std::env::var("BURN_QUEUE_KEY")
        .unwrap_or_else(|_| "burn:queue".to_string());
    let job_key_prefix = std::env::var("BURN_JOB_PREFIX")
        .unwrap_or_else(|_| "burn:job".to_string());
    let job_ttl_secs:u64 = std::env::var("BURN_JOB_TTL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(15*60);

    let client = redis::Client::open(redis_url)?;
    let manager = ConnectionManager::new(client).await?;
    let redis = Arc::new(manager);
    let default_workers = std::thread::available_parallelism()
        .map(|n| n.get().min(4))
        .unwrap_or(2);

    let worker_count:usize = std::env::var("BURN_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default_workers);

    let state = AppState {
        redis,
        queue_key,
        job_key_prefix,
        job_ttl:Duration::from_secs(job_ttl_secs)
    };
    println!("Starting {} redis workers...",worker_count);
    let mut handles = Vec::with_capacity(worker_count);
    for idx in 0..worker_count{
        handles.push(spawn_worker(idx,state.clone()));
    }
    Ok((state,handles))


}