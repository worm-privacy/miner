mod fp;
mod poseidon2;

use std::{path::PathBuf, process::Command, time::Duration};

use alloy_rlp::Decodable;
use anyhow::anyhow;
use ff::{Field, PrimeField};
use fp::{Fp, FpRepr};
use poseidon2::poseidon2;
use serde::{Deserialize, Serialize};
use serde_json::json;
use structopt::StructOpt;
use worm_witness_gens::{generate_proof_of_burn_witness_file, rapidsnark};

#[derive(Debug, Clone)]
struct Network {
    rpc: reqwest::Url,
    beth: Address,
    worm: Address,
}

lazy_static::lazy_static! {
    static ref NETWORKS: HashMap<String, Network> = {
        [
            (
                "anvil".into(),
                Network {
                    rpc: "http://127.0.0.1:8545".parse().unwrap(),
                    beth: address!("0xe78A0F7E598Cc8b0Bb87894B0F60dD2a88d6a8Ab"),
                    worm: address!("0x5b1869D9A4C187F2EAa108f3062412ecf0526b24"),
                },
            ),
            (
                "sepolia".into(),
                Network {
                    rpc: "https://sepolia.drpc.org".parse().unwrap(),
                    beth: address!("0x6fa638704a839B28C5B7168C8916AdD9F75CDEEc"),
                    worm: address!("0x557E9e7Eed905C7d21183Ec333dB2a8B1e34A85F"),
                },
            ),
        ]
        .into_iter()
        .collect()
    };
}

#[derive(StructOpt)]
struct InfoOpt {
    #[structopt(long, default_value = "anvil")]
    network: String,
    #[structopt(long)]
    private_key: PrivateKeySigner,
}

#[derive(StructOpt)]
struct ClaimOpt {
    #[structopt(long, default_value = "anvil")]
    network: String,
    #[structopt(long)]
    private_key: PrivateKeySigner,
    #[structopt(long, default_value = "10")]
    epochs_to_check: usize,
}

#[derive(StructOpt)]
struct MineOpt {
    #[structopt(long, default_value = "anvil")]
    network: String,
    #[structopt(long)]
    private_key: PrivateKeySigner,
    #[structopt(long)]
    min_beth_per_epoch: String,
    #[structopt(long)]
    max_beth_per_epoch: String,
    #[structopt(long)]
    assumed_worm_price: String,
    #[structopt(long)]
    future_epochs: usize,
}

#[derive(StructOpt)]
struct ParticipateOpt {
    #[structopt(long, default_value = "anvil")]
    network: String,
    #[structopt(long)]
    private_key: PrivateKeySigner,
    #[structopt(long)]
    amount_per_epoch: String,
    #[structopt(long)]
    num_epochs: usize,
}

#[derive(StructOpt)]
struct BurnOpt {
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
    Info(InfoOpt),
    Participate(ParticipateOpt),
    Claim(ClaimOpt),
    Rapidsnark {
        #[structopt(long)]
        zkey: PathBuf,
        #[structopt(long)]
        witness: PathBuf,
    },
    GenerateWitness(GenerateWitnessOpt),
    Burn(BurnOpt),
    Mine(MineOpt),
}

use alloy::{
    eips::BlockId,
    hex::ToHexExt,
    network::TransactionBuilder,
    primitives::{
        Address, B256, U256, address, keccak256,
        map::HashMap,
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

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    WORM,
    "./src/WORM.abi.json"
);

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt = MinerOpt::from_args();
    let params_dir = homedir::my_home()?
        .ok_or(anyhow!("Can't find user's home directory!"))?
        .join(".worm-miner");

    match opt {
        MinerOpt::Claim(claim_opt) => {
            let addr = claim_opt.private_key.address();
            let net = NETWORKS.get(&claim_opt.network).expect("Invalid network!");
            let provider = ProviderBuilder::new()
                .wallet(claim_opt.private_key)
                .connect_http(net.rpc.clone());
            let worm = WORM::new(net.worm, provider.clone());
            let epoch = worm.currentEpoch().call().await?;
            let num_epochs_to_check = std::cmp::min(epoch, U256::from(claim_opt.epochs_to_check));
            let receipt = worm
                .claim(
                    epoch.saturating_sub(U256::from(num_epochs_to_check)),
                    num_epochs_to_check,
                )
                .send()
                .await?
                .get_receipt()
                .await?;
            if receipt.status() {
                println!("Success!");
                let worm_balance = worm.balanceOf(addr).call().await?;
                println!("WORM balance: {}", format_ether(worm_balance));
            }
        }
        MinerOpt::Info(info_opt) => {
            let addr = info_opt.private_key.address();
            let net = NETWORKS.get(&info_opt.network).expect("Invalid network!");
            let provider = ProviderBuilder::new()
                .wallet(info_opt.private_key)
                .connect_http(net.rpc.clone());
            let worm = WORM::new(net.worm, provider.clone());
            let beth = BETH::new(net.beth, provider.clone());
            let worm_balance = worm.balanceOf(addr).call().await?;
            let epoch = worm.currentEpoch().call().await?;
            let beth_balance = beth.balanceOf(addr).call().await?;
            let num_epochs_to_check = std::cmp::min(epoch, U256::from(10));
            let claimable_worm = worm
                .calculateMintAmount(
                    epoch.saturating_sub(num_epochs_to_check),
                    num_epochs_to_check,
                    addr,
                )
                .call()
                .await?;
            println!("Current epoch: {}", epoch);
            println!("BETH balance: {}", format_ether(beth_balance));
            println!("WORM balance: {}", format_ether(worm_balance));
            println!(
                "Claimable WORM (10 last epochs): {}",
                format_ether(claimable_worm)
            );
            let epoch_u64 = epoch.as_limbs()[0];
            for e in epoch_u64..epoch_u64 + 10 {
                let total = worm.epochTotal(U256::from(e)).call().await?;
                let user = worm.epochUser(U256::from(e), addr).call().await?;
                let share = if !total.is_zero() {
                    user * U256::from(50) * U256::from(10).pow(U256::from(18)) / total
                } else {
                    U256::ZERO
                };
                println!(
                    "Epoch #{} => {} / {} (Expecting {} WORM)",
                    e,
                    format_ether(user),
                    format_ether(total),
                    format_ether(share)
                );
            }
        }
        MinerOpt::Participate(participate_opt) => {
            let net = NETWORKS
                .get(&participate_opt.network)
                .expect("Invalid network!");
            let provider = ProviderBuilder::new()
                .wallet(participate_opt.private_key)
                .connect_http(net.rpc.clone());
            let amount_per_epoch = parse_ether(&participate_opt.amount_per_epoch)?;
            let worm = WORM::new(net.worm, provider.clone());
            let beth = BETH::new(net.beth, provider.clone());
            println!("Approving BETH...");
            let beth_approve_receipt = beth
                .approve(
                    net.worm,
                    amount_per_epoch * U256::from(participate_opt.num_epochs),
                )
                .send()
                .await?
                .get_receipt()
                .await?;
            if !beth_approve_receipt.status() {
                panic!("Failed on BETH approval!");
            }
            let receipt = worm
                .participate(amount_per_epoch, U256::from(participate_opt.num_epochs))
                .send()
                .await?
                .get_receipt()
                .await?;
            if receipt.status() {
                println!("Success!");
            }
        }
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
            let net = NETWORKS.get(&burn_opt.network).expect("Invalid network!");

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

            if amount > parse_ether("1")? {
                return Err(anyhow!("Can't burn more than 1 ETH!"));
            }

            if fee + spend > amount {
                return Err(anyhow!(
                    "Sum of --fee and --spend should be less than --amount!"
                ));
            }

            let wallet_addr = burn_opt.private_key.address();

            let provider = ProviderBuilder::new()
                .wallet(burn_opt.private_key)
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

            let proof_dir = tempdir()?;
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
        }
        MinerOpt::Mine(mine_opt) => {
            let assumed_worm_price = parse_ether(&mine_opt.assumed_worm_price)?;
            let minimum_beth_per_epoch = parse_ether(&mine_opt.min_beth_per_epoch)?;
            let maximum_beth_per_epoch = parse_ether(&mine_opt.max_beth_per_epoch)?;
            let addr = mine_opt.private_key.address();
            let net = NETWORKS.get(&mine_opt.network).expect("Invalid network!");
            let provider = ProviderBuilder::new()
                .wallet(mine_opt.private_key)
                .connect_http(net.rpc.clone());
            let worm = WORM::new(net.worm, provider.clone());
            let beth = BETH::new(net.beth, provider.clone());
            if beth.allowance(addr, net.worm).call().await?.is_zero() {
                println!("Approving infinite BETH allowance to WORM contract...");
                let beth_approve_receipt = beth
                    .approve(net.worm, U256::MAX)
                    .send()
                    .await?
                    .get_receipt()
                    .await?;
                if !beth_approve_receipt.status() {
                    panic!("Failed on BETH approval!");
                }
            }
            if beth.balanceOf(addr).call().await?.is_zero() {
                println!(
                    "You don't have any BETH! Mine some BETH through the `worm-miner burn` command."
                );
            } else {
                // WORM miner equation:
                // userShare / (userShare + totalShare) * 50 * assumedWormPrice >= userShare
                // if totalShare > 0 => userShare = 50 * assumedWormPrice - totalShare
                // if totalShare = 0 => userShare = minimumBethPerEpoch
                loop {
                    let epoch = worm.currentEpoch().call().await?;
                    let previous_epoch = epoch.saturating_sub(U256::ONE);
                    let previous_total_share = worm.epochTotal(previous_epoch).call().await?;
                    let current_total_share = worm.epochTotal(epoch).call().await?;
                    let current_user_share = worm.epochUser(epoch, addr).call().await?;
                    let user_share = std::cmp::min(
                        std::cmp::max(
                            if current_total_share.is_zero() {
                                minimum_beth_per_epoch
                            } else {
                                (U256::from(50) * assumed_worm_price)
                                    .saturating_sub(previous_total_share)
                            },
                            minimum_beth_per_epoch,
                        ),
                        maximum_beth_per_epoch,
                    )
                    .saturating_sub(current_user_share);

                    let num_epochs_to_check = std::cmp::min(epoch, U256::from(10));
                    let claimable_worm = worm
                        .calculateMintAmount(
                            epoch.saturating_sub(num_epochs_to_check),
                            num_epochs_to_check,
                            addr,
                        )
                        .call()
                        .await?;

                    if user_share >= minimum_beth_per_epoch {
                        println!(
                            "Participating {} x {} for epochs {}..{}",
                            mine_opt.future_epochs,
                            format_ether(user_share),
                            epoch,
                            epoch + U256::from(mine_opt.future_epochs)
                        );
                        let receipt = worm
                            .participate(user_share, U256::from(mine_opt.future_epochs as u64))
                            .send()
                            .await?
                            .get_receipt()
                            .await?;
                        if receipt.status() {
                            println!("Success!");
                        }

                        if !(epoch % U256::from(10)).is_zero() && claimable_worm.is_zero() {
                            println!("Claiming WORMs...");
                            let receipt = worm
                                .claim(
                                    epoch.saturating_sub(num_epochs_to_check),
                                    num_epochs_to_check,
                                )
                                .send()
                                .await?
                                .get_receipt()
                                .await?;
                            if receipt.status() {
                                println!("Success!");
                            }
                        }
                    }

                    let eth_balance = provider.get_balance(addr).await?;
                    let beth_balance = beth.balanceOf(addr).call().await?;
                    let worm_balance = worm.balanceOf(addr).call().await?;

                    println!(
                        "ETH: {} BETH: {} WORM: {} Claimable WORM: {}",
                        format_ether(eth_balance),
                        format_ether(beth_balance),
                        format_ether(worm_balance),
                        format_ether(claimable_worm)
                    );

                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
            }
        }
    }
    Ok(())
}
