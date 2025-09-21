use crate::constants::poseidon_burn_address_prefix;
use crate::fp::Fp;
use crate::server::types::{ProofInput, ProofOutput};
use crate::utils::{RapidsnarkOutput};
use crate::utils::{compute_nullifier,build_and_prove_burn ,compute_remaining_coin,fetch_block_and_header_bytes};
use alloy::{
    primitives::{Address, U256, utils::parse_ether},
    providers::{Provider, ProviderBuilder},
};
use anyhow::{anyhow, Result};
use ff::PrimeField;
use std::path::{Path};
use std::str::FromStr;


fn derive_burn_and_nullifier_from_input(input: &ProofInput) -> Result<(
    Address, 
    Fp,      
    U256,    
    U256,    
    U256,    
    Address, 
    Fp,      
    U256,  
)> {
    let wallet_addr = Address::from_str(input.wallet_address.trim())
        .map_err(|e| anyhow!("Invalid wallet address: {}", e))?;

    let fee = parse_ether(&input.fee)?;
    let spend = parse_ether(&input.spend)?;
    let amount = parse_ether(&input.amount)?;

    let burn_key_fp = Fp::from_str_vartime(&input.burn_key).ok_or(anyhow!("Invalid burn_key"))?;

    let burn_const = poseidon_burn_address_prefix();
    let burn_addr = crate::utils::generate_burn_address(burn_const, burn_key_fp, wallet_addr, fee);

    let (nullifier_fp, nullifier_u256) = compute_nullifier(burn_key_fp);

    Ok((
        wallet_addr,
        burn_key_fp,
        fee,
        spend,
        amount,
        burn_addr,
        nullifier_fp,
        nullifier_u256,
    ))
}


async fn gen_input_witness_proof<P: Provider>(
    provider: &P,
    params_dir: &Path,
    burn_addr: Address,
    burn_key_fp: Fp,
    fee: U256,
    spend: U256,
    wallet_addr: Address,
) -> Result<(RapidsnarkOutput, u64)> {
    let (block_number, header_bytes) = fetch_block_and_header_bytes(provider).await?;
    let (proof, _out_path) = build_and_prove_burn(
            provider,
            params_dir,
            header_bytes,
            burn_addr,
            burn_key_fp,
            fee,
            spend,
            wallet_addr,
            "input.json",
            "witness.wtns",
        )
        .await?;
    Ok((proof, block_number))
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
    ) = derive_burn_and_nullifier_from_input(&input)?;

    println!("[compute_proof] Burn address: {:?}", burn_addr);

    println!("[compute_proof] Checking burn address balance...");
    let balance = provider.get_balance(burn_addr).await?;
    println!("[compute_proof] Balance: {}", balance);
    if balance.is_zero() {
        return Err(anyhow!("No ETH present in the burn address"));
    }

    let (_remaining_fp, remaining_coin_u256) =
        compute_remaining_coin(burn_key_fp, amount, fee, spend)?;

    let params_dir = homedir::my_home()?
        .ok_or(anyhow!("Can't find home directory"))?
        .join(".worm-miner");
    let (json_output, block_number) = gen_input_witness_proof(
        &provider,
        &params_dir.as_path(),
        burn_addr,
        burn_key_fp,
        fee,
        spend,
        wallet_addr,
    )
    .await?;

    println!("[compute_proof] ✅ Proof generated successfully!");
    Ok(ProofOutput {
        burn_address: burn_addr.to_string(),
        proof: serde_json::to_value(&json_output)?,
        block_number,
        nullifier_u256: nullifier_u256.to_string(),
        remaining_coin: remaining_coin_u256.to_string(),
        fee: fee.to_string(),
        spend: spend.to_string(),
        wallet_address: input.wallet_address,
    })
}

