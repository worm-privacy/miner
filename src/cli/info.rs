use structopt::StructOpt;

use crate::networks::NETWORKS;

use crate::utils::{BETH, WORM};

use alloy::{
    primitives::{U256, utils::format_ether},
    providers::ProviderBuilder,
    signers::local::PrivateKeySigner,
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
