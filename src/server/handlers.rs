use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
    http::{StatusCode}
};


use uuid::Uuid;


use crate::server::{
    types::{AppState,JobStatus,ApiResponse,JobResponse,ProofInput,ProofOutput},
    queue::QueueError,
};
pub async fn start_proof(
    State(state): State<AppState>,
    Json(payload): Json<ProofInput>,
) -> impl IntoResponse {
    let job_id = Uuid::new_v4();

    state.jobs.insert(job_id, JobStatus::Pending);

   match state.job_queue.submit(job_id, payload) {
        Ok(()) => {
            let queued_now = state.job_queue.queued_len();
            let in_prog   = state.job_queue.in_progress();
            let ahead = queued_now.saturating_sub(1) + in_prog;
            let position = ahead + 1;

            let msg = format!("Job enqueued. You are #{} in the queue.", position);

            (
                StatusCode::OK,
                Json(ApiResponse {
                    status: "queued".into(),
                    message: msg,
                    result: Some(JobResponse { job_id: job_id.to_string() }),
                }),
            )
        }
        Err(QueueError::Full) => {
            state.jobs.remove(&job_id);
            (
                StatusCode::TOO_MANY_REQUESTS,
                Json(ApiResponse::<JobResponse> {
                    status: "error".into(),
                    message: "Queue is full, try again later".into(),
                    result: None,
                }),
            )
        }
        Err(QueueError::Closed) => {
            state.jobs.remove(&job_id);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::<JobResponse> {
                    status: "error".into(),
                    message: "Queue is not accepting jobs".into(),
                    result: None,
                }),
            )
        }
    }
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
                JobStatus::Pending => {
                    let queued_now = state.job_queue.queued_len();
                    let in_prog = state.job_queue.in_progress();
                    let ahead = queued_now + in_prog;
                    let pos = ahead + 1;

                    (
                        StatusCode::OK,
                        Json(ApiResponse::<ProofOutput> {
                            status: "pending".into(),
                            message: format!("Job is pending. You are approximately #{pos} in the queue."),
                            result: None,
                        }),
                    )
                }
                JobStatus::InProgress => (
                    StatusCode::OK,
                    Json(ApiResponse::<ProofOutput> {
                        status: "in_progress".into(),
                        message: "Job is currently being processed. You are #1 in the queue.".into(),
                        result: None,
                    }),
                ),
                JobStatus::CompletedProof { result } => (
                    StatusCode::OK,
                    Json(ApiResponse::<ProofOutput> {
                        status: "completed".into(),
                        message: "Job completed".into(),
                        result: Some(result.clone()),
                    }),
                ),
                JobStatus::Failed(err) => (
                    StatusCode::OK, // or 500
                    Json(ApiResponse::<ProofOutput> {
                        status: "error".into(),
                        message: format!("Job failed: {err}"),
                        result: None,
                    }),
                ),
            },
        },
    }
}
