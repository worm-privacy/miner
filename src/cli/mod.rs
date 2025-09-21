mod burn;
mod claim;
mod generate_witness;
mod info;
mod ls;
mod mine;
mod participate;
mod recover;
mod spend;
mod utils;
use crate::fp::Fp;
use crate::utils::{RapidsnarkOutput,build_and_prove_burn};
use alloy::signers::local::PrivateKeySigner;
use alloy::{
    primitives::{Address, U256},
    providers::{Provider, ProviderBuilder},
};
use anyhow::anyhow;
use reqwest::Url;
use serde_json::json;
use structopt::StructOpt;
use crate::cli::utils::{
    append_new_entry, burn_file, coins_file, init_coins_file, next_id,
};
use crate::constants::{
    poseidon_burn_address_prefix,
};
use crate::utils::{find_burn_key, generate_burn_address,compute_nullifier,compute_remaining_coin,compute_previous_coin,fetch_block_and_header_bytes};
use alloy::{
    hex::ToHexExt,
    network::TransactionBuilder,
    primitives::{
        utils::{format_ether, parse_ether},
    },
    rpc::types::TransactionRequest,
};
use ff::PrimeField;
use std::path::PathBuf;

use anyhow::Result;

#[derive(StructOpt)]
pub struct CommonOpt {
    #[structopt(long, default_value = "anvil")]
    network: String,
    #[structopt(long)]
    private_key: PrivateKeySigner,
    #[structopt(long)]
    custom_rpc: Option<Url>,
}
use crate::utils::BETH;
use std::path::Path;

#[derive(Debug)]
pub struct RuntimeContext<P: Provider> {
    pub network: Network,
    pub wallet_address: Address,
    pub provider: P,
}

impl CommonOpt {
    pub fn overridden_network(&self) -> Result<Network, anyhow::Error> {
        let mut net = NETWORKS
            .get(&self.network)
            .ok_or(anyhow!("Network not found!"))?
            .clone();
        if let Some(custom_rpc) = &self.custom_rpc {
            net.rpc = custom_rpc.clone();
        }
        Ok(net)
    }
    pub async fn setup(&self) -> Result<RuntimeContext<impl Provider>, anyhow::Error> {
        let mut net = NETWORKS
            .get(&self.network)
            .ok_or(anyhow!("Network not found!"))?
            .clone();
        if let Some(custom_rpc) = &self.custom_rpc {
            net.rpc = custom_rpc.clone();
        }
        let wallet_addr = self.private_key.address();
        let provider = ProviderBuilder::new()
            .wallet(self.private_key.clone())
            .connect_http(net.rpc.clone());
        if provider.get_code_at(net.beth).await?.0.is_empty() {
            panic!("BETH contract does not exist!");
        }
        Ok(RuntimeContext {
            network: net,
            wallet_address: wallet_addr,
            provider,
        })
    }

    pub async fn broadcast_mint(
        &self,
        proof: &RapidsnarkOutput,
        block_number: u64,
        nullifier: U256,
        remaining_coin: U256,
        fee: U256,
        spend: U256,
    ) -> Result<()> {
        let rt = self.setup().await?; // get provider, wallet, network from self
        println!("Broadcasting mint transaction...");
        let net = &rt.network;
        // instantiate your BETH binding
        let beth = BETH::new(net.beth, rt.provider);

        // call the zk-proof mintCoin(...) method
        let receipt = beth
            .mintCoin(
                // pi_a
                [proof.proof.pi_a[0], proof.proof.pi_a[1]],
                // pi_b (flipped coordinates)
                [
                    [proof.proof.pi_b[0][1], proof.proof.pi_b[0][0]],
                    [proof.proof.pi_b[1][1], proof.proof.pi_b[1][0]],
                ],
                // pi_c
                [proof.proof.pi_c[0], proof.proof.pi_c[1]],
                // block number as U256
                U256::from(block_number),
                // nullifier & remaining_coin
                nullifier,
                remaining_coin,
                // fee & spend
                fee,
                spend,
                // user’s address
                rt.wallet_address,
            )
            .send()
            .await?
            .get_receipt()
            .await?;

        if receipt.status() {
            println!("Success!");
        } else {
            println!("Transaction failed!");
        }
        Ok(())
    }
    pub async fn broadcast_spend(
        &self,
        proof: &crate::utils::RapidsnarkOutput,
        previous_coin: U256,
        out_amount: U256,
        remaining_coin: U256,
        fee: U256,
        receiver: alloy::primitives::Address,
    ) -> anyhow::Result<()> {
        let rt = self.setup().await?;
        let beth = crate::utils::BETH::new(rt.network.beth, rt.provider);
        let receipt = beth
            .spendCoin(
                [proof.proof.pi_a[0], proof.proof.pi_a[1]],
                [
                    [proof.proof.pi_b[0][1], proof.proof.pi_b[0][0]],
                    [proof.proof.pi_b[1][1], proof.proof.pi_b[1][0]],
                ],
                [proof.proof.pi_c[0], proof.proof.pi_c[1]],
                previous_coin,
                out_amount,
                remaining_coin,
                fee,
                receiver,
            )
            .send()
            .await?
            .get_receipt()
            .await?;
        if !receipt.status() {
            return Err(anyhow::anyhow!("Spend transaction failed"));
        }
        println!("✓ Spend transaction successful!");
        Ok(())
    }

    
    pub async fn write_spend_input_json<P: AsRef<Path>>(
        &self,
        burn_key: Fp,
        balance: &str,
        withdrawn_balance: &str,
        receiver_address: &str,
        fee: &str,
        output_path: P,
    ) -> std::io::Result<()> {
        println!(
            "Generating spend input JSON file at: {}",
            output_path.as_ref().display()
        );
        let json_value = json!({
            "burnKey": U256::from_le_bytes(burn_key.to_repr().0).to_string(),
            "balance": balance,
            "withdrawnBalance": withdrawn_balance,
            "receiverAddress": receiver_address,
            "fee": fee
        });

        let json_str = serde_json::to_string_pretty(&json_value)?;
        std::fs::write(output_path, json_str)?;
        println!("Spend input JSON file generated successfully.");
        Ok(())
    }


    pub async fn prepare_inputs(
        &self,
        amount: U256,
        fee: U256,
        spend: U256,
    ) -> Result<(Fp, Address, Fp,U256, Fp, U256)> {
        let rt = self.setup().await?;

        if fee + spend > amount {
            return Err(anyhow!("Sum of --fee and --spend should be less than --amount!"));
        }
        if amount > parse_ether("1")? {
            return Err(anyhow!("Can't burn more than 1 ETH in a single call!"));
        }

        // 1) burn_key
        println!("Generating a burn-key...");
        let burn_key = find_burn_key(3, rt.wallet_address, fee);
        println!("Your burn_key: {:?}", burn_key);
        println!(
            "Your burn-key as string: {}",
            U256::from_le_bytes(burn_key.to_repr().0).to_string()
        );

        // 2) burn address
        let burn_addr_prefix = poseidon_burn_address_prefix();
        let burn_addr = generate_burn_address(burn_addr_prefix, burn_key, rt.wallet_address, fee);

        // 3) nullifier (Fp only needed by caller)
        let (nullifier_fp, nullifier_u256) = compute_nullifier(burn_key);

        // 4) remaining coin (both fp and u256)
        let (remaining_coin_val_fp, remaining_coin_u256) =
            compute_remaining_coin(burn_key, amount, fee, spend)?;

        Ok((
            burn_key,
            burn_addr,
            nullifier_fp,
            nullifier_u256,
            remaining_coin_val_fp,
            remaining_coin_u256,
        ))
    }

    pub async fn recover_prepare_from_key(
        &self,
        burn_key: Fp,
        fee: U256,
    ) -> anyhow::Result<(alloy::primitives::Address, Fp)> {
        let rt = self.setup().await?; // wallet addr + provider + network
        let burn_addr_prefix = crate::constants::poseidon_burn_address_prefix();

        let burn_addr =
            crate::utils::generate_burn_address(burn_addr_prefix, burn_key, rt.wallet_address, fee);
        let (nullifier_fp, _nullifier_u256) = compute_nullifier(burn_key);
        Ok((burn_addr, nullifier_fp))
    }


    /// 2) Send ETH to burn address & check the receipt
    pub async fn send_burn_tx(
        &self,
        to_burn_addr: Address,
        amount: U256,
    ) -> Result<(String, bool)> {
        let rt = self.setup().await?;

        let nonce = rt.provider.get_transaction_count(rt.wallet_address).await?;
        let tx = TransactionRequest::default()
            .with_to(to_burn_addr)
            .with_nonce(nonce)
            .with_chain_id(rt.provider.get_chain_id().await?)
            .with_value(amount)
            .with_gas_limit(21_000)
            .with_max_priority_fee_per_gas(1_000_000_000)
            .with_max_fee_per_gas(20_000_000_000);

        let pending_tx = rt.provider.send_transaction(tx).await?;
        let tx_hash = pending_tx.tx_hash().encode_hex();
        let receipt = pending_tx.get_receipt().await?;
        if receipt.status() {
            println!(
                "Successfully burnt {} ETH! Tx-hash: {}",
                format_ether(amount),
                tx_hash
            );
            Ok((tx_hash, true))
        } else {
            println!("Burn failed! Tx-hash: {}", tx_hash);
            Ok((tx_hash, false))
        }
    }

    /// 3) Build input.json, generate witness, run rapidsnark
    pub async fn build_and_prove_burn(
        &self,
        params_dir: &Path,
        burn_addr: Address,
        burn_key: Fp,
        fee: U256,
        spend: U256,
        input_json_path: &str,
        witness_path: &str,
    ) -> Result<(RapidsnarkOutput, u64, PathBuf)> {
        let rt = self.setup().await?;

        let (block_number, header_bytes) = fetch_block_and_header_bytes(&rt.provider).await?;

        let (proof, out_path) = build_and_prove_burn(
            &rt.provider,
            params_dir,
            header_bytes,
            burn_addr,
            burn_key,
            fee,
            spend,
            rt.wallet_address,
            input_json_path,
            witness_path,
        )
        .await?;

        Ok((proof, block_number, out_path))
    }

    pub fn persist_burn_data(
        &self,
        params_dir: &Path,
        burn_key: Fp,
        remaining_coin_val: Fp,
        fee: Option<U256>,
        spend: Option<U256>,
        coins_only: bool,
    ) -> Result<()> {
        if !coins_only {
            let (fee, spend) = match (fee, spend) {
                (Some(f), Some(s)) => (f, s),
                _ => return Err(anyhow!("fee and spend are required when coins_only == false")),
            };
            let burn_path = params_dir.join("burn.json");
            init_coins_file(&burn_path)?;
            let burn_id = next_id(&burn_path)?;
            let new_burn = burn_file(burn_id, burn_key, fee, &self.network, spend)?;
            append_new_entry(&burn_path, new_burn)?;
        }

        // Always write coins.json
        let coins_path = params_dir.join("coins.json");
        println!("Generating coins.json file at: {}", coins_path.display());
        init_coins_file(&coins_path)?;
        let remaining_coin_u256 = U256::from_le_bytes(remaining_coin_val.to_repr().0);

        let coin_id = next_id(&coins_path)?;
        let new_coin = coins_file(coin_id, burn_key, remaining_coin_u256, &self.network)?;
        append_new_entry(&coins_path, new_coin)?;
        println!("New coin entry added");

        Ok(())
    }

    pub async fn recover_check_balance_and_compute_remaining(
        &self,
        burn_addr: alloy::primitives::Address,
        burn_key: Fp,
        fee: U256,
        spend: U256,
    ) -> anyhow::Result<(Fp, U256)> {
        let rt = self.setup().await?;
        let balance = rt.provider.get_balance(burn_addr).await?;
        if balance.is_zero() {
            // Redundant safety (ensure_* should already panic), but keep guard here too
            panic!("No ETH is present in the burn address!");
        }

        let (_remaining_fp, remaining_coin_u256) =
        compute_remaining_coin(burn_key, balance, fee, spend)?;
        Ok((_remaining_fp, remaining_coin_u256))
    }



    pub fn spend_prepare_from_coin(
        &self,
        burn_key: Fp,
        original_amount: U256,
        out_amount: U256,
        fee: U256,
    ) -> anyhow::Result<(U256, Fp, U256)> {
        if out_amount + fee > original_amount {
            return Err(anyhow::anyhow!(
                "Sum of --fee and --amount should be less than or equal to the original amount!"
            ));
        }
        let (_previous_fp, previous_coin_u256) =
        compute_previous_coin(burn_key, original_amount)?;


        let (_remaining_fp, remaining_coin_u256) =
        compute_remaining_coin(burn_key, original_amount, fee, out_amount)?;

        Ok((previous_coin_u256, _remaining_fp, remaining_coin_u256))
    }

    pub async fn build_and_prove_spend(
        &self,
        params_dir: &std::path::Path,
        burn_key: Fp,
        original_amount: U256,
        out_amount: U256,
        fee: U256,
        receiver: alloy::primitives::Address,
        input_json_path: &str,
        witness_path: &str,
    ) -> anyhow::Result<crate::utils::RapidsnarkOutput> {
        // 1) Write spend input JSON (reuses existing helper)
        self.write_spend_input_json(
            burn_key,
            &original_amount.to_string(),
            &out_amount.to_string(),
            &receiver.to_string(),
            &fee.to_string(),
            input_json_path,
        )
        .await?;

        // 2) Generate witness.wtns with the "spend" circuit
        let proc_path = std::env::current_exe().expect("Failed to get current exe path");
        println!("Generating witness.wtns file at: {}", witness_path);
        let witness_output = std::process::Command::new(&proc_path)
            .arg("generate-witness")
            .arg("spend")
            .arg("--input")
            .arg(input_json_path)
            .arg("--dat")
            .arg(params_dir.join("spend.dat"))
            .arg("--witness")
            .arg(witness_path)
            .output()?;

        if !witness_output.status.success() {
            return Err(anyhow::anyhow!(
                "generate-witness failed: {}",
                String::from_utf8_lossy(&witness_output.stderr)
            ));
        }

        // 3) Run rapidsnark with spend.zkey
        println!("Generating proof...");
        let out_path: std::path::PathBuf =
            std::env::current_dir()?.join("rapidsnark_output.json");
        if out_path.exists() {
            let _ = std::fs::remove_file(&out_path);
        }
        println!("[compute_proof] Running rapidsnark -> {}", out_path.display());
        let raw_output = std::process::Command::new(&proc_path)
            .arg("rapidsnark")
            .arg("--zkey")
            .arg(params_dir.join("spend.zkey"))
            .arg("--witness")
            .arg(witness_path)
            .arg("--out")
            .arg(&out_path)
            .output()?;

        if !raw_output.status.success() {
            return Err(anyhow::anyhow!(
                "rapidsnark failed: {}",
                String::from_utf8_lossy(&raw_output.stderr)
            ));
        }
        if raw_output.stdout.is_empty() {
            return Err(anyhow::anyhow!(
                "rapidsnark stdout was empty — cannot parse JSON"
            ));
        }
        println!(
            "[Rapidsnark] output: {}",
            String::from_utf8_lossy(&raw_output.stdout)
        );

        let json_bytes = std::fs::read(&out_path)?;
        let output: crate::utils::RapidsnarkOutput = serde_json::from_slice(&json_bytes)?;
        println!("Generated proof successfully!");
        Ok(output)
    }


    
}

use crate::networks::{NETWORKS, Network};
pub use burn::BurnOpt;
pub use claim::ClaimOpt;
pub use generate_witness::GenerateWitnessOpt;
pub use info::InfoOpt;
pub use ls::LsOpt;
pub use mine::MineOpt;
pub use participate::ParticipateOpt;
pub use recover::RecoverOpt;
pub use spend::SpendOpt;
pub use ls::LsCommand;