use crate::constants::{
    poseidon_burn_address_prefix, poseidon_nullifier_prefix,
};
use crate::polling::types::{BurnInput,BurnOutput};
use anyhow::Result;
use alloy::{primitives::{U256,utils::parse_ether}};
// use tokio::net::TcpListener;
// use axum::{http::uri, routing::get, Router};
// use tracing::{info, error,debug};
// use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
// use axum::{response::{Response, IntoResponse}, Json, http::StatusCode};
// use serde::{Deserialize, Serialize};
// use axum::{
//     http::{Method, StatusCode},
//     response::IntoResponse,
//     routing::{post,get},
//     Json, Router,
// };
// use serde::{Deserialize, Serialize};
// use tower_http::cors::{Any, CorsLayer};
// use std::{net::SocketAddr, str::FromStr};
// use crate::cli::utils::{
//     append_new_entry, check_required_files, coins_file,burn_file ,init_coins_file, next_id,
// };

// use crate::fp::{Fp, FpRepr};
// use alloy::rlp::Encodable;
// use alloy::{
//     eips::BlockId,
//     hex::ToHexExt,
//     network::TransactionBuilder,
//     primitives::{
//         utils::{format_ether,},
//     },
//     providers::Provider,
//     rpc::types::TransactionRequest,
// };
use anyhow::anyhow;
use ff::PrimeField;
use tracing_subscriber::{EnvFilter, fmt};

// use std::process::Command;
// use structopt::StructOpt;

use crate::utils::{find_burn_key,generate_burn_address};
use crate::poseidon::poseidon2;
pub fn compute_burn_address(input: BurnInput) -> Result<BurnOutput> {
    // Removed: check_required_files(params_dir)?;

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