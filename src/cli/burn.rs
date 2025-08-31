use super::CommonOpt;
use crate::cli::utils::{
    append_coin_entry, check_required_files, coins_file, init_coins_file, next_coin_id,
};
use crate::constants::{
    poseidon_burn_address_prefix, poseidon_coin_prefix, poseidon_nullifier_prefix,
};
use crate::fp::{Fp, FpRepr};
use crate::poseidon::{poseidon2, poseidon3};
use crate::utils::{RapidsnarkOutput, find_burn_key, generate_burn_address};
use alloy::rlp::Encodable;
use alloy::{
    eips::BlockId,
    hex::ToHexExt,
    network::TransactionBuilder,
    primitives::{
        U256,
        utils::{format_ether, parse_ether},
    },
    providers::Provider,
    rpc::types::TransactionRequest,
};
use anyhow::anyhow;
use ff::PrimeField;
use std::process::Command;
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
    pub async fn run(self, params_dir: &std::path::Path) -> Result<(), anyhow::Error> {
        check_required_files(params_dir)?;
        let runtime_context = self.common_opt.setup().await?;
        let net = runtime_context.network;
        let provider = runtime_context.provider;
        let wallet_addr = runtime_context.wallet_address;
        let burn_addr_constant = poseidon_burn_address_prefix();
        let nullifier_constant = poseidon_nullifier_prefix();
        let coin_constant = poseidon_coin_prefix();
        let fee = parse_ether(&self.fee)?;
        let spend = parse_ether(&self.spend)?;
        let amount = parse_ether(&self.amount)?;

        if amount > parse_ether("1")? {
            return Err(anyhow!("Can't burn more than 1 ETH!"));
        }

        if fee + spend > amount {
            return Err(anyhow!(
                "Sum of --fee and --spend should be less than --amount!"
            ));
        }

        println!("Generating a burn-key...");
        let burn_key = find_burn_key(3, wallet_addr, fee);

        let burn_addr = generate_burn_address(burn_addr_constant, burn_key, wallet_addr, fee);
        let nullifier = poseidon2(nullifier_constant, burn_key);

        let remaining_coin_val =
            Fp::from_repr(FpRepr((amount - fee - spend).to_le_bytes::<32>())).unwrap();

        let remaining_coin = poseidon3(coin_constant, burn_key, remaining_coin_val);
        let remaining_coin_u256 = U256::from_le_bytes(remaining_coin.to_repr().0);

        let nonce = provider.get_transaction_count(wallet_addr).await?;

        // Build a transaction to send 100 wei from Alice to Bob.
        // The `from` field is automatically filled to the first signer's address (Alice).
        let tx = TransactionRequest::default()
            .with_to(burn_addr)
            .with_nonce(nonce)
            .with_chain_id(provider.get_chain_id().await?)
            .with_value(amount)
            .with_gas_limit(21_000)
            .with_max_priority_fee_per_gas(1_000_000_000)
            .with_max_fee_per_gas(20_000_000_000);

        // Send the transaction and wait for the broadcast.
        let pending_tx = provider.send_transaction(tx).await?;
        let tx_hash = pending_tx.tx_hash().encode_hex();
        let receipt = pending_tx.get_receipt().await?;
        if receipt.status() {
            println!(
                "Successfully burnt {} ETH! Tx-hash: {}",
                format_ether(amount),
                tx_hash
            );
        } else {
            println!("Burn failed! Tx-hash: {}", tx_hash);
        }

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

        let witness_path = "witness.wtns";

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

        let coins_json_path = "coins.json";
        let coins_path = params_dir.join(coins_json_path);
        println!("Generating coins.json file at: {}", coins_path.display());
        init_coins_file(&coins_path)?;
        let remaining_coin_str = U256::from_le_bytes(remaining_coin_val.to_repr().0);

        let next_id = next_coin_id(&coins_path)?;
        let new_coin = coins_file(
            next_id,
            burn_key,
            remaining_coin_str,
            &self.common_opt.network,
        )?;
        append_coin_entry(&coins_path, new_coin)?;
        println!("New coin entry added",);
        output.status.success().then_some(()).ok_or_else(|| {
            anyhow!(
                "Failed to generate proof: {}",
                String::from_utf8_lossy(&output.stderr)
            )
        })?;
        let json_output: RapidsnarkOutput = serde_json::from_slice(&output.stdout)?;
        println!("Generated proof successfully!");

        let nullifier_u256 = U256::from_le_bytes(nullifier.to_repr().0);

        println!("Broadcasting mint transaction...");
        let result = self
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
