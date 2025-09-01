use alloy::primitives::{Address, address};
use lazy_static::lazy_static;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Network {
    pub rpc: reqwest::Url,
    pub beth: Address,
    pub worm: Address,
}

lazy_static! {
    pub static ref NETWORKS: HashMap<String, Network> = {
        [
            (
                "anvil".into(),
                Network {
                    rpc: "http://127.0.0.1:8545".parse().unwrap(),
                    beth: address!("0xe78A0F7E598Cc8b0Bb87894B0F60dD2a88d6a8Ab"),
                    worm: address!("0xC89Ce4735882C9F0f0FE26686c53074E09B0D550"),
                },
            ),
            (
                "sepolia".into(),
                Network {
                    rpc: "https://sepolia.drpc.org".parse().unwrap(),
                    beth: address!("0x1b218670EcaDA5B15e2cE1879074e5D903b55334"),
                    worm: address!("0x78eFE1D19d5F5e9AED2C1219401b00f74166A1d9"),
                },
            ),
        ]
        .into_iter()
        .collect()
    };
}
