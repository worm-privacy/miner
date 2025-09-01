use structopt::StructOpt;

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

#[derive(StructOpt)]
pub struct LsOpt {
    #[structopt(long, default_value = "anvil")]
    network: String,
}

impl LsOpt {
    pub async fn run(self, params_dir: &Path) -> Result<(), anyhow::Error> {
        println!("Using params directory: {}", params_dir.display());

        let coins_path = params_dir.join("coins.json");

        if !coins_path.exists() {
            println!("No coins.json found at {}", coins_path.display());
            return Ok(());
        }
        println!("Reading coins from {}", coins_path.display());
        let data = tokio::fs::read_to_string(&coins_path)
            .await
            .with_context(|| format!("failed to read {}", coins_path.display()))?;
        print!("Parsing coins from {}", coins_path.display());
        let coins: Vec<Value> = serde_json::from_str(&data)
            .with_context(|| format!("failed to parse {}", coins_path.display()))?;

        println!("Found {} entries in coins.json", coins.len());
        let matches = coins
            .into_iter()
            .filter(|coin| {
                coin.get("network")
                    .and_then(Value::as_str)
                    .map_or(false, |net| net == self.network)
            })
            .collect::<Vec<_>>();
        println!("Filtering entries for network: \"{}\"", self.network);
        if matches.is_empty() {
            println!(
                "No entries found for network: \"{}\" in {}",
                self.network,
                coins_path.display()
            );
            return Ok(());
        } else {
            println!(
                "Found {} entries for network: \"{}\" :",
                matches.len(),
                self.network,
            );
            for (i, coin) in matches.into_iter().enumerate() {
                println!("  {}: {}", i + 1, serde_json::to_string_pretty(&coin)?);
            }
        }

        println!("Done reading coins.json");
        Ok(())
    }
}
