use structopt::StructOpt;

use std::{path::PathBuf, process::Command, time::Duration};

// use alloy_rlp::Decodable;
use crate::fp::{Fp, FpRepr};
use anyhow::anyhow;
use ff::{Field, PrimeField};
// use poseidon2::poseidon2;
// use serde::{Deserialize, Serialize};
// use serde_json::json;
use crate::networks::{NETWORKS, Network};
use crate::poseidon2::poseidon2;
use crate::utils::{RapidsnarkOutput, find_burn_key, generate_burn_address, input_file};
use alloy::rlp::Encodable;
use worm_witness_gens::generate_proof_of_burn_witness_file;
// use alloy::sol;
use tempfile::tempdir;

use crate::utils::{BETH, WORM};

use alloy::{
    // eips::BlockId,
    // hex::ToHexExt,
    network::TransactionBuilder,
    primitives::{
        B256,
        U256,
        // map::HashMap,
        utils::{format_ether, parse_ether},
    },
    providers::{Provider, ProviderBuilder},
    // rlp::RlpDecodable,
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
    // transports::http::reqwest,
};

#[derive(StructOpt)]
pub struct ParticipateOpt {
    #[structopt(long, default_value = "anvil")]
    network: String,
    #[structopt(long)]
    private_key: PrivateKeySigner,
    #[structopt(long)]
    amount_per_epoch: String,
    #[structopt(long)]
    num_epochs: usize,
}

impl ParticipateOpt {
    pub async fn run(self) -> Result<(), anyhow::Error> {
        let net = NETWORKS.get(&self.network).expect("Invalid network!");
        let provider = ProviderBuilder::new()
            .wallet(self.private_key)
            .connect_http(net.rpc.clone());
        let amount_per_epoch = parse_ether(&self.amount_per_epoch)?;
        let worm = WORM::new(net.worm, provider.clone());
        let beth = BETH::new(net.beth, provider.clone());
        println!("Approving BETH...");
        let beth_approve_receipt = beth
            .approve(net.worm, amount_per_epoch * U256::from(self.num_epochs))
            .send()
            .await?
            .get_receipt()
            .await?;
        if !beth_approve_receipt.status() {
            panic!("Failed on BETH approval!");
        }
        let receipt = worm
            .participate(amount_per_epoch, U256::from(self.num_epochs))
            .send()
            .await?
            .get_receipt()
            .await?;
        if receipt.status() {
            println!("Success!");
        }
        Ok(())
    }
}
