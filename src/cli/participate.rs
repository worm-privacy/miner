use crate::utils::{BETH, WORM};
use alloy::{
    primitives::{U256, utils::parse_ether},
    providers::ProviderBuilder,
};
use structopt::StructOpt;
use super::CommonOpt;

#[derive(StructOpt)]
pub struct ParticipateOpt {
    #[structopt(flatten)]
    common_opt: CommonOpt,
    #[structopt(long)]
    amount_per_epoch: String,
    #[structopt(long)]
    num_epochs: usize,
}

impl ParticipateOpt {
    pub async fn run(self) -> Result<(), anyhow::Error> {
        let net = self.common_opt.overridden_network()?;
        let provider = ProviderBuilder::new()
            .wallet(self.common_opt.private_key)
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
