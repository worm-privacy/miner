use crate::constants::poseidon_burn_address_prefix;
use crate::fp::Fp;
use crate::server::types::{ProofInput, ProofOutput};
use crate::server::verify_proof::verify_proof;
use crate::utils::RapidsnarkOutput;
use crate::utils::{
    build_and_prove_burn_logic, compute_nullifier, compute_remaining_coin,
    fetch_block_and_header_bytes, get_account_proof,
};
use alloy::{
    primitives::{Address, U256, utils::parse_ether},
    providers::{Provider, ProviderBuilder},
    rpc::types::EIP1186AccountProofResponse,
};
use anyhow::{Result, anyhow};
use ff::PrimeField;
use std::path::Path;
use std::str::FromStr;

fn derive_burn_and_nullifier_from_input(
    input: &ProofInput,
) -> Result<(Address, Fp, U256, U256, U256, Address, Fp, U256, U256)> {
    let wallet_addr = Address::from_str(input.wallet_address.trim())
        .map_err(|e| anyhow!("Invalid wallet address: {}", e))?;
    println!("{:?}", input);
    let broadcaster_fee = parse_ether(&input.broadcaster_fee)?;
    let prover_fee = parse_ether(&input.prover_fee)?;
    let spend = parse_ether(&input.spend)?;
    let amount = parse_ether(&input.amount)?;

    let burn_key_fp = Fp::from_str_vartime(&input.burn_key).ok_or(anyhow!("Invalid burn_key"))?;

    let burn_const = poseidon_burn_address_prefix();
    let (burn_addr, extra_commit) = crate::utils::generate_burn_address(
        burn_const,
        burn_key_fp,
        wallet_addr,
        prover_fee,
        broadcaster_fee,
        spend,
    );
    println!("Extra commit: {}", extra_commit.to_string());

    let (nullifier_fp, nullifier_u256) = compute_nullifier(burn_key_fp);

    Ok((
        wallet_addr,
        burn_key_fp,
        broadcaster_fee,
        spend,
        amount,
        burn_addr,
        nullifier_fp,
        nullifier_u256,
        extra_commit,
    ))
}

async fn gen_input_witness_proof<P: Provider>(
    provider: &P,
    params_dir: &Path,
    burn_addr: Address,
    burn_key_fp: Fp,
    spend: U256,
    burn_extra_commit: U256,
    prover: Address,
    proof: Option<EIP1186AccountProofResponse>,
    block_number: Option<u64>,
) -> Result<(RapidsnarkOutput, u64)> {
    let (block_number_val, header_bytes) =
        fetch_block_and_header_bytes(provider, block_number).await?;

    let proof = match (proof, block_number) {
        (Some(p), Some(block_number)) => {
            verify_proof(provider, p.clone(), block_number)
                .await
                .map_err(|e| anyhow::anyhow!("Proof verification failed: {:?}", e))?;
            p
        }
        (None, None) => get_account_proof(provider, burn_addr).await?,
        _ => unreachable!(),
    };
    let effective_block_number = block_number.unwrap_or(block_number_val);

    let (proof, _out_path) = build_and_prove_burn_logic(
        params_dir,
        header_bytes,
        burn_key_fp,
        spend,
        burn_extra_commit,
        prover,
        "input.json",
        "witness.wtns",
        proof,
    )
    .await?;
    Ok((proof, effective_block_number))
}

pub async fn compute_proof(input: ProofInput) -> Result<ProofOutput> {
    println!("[compute_proof] Starting proof generation job");

    let net = crate::networks::NETWORKS
        .get(&input.network)
        .ok_or(anyhow!("Network not found!"))?
        .clone();
    println!("[compute_proof] Selected network: {}", input.network);

    println!("[compute_proof] Connecting to provider...");
    let provider = ProviderBuilder::new().connect_http(net.rpc.clone());

    let (
        wallet_addr,
        burn_key_fp,
        fee,
        spend,
        amount,
        burn_addr,
        _nullifier_fp,
        nullifier_u256,
        burn_extra_commit,
    ) = derive_burn_and_nullifier_from_input(&input)?;

    println!("[compute_proof] Burn address: {:?}", burn_addr);

    println!("[compute_proof] Checking burn address balance...");
    let balance = provider.get_balance(burn_addr).await?;
    println!("[compute_proof] Balance: {}", balance);
    if balance.is_zero() {
        return Err(anyhow!("No ETH present in the burn address"));
    }

    let (_remaining_fp, remaining_coin_u256) = compute_remaining_coin(burn_key_fp, amount, spend)?;

    let params_dir = homedir::my_home()?
        .ok_or(anyhow!("Can't find home directory"))?
        .join(".worm-miner");

    let (json_output, block_number) = gen_input_witness_proof(
        &provider,
        &params_dir.as_path(),
        burn_addr,
        burn_key_fp,
        spend,
        burn_extra_commit,
        wallet_addr, // TODO: prover
        input.proof,
        input.block_number,
    )
    .await?;

    println!("[compute_proof] ✅ Proof generated successfully!");
    Ok(ProofOutput {
        burn_address: burn_addr.to_string(),
        proof: serde_json::to_value(&json_output)?,
        block_number,
        nullifier_u256: nullifier_u256.to_string(),
        remaining_coin: remaining_coin_u256.to_string(),
        broadcaster_fee: fee.to_string(),
        prover_fee: "0".to_string(),
        prover: wallet_addr.to_string(), // TODO: prover
        reveal_amount: spend.to_string(),
        wallet_address: input.wallet_address,
    })
}
