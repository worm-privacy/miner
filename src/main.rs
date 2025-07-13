mod fp;
mod poseidon2;

use std::{path::PathBuf, process::Command};

use alloy_rlp::Decodable;
use anyhow::anyhow;
use ff::{Field, PrimeField};
use fp::{Fp, FpRepr};
use poseidon2::poseidon2;
use serde::{Deserialize, Serialize};
use serde_json::json;
use structopt::StructOpt;
use worm_witness_gens::{
    generate_proof_of_burn_witness_file, generate_spend_witness_file, rapidsnark,
};

#[derive(StructOpt)]
struct BurnOpt {
    #[structopt(long, default_value = "http://127.0.0.1:8545")]
    rpc: reqwest::Url,
    #[structopt(long)]
    private_key: PrivateKeySigner,
    #[structopt(long)]
    amount: String,
    #[structopt(long, default_value = "0")]
    fee: String,
    #[structopt(long, default_value = "0")]
    spend: String,
    #[structopt(long)]
    receiver: Address,
    #[structopt(long)]
    contract: Address,
}

#[derive(StructOpt)]
struct GenerateWitnessProofOfBurnOpt {
    #[structopt(long)]
    dat: PathBuf,
    #[structopt(long)]
    input: PathBuf,
    #[structopt(long)]
    witness: PathBuf,
}

#[derive(StructOpt)]
enum GenerateWitnessOpt {
    Spend,
    ProofOfBurn(GenerateWitnessProofOfBurnOpt),
}

#[derive(StructOpt)]
enum MinerOpt {
    Rapidsnark {
        #[structopt(long)]
        zkey: PathBuf,
        #[structopt(long)]
        witness: PathBuf,
    },
    GenerateWitness(GenerateWitnessOpt),
    Burn(BurnOpt),
    Mine,
}

use alloy::{
    eips::BlockId,
    hex::ToHexExt,
    network::TransactionBuilder,
    primitives::{
        Address, B256, U256, keccak256,
        utils::{format_ether, parse_ether},
    },
    providers::{Provider, ProviderBuilder},
    rlp::RlpDecodable,
    rpc::types::{EIP1186AccountProofResponse, TransactionRequest},
    signers::local::PrivateKeySigner,
    transports::http::reqwest,
};

fn find_burn_key(pow_min_zero_bytes: usize) -> Fp {
    let mut curr: U256 = U256::from_le_bytes(Fp::random(ff::derive::rand_core::OsRng).to_repr().0);
    loop {
        let mut inp: [u8; 40] = [0; 40];
        inp[..32].copy_from_slice(&curr.to_be_bytes::<32>());
        inp[32..].copy_from_slice(b"EIP-7503");
        let hash: U256 = keccak256(inp).into();
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

#[derive(Serialize, Deserialize, Debug)]
struct RapidsnarkProof {
    pi_a: [U256; 3],
    pi_b: [[U256; 2]; 3],
    pi_c: [U256; 3],
    protocol: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct RapidsnarkOutput {
    proof: RapidsnarkProof,
    public: Vec<U256>,
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

use alloy::rlp::Encodable;
use alloy::sol;
use tempfile::tempdir;

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    BETH,
    "./src/BETH.abi.json"
);

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = MinerOpt::from_args();
    let params_dir = homedir::my_home()?
        .ok_or(anyhow!("Can't find user's home directory!"))?
        .join(".worm-miner");

    match opt {
        MinerOpt::Rapidsnark { zkey, witness } => {
            let params = std::fs::read(zkey)?;
            let witness = std::fs::read(witness)?;
            let proof = rapidsnark(&params, &witness)?;
            let proof_proof: RapidsnarkProof = serde_json::from_str(&proof.proof)?;
            let proof_public: Vec<U256> = serde_json::from_str(&proof.public)?;
            println!(
                "{}",
                serde_json::to_string(&RapidsnarkOutput {
                    proof: proof_proof,
                    public: proof_public
                })?
            );
        }
        MinerOpt::GenerateWitness(gw_opt) => match gw_opt {
            GenerateWitnessOpt::ProofOfBurn(gw_pob_opt) => {
                generate_proof_of_burn_witness_file(
                    gw_pob_opt.dat,
                    gw_pob_opt.input,
                    gw_pob_opt.witness,
                )?;
            }
            GenerateWitnessOpt::Spend => {
                unimplemented!()
            }
        },
        MinerOpt::Burn(burn_opt) => {
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

            let fee = parse_ether(&burn_opt.fee)?;
            let spend = parse_ether(&burn_opt.spend)?;
            let amount = parse_ether(&burn_opt.amount)?;

            if fee + spend > amount {
                return Err(anyhow!(
                    "Sum of --fee and --spend should be less than --amount!"
                ));
            }

            let wallet_addr = burn_opt.private_key.address();

            let provider = ProviderBuilder::new()
                .wallet(burn_opt.private_key)
                .connect_http(burn_opt.rpc);

            if provider.get_code_at(burn_opt.contract).await?.0.is_empty() {
                panic!("BETH contract does not exist!");
            }

            println!("Generating a burn-key...");
            let burn_key = find_burn_key(3);

            let burn_addr = generate_burn_address(burn_key, burn_opt.receiver);
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

            let proof_dir = tempdir()?;
            let input_json_path = "input.json";
            let witness_path = "witness.wtns"; //proof_dir.path().join("witness.wtns");

            println!("Generating input.json file at: {}", input_json_path);
            std::fs::write(
                &input_json_path,
                input_file(proof, header_bytes, burn_key, fee, spend, burn_opt.receiver)?
                    .to_string(),
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
            let beth = BETH::new(burn_opt.contract, provider);
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
                    burn_opt.receiver,
                )
                .send()
                .await?
                .get_receipt()
                .await?;

            println!("Receipt: {:?}", mint_receipt);
        }
        MinerOpt::Mine => {}
    }
    Ok(())
}
