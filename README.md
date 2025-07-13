# WORM miner

Steps:

1. Install Rust toolchain:
    `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
2. Clone the repo:
    `git clone https://github.com/worm-privacy/miner && cd miner`
3. Download parameters files:
    `make download_params`
4. Install `worm-miner`:
    `cargo install --path .`
5. Run Anvil `anvil --mnemonic "myth like bonus scare over problem client lizard pioneer submit female collect"`, burn some ETH and generate a proof:
    `worm-miner burn --private-key 0x4f3edf983ac636a65a842ce7c78d9aa706d3b113bce9c46f30d7d21715b23b1d --amount 0.01 --fee 0.001 --receiver 0x90F8bf6A479f320ead074411a4B0e7944Ea8c9C1`
