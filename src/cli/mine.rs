use super::CommonOpt;
use crate::utils::{BETH, WORM};
use alloy::{
    primitives::{
        U256,
        utils::{format_ether, parse_ether},
    },
    providers::{Provider, ProviderBuilder},
};
use std::time::Duration;
use structopt::StructOpt;

#[derive(StructOpt)]
pub struct MineOpt {
    #[structopt(flatten)]
    common_opt: CommonOpt,
    #[structopt(long)]
    amount_per_epoch: String,
    #[structopt(long)]
    num_epochs: usize,
    #[structopt(long)]
    claim_interval: usize,
}

impl MineOpt {
    pub async fn run(self) -> Result<(), anyhow::Error> {
        let amount_per_epoch = parse_ether(&self.amount_per_epoch)?;
        let addr = self.common_opt.private_key.address();
        let net = self.common_opt.overridden_network()?;
        let provider = ProviderBuilder::new()
            .wallet(self.common_opt.private_key)
            .connect_http(net.rpc.clone());
        let worm = WORM::new(net.worm, provider.clone());
        let beth = BETH::new(net.beth, provider.clone());
        if beth.allowance(addr, net.worm).call().await?.is_zero() {
            println!("Approving infinite BETH allowance to WORM contract...");
            let beth_approve_receipt = beth
                .approve(net.worm, U256::MAX)
                .send()
                .await?
                .get_receipt()
                .await?;
            if !beth_approve_receipt.status() {
                panic!("Failed on BETH approval!");
            }
        }
        if beth.balanceOf(addr).call().await?.is_zero() {
            println!(
                "You don't have any BETH! Mine some BETH through the `worm-miner burn` command."
            );
        } else {
            loop {
                let epoch = worm.currentEpoch().call().await?;
                let current_amount = worm.epochUser(epoch, addr).call().await?;

                let num_epochs_to_check = std::cmp::min(epoch, U256::from(self.claim_interval));
                let claimable_worm = worm
                    .calculateMintAmount(
                        epoch.saturating_sub(num_epochs_to_check),
                        num_epochs_to_check,
                        addr,
                    )
                    .call()
                    .await?;

                if current_amount < amount_per_epoch {
                    println!(
                        "Participating {} x {} for epochs {}..{}",
                        self.num_epochs,
                        format_ether(amount_per_epoch),
                        epoch,
                        epoch + U256::from(self.num_epochs)
                    );
                    let receipt = worm
                        .participate(amount_per_epoch, U256::from(self.num_epochs as u64))
                        .send()
                        .await?
                        .get_receipt()
                        .await?;
                    if receipt.status() {
                        println!("Success!");
                    }

                    if !(epoch % U256::from(self.claim_interval)).is_zero()
                        && !claimable_worm.is_zero()
                    {
                        println!("Claiming WORMs...");
                        let receipt = worm
                            .claim(
                                epoch.saturating_sub(num_epochs_to_check),
                                num_epochs_to_check,
                            )
                            .send()
                            .await?
                            .get_receipt()
                            .await?;
                        if receipt.status() {
                            println!("Success!");
                        }
                    }
                }

                let eth_balance = provider.get_balance(addr).await?;
                let beth_balance = beth.balanceOf(addr).call().await?;
                let worm_balance = worm.balanceOf(addr).call().await?;

                println!(
                    "ETH: {} BETH: {} WORM: {} Claimable WORM: {}",
                    format_ether(eth_balance),
                    format_ether(beth_balance),
                    format_ether(worm_balance),
                    format_ether(claimable_worm)
                );

                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }
        Ok(())
    }
}
