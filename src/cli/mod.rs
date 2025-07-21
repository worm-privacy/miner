mod burn;
mod claim;
mod generate_witness;
mod info;
mod mine;
mod participate;
mod recover;

use alloy::signers::local::PrivateKeySigner;
use anyhow::anyhow;
use reqwest::Url;
use structopt::StructOpt;

#[derive(StructOpt)]
pub struct CommonOpt {
    #[structopt(long, default_value = "anvil")]
    network: String,
    #[structopt(long)]
    private_key: PrivateKeySigner,
    #[structopt(long)]
    custom_rpc: Option<Url>,
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
}

pub use burn::BurnOpt;
pub use claim::ClaimOpt;
pub use generate_witness::GenerateWitnessOpt;
pub use info::InfoOpt;
pub use mine::MineOpt;
pub use participate::ParticipateOpt;
pub use recover::RecoverOpt;

use crate::networks::{NETWORKS, Network};
