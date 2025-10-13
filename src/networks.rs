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
                    worm: address!("0x5b1869D9A4C187F2EAa108f3062412ecf0526b24"),
                },
            ),
            (
                "sepolia".into(),
                Network {
                    rpc: "https://sepolia.drpc.org".parse().unwrap(),
                    beth: address!("0x198dbCAB39377f4219553Cc0e7133b7f37c6ca9e"),
                    worm: address!("0x7745F3fD93ad92DA828363Dc26EDbc9b2C788935"),
                },
            ),
        ]
        .into_iter()
        .collect()
    };
}
