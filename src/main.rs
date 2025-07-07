mod fp;
mod poseidon2;
use std::process::exit;

use alloy_rlp::Decodable;
use anyhow::anyhow;
use ff::{Field, PrimeField, derive::rand_core};
use fp::{Fp, FpRepr};
use serde_json::json;
use structopt::StructOpt;

#[derive(StructOpt)]
struct BurnOpt {
    #[structopt(long, default_value = "https://eth.meowrpc.com")]
    rpc: String,
    #[structopt(long)]
    private_key: PrivateKeySigner,
}

#[derive(StructOpt)]
enum MinerOpt {
    Burn(BurnOpt),
    Mine,
}

use alloy::{
    eips::BlockId,
    hex::ToHexExt,
    network::{Ethereum, TransactionBuilder},
    primitives::{Address, FixedBytes, U160, U256, address, keccak256},
    providers::{Provider, ProviderBuilder, RootProvider},
    rlp::{RlpDecodable, RlpEncodable},
    rpc::types::{EIP1186AccountProofResponse, TransactionRequest},
    signers::{k256::ecdsa::SigningKey, local::PrivateKeySigner},
};

fn find_burn_key(pow_min_zero_bytes: usize) -> Fp {
    let mut curr: U256 = U256::from_le_bytes(Fp::random(ff::derive::rand_core::OsRng).to_repr().0);
    loop {
        let hash: U256 = keccak256(curr.to_be_bytes::<32>()).into();
        if hash.leading_zeros() >= pow_min_zero_bytes * 8 {
            return Fp::from_be_bytes(&curr.to_be_bytes::<32>());
        }
        curr += U256::ONE;
    }
}

fn generate_burn_address(burn_key: Fp, receiver: Address) -> Address {
    let receiver_fp = Fp::from_be_bytes(receiver.as_slice());
    let hash = poseidon2::poseidon2([burn_key, receiver_fp]);
    let mut hash_be = hash.to_repr().0[12..32].to_vec();
    hash_be.reverse();
    Address::from_slice(&hash_be)
}

fn bytes_to_bits(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .map(|b| (0..8).into_iter().map(move |i| ((b & (1 << i)) != 0) as u8))
        .flatten()
        .collect()
}

#[derive(Debug, RlpDecodable, PartialEq)]
struct RlpLeaf {
    key: alloy::rlp::Bytes,
    value: alloy::rlp::Bytes,
}

fn input_file(
    proof: EIP1186AccountProofResponse,
    header_bytes: Vec<u8>,
    burn_key: Fp,
    fee: U256,
    spend: U256,
    receiver: Address,
) -> Result<serde_json::Value, anyhow::Error> {
    let max_layers = 4;
    let max_layer_len = 4 * 136;
    let max_header_len = 5 * 136;

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
        layers.push(bytes_to_bits(&extended_layer));
    }
    while layers.len() < max_layers {
        layers.push(vec![0; max_layer_len * 8]);
    }
    let mut layer_bits_lens = proof
        .account_proof
        .iter()
        .map(|l| l.len() * 8)
        .collect::<Vec<_>>();
    layer_bits_lens.resize(max_layers, 0);
    let mut extended_header = header_bytes.to_vec();
    extended_header.resize(max_header_len, 0);

    Ok(json!({
        "balance": proof.balance.to_string(),
        "numLayers": proof.account_proof.len(),
        "layerBitsLens": layer_bits_lens,
        "layerBits": layers,
        "blockHeader": bytes_to_bits(&extended_header),
        "blockHeaderLen": header_bytes.len() * 8,
        "receiverAddress": U256::from_be_slice(receiver.as_slice()).to_string(),
        "numLeafAddressNibbles": num_addr_hash_nibbles.to_string(),
        "burnKey": U256::from_le_bytes(burn_key.to_repr().0).to_string(),
        "fee": fee.to_string(),
        "spend": spend.to_string(),
    }))
}

use alloy::rlp::Encodable;
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = MinerOpt::from_args();

    match opt {
        MinerOpt::Burn(burn_opt) => {
            let provider = ProviderBuilder::new()
                .wallet(burn_opt.private_key)
                .connect_http("http://127.0.0.1:8545".parse()?);
            //println!("Generating a burn-key...");
            let burn_key = find_burn_key(2);
            let fee = U256::from(123);
            let spend = U256::from(234);
            let receiver = address!("0x90f8bf6a479f320ead074411a4b0e7944ea8c9c1");
            let burn_addr = generate_burn_address(burn_key, receiver);
            let amount = U256::from(10).pow(U256::from(18));
            // Build a transaction to send 100 wei from Alice to Bob.
            // The `from` field is automatically filled to the first signer's address (Alice).
            let tx = TransactionRequest::default()
                .with_to(burn_addr)
                .with_nonce(0)
                .with_chain_id(provider.get_chain_id().await?)
                .with_value(amount)
                .with_gas_limit(21_000)
                .with_max_priority_fee_per_gas(1_000_000_000)
                .with_max_fee_per_gas(20_000_000_000);

            // Send the transaction and wait for the broadcast.
            let pending_tx = provider.send_transaction(tx).await?;
            let receipt = pending_tx.get_receipt().await?;

            let block = provider
                .get_block(BlockId::latest())
                .await?
                .ok_or(anyhow!("Block not found!"))?;
            let mut header_bytes = Vec::new();
            block.header.inner.encode(&mut header_bytes);
            let proof = provider.get_proof(burn_addr, vec![]).await?;
            println!(
                "{}",
                input_file(proof, header_bytes, burn_key, fee, spend, receiver)?.to_string()
            );
            //println!("Send funds to this burn address: {}", burn_addr);
            //println!("Hello, world!");
        }
        MinerOpt::Mine => {}
    }
    Ok(())
}
