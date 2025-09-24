use alloy::primitives::{Address, Bytes};
use alloy::providers::Provider;

use anyhow::{Context, Ok, Result as AnyResult, ensure};

use alloy::rpc::types::EIP1186AccountProofResponse;
use ethereum_triedb::{
    EIP1186Layout, StorageProof,
    keccak::{KeccakHasher, keccak_256},
};
// use hex_literal::hex;
use alloy::rpc::types::BlockNumberOrTag;
use primitive_types::{H256, U256};
use rlp::{Decodable, Rlp};
pub use rlp_types::Account;
use trie_db::{Trie, TrieDBBuilder};
mod rlp_types {
    use primitive_types::{H256, U256};
    use rlp_derive::RlpDecodable;
    use std::result::Result;

    #[derive(RlpDecodable, Debug)]
    pub struct Account {
        pub nonce: u64,
        pub balance: U256,
        pub storage_root: H256,
        pub code_hash: H256,
    }
}

pub fn verify_proof_logic(
    key: Address,
    balance: U256,
    nonce: u64,
    code_hash: H256,
    storage_hash: H256,
    account_proof: Vec<Vec<u8>>,
    state_root: H256,
) -> AnyResult<()> {
    let account_key = keccak_256(key.as_ref());

    let db = StorageProof::new(account_proof.clone()).into_memory_db::<KeccakHasher>();

    let trie = TrieDBBuilder::<EIP1186Layout<KeccakHasher>>::new(&db, &state_root).build();

    let encoded = trie
        .get(&account_key)
        .context("Account key not found in trie (possible bad proof or mismatched state_root)?")?
        .context("Empty value at account key")?;

    let account = Account::decode(&Rlp::new(&encoded)).context("RLP decoding of account failed")?;

    ensure!(
        account.balance == balance,
        "Balance mismatch: expected {:?}, got {:?}",
        balance,
        account.balance
    );
    ensure!(
        account.nonce == nonce,
        "Nonce mismatch: expected {}, got {}",
        nonce,
        account.nonce
    );
    ensure!(
        account.code_hash == code_hash,
        "Code hash mismatch: expected {:?}, got {:?}",
        code_hash,
        account.code_hash
    );
    ensure!(
        account.storage_root == storage_hash,
        "Storage root mismatch: expected {:?}, got {:?}",
        storage_hash,
        account.storage_root
    );

    Ok(())
}
pub async fn verify_proof<P: Provider>(
    provider: &P,
    proof: EIP1186AccountProofResponse,
    block_number: u64,
) -> AnyResult<()> {
    let address = proof.address;

    let be = proof.balance.to_be_bytes::<32>();
    let balance = U256::from_big_endian(&be);

    let code_hash = {
        let mut arr = [0u8; 32];
        arr.copy_from_slice(proof.code_hash.as_slice());
        H256(arr)
    };

    let nonce = proof.nonce;

    let storage_hash = {
        let mut arr = [0u8; 32];
        arr.copy_from_slice(proof.storage_hash.as_slice());
        H256(arr)
    };

    let account_proof: Vec<Vec<u8>> = proof
        .account_proof
        .into_iter()
        .map(|b: Bytes| b.to_vec())
        .collect();

    let block = provider
        .get_block_by_number(BlockNumberOrTag::Number(block_number))
        .await?
        .ok_or_else(|| anyhow::anyhow!("Block not found at height {}", block_number))?;

    let state_root = {
        let mut arr = [0u8; 32];
        arr.copy_from_slice(block.header.inner.state_root.as_slice());
        H256(arr)
    };

    verify_proof_logic(
        address,
        balance,
        nonce,
        code_hash,
        storage_hash,
        account_proof,
        state_root,
    )?;

    Ok(())
}
