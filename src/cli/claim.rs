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
    eips::BlockId,
    hex::ToHexExt,
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
pub struct ClaimOpt {
    #[structopt(long, default_value = "anvil")]
    network: String,
    #[structopt(long)]
    private_key: PrivateKeySigner,
    #[structopt(long, default_value = "10")]
    epochs_to_check: usize,
}

impl ClaimOpt {
    pub async fn run(self) -> Result<(), anyhow::Error> {
        let addr = self.private_key.address();
        let net = NETWORKS.get(&self.network).expect("Invalid network!");
        let provider = ProviderBuilder::new()
            .wallet(self.private_key)
            .connect_http(net.rpc.clone());
        let worm = WORM::new(net.worm, provider.clone());
        let epoch = worm.currentEpoch().call().await?;
        let num_epochs_to_check = std::cmp::min(epoch, U256::from(self.epochs_to_check));
        let receipt = worm
            .claim(
                epoch.saturating_sub(U256::from(num_epochs_to_check)),
                num_epochs_to_check,
            )
            .send()
            .await?
            .get_receipt()
            .await?;
        if receipt.status() {
            println!("Success!");
            let worm_balance = worm.balanceOf(addr).call().await?;
            println!("WORM balance: {}", format_ether(worm_balance));
        }
        Ok(())
    }
}
