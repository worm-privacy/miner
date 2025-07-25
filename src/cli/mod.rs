mod burn;
mod claim;
mod generate_witness;
mod info;
mod mine;
mod participate;
mod recover;
mod utils;
use crate::utils::{RapidsnarkOutput, input_file};
use alloy::{signers::local::PrivateKeySigner};
use anyhow::anyhow;
use reqwest::Url;
use structopt::StructOpt;

use alloy::{
    primitives::{
        Address, U256,
    },
    providers::{Provider, ProviderBuilder},
};

use anyhow::Result;

#[derive(StructOpt)]
pub struct CommonOpt {
    #[structopt(long, default_value = "anvil")]
    network: String,
    #[structopt(long)]
    private_key: PrivateKeySigner,
    #[structopt(long)]
    custom_rpc: Option<Url>,
}
use crate::fp::{Fp,};
use crate::utils::BETH;
use std::path::Path;

#[derive(Debug)]
pub struct RuntimeContext<P: Provider> {
    pub network: Network,
    pub wallet_address: Address,
    pub provider: P,
}

impl CommonOpt {
    pub fn overridden_network(&self) -> Result<Network, anyhow::Error> {
        let mut net = NETWORKS
            .get(&self.network)
            .ok_or(anyhow!("Network not found!"))?
            .clone();
        if let Some(custom_rpc) = &self.custom_rpc {
            net.rpc = custom_rpc.clone();
        }
        Ok(net)
    }
    pub async fn setup(&self) -> Result<RuntimeContext<impl Provider>, anyhow::Error> {
        let mut net = NETWORKS
            .get(&self.network)
            .ok_or(anyhow!("Network not found!"))?
            .clone();
        if let Some(custom_rpc) = &self.custom_rpc {
            net.rpc = custom_rpc.clone();
        }
        let wallet_addr = self.private_key.address();
        let provider = ProviderBuilder::new()
            .wallet(self.private_key.clone())
            .connect_http(net.rpc.clone());
        if provider.get_code_at(net.beth).await?.0.is_empty() {
            panic!("BETH contract does not exist!");
        }
        Ok(RuntimeContext {
            network: net,
            wallet_address: wallet_addr,
            provider,
        })
    }

    pub async fn broadcast_mint<P: Provider>(
        &self,
        net: &Network,
        provider: P,
        proof: &RapidsnarkOutput,
        block_number: u64,
        nullifier: U256,
        remaining_coin: U256,
        fee: U256,
        spend: U256,
        wallet_addr: Address,
    ) -> Result<()> {
        println!("Broadcasting mint transaction...");
        // instantiate your BETH binding
        let beth = BETH::new(net.beth, provider);

        // call the zk-proof mintCoin(...) method
        let receipt = beth
            .mintCoin(
                // pi_a
                [proof.proof.pi_a[0], proof.proof.pi_a[1]],
                // pi_b (flipped coordinates)
                [
                    [proof.proof.pi_b[0][1], proof.proof.pi_b[0][0]],
                    [proof.proof.pi_b[1][1], proof.proof.pi_b[1][0]],
                ],
                // pi_c
                [proof.proof.pi_c[0], proof.proof.pi_c[1]],
                // block number as U256
                U256::from(block_number),
                // nullifier & remaining_coin
                nullifier,
                remaining_coin,
                // fee & spend
                fee,
                spend,
                // user’s address
                wallet_addr,
            )
            .send()
            .await?
            .get_receipt()
            .await?;

        if receipt.status() {
            println!("Success!");
        } else {
            println!("Transaction failed!");
        }
        Ok(())
    }

    pub async fn generate_input_file<P: Provider>(
        &self,
        provider: &P,
        header_bytes: Vec<u8>,
        burn_addr: Address,
        burn_key: Fp,
        fee: U256,
        spend: U256,
        wallet_addr: Address,
        input_path: impl AsRef<Path>,
    ) -> Result<()> {
        let proof = provider.get_proof(burn_addr, vec![]).await?;
        let json = input_file(proof, header_bytes, burn_key, fee, spend, wallet_addr)?.to_string();
        let path = input_path.as_ref();
        println!("Generating input.json file at: {}", path.display());
        std::fs::write(path, json)?;
        Ok(())
    }
}

use crate::networks::{NETWORKS, Network};
pub use burn::BurnOpt;
pub use claim::ClaimOpt;
pub use generate_witness::GenerateWitnessOpt;
pub use info::InfoOpt;
pub use mine::MineOpt;
pub use participate::ParticipateOpt;
pub use recover::RecoverOpt;
