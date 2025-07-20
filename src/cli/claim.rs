use structopt::StructOpt;

use crate::networks::NETWORKS;

use crate::utils::WORM;

use alloy::{
    primitives::{U256, utils::format_ether},
    providers::ProviderBuilder,
    signers::local::PrivateKeySigner,
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
