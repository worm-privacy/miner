use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;
use structopt::StructOpt;

#[derive(StructOpt)]
pub enum LsCommand {
    /// List coins from coins.json
    Coin(LsOpt),
    /// List burn entries from burn.json
    Burn(LsOpt),
}

#[derive(StructOpt)]
pub struct LsOpt {
    /// The network to filter by (default: anvil)
    #[structopt(long, default_value = "anvil")]
    network: String,
}

impl LsCommand {
    pub async fn run(self, params_dir: &Path) -> Result<()> {
        match self {
            LsCommand::Coin(opt) => opt.run("coins.json", params_dir).await,
            LsCommand::Burn(opt) => opt.run("burn.json", params_dir).await,
        }
    }
}

impl LsOpt {
    pub async fn run(&self, filename: &str, params_dir: &Path) -> Result<()> {
        println!("Using params directory: {}", params_dir.display());

        let path = params_dir.join(filename);

        if !path.exists() {
            println!("No {} found at {}", filename, path.display());
            return Ok(());
        }

        println!("Reading from {}", path.display());

        let data = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("Failed to read {}", path.display()))?;

        let entries: Vec<Value> = serde_json::from_str(&data)
            .with_context(|| format!("Failed to parse {}", path.display()))?;

        println!("Found {} entries in {}", entries.len(), filename);

        let matches = entries
            .into_iter()
            .filter(|entry| {
                entry
                    .get("network")
                    .and_then(Value::as_str)
                    .map_or(false, |net| net == self.network)
            })
            .collect::<Vec<_>>();

        println!("Filtering entries for network: \"{}\"", self.network);
        if matches.is_empty() {
            println!(
                "No entries found for network: \"{}\" in {}",
                self.network,
                path.display()
            );
        } else {
            println!(
                "Found {} entries for network \"{}\":",
                matches.len(),
                self.network,
            );
            for (i, entry) in matches.into_iter().enumerate() {
                println!("  {}: {}", i + 1, serde_json::to_string_pretty(&entry)?);
            }
        }

        println!("Done reading {}", filename);
        Ok(())
    }
}
