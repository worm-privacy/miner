
use super::CommonOpt;
use crate::cli::utils::{check_required_files};
use crate::fp::{Fp,};


use alloy::primitives::{U256, Address, utils::parse_ether};
use anyhow::{Result, Context, anyhow, bail};
use serde_json::Value;
use std::{fs, str::FromStr};
use structopt::StructOpt;

use ff::PrimeField;

#[derive(StructOpt)]
pub struct SpendOpt {
    #[structopt(flatten)]
    common_opt: CommonOpt,
    #[structopt(long, default_value = "anvil")]
    id: String,
    #[structopt(long)]
    amount: String,
    #[structopt(long)]
    fee: String,
    #[structopt(long)]
    receiver: Address,
}

impl SpendOpt {
    pub async fn run(self, params_dir: &std::path::Path) -> Result<(), anyhow::Error> {
        println!("starting spend operation...");
        check_required_files(params_dir)?;

        let coins_path = params_dir.join("coins.json");
        if !coins_path.exists() {
            println!("No coins.json found at {}", coins_path.display());
            return Ok(());
        }
        let data = fs::read_to_string(&coins_path)
            .with_context(|| format!("failed to read {}", coins_path.display()))?;
        let json: Value = serde_json::from_str(&data)
            .with_context(|| format!("failed to parse {} as JSON", coins_path.display()))?;
        let arr = json.as_array().with_context(|| {
            format!("expected {} to be a JSON array", coins_path.display())
        })?;

        let coin = arr
            .iter()
            .find(|obj| {
                obj.get("id").map_or(false, |v| match v {
                    Value::String(s) => s == &self.id,
                    Value::Number(n) => n.to_string() == self.id,
                    _ => false,
                })
            })
            .ok_or_else(|| {
                anyhow!(
                    "no coin with id {} found in {}",
                    self.id,
                    coins_path.display()
                )
            })?;
        println!("{}", serde_json::to_string_pretty(coin)?);

        let burn_key_str = match coin.get("burnKey") {
            Some(Value::String(key)) => key.clone(),
            _ => bail!("burn_key not found in the coin object"),
        };
        let original_amount_str = match coin.get("amount") {
            Some(Value::String(amount)) => amount.clone(),
            _ => bail!("amount not found in the coin object"),
        };

        let burn_key_fp = Fp::from_str_vartime(&burn_key_str.to_string()).unwrap();
        let original_amount_u256 =
            U256::from_str(&original_amount_str).expect("Invalid U256 string");

        let fee = parse_ether(&self.fee)?;
        let out_amount = parse_ether(&self.amount)?;

        let (previous_coin_u256, remaining_coin_val_fp, remaining_coin_u256) =
            self.common_opt
                .spend_prepare_from_coin(burn_key_fp, original_amount_u256, out_amount, fee)?;

        let proof = self
            .common_opt
            .build_and_prove_spend(
                params_dir,
                burn_key_fp,
                original_amount_u256,
                out_amount,
                fee,
                self.receiver,
                "spend_input.json",
                "spend_witness.wtns",
            )
            .await?;
        
        self.common_opt
            .persist_burn_data(params_dir, burn_key_fp, remaining_coin_val_fp,None,None,true)?;

        self.common_opt
            .broadcast_spend(
                &proof,
                previous_coin_u256,
                out_amount,
                remaining_coin_u256,
                fee,
                self.receiver,
            )
            .await?;
        
        Ok(())
    }
}
