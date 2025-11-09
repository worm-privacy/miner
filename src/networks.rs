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
                    beth: address!("0xCfEB869F69431e42cdB54A4F4f105C19C080A601"),
                    worm: address!("0x254dffcd3277C0b1660F6d42EFbB754edaBAbC2B"),
                },
            ),
            (
                "sepolia".into(),
                Network {
                    rpc: "https://sepolia.drpc.org".parse().unwrap(),
                    beth: address!("0xEc71903c94fe79634164c0ad1ba67be41f37e804"),
                    worm: address!("0xF25453f75ff520f9cE922E689B1cCE65DE3dC646"),
                },
            ),
        ]
        .into_iter()
        .collect()
    };
}
