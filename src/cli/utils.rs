use crate::fp::Fp;
use alloy::primitives::U256;
use anyhow::{Context, Result};
use ff::PrimeField;
use serde_json::{Value, json};
use std::{fs, path::Path};

pub fn check_required_files(params_dir: &std::path::Path) -> Result<(), anyhow::Error> {
    let required_files = [
        "proof_of_burn.dat",
        "proof_of_burn.zkey",
        "spend.dat",
        "spend.zkey",
    ];

    for req_file in required_files {
        let full_path = params_dir.join(req_file);
        if !std::fs::exists(&full_path)? {
            panic!(
                "File {} does not exist! Make sure you have downloaded all required files through `make download_params`!",
                full_path.display()
            );
        }
    }
    Ok(())
}

pub fn coins_file(
    coin_id: U256,
    burn_key: Fp,
    remaining_coin: U256,
    network: &str,
) -> Result<Value> {
    Ok(json!({
        "id": coin_id.to_string(),
        "burnKey": U256::from_le_bytes(burn_key.to_repr().0).to_string(),
        "amount": remaining_coin.to_string(),
        "network": network,
    }))
}

pub fn burn_file(
    coin_id: U256,
    burn_key: Fp,
    fee: U256,
    network: &str,
    spend: U256,
) -> Result<Value> {
    Ok(json!({
        "id": coin_id.to_string(),
        "burnKey": U256::from_le_bytes(burn_key.to_repr().0).to_string(),
        "fee": fee.to_string(),
        "spend":spend.to_string(),
        "network": network,
    }))
}

pub fn next_id<P: AsRef<Path>>(coins_path: P) -> Result<U256, anyhow::Error> {
    let path = coins_path.as_ref();

    // If the file doesn't exist, first ID is 1.
    if !path.exists() {
        return Ok(U256::from(1u64));
    }

    let data =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let json: Value = serde_json::from_str(&data)
        .with_context(|| format!("failed to parse {} as JSON", path.display()))?;

    let arr = json
        .as_array()
        .with_context(|| format!("expected {} to be a JSON array", path.display()))?;

    Ok(U256::from((arr.len() as u64) + 1))
}

pub fn init_coins_file<P: AsRef<Path>>(coins_path: P) -> Result<(), anyhow::Error> {
    let path = coins_path.as_ref();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .with_context(|| format!("failed to create parent dir {}", dir.display()))?;
    }
    if !path.exists() {
        fs::write(path, "[]")
            .with_context(|| format!("failed to create new {}", path.display()))?;
    }
    Ok(())
}

pub fn append_new_entry<P: AsRef<Path>>(coins_path: P, entry: Value) -> Result<(), anyhow::Error> {
    let path = coins_path.as_ref();
    let data =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut json_val: Value = serde_json::from_str(&data)
        .with_context(|| format!("{} was not valid JSON", path.display()))?;
    let arr = json_val
        .as_array_mut()
        .with_context(|| format!("{} was not a JSON array", path.display()))?;
    arr.push(entry);
    let pretty = serde_json::to_string_pretty(&json_val)
        .with_context(|| "failed to serialize updated JSON")?;
    fs::write(path, pretty).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
