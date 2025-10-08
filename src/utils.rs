use crate::fp::Fp;
use alloy::rpc::types::BlockNumberOrTag;
use alloy::{
    primitives::{Address, U256, keccak256},
    providers::Provider,
    rlp::RlpDecodable,
    rpc::types::EIP1186AccountProofResponse,
};

use anyhow::Result;
use anyhow::anyhow;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

use crate::constants::{poseidon_coin_prefix, poseidon_nullifier_prefix};
use crate::fp::FpRepr;
use crate::poseidon;
use crate::poseidon::{poseidon2, poseidon3};
use alloy::sol;
use alloy::{eips::BlockId, rlp::Encodable};
use alloy_rlp::Decodable;
use ff::{Field, PrimeField};
use serde::{Deserialize, Serialize};
sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    BETH,
    "./src/BETH.abi.json"
);

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    WORM,
    "./src/WORM.abi.json"
);

#[derive(Serialize, Deserialize, Debug)]
pub struct RapidsnarkProof {
    pub pi_a: [U256; 3],
    pub pi_b: [[U256; 2]; 3],
    pub pi_c: [U256; 3],
    pub protocol: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RapidsnarkOutput {
    pub proof: RapidsnarkProof,
    pub public: Vec<U256>,
}

pub fn find_burn_key(
    pow_min_zero_bytes: usize,
    receiver_addr: Address,
    prover_fee: U256,
    broadcaster_fee: U256,
    reveal: U256,
) -> Fp {
    let mut curr: U256 = U256::from_le_bytes(Fp::random(ff::derive::rand_core::OsRng).to_repr().0);
    loop {
        let mut inp: [u8; 156] = [0; 156];
        inp[..32].copy_from_slice(&curr.to_be_bytes::<32>());
        inp[32..52].copy_from_slice(&receiver_addr.as_slice());
        inp[52..84].copy_from_slice(&prover_fee.to_be_bytes::<32>());
        inp[84..116].copy_from_slice(&broadcaster_fee.to_be_bytes::<32>());
        inp[116..148].copy_from_slice(&reveal.to_be_bytes::<32>());
        inp[148..].copy_from_slice(b"EIP-7503");
        let hash: U256 = keccak256(inp).into();
        if hash.leading_zeros() >= pow_min_zero_bytes * 8 {
            return Fp::from_be_bytes(&curr.to_be_bytes::<32>());
        }
        curr += U256::ONE;
    }
}

pub fn generate_burn_address(
    burn_addr_constant: Fp,
    burn_key: Fp,
    receiver: Address,
    prover_fee: U256,
    broadcaster_fee: U256,
    reveal: U256,
) -> Address {
    let receiver_fp = Fp::from_be_bytes(receiver.as_slice());
    let prover_fee_be: [u8; 32] = prover_fee.to_be_bytes();
    let broadcaster_fee_be: [u8; 32] = broadcaster_fee.to_be_bytes();
    let reveal_be: [u8; 32] = reveal.to_be_bytes();
    let prover_fee_fp = Fp::from_be_bytes(&prover_fee_be);
    let broadcaster_fee_fp = Fp::from_be_bytes(&broadcaster_fee_be);
    let reveal_fp = Fp::from_be_bytes(&reveal_be);
    let hash = poseidon::poseidon6(
        burn_addr_constant,
        burn_key,
        receiver_fp,
        prover_fee_fp,
        broadcaster_fee_fp,
        reveal_fp,
    );
    let mut hash_be = hash.to_repr().0[12..32].to_vec();
    hash_be.reverse();
    Address::from_slice(&hash_be)
}

pub fn compute_remaining_coin(
    burn_key: Fp,
    amount: U256,
    fee: U256,
    spend: U256,
) -> Result<(Fp, U256), anyhow::Error> {
    if fee + spend > amount {
        return Err(anyhow!("Sum of fee + spend must be <= amount"));
    }
    let c = poseidon_coin_prefix();
    let rem_fp = Fp::from_repr(FpRepr((amount - fee - spend).to_le_bytes::<32>())).unwrap();
    let rem_coin = poseidon3(c, burn_key, rem_fp);
    let rem_u256 = U256::from_le_bytes(rem_coin.to_repr().0);
    Ok((rem_fp, rem_u256))
}

pub fn compute_previous_coin(burn_key: Fp, amount: U256) -> Result<(Fp, U256), anyhow::Error> {
    let c = poseidon_coin_prefix();
    let previous_fp = Fp::from_repr(FpRepr((amount).to_le_bytes::<32>())).unwrap();
    let previous_coin = poseidon3(c, burn_key, previous_fp);
    let previous_u256 = U256::from_le_bytes(previous_coin.to_repr().0);
    Ok((previous_fp, previous_u256))
}

pub fn compute_nullifier(burn_key: Fp) -> (Fp, U256) {
    let c = poseidon_nullifier_prefix();
    let nf_fp = poseidon2(c, burn_key);
    let nf_u256 = U256::from_le_bytes(nf_fp.to_repr().0);
    (nf_fp, nf_u256)
}

#[derive(Debug, RlpDecodable, PartialEq)]
struct RlpLeaf {
    key: alloy::rlp::Bytes,
    value: alloy::rlp::Bytes,
}

pub async fn generate_input_file(
    header_bytes: Vec<u8>,
    burn_key: Fp,
    fee: U256,
    spend: U256,
    wallet_addr: Address,
    prover: Address,
    input_path: impl AsRef<Path>,
    proof: EIP1186AccountProofResponse,
) -> Result<()> {
    let json = input_file(
        proof,
        header_bytes,
        burn_key,
        fee,
        spend,
        wallet_addr,
        prover,
    )?
    .to_string();
    std::fs::write(input_path.as_ref(), json)?;
    Ok(())
}

pub async fn fetch_block_and_header_bytes<P: Provider>(
    provider: &P,
    block_number: Option<u64>,
) -> Result<(u64, Vec<u8>)> {
    let block = match block_number {
        Some(block_number) => {
            let block = provider
                .get_block_by_number(BlockNumberOrTag::Number(block_number))
                .await?
                .expect("block not found");
            block
        }
        None => {
            let block = provider
                .get_block(BlockId::latest())
                .await?
                .ok_or(anyhow!("Block not found!"))?;
            block
        }
    };

    let mut header_bytes = Vec::new();
    block.header.inner.encode(&mut header_bytes);

    Ok((block.header.number, header_bytes))
}
pub async fn get_account_proof<P: Provider>(
    provider: &P,
    burn_addr: Address,
) -> Result<EIP1186AccountProofResponse> {
    let proof = provider.get_proof(burn_addr, vec![]).await?;
    Ok(proof)
}

pub async fn build_and_prove_burn_logic(
    params_dir: &Path,
    header_bytes: Vec<u8>,
    burn_key: Fp,
    fee: U256,
    spend: U256,
    wallet_addr: Address,
    prover: Address,
    input_json_path: &str,
    witness_path: &str,
    proof: EIP1186AccountProofResponse,
) -> Result<(RapidsnarkOutput, PathBuf)> {
    // 1) input.json (delegated)
    generate_input_file(
        header_bytes,
        burn_key,
        fee,
        spend,
        wallet_addr,
        prover,
        input_json_path,
        proof,
    )
    .await?;

    // 2) witness
    let proc_path = std::env::current_exe().expect("Failed to get current exe path");
    println!("Generating witness.wtns file at: {}", witness_path);
    let output = std::process::Command::new(&proc_path)
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
            "Failed to generate witness file: {}$",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;

    // 3) rapidsnark
    println!("Generating proof...");
    let out_path: PathBuf = std::env::current_dir()?.join("rapidsnark_output.json");
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
        .arg(params_dir.join("proof_of_burn.zkey"))
        .arg("--witness")
        .arg(witness_path)
        .arg("--out")
        .arg(&out_path)
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
    println!(
        "[Rapidsnark] output: {}",
        String::from_utf8_lossy(&raw_output.stdout)
    );

    // 4) parse
    let json_bytes = fs::read(&out_path)?;
    let json_output: RapidsnarkOutput = serde_json::from_slice(&json_bytes)?;
    println!("Generated proof successfully!");

    Ok((json_output, out_path))
}

pub fn input_file(
    proof: EIP1186AccountProofResponse,
    header_bytes: Vec<u8>,
    burn_key: Fp,
    fee: U256,
    spend: U256,
    receiver: Address,
    prover: Address,
) -> Result<serde_json::Value, anyhow::Error> {
    let max_layers = 16;
    let max_layer_len = 4 * 136;
    let max_header_len = 8 * 136;

    let leaf = proof
        .account_proof
        .last()
        .ok_or(anyhow!("Leaf doesn't exist!"))?;
    let rlp_leaf = RlpLeaf::decode(&mut leaf.as_ref())?;
    let num_addr_hash_nibbles = if rlp_leaf.key[0] & 0xf0 == 0x20 {
        2 * rlp_leaf.key.len() - 2
    } else if rlp_leaf.key[0] & 0xf0 == 0x30 {
        2 * rlp_leaf.key.len() - 1
    } else {
        return Err(anyhow!("Unexpected leaf-key prefix!"));
    };

    let mut layers = vec![];
    for layer in proof.account_proof.iter() {
        let mut extended_layer = layer.to_vec();
        extended_layer.resize(max_layer_len, 0);
        layers.push(extended_layer);
    }
    while layers.len() < max_layers {
        layers.push(vec![0; max_layer_len]);
    }
    let mut layer_bits_lens = proof
        .account_proof
        .iter()
        .map(|l| l.len())
        .collect::<Vec<_>>();
    layer_bits_lens.resize(max_layers, 32);
    let mut extended_header = header_bytes.to_vec();
    extended_header.resize(max_header_len, 0);

    let extra_commitment =
        U256::from_be_slice(keccak256(prover.as_slice()).as_slice()) >> U256::from(8);

    Ok(json!({
        "balance": proof.balance.to_string(),
        "numLayers": proof.account_proof.len(),
        "layerLens": layer_bits_lens,
        "layers": layers,
        "blockHeader": extended_header,
        "blockHeaderLen": header_bytes.len(),
        "receiverAddress": U256::from_be_slice(receiver.as_slice()).to_string(),
        "numLeafAddressNibbles": num_addr_hash_nibbles.to_string(),
        "burnKey": U256::from_le_bytes(burn_key.to_repr().0).to_string(),
        "broadcasterFeeAmount": fee.to_string(),
        "revealAmount": spend.to_string(),
        "byteSecurityRelax": 0,
        "proverFeeAmount": 0,
        "_extraCommitment": extra_commitment.to_string()
    }))
}
