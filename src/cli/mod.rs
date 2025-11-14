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
use crate::cli::utils::{append_new_entry, burn_file, coins_file, init_coins_file, next_id};
use crate::constants::poseidon_burn_address_prefix;
use crate::fp::Fp;
pub use recover::RecoverOpt;
use crate::utils::{RapidsnarkOutput, build_and_prove_burn_logic, generate_burn_extra_commit};
use crate::utils::{
    compute_nullifier, compute_previous_coin, compute_remaining_coin, fetch_block_and_header_bytes,
    find_burn_key, generate_burn_address, get_account_proof,
};
use alloy::consensus::Receipt;
use alloy::primitives::{Bytes, U160, address};
use alloy::signers::local::PrivateKeySigner;
use alloy::sol_types::{SolCall, SolValue};
use alloy::{
    hex::ToHexExt,
    network::TransactionBuilder,
    primitives::utils::{format_ether, parse_ether},
    rpc::types::TransactionRequest,
};
use alloy::{
    primitives::{Address, U256},
    providers::{Provider, ProviderBuilder},
};
use anyhow::anyhow;
use ff::PrimeField;
use reqwest::Url;
use serde_json::json;
use std::path::PathBuf;
use structopt::StructOpt;

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

use alloy::dyn_abi::DynSolValue;
use alloy::primitives::I256;
use alloy::{hex, sol};

sol! {
    interface IUniswapV3Pool {
        /// swap function
        function swap(
            address recipient,
            bool zeroForOne,
            int256 amountSpecified,
            uint160 sqrtPriceLimitX96,
            bytes calldata data
        ) external returns (int256 amount0, int256 amount1);
    }
}

/// Generate calldata for Uniswap V3 pool swap (BETH -> ETH)
fn get_swap_calldata(amount_in: U256, recipient: Address) -> Vec<u8> {
    let zero_for_one = false;
    let amount_specified = I256::from_raw(amount_in);

    let sqrt_price_limit_x96 = if zero_for_one {
        U160::from_str_radix("4295128741", 10).unwrap()
    } else {
        U160::from_str_radix("1461446703485210103287273052203988822378723970340", 10).unwrap()
    };
    let data = Vec::new();
    let swap_call = IUniswapV3Pool::swapCall {
        recipient,
        zeroForOne: zero_for_one,
        amountSpecified: amount_specified,
        sqrtPriceLimitX96: sqrt_price_limit_x96,
        data: data.into(),
    }
    .abi_encode();

    (
        address!("0x646b5eB499411390448b5e21838aCB8B2FF548dA"),
        amount_in,
        swap_call,
    )
        .abi_encode_params()
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
        swap_calldata: Bytes,
    ) -> Result<()> {
        let rt = self.setup().await?; // get provider, wallet, network from self
        println!("Broadcasting mint transaction...");
        let net = &rt.network;
        // instantiate your BETH binding
        let beth = BETH::new(net.beth, rt.provider);

        // call the zk-proof mintCoin(...) method
        let pending_tx = beth
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
                U256::ZERO,
                rt.wallet_address,
                swap_calldata,
                Bytes::new(),
            )
            .send()
            .await;
        match pending_tx {
            Ok(pending) => {
                // transaction mined successfully
                let receipt = pending.get_receipt().await?;
                if receipt.status() {
                    println!("Success!");
                } else {
                    println!("Transaction failed!");
                }
            }
            Err(err) => {
                // transaction reverted, err may contain revert data
                if let Some(revert_bytes) = err.as_revert_data() {
                    // revert_bytes: Vec<u8> — ABI encoded
                    println!("Revert data (raw): 0x{}", hex::encode(revert_bytes.clone()));

                    // decode revert reason as string
                    if let Ok(reason) =
                        ethers::abi::decode(&[ethers::abi::ParamType::String], &revert_bytes)
                    {
                        println!("Revert reason: {}", reason[0].to_string());
                    }
                } else {
                    println!("Transaction failed without revert data: {:?}", err);
                }
            }
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
        receiver_hook: Bytes,
    ) -> Result<(Fp, Address, Fp, U256, Fp, U256, U256)> {
        let rt = self.setup().await?;

        if fee + spend > amount {
            return Err(anyhow!(
                "Sum of --fee and --spend should be less than --amount!"
            ));
        }
        if amount > parse_ether("10")? {
            return Err(anyhow!("Can't burn more than 10 ETH in a single call!"));
        }

        // 1) burn_key
        println!("Generating a burn-key...");
        let extra_commit =
            generate_burn_extra_commit(rt.wallet_address, U256::ZERO, fee, receiver_hook.clone());
        let burn_key = find_burn_key(2, extra_commit, spend);
        println!("Your burn_key: {:?}", burn_key);
        println!(
            "Your burn-key as string: {}",
            U256::from_le_bytes(burn_key.to_repr().0).to_string()
        );

        // 2) burn address
        let burn_addr_prefix = poseidon_burn_address_prefix();
        let (burn_addr, burn_extra_commit) = generate_burn_address(
            burn_addr_prefix,
            burn_key,
            rt.wallet_address,
            U256::ZERO,
            fee,
            spend,
            receiver_hook.clone(),
        );

        // 3) nullifier (Fp only needed by caller)
        let (nullifier_fp, nullifier_u256) = compute_nullifier(burn_key);

        // 4) remaining coin (both fp and u256)
        let (remaining_coin_val_fp, remaining_coin_u256) =
            compute_remaining_coin(burn_key, amount, spend)?;

        Ok((
            burn_key,
            burn_addr,
            nullifier_fp,
            nullifier_u256,
            remaining_coin_val_fp,
            remaining_coin_u256,
            burn_extra_commit,
        ))
    }

    pub async fn recover_prepare_from_key(
        &self,
        burn_key: Fp,
        fee: U256,
        reveal: U256,
        receiver_hook: Bytes,
    ) -> anyhow::Result<(alloy::primitives::Address, Fp, U256)> {
        let rt = self.setup().await?; // wallet addr + provider + network
        let burn_addr_prefix = crate::constants::poseidon_burn_address_prefix();

        let (burn_addr, _burn_extra_commit) = crate::utils::generate_burn_address(
            burn_addr_prefix,
            burn_key,
            rt.wallet_address,
            U256::ZERO,
            fee,
            reveal,
            receiver_hook,
        );
        let (nullifier_fp, _nullifier_u256) = compute_nullifier(burn_key);
        Ok((burn_addr, nullifier_fp, _burn_extra_commit))
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
        spend: U256,
        burn_extra_commit: U256,
        input_json_path: &str,
        witness_path: &str,
    ) -> Result<(RapidsnarkOutput, u64, PathBuf)> {
        let rt = self.setup().await?;

        let (block_number, header_bytes) = fetch_block_and_header_bytes(&rt.provider, None).await?;
        let account_proof = get_account_proof(&rt.provider, burn_addr).await?;
        let (proof, out_path) = build_and_prove_burn_logic(
            params_dir,
            header_bytes,
            burn_key,
            spend,
            burn_extra_commit,
            rt.wallet_address,
            input_json_path,
            witness_path,
            account_proof,
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
                _ => {
                    return Err(anyhow!(
                        "fee and spend are required when coins_only == false"
                    ));
                }
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
            compute_remaining_coin(burn_key, balance, spend)?;
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
        let (_previous_fp, previous_coin_u256) = compute_previous_coin(burn_key, original_amount)?;

        let (_remaining_fp, remaining_coin_u256) =
            compute_remaining_coin(burn_key, original_amount, out_amount)?;

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
        let out_path: std::path::PathBuf = std::env::current_dir()?.join("rapidsnark_output.json");
        if out_path.exists() {
            let _ = std::fs::remove_file(&out_path);
        }
        println!(
            "[compute_proof] Running rapidsnark -> {}",
            out_path.display()
        );
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
pub use ls::LsCommand;
pub use ls::LsOpt;
pub use mine::MineOpt;
pub use participate::ParticipateOpt;
pub use spend::SpendOpt;
