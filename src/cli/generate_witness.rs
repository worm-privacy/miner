use anyhow::Ok;
use std::path::PathBuf;
use structopt::StructOpt;
use worm_witness_gens::{generate_proof_of_burn_witness_file, generate_spend_witness_file};

#[derive(StructOpt)]
pub struct GenerateWitnessProofOfBurnOpt {
    #[structopt(long)]
    dat: PathBuf,
    #[structopt(long)]
    input: PathBuf,
    #[structopt(long)]
    witness: PathBuf,
}

#[derive(StructOpt)]
pub struct GenerateWitnessSpendOpt {
    #[structopt(long)]
    dat: PathBuf,
    #[structopt(long)]
    input: PathBuf,
    #[structopt(long)]
    witness: PathBuf,
}
#[derive(StructOpt)]
pub enum GenerateWitnessOpt {
    Spend(GenerateWitnessSpendOpt),
    ProofOfBurn(GenerateWitnessProofOfBurnOpt),
}

impl GenerateWitnessOpt {
    pub async fn run(self) -> Result<(), anyhow::Error> {
        match self {
            GenerateWitnessOpt::ProofOfBurn(gw_pob_opt) => {
                if let Err(e) = generate_proof_of_burn_witness_file(
                    gw_pob_opt.dat,
                    gw_pob_opt.input,
                    gw_pob_opt.witness,
                ) {
                    eprintln!("[Error: ProofOfBurn witness generation failed] {e}");
                    return Err(e);
                }
            }
            GenerateWitnessOpt::Spend(opt) => {
                if let Err(e) = generate_spend_witness_file(opt.dat, opt.input, opt.witness) {
                    eprintln!("[Error: Spend witness generation failed] {e}");
                    return Err(e);
                }
                println!("✅ Spend witness generated successfully.");
            }
        }
        Ok(())
    }
}
