use super::CommonOpt;
use crate::cli::{get_swap_calldata, utils::check_required_files};
use alloy::primitives::utils::parse_ether;
use anyhow::Result;
use structopt::StructOpt;

#[derive(StructOpt)]
pub struct BurnOpt {
    #[structopt(flatten)]
    common_opt: CommonOpt,
    #[structopt(long)]
    amount: String,
    #[structopt(long, default_value = "0")]
    fee: String,
    #[structopt(long, default_value = "0")]
    spend: String,
}

impl BurnOpt {
    pub async fn run(self, params_dir: &std::path::Path) -> Result<()> {
        check_required_files(params_dir)?;

        let amount = parse_ether(&self.amount)?;
        let fee = parse_ether(&self.fee)?;
        let spend = parse_ether(&self.spend)?;

        // let receiver_hook = get_swap_calldata(
        //     parse_ether("0.0001").unwrap(),
        //     self.common_opt.private_key.address(),
        // );
        let receiver_hook = Vec::new();

        let (
            burn_key,
            burn_addr,
            _nullifier_fp,
            nullifier_u256,
            remaining_coin_val,
            remaining_coin_u256,
            burn_extra_commit,
        ) = self
            .common_opt
            .prepare_inputs(amount, fee, spend, receiver_hook.clone().into())
            .await?;

        let (_tx_hash, _ok) = self.common_opt.send_burn_tx(burn_addr, amount).await?;
        self.common_opt.persist_burn_data(
            params_dir,
            burn_key,
            remaining_coin_val,
            Some(fee),
            Some(spend),
            false,
        )?;
        println!("Your Burn address:{:?}", burn_addr);
        let (proof, block_number, _out_json_path) = self
            .common_opt
            .build_and_prove_burn(
                params_dir,
                burn_addr,
                burn_key,
                spend,
                burn_extra_commit,
                "input.json",
                "witness.wtns",
            )
            .await?;

        self.common_opt
            .broadcast_mint(
                &proof,
                block_number,
                nullifier_u256,
                remaining_coin_u256,
                fee,
                spend,
                receiver_hook.into(),
            )
            .await?;

        Ok(())
    }
}
