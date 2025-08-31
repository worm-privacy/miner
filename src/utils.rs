use crate::fp::Fp;
use alloy::{
    primitives::{Address, U256, keccak256},
    rlp::RlpDecodable,
    rpc::types::EIP1186AccountProofResponse,
};
use anyhow::anyhow;
use serde_json::json;

use alloy::sol;

use crate::poseidon;
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

pub fn find_burn_key(pow_min_zero_bytes: usize,receiver_addr: Address,fee: U256) -> Fp {
    let mut curr: U256 = U256::from_le_bytes(Fp::random(ff::derive::rand_core::OsRng).to_repr().0);
    loop {
        let mut inp: [u8; 92] = [0; 92];
        inp[..32].copy_from_slice(&curr.to_be_bytes::<32>());
        inp[32..52].copy_from_slice(&receiver_addr.as_slice());
        inp[52..84].copy_from_slice(&fee.to_be_bytes::<32>());
        inp[84..].copy_from_slice(b"EIP-7503");
        let hash: U256 = keccak256(inp).into();
        if hash.leading_zeros() >= pow_min_zero_bytes * 8 {
            return Fp::from_be_bytes(&curr.to_be_bytes::<32>());
        }
        curr += U256::ONE;
    }
}

pub fn generate_burn_address(burn_addr_constant:Fp,burn_key: Fp, receiver: Address,fee: U256) -> Address {
    let receiver_fp = Fp::from_be_bytes(receiver.as_slice());
    let fee_be: [u8; 32] = fee.to_be_bytes();
    let fee_fp = Fp::from_be_bytes(&fee_be);
    // println!("fee_fp: {:?}", fee_fp);
    let hash = poseidon::poseidon4(burn_addr_constant,burn_key, receiver_fp,fee_fp);
    let mut hash_be = hash.to_repr().0[12..32].to_vec();
    hash_be.reverse();
    Address::from_slice(&hash_be)
}

#[derive(Debug, RlpDecodable, PartialEq)]
struct RlpLeaf {
    key: alloy::rlp::Bytes,
    value: alloy::rlp::Bytes,
}

pub fn input_file(
    proof: EIP1186AccountProofResponse,
    header_bytes: Vec<u8>,
    burn_key: Fp,
    fee: U256,
    spend: U256,
    receiver: Address,
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
        "fee": fee.to_string(),
        "spend": spend.to_string(),
        "byteSecurityRelax": 0
    }))
}
