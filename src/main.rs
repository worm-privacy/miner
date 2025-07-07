use structopt::StructOpt;

#[derive(StructOpt)]
struct BurnOpt {
    #[structopt(long, default_value = "https://eth.meowrpc.com")]
    rpc: String,
    #[structopt(long)]
    private_key: String,
}

#[derive(StructOpt)]
enum MinerOpt {
    Burn(BurnOpt),
    Mine,
}

use alloy::primitives::{FixedBytes, U256, keccak256};

fn find_burn_key(pow_min_zero_bytes: usize) -> U256 {
    let mut curr: U256 = FixedBytes::<32>::random().into();
    loop {
        let hash: U256 = keccak256(curr.to_be_bytes::<32>()).into();
        if hash.leading_zeros() >= pow_min_zero_bytes * 8 {
            return curr;
        }
        curr += U256::ONE;
    }
}

fn main() {
    
    let opt = MinerOpt::from_args();
    match opt {
        MinerOpt::Burn(burn_opt) => {
            println!("Generating a burn-key...");
            println!("{}", find_burn_key(3));
            println!("Hello, world!");
        }
        MinerOpt::Mine => {
            println!("Hello, world!");
        }
    }
}
