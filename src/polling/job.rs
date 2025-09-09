
use crate::polling::utils::{job_key,now_millis};
use crate::polling::types::{BurnRequest,BurnInput, BurnResponse, JobStatus};
use uuid::Uuid;
use crate::polling::state::AppState;
use alloy::primitives::Address;
use redis::AsyncCommands;
use std::str::FromStr;
use crate::polling::burn_logic::compute_burn_address;

pub fn spawn_worker(idx:usize,state:AppState) -> tokio::task::JoinHandle<()>{
    tokio::spawn(async move {
        println!("Worke-{} started",idx);
        loop{
            match brpop_job_id(&state, 5).await{
                Ok(Some(job_id))=>{
                    if let Err(e) = process_job(&state,job_id).await{
                        println!("Worker-{}: processing failed:{:#}",idx,e);
                    
                    }
                }
                Ok(None)=>{

                }
                Err(e) => {
                    println!("Worker-{}: BRPOP error:{:#}",idx, e);
                }
            }
        }
    })
}

pub async fn brpop_job_id(state:&AppState,timeout_secs:u64) -> anyhow::Result::<Option<Uuid>> {
    let mut conn = state.redis.as_ref().clone();
    let res:Option<(String,String)> = redis::cmd("BRPOP")
        .arg(&state.queue_key)
        .arg(timeout_secs)
        .query_async(&mut conn)
        .await?;
    if let Some((_list,val)) = res {
        Ok(Some(Uuid::parse_str(&val)?))
    }else {
        Ok(None)
    }
}

pub async fn process_job(state:&AppState,job_id:Uuid) -> anyhow::Result<()> {
    let now_ms = now_millis();
    {
        let mut conn = state.redis.as_ref().clone();
        let key = job_key(&state.job_key_prefix,job_id);
        let _: () = redis::pipe()
            .hset(&key,"status",serde_json::to_string(&JobStatus::Running)?)
            .hset(&key,"updated_at_ms",now_ms)
            .query_async(&mut conn)
            .await?;


    }
    let req :BurnRequest = {
        let mut conn = state.redis.as_ref().clone();
        let key = job_key(&state.job_key_prefix, job_id);
        let input_json : Option<String> = conn.hget(&key,"input").await?;
        match input_json {
            Some(j) => serde_json::from_str(&j)?,
            None => {
                crate::polling::job::mark_failed(state,job_id,"missing input in job".to_string());
                return Ok(());
            }
        }
     
    };
    let result = crate::polling::job::compute_burn_from_request(req.clone()).await;
    match result {
        Ok(res) => mark_completed(state,job_id,res).await?,
        Err(e) => mark_failed(state,job_id,format!("{e:#}")).await?
    }
    Ok(())
}

async fn compute_burn_from_request(req:BurnRequest ) -> anyhow::Result<BurnResponse>{
    let wallet_address = Address::from_str(req.wallet_address.trim())
        .map_err(|_| anyhow::anyhow!("Invalid wallet_address; expected 0x-prefixed hex"))?;

    let input = BurnInput {
        fee: req.fee,
        spend: req.spend,
        wallet_address,
    };

    let out = compute_burn_address(input)?;
    let resp = BurnResponse {
                burn_address: format!("{:#x}", out.burn_address),
                burn_key_u256_le: out.burn_key_u256_le.to_string(),
                nullifier_u256_le: out.nullifier_u256_le.to_string(),
                fee_wei: out.fee_wei.to_string(),
                spend_wei: out.spend_wei.to_string(),
            };
    Ok(resp)
}

pub async fn mark_completed(state:&AppState,job_id:Uuid,res:BurnResponse) -> anyhow::Result<()>{
    
    let mut conn = state.redis.as_ref().clone();
    let key = job_key(&state.job_key_prefix,job_id);
    let now_ms = now_millis();
    let _:() = redis::pipe()
        .hset(&key,"status",serde_json::to_string(&JobStatus::Completed)?)
        .hset(&key,"result",serde_json::to_string(&res)?)
        .hset(&key,"updated_at_ms",now_ms)
        .expire(&key,state.job_ttl.as_secs().try_into().unwrap())
        .query_async(&mut conn)
        .await?;
    Ok(())
}
pub async fn mark_failed(state:&AppState,job_id:Uuid,err:String) -> anyhow::Result<()>{
    let mut conn = state.redis.as_ref().clone();
    let key = job_key(&state.job_key_prefix,job_id);
    let now_ms = now_millis();
    let _:() = redis::pipe()
        .hset(&key,"status",serde_json::to_string(&JobStatus::Failed)?)
        .hset(&key,"error",err)
        .hset(&key,"updated_at_ms",now_ms)
        .expire(&key,state.job_ttl.as_secs().try_into().unwrap())
        .query_async(&mut conn)
        .await?;
    Ok(())
}