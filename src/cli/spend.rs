use alloy::network::any;
use structopt::StructOpt;

use std::process::Command;

use crate::fp::{Fp, FpRepr};
use anyhow::{anyhow, Ok};
use ff::PrimeField;
use std::io::{self, Write};
use serde_json::Value;
use crate::networks::NETWORKS;
use crate::poseidon2::poseidon2;
use crate::utils::{RapidsnarkOutput, find_burn_key, generate_burn_address, input_file};
use alloy::rlp::Encodable;
use tempfile::tempdir;
use std::{fs, path::Path};
use crate::cli::utils::check_required_files;

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
use anyhow::{bail, Context, Result};
use super::CommonOpt;



#[derive(StructOpt)]
pub struct SpendOpt {
    #[structopt(flatten)]
    common_opt: CommonOpt,
    #[structopt(long, default_value = "anvil")]
    id: String,
    #[structopt(long)]
    amount: String,
    #[structopt(long)]
    fee:String,
  
}

// fn prompt_line(prompt: &str) -> Result<String> {
//     print!("{prompt}");
//     io::stdout().flush().ok();
//     let mut s = String::new();
//     io::stdin().read_line(&mut s).context("failed to read from stdin")?;
//     Ok(s.trim().to_owned())
// }
// fn parse_u256_any(s: &str) -> Result<U256> {
//     let s = s.trim();
//     if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
//         let bytes = hex::decode(hex).context("invalid hex for U256")?;
//         if bytes.len() > 32 {
//             bail!("hex too long for U256 ({} bytes)", bytes.len());
//         }
//         let mut be = [0u8; 32];
//         be[32 - bytes.len()..].copy_from_slice(&bytes); // left-pad
//         Ok(U256::from_big_endian(&be))
//     } else {
//         U256::from_dec_str(s).context("invalid decimal for U256")
//     }
// }
impl SpendOpt {
    pub async fn run(self, params_dir: &std::path::Path) -> Result<(), anyhow::Error> {
        // let net = NETWORKS.get(&self.network).expect("Invalid network!");
        // let required_files = [
        //     "proof_of_burn.dat",
        //     "proof_of_burn.zkey",
        //     "spend.dat",
        //     "spend.zkey",
        // ];

        // for req_file in required_files {
        //     let full_path = params_dir.join(req_file);
        //     if !std::fs::exists(&full_path)? {
        //         panic!(
        //             "File {} does not exist! Make sure you have downloaded all required files through `make download_params`!",
        //             full_path.display()
        //         );
        //     }
        // }

        // let provider = ProviderBuilder::new()
        //     .wallet(&self.private_key)
        //     .connect_http(net.rpc.clone());
        check_required_files(params_dir)?;
        let runtime_context = self.common_opt.setup().await?;
        let net = runtime_context.network;
        let provider = runtime_context.provider;
        let wallet_addr = runtime_context.wallet_address;
        // 1.get burn key from coins.json
        println!("✓ burn-key id = {}", &self.id);
        println!("✓ amount      = {}", &self.amount);
        println!("✓ fee         = {}", &self.fee);
        let coins_path = params_dir.join("coins.json");
        if !coins_path.exists() {
            println!("No coins.json found at {}", coins_path.display());
            return Ok(());
        }
        let data = fs::read_to_string(&coins_path)
        .with_context(|| format!("failed to read {}", coins_path.display()))?;

        let json: Value = serde_json::from_str(&data)
        .with_context(|| format!("failed to parse {} as JSON", coins_path.display()))?;

        let arr = json.as_array()
        .with_context(|| format!("expected {} to be a JSON array", coins_path.display()))?;

        let coin = arr.iter().find(|obj|{
            obj.get("id").map_or(false,|v| match v{
                Value::String(s)=> s == &self.id,
                Value::Number(n)=> n.to_string() == self.id,
                _ => false,

            })
        }).ok_or_else(|| anyhow!("no coin with id {} found in {}", self.id, coins_path.display()))?;
        println!("{}", serde_json::to_string_pretty(coin)?);
        let burn_key = match coin.get("burnKey") {
            Some(Value::String(key)) => key.clone(),
            _ => bail!("burn_key not found in the coin object"),
        };
        /*

             2. get previous coin


         */
        let original_amount = match coin.get("amount") {
            Some(Value::String(amount)) => amount.clone(),
            _ => bail!("amount not found in the coin object"),
        };
        let original_amount_u256 = parse_ether(&original_amount)?;
        println!("✓ burn-key = {}", burn_key);
         /*

            3. compare prevous coin amount with new amount and fee

            
          */
        let fee = parse_ether(&self.fee)?;
        let amount = parse_ether(&self.amount)?;
        if amount + fee > parse_ether(&original_amount)? {
            return Err(anyhow!(
                "Sum of --fee and --amount should be less than the original amount!"
            ));
        }
        let burnkey = Fp::from_str_vartime(&burn_key.to_string()).unwrap();
        
        let remaining_coin_val = Fp::from_repr(FpRepr((original_amount_u256 - fee - amount).to_le_bytes::<32>())).unwrap();
        let remaining_coin = poseidon2([burnkey, remaining_coin_val]);
        /*
                    4. generate new proof
        */
        let block = provider
            .get_block(BlockId::latest())
            .await?
            .ok_or(anyhow!("Block not found!"))?;

        let mut header_bytes = Vec::new();
        block.header.inner.encode(&mut header_bytes);
        let _proof_dir = tempdir()?;
        let input_json_path = "input.json";
        let witness_path = "witness.wtns"; //proof_dir.path().join("witness.wtns");



        let proc_path = std::env::current_exe().expect("Failed to get current exe path");

        println!("Generating witness.wtns file at: {}", witness_path);
        Command::new(&proc_path)
            .arg("generate-witness")
            .arg("proof-of-burn")
            .arg("--input")
            .arg(input_json_path)
            .arg("--dat")
            .arg(params_dir.join("spend.dat"))
            .arg("--witness")
            .arg(witness_path)
            .output()?;

        println!("Generating proof...");
        let output: RapidsnarkOutput = serde_json::from_slice(
            &Command::new(&proc_path)
                .arg("rapidsnark")
                .arg("--zkey")
                .arg(params_dir.join("spend.zkey"))
                .arg("--witness")
                .arg(witness_path)
                .output()?
                .stdout,
        )?;

        println!("Generated proof successfully! {:?}", output);
        // 5. broadcast spend transaction 
        let beth = BETH::new(net.beth, provider);
        // let spend_receipt = beth
        //     .spendCoin(
        //         [output.proof.pi_a[0], output.proof.pi_a[1]],
        //         [
        //             [output.proof.pi_b[0][1], output.proof.pi_b[0][0]],
        //             [output.proof.pi_b[1][1], output.proof.pi_b[1][0]],
        //         ],
        //         [output.proof.pi_c[0], output.proof.pi_c[1]],
        //         U256::from(block.header.number),
        //         U256::from_le_bytes(nullifier.to_repr().0),
        //         U256::from_le_bytes(remaining_coin.to_repr().0),
        //         fee,
        //         spend,
        //         wallet_addr,
        //     )
        //     .send()
        //     .await?
        //     .get_receipt()
        //     .await?;
        
        // 6. check spend transaction status
        // 7. update coins.json
        Ok(())
    }
}
