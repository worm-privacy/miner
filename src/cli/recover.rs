use structopt::StructOpt;

use std::process::Command;

use super::CommonOpt;
use crate::fp::{Fp, FpRepr};
use crate::poseidon2::poseidon2;
use crate::utils::BETH;
use crate::utils::{RapidsnarkOutput, generate_burn_address, input_file};
use alloy::rlp::Encodable;
use alloy::{
    eips::BlockId,
    hex::ToHexExt,
    primitives::{B256, U256},
    providers::{Provider, ProviderBuilder},
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
        let net = self.common_opt.overridden_network()?;

        let required_files = [
            "proof_of_burn.dat",
            "proof_of_burn.zkey",
            "spend.dat",
            "spend.zkey",
        ];

        for req_file in required_files {
            let full_path = params_dir.join(req_file);
            if !std::fs::exists(&full_path)? {
                panic!(
                    "File {} does not exist! Make sure you have downloaded all required files through `make download_params`!",
                    full_path.display()
                );
            }
        }

        let wallet_addr = self.common_opt.private_key.address();

        let provider = ProviderBuilder::new()
            .wallet(self.common_opt.private_key)
            .connect_http(net.rpc.clone());

        if provider.get_code_at(net.beth).await?.0.is_empty() {
            panic!("BETH contract does not exist!");
        }

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
        let mut header_bytes = Vec::new();
        block.header.inner.encode(&mut header_bytes);
        let proof = provider.get_proof(burn_addr, vec![]).await?;

        let input_json_path = "input.json";
        let witness_path = "witness.wtns"; //proof_dir.path().join("witness.wtns");

        println!("Generating input.json file at: {}", input_json_path);
        std::fs::write(
            &input_json_path,
            input_file(
                proof,
                header_bytes,
                burn_key,
                U256::ZERO,
                burn_addr_balance,
                wallet_addr,
            )?
            .to_string(),
        )?;

        let proc_path = std::env::current_exe().expect("Failed to get current exe path");

        println!("Generating witness.wtns file at: {}", witness_path);
        Command::new(&proc_path)
            .arg("generate-witness")
            .arg("proof-of-burn")
            .arg("--input")
            .arg(input_json_path)
            .arg("--dat")
            .arg(params_dir.join("proof_of_burn.dat"))
            .arg("--witness")
            .arg(witness_path)
            .output()?;

        println!("Generating proof...");
        let output: RapidsnarkOutput = serde_json::from_slice(
            &Command::new(&proc_path)
                .arg("rapidsnark")
                .arg("--zkey")
                .arg(params_dir.join("proof_of_burn.zkey"))
                .arg("--witness")
                .arg(witness_path)
                .output()?
                .stdout,
        )?;

        println!("Generated proof successfully! {:?}", output);

        println!("Broadcasting mint transaction...");
        let beth = BETH::new(net.beth, provider);
        let mint_receipt = beth
            .mintCoin(
                [output.proof.pi_a[0], output.proof.pi_a[1]],
                [
                    [output.proof.pi_b[0][1], output.proof.pi_b[0][0]],
                    [output.proof.pi_b[1][1], output.proof.pi_b[1][0]],
                ],
                [output.proof.pi_c[0], output.proof.pi_c[1]],
                U256::from(block.header.number),
                U256::from_le_bytes(nullifier.to_repr().0),
                U256::from_le_bytes(remaining_coin.to_repr().0),
                U256::ZERO,
                burn_addr_balance,
                wallet_addr,
            )
            .send()
            .await?
            .get_receipt()
            .await?;

        if mint_receipt.status() {
            println!("Success!");
        }

        Ok(())
    }
}
