
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use alloy::primitives::{U256,Address};
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct BurnRequest {
    pub fee:String,
    pub spend:String,
    pub wallet_address:String
}

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct BurnResponse{
   pub burn_address:String,
    pub burn_key_u256_le:String,
    pub nullifier_u256_le:String,
    pub fee_wei:String,
    pub spend_wei:String,
}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct BurnInput {
    pub fee: String,            // in ETH
    pub spend: String,          // in ETH
    pub wallet_address: Address // alloy address
}

/// Output payload returned by the API.
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct BurnOutput {
    pub burn_address: Address,
    pub burn_key_u256_le: U256,
    pub nullifier_u256_le: U256,
    pub fee_wei: U256,
    pub spend_wei: U256,
}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed
}

// #[derive(Debug,Clone,Serialize,Deserialize)]
// pub struct BurnRequest {
//     pub fee:String,
//     pub spend:String,
//     pub wallet_address:String
// }

// #[derive(Debug,Clone,Serialize,Deserialize)]
// pub struct BurnResponse{
//     pub burn_addres:String,
//     pub burn_key_u256_le:String,
//     pub nullifier_u256_le:String,
//     pub fee_wei:String,
//     pub spend_wei:String
// }


// #[derive(Debug,Clone,Serialize,Deserialize)]
// pub struct JobRecord{
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

#[derive(Debug,Clone,Serialize)]
pub struct EnqueueResponse{
    pub job_id:Uuid,
    pub status:JobStatus,
    pub queued:bool
}

#[derive(Debug,Clone,Serialize)]
pub struct JobQueryResponse{
    pub job_id:Uuid,
    pub status:JobStatus,
    pub result:Option<BurnResponse>,
    pub error:Option<String>,
}

#[derive(Debug,Clone,Serialize)]
pub struct ErrorMsg{
    pub error:String,
}

#[derive(Deserialize)]
pub struct PollQuery{
    pub wait_ms:Option<u64>,
}