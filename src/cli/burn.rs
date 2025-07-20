use structopt::StructOpt;

use std::process::Command;

use crate::fp::{Fp, FpRepr};
use anyhow::anyhow;
use ff::PrimeField;

use crate::networks::NETWORKS;
use crate::poseidon2::poseidon2;
use crate::utils::{RapidsnarkOutput, find_burn_key, generate_burn_address, input_file};
use alloy::rlp::Encodable;
use tempfile::tempdir;

use crate::utils::BETH;

use alloy::{
    eips::BlockId,
    hex::ToHexExt,
    network::TransactionBuilder,
    primitives::{
        B256,
        U256,
        // map::HashMap,
        utils::{format_ether, parse_ether},
    },
    providers::{Provider, ProviderBuilder},
    // rlp::RlpDecodable,
    rpc::types::TransactionRequest,
    signers::local::PrivateKeySigner,
    // transports::http::reqwest,
};

#[derive(StructOpt)]
pub struct BurnOpt {
    #[structopt(long, default_value = "anvil")]
    network: String,
    #[structopt(long)]
    private_key: PrivateKeySigner,
    #[structopt(long)]
    amount: String,
    #[structopt(long, default_value = "0")]
    fee: String,
    #[structopt(long, default_value = "0")]
    spend: String,
}

impl BurnOpt {
    pub async fn run(self, params_dir: &std::path::Path) -> Result<(), anyhow::Error> {
        let net = NETWORKS.get(&self.network).expect("Invalid network!");

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

        let wallet_addr = self.private_key.address();

        let provider = ProviderBuilder::new()
            .wallet(self.private_key)
            .connect_http(net.rpc.clone());

        if provider.get_code_at(net.beth).await?.0.is_empty() {
            panic!("BETH contract does not exist!");
        }

        println!("Generating a burn-key...");
        let burn_key = find_burn_key(3);

        let burn_addr = generate_burn_address(burn_key, wallet_addr);
        let nullifier = poseidon2([burn_key, Fp::from(1)]);

        let remaining_coin_val =
            Fp::from_repr(FpRepr((amount - fee - spend).to_le_bytes::<32>())).unwrap();
        let remaining_coin = poseidon2([burn_key, remaining_coin_val]);

        println!(
            "Your burn-key: {}",
            B256::from(U256::from_le_bytes(burn_key.to_repr().0)).encode_hex()
        );
        println!("Your burn-address is: {}", burn_addr);

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
        let mut header_bytes = Vec::new();
        block.header.inner.encode(&mut header_bytes);
        let proof = provider.get_proof(burn_addr, vec![]).await?;

        let _proof_dir = tempdir()?;
        let input_json_path = "input.json";
        let witness_path = "witness.wtns"; //proof_dir.path().join("witness.wtns");

        println!("Generating input.json file at: {}", input_json_path);
        std::fs::write(
            &input_json_path,
            input_file(proof, header_bytes, burn_key, fee, spend, wallet_addr)?.to_string(),
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
                fee,
                spend,
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
