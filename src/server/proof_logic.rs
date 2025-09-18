use crate::constants::{
    poseidon_burn_address_prefix, poseidon_coin_prefix, poseidon_nullifier_prefix,
};
use crate::fp::{Fp, FpRepr};
use crate::poseidon::{poseidon2, poseidon3};
use crate::server::types::{ProofInput, ProofOutput};
use crate::utils::{RapidsnarkOutput, input_file};
use alloy::{
    eips::BlockId,
    primitives::{Address, U256, utils::parse_ether},
    providers::{Provider, ProviderBuilder},
    rlp::Encodable,
};
use anyhow::Result;
use anyhow::anyhow;
use ff::PrimeField;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::str::FromStr;

pub async fn compute_proof(input: ProofInput) -> Result<ProofOutput> {
    println!("[compute_proof] Starting proof generation job");

    // 1. Network
    let net = crate::networks::NETWORKS
        .get(&input.network)
        .ok_or(anyhow!("Network not found!"))?
        .clone();
    println!("[compute_proof] Selected network: {}", input.network);

    // 2. Provider
    println!("[compute_proof] Connecting to provider...");
    let provider = ProviderBuilder::new().connect_http(net.rpc.clone());

    // 3. Constants
    let burn_const = poseidon_burn_address_prefix();
    let nullifier_const = poseidon_nullifier_prefix();
    let coin_const = poseidon_coin_prefix();

    // 4. Wallet address
    println!("[compute_proof] Parsing wallet address...");
    let wallet_addr = Address::from_str(input.wallet_address.trim())
        .map_err(|e| anyhow!("Invalid wallet address: {}", e))?;

    // 5. Parse amounts
    println!("[compute_proof] Parsing ether values...");
    let fee = parse_ether(&input.fee)?;
    let spend = parse_ether(&input.spend)?;
    let amount = parse_ether(&input.amount)?;

    // 6. Burn key
    println!("[compute_proof] Parsing burn key...");
    let burn_key_fp = Fp::from_str_vartime(&input.burn_key).ok_or(anyhow!("Invalid burn_key"))?;

    // 7. Generate burn address
    println!("[compute_proof] Generating burn address...");
    let burn_addr = crate::utils::generate_burn_address(burn_const, burn_key_fp, wallet_addr, fee);
    println!("[compute_proof] Burn address: {:?}", burn_addr);

    // 8. Check balance
    println!("[compute_proof] Checking burn address balance...");
    let balance = provider.get_balance(burn_addr).await?;
    println!("[compute_proof] Balance: {}", balance);

    if balance.is_zero() {
        return Err(anyhow!("No ETH present in the burn address"));
    }

    // 9. Generate nullifier
    println!("[compute_proof] Generating nullifier...");
    let nullifier = poseidon2(nullifier_const, burn_key_fp);
    let nullifier_u256 = U256::from_le_bytes(nullifier.to_repr().0);

    // 10. Remaining coin
    println!("[compute_proof] Generating remaining coin...");
    let remaining_fp = Fp::from_repr(FpRepr((amount - fee - spend).to_le_bytes::<32>())).unwrap();
    let remaining_coin = poseidon3(coin_const, burn_key_fp, remaining_fp);
    let remaining_coin_u256 = U256::from_le_bytes(remaining_coin.to_repr().0);

    // 11. Fetch block
    println!("[compute_proof] Fetching latest block...");
    let block = provider
        .get_block(BlockId::latest())
        .await?
        .ok_or(anyhow!("Block not found!"))?;

    let mut header_bytes = Vec::new();
    block.header.inner.encode(&mut header_bytes);
    println!("[compute_proof] Block number: {}", block.header.number);

    // 12. Get proof
    println!("[compute_proof] Fetching account proof...");
    let proof = provider.get_proof(burn_addr, vec![]).await?;

    // 13. Build input.json
    println!("[compute_proof] Creating input.json...");
    let json = input_file(proof, header_bytes, burn_key_fp, fee, spend, wallet_addr)?.to_string();
    fs::write("input.json", json)?;

    // 14. Paths
    let proc_path = std::env::current_exe()?;
    let params_dir = homedir::my_home()?
        .ok_or(anyhow!("Can't find home directory"))?
        .join(".worm-miner");

    // 15. Generate witness
    println!("[compute_proof] Running generate-witness...");
    let output = Command::new(&proc_path)
        .arg("generate-witness")
        .arg("proof-of-burn")
        .arg("--input")
        .arg("input.json")
        .arg("--dat")
        .arg(params_dir.join("proof_of_burn.dat"))
        .arg("--witness")
        .arg("witness.wtns")
        .output()?;

    if !output.status.success() {
        println!(
            "[compute_proof] Error: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("Failed to generate witness"));
    }

    // 16. Generate zk proof
    let out_path: PathBuf = std::env::current_dir()?.join("rapidsnark_output.json");
    if out_path.exists() {
        let _ = std::fs::remove_file(&out_path);
    }
    println!(
        "[compute_proof] Running rapidsnark -> {}",
        out_path.display()
    );
    let raw_output = Command::new(&proc_path)
        .arg("rapidsnark")
        .arg("--zkey")
        .arg(params_dir.join("proof_of_burn.zkey"))
        .arg("--witness")
        .arg("witness.wtns")
        .arg("--out")
        .arg(&out_path)
        .output()?;

    if !raw_output.status.success() {
        println!(
            "[compute_proof] rapidsnark error: {}",
            String::from_utf8_lossy(&raw_output.stderr)
        );
        return Err(anyhow!("Failed to generate proof"));
    }
    println!(
        "[Rapidsnark] output: {}",
        String::from_utf8_lossy(&raw_output.stdout)
    );
    // 17. Parse final output
    let json_bytes = std::fs::read(&out_path)?;

    let json_output: RapidsnarkOutput = serde_json::from_slice(&json_bytes)?;
    let json_value = serde_json::to_value(&json_output)?;

    println!("[compute_proof] ✅ Proof generated successfully!");

    Ok(ProofOutput {
        burn_address: burn_addr.to_string(),
        proof: json_value,
        block_number: block.header.number,
        nullifier_u256: nullifier_u256.to_string(),
        remaining_coin: remaining_coin_u256.to_string(),
        fee: fee.to_string(),
        spend: spend.to_string(),
        wallet_address: input.wallet_address,
    })
}
