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
use anyhow::{Result};

#[derive(StructOpt)]
pub struct MineOpt {
    #[structopt(flatten)]
    common_opt: CommonOpt,
    #[structopt(long)]
    min_beth_per_epoch: String,
    #[structopt(long)]
    max_beth_per_epoch: String,
    #[structopt(long)]
    assumed_worm_price: String,
    #[structopt(long)]
    future_epochs: usize,
}

impl MineOpt {
    pub async fn run(self) -> Result<(), anyhow::Error> {
        let assumed_worm_price = parse_ether(&self.assumed_worm_price)?;
        let minimum_beth_per_epoch = parse_ether(&self.min_beth_per_epoch)?;
        let maximum_beth_per_epoch = parse_ether(&self.max_beth_per_epoch)?;
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
            // WORM miner equation:
            // userShare / (userShare + totalShare) * 50 * assumedWormPrice >= userShare
            // if totalShare > 0 => userShare = 50 * assumedWormPrice - totalShare
            // if totalShare = 0 => userShare = minimumBethPerEpoch
            
            loop {
                let result = async {
                    let epoch = worm.currentEpoch().call().await?;
                    let previous_epoch = epoch.saturating_sub(U256::ONE);
                    let previous_total_share = worm.epochTotal(previous_epoch).call().await?;
                    let current_total_share = worm.epochTotal(epoch).call().await?;
                    let current_user_share = worm.epochUser(epoch, addr).call().await?;
                    let user_share = std::cmp::min(
                        std::cmp::max(
                            if current_total_share.is_zero() {
                                minimum_beth_per_epoch
                            } else {
                                (U256::from(50) * assumed_worm_price)
                                    .saturating_sub(previous_total_share)
                            },
                            minimum_beth_per_epoch,
                        ),
                        maximum_beth_per_epoch,
                    )
                    .saturating_sub(current_user_share);

                    let num_epochs_to_check = std::cmp::min(epoch, U256::from(10));
                    let claimable_worm = worm
                        .calculateMintAmount(
                            epoch.saturating_sub(num_epochs_to_check),
                            num_epochs_to_check,
                            addr,
                        )
                        .call()
                        .await?;

                    if user_share >= minimum_beth_per_epoch {
                        println!(
                            "Participating {} x {} for epochs {}..{}",
                            self.future_epochs,
                            format_ether(user_share),
                            epoch,
                            epoch + U256::from(self.future_epochs)
                        );
                        let receipt = worm
                            .participate(user_share, U256::from(self.future_epochs as u64))
                            .send()
                            .await?
                            .get_receipt()
                            .await?;
                        if receipt.status() {
                            println!("Success!");
                        }

                        if !(epoch % U256::from(10)).is_zero() && claimable_worm.is_zero() {
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
                    Ok::<(), anyhow::Error>(())
                }
                .await;
                if let Err(e) = result {
                    eprintln!("Something went wrong. Error: {}", e);
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
                println!("Success! Continuing...");
                
            }
        }
        Ok(())
    }
}
