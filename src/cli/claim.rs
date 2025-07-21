use super::CommonOpt;
use crate::utils::WORM;
use alloy::{
    primitives::{U256, utils::format_ether},
    providers::ProviderBuilder,
};
use structopt::StructOpt;

#[derive(StructOpt)]
pub struct ClaimOpt {
    #[structopt(flatten)]
    common_opt: CommonOpt,
    #[structopt(long)]
    from_epoch: usize,
    #[structopt(long)]
    num_epochs: usize,
}

impl ClaimOpt {
    pub async fn run(self) -> Result<(), anyhow::Error> {
        let net = self.common_opt.overridden_network()?;
        let addr = self.common_opt.private_key.address();
        let provider = ProviderBuilder::new()
            .wallet(self.common_opt.private_key)
            .connect_http(net.rpc.clone());
        let worm = WORM::new(net.worm, provider.clone());
        let epoch = worm.currentEpoch().call().await?;
        let num_epochs = std::cmp::min(epoch, U256::from(self.num_epochs as u64));
        let receipt = worm
            .claim(U256::from(self.from_epoch as u64), num_epochs)
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
