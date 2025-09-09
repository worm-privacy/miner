use axum::{
    extract::{Path, Query, State},
    http::{status, StatusCode},
    response::IntoResponse,
    Json,
};
use uuid::Uuid;
use crate::polling::{
    job::{self, mark_completed, mark_failed, process_job, spawn_worker}, state::AppState, types::{BurnRequest, EnqueueResponse, ErrorMsg, JobQueryResponse, JobStatus, PollQuery}, utils::{job_key, now_millis}
};

use std::str::FromStr;

pub async fn enqueue_burn(State(state):State<AppState>,Json(req):Json<BurnRequest>) -> impl IntoResponse {
    use redis::AsyncCommands;
    if alloy::primitives::Address::from_str(req.wallet_address.trim()).is_err(){
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorMsg{
                error:"Invalid wallet_address; expected 0x-prefixed hex".into(),
            })

        )
        .into_response()
    }

    let job_id = Uuid::new_v4();
    let key = job_key(&state.job_key_prefix, job_id);
    let now_ms = now_millis();

    let input_json = serde_json::to_string(&req).unwrap();
    let status_json = serde_json::to_string(&JobStatus::Queued).unwrap();

    let mut conn = state.redis.as_ref().clone();
    let mut pipe = redis::pipe();
    pipe.atomic()
        .hset(&key,"status",status_json)
        .hset(&key, "input", input_json)
        .hset(&key,"created_at_ms",now_ms)
        .hset(&key,"updated_at_ms",now_ms)
        .rpush(&state.queue_key, job_id.to_string());
    match pipe.query_async::<_,()>(&mut conn).await{
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(EnqueueResponse{
                job_id,
                status:JobStatus::Queued,
                queued:true,
            }),

        )
        .into_response(),
        Err(e) =>(
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorMsg{
                error: format!("enqueue failed: {e}"),
            })
        )
        .into_response()
    }
}


pub async fn get_job(
    State(state):State<AppState>,
    Path(job_id):Path<Uuid>,
    Query(q):Query<PollQuery>
) -> impl IntoResponse{
    use tokio::time::{Instant,Duration};
    let wait_ms = q.wait_ms.unwrap_or(0);
    if wait_ms == 0 {
        return (StatusCode::OK,axum::Json(read_job_once(&state,job_id).await));
    }
    let max_wait = Duration::from_millis(wait_ms.min(30_000));
    let deadline = Instant::now() + max_wait;
    loop{
        let resp = read_job_once(&state,job_id).await;
        if matches!(resp.status, JobStatus::Completed | JobStatus::Failed){
            return (StatusCode::OK,Json(resp))
        }
        if Instant::now() >=deadline {
            return (StatusCode::OK,Json(resp))
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

}

async fn read_job_once(state:&AppState,job_id:Uuid) -> JobQueryResponse {
    use redis::AsyncCommands;
    let key = job_key(&state.job_key_prefix, job_id);
    let mut conn = state.redis.as_ref().clone();

    let (status_s,result_s,error_s):(Option<String>,Option<String>,Option<String>,) = 
        match redis::pipe()
            .hget(&key, "status")
            .hget(&key, "result")
            .hget(&key, "error")
            .query_async(&mut conn)
            .await
            {
                Ok(v) => v,
                Err(_) =>(None,None,None)
            };
    if let Some(status_json) = status_s{
        let status: JobStatus = serde_json::from_str(&status_json).unwrap_or(
            JobStatus::Failed
        );
        let result :Option<crate::polling::types::BurnResponse> = match  result_s {
            Some(j)=> serde_json::from_str(&j).ok(),
            None => None,
        };
        let error = error_s;
        JobQueryResponse{
            job_id,
            status,
            result,
            error
        }
    } else {
        JobQueryResponse{
            job_id,
            status: JobStatus::Failed,
            result:None,
            error:Some("job not found(expired or invalid id)".into())
        }
    }

}