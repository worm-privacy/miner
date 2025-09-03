use alloy::primitives::utils::parse_ether;
use structopt::StructOpt;
use tokio::time::Sleep;

use std::process::Command;
use std::time::Duration;
use anyhow::{Context, bail};
use serde_json::Value;

use std::fs;
use super::CommonOpt;
use crate::cli::utils::{
    append_new_entry, check_required_files, coins_file, init_coins_file, next_id,
};
use crate::constants::{poseidon_burn_address_prefix, poseidon_nullifier_prefix,poseidon_coin_prefix};
use crate::fp::{Fp, FpRepr};
use crate::poseidon::{poseidon2,poseidon3};
use crate::utils::{RapidsnarkOutput, generate_burn_address};

use alloy::rlp::Encodable;
use alloy::{
    eips::BlockId,
    hex::ToHexExt,
    primitives::{B256, U256},
    providers::Provider,
};
use anyhow::anyhow;
use ff::{PrimeField};
use alloy::hex;

#[derive(StructOpt,)]
pub enum RecoverOpt {
    /// Recover using a saved ID (load burn_key, fee, spend from file)
    ById {
        #[structopt(flatten)]
        common_opt: CommonOpt,
        #[structopt(long)]
        id: String,
         #[structopt(long)]
        spend:Option<String>,
    },

    /// Recover by providing burn_key, spend, and fee manually
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
                let spend =  parse_ether(&spend)?;

                (burn_key, spend, fee, common_opt)
            },

            RecoverOpt::ById { id, common_opt,spend } => {
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

                let arr = json
                    .as_array()
                    .with_context(|| format!("expected {} to be a JSON array", burn_path.display()))?;

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
                        anyhow!(
                            "no coin with id {} found in {}",
                            id,
                            burn_path.display()
                        )
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
        let runtime_context = common_opt.setup().await?;
        let net = runtime_context.network;
        let provider = runtime_context.provider;
        let wallet_addr = runtime_context.wallet_address;
        let burn_addr_constant = poseidon_burn_address_prefix();
        let nullifier_constant = poseidon_nullifier_prefix();
        let coin_constant = poseidon_coin_prefix();
        println!("Generating a burn-key...");
        
        
        let burn_addr = generate_burn_address(burn_addr_constant, burn_key, wallet_addr, fee);
        let nullifier = poseidon2(nullifier_constant, burn_key);
        let nullifier_u256 = U256::from_le_bytes(nullifier.to_repr().0);

        let burn_addr_balance = provider.get_balance(burn_addr).await?;
        if burn_addr_balance.is_zero() {
            panic!("No ETH is present in the burn address!");
        }
        let remaining_coin_val = Fp::from_repr(FpRepr((burn_addr_balance - fee - spend).to_le_bytes::<32>())).unwrap();
        let remaining_coin = poseidon3(coin_constant,burn_key,remaining_coin_val);
        let remaining_coin_u256 = U256::from_le_bytes(remaining_coin.to_repr().0);

        println!(
            "Your burn-key as string: {}",
           U256::from_le_bytes(burn_key.to_repr().0).to_string()
        );
        println!("Your burn-address is: {}", burn_addr);

        let block = provider
            .get_block(BlockId::latest())
            .await?
            .ok_or(anyhow!("Block not found!"))?;

        println!("Generated proof for block #{}", block.header.number);

        let input_json_path = "input.json";
        let mut header_bytes = Vec::new();
        block.header.inner.encode(&mut header_bytes);
        println!("Generating input.json file at: {}", input_json_path);
        common_opt
            .generate_input_file(
                &provider,
                header_bytes,
                burn_addr,
                burn_key,
                fee,
                spend,
                wallet_addr,
                input_json_path,
            )
            .await?;

        let witness_path = "witness.wtns";

        let proc_path = std::env::current_exe().expect("Failed to get current exe path");

        println!("Generating witness.wtns file at: {}", witness_path);
        let witness_output = Command::new(&proc_path)
            .arg("generate-witness")
            .arg("proof-of-burn")
            .arg("--input")
            .arg(input_json_path)
            .arg("--dat")
            .arg(params_dir.join("proof_of_burn.dat"))
            .arg("--witness")
            .arg(witness_path)
            .output()?;

        witness_output.status.success().then_some(()).ok_or_else(|| {
            anyhow!(
                "Failed to generate witness file: {}",
                String::from_utf8_lossy(&witness_output.stderr)
            )
        })?;
        println!("Generating proof...");
        let output = Command::new(&proc_path)
            .arg("rapidsnark")
            .arg("--zkey")
            .arg(params_dir.join("proof_of_burn.zkey"))
            .arg("--witness")
            .arg(witness_path)
            .output()?;

        output.status.success().then_some(()).ok_or_else(|| {
            anyhow!(
                "Failed to generate proof: {}",
                String::from_utf8_lossy(&output.stderr)
            )
        })?;

        let json_output: RapidsnarkOutput = serde_json::from_slice(&output.stdout)?;
        println!("Generated proof successfully! {:?}", output);
        let coins_json_path = "coins.json";
        let coins_path = params_dir.join(coins_json_path);
        println!("Generating coins.json file at: {}", coins_path.display());
        init_coins_file(&coins_path)?;
        let remaining_coin_str = U256::from_le_bytes(remaining_coin_val.to_repr().0);

        let next_id = next_id(&coins_path)?;
        let new_coin = coins_file(
            next_id,
            burn_key,
            remaining_coin_str,
            &common_opt.network,
        )?;
        append_new_entry(&coins_path, new_coin)?;
        println!("Broadcasting mint transaction...");
        let result = common_opt
            .broadcast_mint(
                &net,
                provider,
                &json_output,        
                block.header.number,
                nullifier_u256,
                remaining_coin_u256,
                fee,
                spend,
                wallet_addr,
            )
            .await;
        match &result {
            Ok(_) => {
                println!(
                    "broadcast_mint succeeded (block: {}, nullifier: {:?})",
                    block.header.number, nullifier_u256,
                );
            }
            Err(e) => {
                eprintln!(
                    "broadcast_mint failed: {} (block: {}, nullifier: {:?})",
                    e, block.header.number, nullifier_u256,
                );
            }
        }
        Ok(())
    }
}
