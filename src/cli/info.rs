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
        utils::{format_ether},
    },
    providers::{Provider, ProviderBuilder},
    // rlp::RlpDecodable,
    // rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
    // transports::http::reqwest,
};

#[derive(StructOpt)]
pub struct InfoOpt {
    #[structopt(long, default_value = "anvil")]
    network: String,
    #[structopt(long)]
    private_key: PrivateKeySigner,
}

impl InfoOpt {
    pub async fn run(self) -> Result<(), anyhow::Error> {
        let addr = self.private_key.address();
        let net = NETWORKS.get(&self.network).expect("Invalid network!");
        let provider = ProviderBuilder::new()
            .wallet(self.private_key)
            .connect_http(net.rpc.clone());
        let worm = WORM::new(net.worm, provider.clone());
        let beth = BETH::new(net.beth, provider.clone());
        let worm_balance = worm.balanceOf(addr).call().await?;
        let epoch = worm.currentEpoch().call().await?;
        let beth_balance = beth.balanceOf(addr).call().await?;
        let num_epochs_to_check = std::cmp::min(epoch, U256::from(10));
        let claimable_worm = worm
            .calculateMintAmount(
                epoch.saturating_sub(num_epochs_to_check),
                num_epochs_to_check,
                addr,
            )
            .call()
            .await?;
        println!("Current epoch: {}", epoch);
        println!("BETH balance: {}", format_ether(beth_balance));
        println!("WORM balance: {}", format_ether(worm_balance));
        println!(
            "Claimable WORM (10 last epochs): {}",
            format_ether(claimable_worm)
        );
        let epoch_u64 = epoch.as_limbs()[0];
        for e in epoch_u64..epoch_u64 + 10 {
            let total = worm.epochTotal(U256::from(e)).call().await?;
            let user = worm.epochUser(U256::from(e), addr).call().await?;
            let share = if !total.is_zero() {
                user * U256::from(50) * U256::from(10).pow(U256::from(18)) / total
            } else {
                U256::ZERO
            };
            println!(
                "Epoch #{} => {} / {} (Expecting {} WORM)",
                e,
                format_ether(user),
                format_ether(total),
                format_ether(share)
            );
        }
        Ok(())
    }
}
