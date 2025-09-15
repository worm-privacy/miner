use crate::constants::{
    poseidon_burn_address_prefix, poseidon_coin_prefix, poseidon_nullifier_prefix,
};
use crate::fp::{Fp, FpRepr};
use crate::server3::{ProofInput, ProofOutput};
use alloy::{
    eips::BlockId,
    hex::ToHexExt,
    primitives::{Address, U256, utils::parse_ether},
    providers::{Provider, ProviderBuilder},
    rlp::Encodable,
};
use anyhow::Result;

use crate::networks::{NETWORKS, Network};
use anyhow::anyhow;
use ff::PrimeField;
use std::path::Path;
use tracing_subscriber::{EnvFilter, fmt};

use crate::utils::{RapidsnarkOutput, input_file};
use std::str::FromStr;
use crate::poseidon::{poseidon2, poseidon3};
use crate::utils::{find_burn_key, generate_burn_address};

use worm_witness_gens::{generate_proof_of_burn_witness_file, generate_spend_witness_file};

pub async fn compute_proof(input: ProofInput) -> Result<ProofOutput> {
    let mut net = NETWORKS
        .get(&input.network)
        .ok_or(anyhow!("Network not found!"))?
        .clone();
    println!("start to get provider");
    let provider = ProviderBuilder::new().connect_http(net.rpc.clone());
    let burn_addr_constant = poseidon_burn_address_prefix();
    println!("start to get poseidon");
    let nullifier_constant = poseidon_nullifier_prefix();
    
    let coin_constant = poseidon_coin_prefix();
    // let receiver_addr = input.receiver_address;
    println!("start to get wallet");
    let wallet_addr =  Address::from_str(input.wallet_address.trim()).unwrap();

    let witness_path = "witness.wtns";
    let input_path = "input.json";
    println!("start to parse");
    let fee = parse_ether(&input.fee)?;
    let spend = parse_ether(&input.spend)?;
    let amount = parse_ether(&input.amount)?;
    println!("start to get burn key fp");
    let burn_key_fp = Fp::from_str_vartime(&input.burn_key.to_string()).unwrap();
    let burn_addr = generate_burn_address(burn_addr_constant, burn_key_fp, wallet_addr, fee);
    println!("start to generate burn address");
    let burn_addr_balance = provider.get_balance(burn_addr).await?;
        if burn_addr_balance.is_zero() {
            panic!("No ETH is present in the burn address!");
        }
    let nullifier = poseidon2(nullifier_constant, burn_key_fp);
    let nullifier_u256 = U256::from_le_bytes(nullifier.to_repr().0);
    println!("start to gen nullifier");
    let remaining_coin_val =
        Fp::from_repr(FpRepr((amount - fee - spend).to_le_bytes::<32>())).unwrap();
    println!("start to gen remainig coin");
    let remaining_coin = poseidon3(coin_constant, burn_key_fp, remaining_coin_val);
    let remaining_coin_u256 = U256::from_le_bytes(remaining_coin.to_repr().0);
    let block = provider
        .get_block(BlockId::latest())
        .await?
        .ok_or(anyhow!("Block not found!"))?;
    println!("start to get header bytes");
    let mut header_bytes = Vec::new();
    block.header.inner.encode(&mut header_bytes);
    println!("start to get proof");
    let proof = provider.get_proof(burn_addr, vec![]).await?;
    println!("start to gen input.json");
    let json = input_file(proof, header_bytes, burn_key_fp, fee, spend, wallet_addr)?.to_string();
    // let input_path_json = input_path.as_ref();
    std::fs::write(input_path, json)?;
    println!("here");
    let proc_path = std::env::current_exe().expect("Failed to get current exe path");
    println!("proc_path {:?}",proc_path);
    println!("Generating witness.wtns file at: {}", witness_path);
    use std::process::Command;
    let params_dir = homedir::my_home()?
        .ok_or(anyhow::anyhow!("Can't find user's home directory!"))?
        .join(".worm-miner");
    // use crate::GenerateWitnessOpt;
    // use crate::cli::generate_witness::GenerateWitnessProofOfBurnOpt;
    let output = Command::new(&proc_path)
        .arg("generate-witness")
        .arg("proof-of-burn")
        .arg("--input")
        .arg(&input_path)
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
    // println!("finally gen witness");
    // let witness_command = GenerateWitnessOpt::ProofOfBurn(GenerateWitnessProofOfBurnOpt {
    // dat: params_dir.join("proof_of_burn.dat"),
    // input: input_path.into(),
    // witness: witness_path.into(),
    //     });
    // let output = witness_command.run().await?;
    println!("Generating proof...");
    let raw_output = Command::new(&proc_path)
        .arg("rapidsnark")
        .arg("--zkey")
        .arg(params_dir.join("proof_of_burn.zkey"))
        .arg("--witness")
        .arg(witness_path)
        .output()?;

    println!(
        "[rapidsnark] stderr:\n{}",
        String::from_utf8_lossy(&raw_output.stderr)
    );
    raw_output.status.success().then_some(()).ok_or_else(|| {
        anyhow!(
            "Failed to generate proof: {}",
            String::from_utf8_lossy(&raw_output.stderr)
        )
    })?;
    let json_output: RapidsnarkOutput = serde_json::from_slice(&raw_output.stdout)?;
    let json_string = serde_json::to_value(&json_output)?;
    println!("Generated proof successfully!");
    Ok(ProofOutput {
        burn_address: burn_addr.to_string(),
        proof: json_string,
        block_number: block.header.number,
        nullifier_u256: nullifier_u256.to_string(),
        remaining_coin: remaining_coin_u256.to_string(),
        fee: fee.to_string(),
        spend: spend.to_string(),
        wallet_address: input.wallet_address,
    })
}
