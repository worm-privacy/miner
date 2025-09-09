mod fp;
mod poseidon;

use cli::RecoverOpt;

use std::path::PathBuf;
use structopt::StructOpt;
pub mod cli;
pub mod constants;
pub mod networks;
use crate::cli::{
    BurnOpt, ClaimOpt, GenerateWitnessOpt, InfoOpt, LsCommand, MineOpt, ParticipateOpt, SpendOpt,
};
mod utils;
use crate::utils::{RapidsnarkOutput, RapidsnarkProof};

use alloy::rlp::RlpDecodable;

#[derive(StructOpt)]
enum MinerOpt {
    Info(InfoOpt),
    Ls(LsCommand),
    Spend(SpendOpt),
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
    Recover(RecoverOpt),
}

impl MinerOpt {
    pub async fn run(self, params_dir: &std::path::Path) -> Result<(), anyhow::Error> {
        match self {
            MinerOpt::Burn(cmd) => cmd.run(params_dir).await,
            MinerOpt::Spend(cmd) => cmd.run(params_dir).await,
            MinerOpt::Ls(cmd) => cmd.run(params_dir).await,
            MinerOpt::GenerateWitness(cmd) => cmd.run().await,
            MinerOpt::Info(cmd) => cmd.run().await,
            MinerOpt::Claim(cmd) => cmd.run().await,
            MinerOpt::Participate(cmd) => cmd.run().await,
            MinerOpt::Mine(cmd) => cmd.run().await,
            MinerOpt::Rapidsnark { zkey, witness } => {
                let params = std::fs::read(zkey)?;
                let witness = std::fs::read(witness)?;
                let proof = worm_witness_gens::rapidsnark(&params, &witness)?;
                let proof_proof: crate::RapidsnarkProof = serde_json::from_str(&proof.proof)?;
                let proof_public: Vec<alloy::primitives::U256> =
                    serde_json::from_str(&proof.public)?;

                println!(
                    "{}",
                    serde_json::to_string(&crate::RapidsnarkOutput {
                        proof: proof_proof,
                        public: proof_public
                    })?
                );

                Ok(())
            }

            MinerOpt::Recover(cmd) => cmd.run(params_dir).await,
        }
    }
}

#[derive(Debug, RlpDecodable, PartialEq)]
struct RlpLeaf {
    key: alloy::rlp::Bytes,
    value: alloy::rlp::Bytes,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let params_dir = homedir::my_home()?
        .ok_or(anyhow::anyhow!("Can't find user's home directory!"))?
        .join(".worm-miner");

    match MinerOpt::from_args().run(&params_dir).await {
        Ok(()) => {}
        Err(e) => eprintln!("Error running command: {:?}", e),
    }

    Ok(())
}
