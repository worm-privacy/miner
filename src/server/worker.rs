use crate::server::{
    proof_logic::compute_proof,
    queue::JobQueue,
    types::{JobStatus, ProofInput},
};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;
use uuid::Uuid;

pub fn spawn_job_worker(
    mut receiver: Receiver<(Uuid, ProofInput)>,
    jobs: Arc<DashMap<Uuid, JobStatus>>,
    job_queue: JobQueue,
) {
    tokio::spawn(async move {
        while let Some((job_id, input)) = receiver.recv().await {
            job_queue.dec_queued();
            job_queue.inc_in_progress();

            println!("[worker] picked job {}", job_id);
            jobs.insert(job_id, JobStatus::InProgress);

            let handle = tokio::runtime::Handle::current();
            let res =
                tokio::task::spawn_blocking(move || handle.block_on(compute_proof(input))).await;

            job_queue.dec_in_progress();

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
