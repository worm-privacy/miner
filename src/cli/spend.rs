use crate::constants::{poseidon_coin_prefix, poseidon_nullifier_prefix};
use crate::fp::{Fp, FpRepr};
use crate::poseidon::poseidon3;
use crate::utils::RapidsnarkOutput;
use alloy::primitives::Address;
use alloy::rlp::Encodable;
use anyhow::{Context, bail};
use std::str::FromStr;

use super::CommonOpt;
use crate::cli::utils::{
    append_coin_entry, check_required_files, coins_file, init_coins_file, next_coin_id,
};
use crate::utils::BETH;
use alloy::{
    eips::BlockId,
    primitives::{U256, utils::parse_ether},
    providers::Provider,
};
use anyhow::Result;
use anyhow::{Ok, anyhow};
use ff::PrimeField;
use serde_json::Value;
use std::fs;

use std::process::Command;
use structopt::StructOpt;

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
        let runtime_context = self.common_opt.setup().await?;
        let net = runtime_context.network;
        let provider = runtime_context.provider;
        let input_json_path = "spend_input.json";
        let witness_path = "spend_witness.wtns";
        let coins_json_path = "coins.json";
        let zkey_path = params_dir.join("spend.zkey");
        let coins_path = params_dir.join(coins_json_path);
        let coin_constant = poseidon_coin_prefix();
        // 1.get burn key from coins.json

        if !coins_path.exists() {
            println!("No coins.json found at {}", coins_path.display());
            return Ok(());
        }
        let data = fs::read_to_string(&coins_path)
            .with_context(|| format!("failed to read {}", coins_path.display()))?;

        let json: Value = serde_json::from_str(&data)
            .with_context(|| format!("failed to parse {} as JSON", coins_path.display()))?;

        let arr = json
            .as_array()
            .with_context(|| format!("expected {} to be a JSON array", coins_path.display()))?;

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
        let burn_key = match coin.get("burnKey") {
            Some(Value::String(key)) => key.clone(),
            _ => bail!("burn_key not found in the coin object"),
        };
        /*

            2. generate previous coin


        */
        let original_amount = match coin.get("amount") {
            Some(Value::String(amount)) => amount.clone(),
            _ => bail!("amount not found in the coin object"),
        };
        let original_amount_u256 = U256::from_str(&original_amount).expect("Invalid U256 string");
        let burn_key_fp = Fp::from_str_vartime(&burn_key.to_string()).unwrap();
        let previous_coin_val =
            Fp::from_repr(FpRepr(original_amount_u256.to_le_bytes::<32>())).unwrap();

        let previous_coin = poseidon3(coin_constant, burn_key_fp, previous_coin_val);
        let previous_coin_u256 = U256::from_le_bytes(previous_coin.to_repr().0);
        /*

          3. compare prevous coin amount with new amount and fee


        */

        let fee = parse_ether(&self.fee)?;
        let amount = parse_ether(&self.amount)?;
        if amount + fee > original_amount_u256 {
            return Err(anyhow!(
                "Sum of --fee and --amount should be less than the original amount!"
            ));
        }
        /*

          4. generate remaining coin

        */
        let remaining_coin_val = Fp::from_repr(FpRepr(
            (original_amount_u256 - fee - amount).to_le_bytes::<32>(),
        ))
        .unwrap();

        let remaining_coin = poseidon3(coin_constant, burn_key_fp, remaining_coin_val);
        let remaining_coin_u256 = U256::from_le_bytes(remaining_coin.to_repr().0);
        /*
          5. generate input.json file
        */
        let block = provider
            .get_block(BlockId::latest())
            .await?
            .ok_or(anyhow!("Block not found!"))?;

        let mut header_bytes = Vec::new();
        block.header.inner.encode(&mut header_bytes);

        println!("Generating input.json file at: {}", input_json_path);
        self.common_opt
            .write_spend_input_json(
                burn_key_fp,
                &original_amount_u256.to_string(),
                &amount.to_string(),
                &self.receiver.to_string(),
                &fee.to_string(),
                &input_json_path,
            )
            .await?;
        /*
          6. generate witness.wtns file
        */
        let proc_path = std::env::current_exe().expect("Failed to get current exe path");

        println!("Generating witness.wtns file at: {}", witness_path);
        let witness_output = Command::new(&proc_path)
            .arg("generate-witness")
            .arg("spend")
            .arg("--input")
            .arg(input_json_path)
            .arg("--dat")
            .arg(params_dir.join("spend.dat"))
            .arg("--witness")
            .arg(witness_path)
            .output()?;
        println!(
            "stdout:\n{}",
            String::from_utf8_lossy(&witness_output.stdout)
        );
        eprintln!(
            "stderr:\n{}",
            String::from_utf8_lossy(&witness_output.stderr)
        );
        if !witness_output.status.success() {
            return Err(anyhow::anyhow!(
                "generate-witness failed with exit code: {}",
                witness_output.status.code().unwrap_or(-1)
            ));
        }

        /*
          7. generate proof
        */
        println!("Generating proof...");

        let raw_output = Command::new(&proc_path)
            .arg("rapidsnark")
            .arg("--zkey")
            .arg(&zkey_path)
            .arg("--witness")
            .arg(&witness_path)
            .output()
            .with_context(|| format!("Failed to run rapidsnark at {:?}", proc_path))?;

        if !raw_output.status.success() {
            bail!("rapidsnark exited with non-zero status");
        }

        if raw_output.stdout.is_empty() {
            bail!("rapidsnark stdout was empty — cannot parse JSON");
        }
        let stdout_str = String::from_utf8_lossy(&raw_output.stdout);

        let output: RapidsnarkOutput =
            serde_json::from_slice(&raw_output.stdout).with_context(|| {
                format!(
                    "Failed to deserialize rapidsnark output as RapidsnarkOutput:\n{}",
                    stdout_str
                )
            })?;

        println!("Generated proof successfully!");
        /*
          8. send spend transaction
        */
        let beth = BETH::new(net.beth, provider);
        let spend_receipt = beth
            .spendCoin(
                [output.proof.pi_a[0], output.proof.pi_a[1]],
                [
                    [output.proof.pi_b[0][1], output.proof.pi_b[0][0]],
                    [output.proof.pi_b[1][1], output.proof.pi_b[1][0]],
                ],
                [output.proof.pi_c[0], output.proof.pi_c[1]],
                previous_coin_u256,
                amount,
                remaining_coin_u256,
                fee,
                self.receiver,
            )
            .send()
            .await?
            .get_receipt()
            .await?;
        println!(
            "Spend transaction broadcasted successfully! Receipt: {:?}",
            spend_receipt
        );
        if !spend_receipt.status() {
            bail!(
                "Spend transaction failed with status: {:?}",
                spend_receipt.status()
            );
        }
        println!("✓ Spend transaction successful!");
        /*
           9. update coins.json
        */
        init_coins_file(&coins_path)?;
        let remaining_coin_str = U256::from_le_bytes(remaining_coin_val.to_repr().0);
        // 7. update coins.json
        let next_id = next_coin_id(&coins_path)?;
        let new_coin = coins_file(
            next_id,
            burn_key_fp,
            remaining_coin_str,
            &self.common_opt.network,
        )?;
        append_coin_entry(&coins_path, new_coin)?;
        println!("New coin entry added",);

        Ok(())
    }
}
