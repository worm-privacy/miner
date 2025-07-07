use ff::PrimeField;
use num_bigint::BigUint;
use num_traits::{Euclid, Num, ToBytes};

#[derive(PrimeField)]
#[PrimeFieldModulus = "21888242871839275222246405745257275088548364400416034343698204186575808495617"]
#[PrimeFieldGenerator = "7"]
#[PrimeFieldReprEndianness = "little"]
pub struct Fp([u64; 4]);

lazy_static::lazy_static! {
    pub static ref FP_REMAINDER_BIGUINT: BigUint =
        BigUint::from_str_radix(&Fp::MODULUS[2..], 16).unwrap();
}

impl Fp {
    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        let mut bytes = BigUint::from_bytes_be(bytes)
            .rem_euclid(&FP_REMAINDER_BIGUINT)
            .to_le_bytes();
        while bytes.len() < 32 {
            bytes.push(0);
        }
        Fp::from_repr(FpRepr(bytes.try_into().unwrap())).unwrap()
    }
}
