use structopt::StructOpt;

use anyhow::{Context, bail};
use serde_json::Value;

use super::CommonOpt;
use crate::cli::utils::check_required_files;

use crate::fp::Fp;
use alloy::{
    hex,
    primitives::{U256, utils::parse_ether},
};
use anyhow::anyhow;
use ff::PrimeField;
use std::fs;

#[derive(StructOpt)]
pub enum RecoverOpt {
    ById {
        #[structopt(flatten)]
        common_opt: CommonOpt,
        #[structopt(long)]
        id: String,
        #[structopt(long)]
        spend: Option<String>,
    },

    Manual {
        #[structopt(flatten)]
        common_opt: CommonOpt,
        #[structopt(long)]
        burn_key: String,
        #[structopt(long)]
        spend: String,
        #[structopt(long)]
        fee: String,
    },
}
impl RecoverOpt {
    pub async fn run(self, params_dir: &std::path::Path) -> Result<(), anyhow::Error> {
        let (raw_burn_key, spend, fee, common_opt) = match self {
            RecoverOpt::Manual {
                burn_key,
                spend,
                fee,
                common_opt,
            } => {
                let fee = parse_ether(&fee)?;
                let spend = parse_ether(&spend)?;
                (burn_key, spend, fee, common_opt)
            }

            RecoverOpt::ById {
                id,
                common_opt,
                spend,
            } => {
                let burn_json_path = "burn.json";

                let burn_path = params_dir.join(burn_json_path);
                if !burn_path.exists() {
                    println!("No coins.json found at {}", burn_path.display());
                    return Ok(());
                }
                let data = fs::read_to_string(&burn_path)
                    .with_context(|| format!("failed to read {}", burn_path.display()))?;

                let json: Value = serde_json::from_str(&data)
                    .with_context(|| format!("failed to parse {} as JSON", burn_path.display()))?;

                let arr = json.as_array().with_context(|| {
                    format!("expected {} to be a JSON array", burn_path.display())
                })?;

                let coin = arr
                    .iter()
                    .find(|obj| {
                        obj.get("id").map_or(false, |v| match v {
                            Value::String(s) => s == &id,
                            Value::Number(n) => n.to_string() == id,
                            _ => false,
                        })
                    })
                    .ok_or_else(|| {
                        anyhow!("no coin with id {} found in {}", id, burn_path.display())
                    })?;
                println!("{}", serde_json::to_string_pretty(coin)?);
                let burn_key = match coin.get("burnKey") {
                    Some(Value::String(key)) => key.clone(),
                    _ => bail!("burn_key not found in the burn object"),
                };
                let fee_str = match coin.get("fee") {
                    Some(Value::String(key)) => key.clone(),
                    _ => bail!("fee not found in the burn object"),
                };
                let fee = fee_str.parse::<U256>()?;
                let stored_spend = match coin.get("spend") {
                    Some(Value::String(key)) => key.clone(),
                    _ => bail!("spend not found in the burn object"),
                };

                let spend = match spend {
                    Some(s) => parse_ether(&s)?,
                    None => stored_spend.parse::<U256>()?,
                };

                (burn_key, spend, fee, common_opt)
            }
        };

        let burn_key = if raw_burn_key.starts_with("0x") {
            let hex = raw_burn_key.strip_prefix("0x").unwrap();
            let bytes = hex::decode(hex)?;
            Fp::from_be_bytes(&bytes)
        } else {
            Fp::from_str_vartime(&raw_burn_key.to_string()).unwrap()
        };

        check_required_files(params_dir)?;

        let receiver_hook = Vec::new();
        let (burn_addr, nullifier_fp, burn_extra_commit) = common_opt
            .recover_prepare_from_key(burn_key, fee, spend, receiver_hook.clone().into(),)
            .await?;

        println!(
            "Your burn-key as string: {}",
            U256::from_le_bytes(burn_key.to_repr().0).to_string()
        );
        println!("Your burn-address is: {}", burn_addr);

        let (remaining_coin_val, remaining_coin_u256) = common_opt
            .recover_check_balance_and_compute_remaining(burn_addr, burn_key, fee, spend)
            .await?;

        let (json_output, block_number, _out_path) = common_opt
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

        common_opt.persist_burn_data(params_dir, burn_key, remaining_coin_val, None, None, true)?;

        let nullifier_u256 = U256::from_le_bytes(nullifier_fp.to_repr().0);
        common_opt
            .broadcast_mint(
                &json_output,
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
