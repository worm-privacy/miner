use ff::PrimeField;

use crate::fp::Fp;

/// This is the base constant value for the Poseidon prefix,
/// derived from keccak256("EIP-7503") mod P
static POSEIDON_PREFIX_VALUE: &str =
    "5265656504298861414514317065875120428884240036965045859626767452974705356670";

fn poseidon_prefix() -> Fp {
    Fp::from_str_vartime(POSEIDON_PREFIX_VALUE).unwrap()
}

pub fn poseidon_burn_address_prefix() -> Fp {
    poseidon_prefix() + Fp::from(0)
}

pub fn poseidon_nullifier_prefix() -> Fp {
    poseidon_prefix() + Fp::from(1)
}

pub fn poseidon_coin_prefix() -> Fp {
    poseidon_prefix() + Fp::from(2)
}
