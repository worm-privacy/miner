use structopt::StructOpt;

use std::process::Command;

use super::CommonOpt;
use crate::cli::utils::check_required_files;
use crate::fp::{Fp, FpRepr};
use crate::poseidon2::poseidon2;
use crate::utils::{RapidsnarkOutput, generate_burn_address};
use alloy::rlp::Encodable;
use alloy::{
    eips::BlockId,
    hex::ToHexExt,
    primitives::{B256, U256},
    providers::{Provider},
};
use anyhow::anyhow;
use ff::{Field, PrimeField};

#[derive(StructOpt)]
pub struct RecoverOpt {
    #[structopt(flatten)]
    common_opt: CommonOpt,
    #[structopt(long)]
    burn_key: String,
}

impl RecoverOpt {
    pub async fn run(self, params_dir: &std::path::Path) -> Result<(), anyhow::Error> {
        check_required_files(params_dir)?;
        let runtime_context = self.common_opt.setup().await?;
        let net = runtime_context.network;
        let provider = runtime_context.provider;
        let wallet_addr = runtime_context.wallet_address;

        println!("Generating a burn-key...");
        let burn_key = Fp::from_repr(FpRepr(
            U256::from_str_radix(&self.burn_key, 16)?.to_le_bytes(),
        ))
        .into_option()
        .ok_or(anyhow!("Cannot parse burn-key!"))?;

        let burn_addr = generate_burn_address(burn_key, wallet_addr);
        let nullifier = poseidon2([burn_key, Fp::from(1)]);

        let burn_addr_balance = provider.get_balance(burn_addr).await?;

        if burn_addr_balance.is_zero() {
            panic!("No ETH is present in the burn address!");
        }
        let fee = U256::ZERO;
        let spend = burn_addr_balance;

        let remaining_coin = poseidon2([burn_key, Fp::ZERO]);

        println!(
            "Your burn-key: {}",
            B256::from(U256::from_le_bytes(burn_key.to_repr().0)).encode_hex()
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
        self.common_opt
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

        let witness_path = "witness.wtns"; //proof_dir.path().join("witness.wtns");

        let proc_path = std::env::current_exe().expect("Failed to get current exe path");

        println!("Generating witness.wtns file at: {}", witness_path);
        let output = Command::new(&proc_path)
            .arg("generate-witness")
            .arg("proof-of-burn")
            .arg("--input")
            .arg(input_json_path)
            .arg("--dat")
            .arg(params_dir.join("proof_of_burn.dat"))
            .arg("--witness")
            .arg(witness_path)
            .output()?;

        output.status.success().then_some(()).ok_or_else(|| {
            anyhow!(
                "Failed to generate witness file: {}",
                String::from_utf8_lossy(&output.stderr)
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

        let nullifier_u256 = U256::from_le_bytes(nullifier.to_repr().0);
        let remaining_coin_u256 = U256::from_le_bytes(remaining_coin.to_repr().0);

        println!("Broadcasting mint transaction...");
        let _result = self
            .common_opt
            .broadcast_mint(
                &net,
                provider,
                &json_output,        // RapidsnarkOutput
                block.header.number, // u64
                nullifier_u256,
                remaining_coin_u256,
                fee,
                spend,
                wallet_addr,
            )
            .await;

        Ok(())
    }
}
