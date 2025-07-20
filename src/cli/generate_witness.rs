use structopt::StructOpt;

use std::{path::PathBuf, process::Command, time::Duration};

// use alloy_rlp::Decodable;
use crate::fp::{Fp, FpRepr};
use anyhow::anyhow;
use ff::{Field, PrimeField};
// use poseidon2::poseidon2;
// use serde::{Deserialize, Serialize};
// use serde_json::json;
use crate::networks::{NETWORKS, Network};
use crate::poseidon2::poseidon2;
use crate::utils::{RapidsnarkOutput, find_burn_key, generate_burn_address, input_file};
use alloy::rlp::Encodable;
use worm_witness_gens::generate_proof_of_burn_witness_file;
// use alloy::sol;
use tempfile::tempdir;

use crate::utils::{BETH, WORM};

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
