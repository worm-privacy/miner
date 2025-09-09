
use anyhow::Result;
use tokio::net::TcpListener;
// use axum::{http::uri, routing::get, Router};
use tracing::{info, error,debug};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
// use axum::{response::{Response, IntoResponse}, Json, http::StatusCode};
// use serde::{Deserialize, Serialize};
use axum::{
    http::{Method, StatusCode},
    response::IntoResponse,
    routing::{post,get},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};
use std::{net::SocketAddr, str::FromStr};
// use crate::cli::utils::{
//     append_new_entry, check_required_files, coins_file,burn_file ,init_coins_file, next_id,
// };
use crate::constants::{
    poseidon_burn_address_prefix, poseidon_coin_prefix, poseidon_nullifier_prefix,
};
use crate::fp::{Fp, FpRepr};
use crate::poseidon::{poseidon2, poseidon3};
use crate::utils::{RapidsnarkOutput, find_burn_key, generate_burn_address};
use alloy::rlp::Encodable;
use alloy::{
    eips::BlockId,
    hex::ToHexExt,
    network::TransactionBuilder,
    primitives::{
        U256,
        utils::{format_ether, parse_ether},
    },
    providers::Provider,
    rpc::types::TransactionRequest,
};
use anyhow::anyhow;
use ff::PrimeField;
use tracing_subscriber::{EnvFilter, fmt};

use std::process::Command;
use structopt::StructOpt;

use alloy::primitives::{Address};


pub struct BurnInput {
    pub fee: String,            // in ETH
    pub spend: String,          // in ETH
    pub wallet_address: Address // alloy address
}

pub struct BurnOutput {
    pub burn_address: Address,
    pub burn_key_u256_le: U256,
    pub nullifier_u256_le: U256,
    pub fee_wei: U256,
    pub spend_wei: U256,
}
#[derive(Deserialize)]
struct BurnRequest {
    fee: String,
    spend: String,
    wallet_address: String,
}
#[derive(Serialize)]
struct BurnResponse {
    burn_address: String,        
    burn_key_u256_le: String,    
    nullifier_u256_le: String,   
    fee_wei: String,             
    spend_wei: String,           
}
pub fn compute_burn_address(input: BurnInput) -> Result<BurnOutput> {
    let burn_addr_constant = poseidon_burn_address_prefix();
    let nullifier_constant = poseidon_nullifier_prefix();

    let fee    = parse_ether(&input.fee)?;
    let spend  = parse_ether(&input.spend)?;


    let wallet_addr = input.wallet_address;
    let burn_key = find_burn_key(3, wallet_addr, fee);
    let burn_address = generate_burn_address(burn_addr_constant, burn_key, wallet_addr, fee);
    let nullifier_fp = poseidon2(nullifier_constant, burn_key);

    let burn_key_u256_le = U256::from_le_bytes(burn_key.to_repr().0);
    let nullifier_u256_le = U256::from_le_bytes(nullifier_fp.to_repr().0);

    Ok(BurnOutput {
        burn_address,
        burn_key_u256_le,
        nullifier_u256_le,
        fee_wei: fee,
        spend_wei: spend,
    })
}
async fn handle_burn(
    axum::Json(req): axum::Json<BurnRequest>,
) -> Result<axum::Json<BurnResponse>, (StatusCode, axum::Json<ErrorMsg>)> {
    let wallet_address = Address::from_str(req.wallet_address.trim())
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json_error("Invalid wallet_address; expected 0x-prefixed hex"))))?;

    let input = BurnInput {
        fee: req.fee,
        spend: req.spend,
        wallet_address,
    };

    match compute_burn_address(input) {
        Ok(out) => {
            let resp = BurnResponse {
                burn_address: format!("{:#x}", out.burn_address),
                burn_key_u256_le: out.burn_key_u256_le.to_string(),
                nullifier_u256_le: out.nullifier_u256_le.to_string(),
                fee_wei: out.fee_wei.to_string(),
                spend_wei: out.spend_wei.to_string(),
            };
            Ok(Json(resp))
        }
        Err(err) => Err((StatusCode::BAD_REQUEST, Json(json_error(&format!("{err:#}"))))),
    }
}

#[derive(Serialize)]
struct ErrorMsg {
    error: String,
}

fn json_error(msg: &str) -> ErrorMsg {
    ErrorMsg { error: msg.into() }
}


pub async fn fallback(
    uri: axum::http::Uri
) -> impl axum::response::IntoResponse{
 (axum::http::StatusCode::NOT_FOUND, uri.to_string())
}

pub fn app() -> axum::Router{

    axum::Router::new()
    .fallback(fallback)
    .route("/", get(hello))
    .route("/hello",get(hello))


}
pub async fn run_server() -> Result<()> {
    // ---------- Logging Initialization ----------
    let env_filter = EnvFilter::try_from_default_env()
        // fallback if RUST_LOG is not set
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,axum=info"));

    fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .init();

    // ... build router
    let app = Router::new()
        .route("/burn", post(handle_burn))
        .layer(CorsLayer::new().allow_methods([Method::POST, Method::OPTIONS]).allow_origin(Any).allow_headers(Any))
        .layer(tower_http::trace::TraceLayer::new_for_http()); // keep this

    let addr: std::net::SocketAddr = "0.0.0.0:8080".parse().unwrap();
    tracing::info!("Axum burn API listening on http://{addr}");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}
use crate::polling::{
    state::{init_state_and_workers, AppState},
    handlers::{enqueue_burn, get_job},
};

use tower_http::{
    trace::TraceLayer,
};
pub async fn run_server2() -> Result<()>{
    let (state, _handles) = init_state_and_workers().await?;

    // Build router
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_origin(Any)
        .allow_headers(Any);
    let trace = TraceLayer::new_for_http();

    let app = Router::new()
        .route("/burn", post(enqueue_burn))
        .route("/burn/{job_id}", get(get_job))
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

pub async fn hello() -> String {
    "Hello world".into()
}



#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;

    #[tokio::test]
    async fn test(){
        let server = TestServer::new(app()).unwrap();
        server.get("/hello").await.assert_text("Hello world");
    }

    #[tokio::test]
    async fn test_fallback(){
        let server = TestServer::new(app()).unwrap();
        let response = server.get("/foo").await;
        response.assert_status(axum::http::StatusCode::NOT_FOUND);
        response.assert_text("http://localhost/foo");  

    }
}

