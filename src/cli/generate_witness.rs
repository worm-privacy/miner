use std::path::PathBuf;
use structopt::StructOpt;
use worm_witness_gens::generate_proof_of_burn_witness_file;

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
pub enum GenerateWitnessOpt {
    Spend,
    ProofOfBurn(GenerateWitnessProofOfBurnOpt),
}

impl GenerateWitnessOpt {
    pub async fn run(self) -> Result<(), anyhow::Error> {
        match self {
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
        }
        Ok(())
    }
}
