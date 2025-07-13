# WORM miner

Test on Debian/Ubuntu systems:

1. Install requirements:
    `sudo apt install -y build-essential cmake libgmp-dev libsodium-dev nasm curl m4 git wget unzip nlohmann-json3-dev`
2. Install Rust toolchain:
    `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
3. Clone the repo:
    `git clone https://github.com/worm-privacy/miner && cd miner`
4. Download parameters files:
    `make download_params`
5. Install `worm-miner`:
    `cargo install --path .`
6. Run Anvil
    `anvil --mnemonic "myth like bonus scare over problem client lizard pioneer submit female collect"`
7. Deploy BETH on Anvil:
    `git clone https://github.com/worm-privacy/worm && cd worm && forge create --rpc-url http://127.0.0.1:8545 src/BETH.sol:BETH --private-key 0x4f3edf983ac636a65a842ce7c78d9aa706d3b113bce9c46f30d7d21715b23b1d --broadcast`
8. Make sure the contract is deployed at: `0xe78A0F7E598Cc8b0Bb87894B0F60dD2a88d6a8Ab`
9. Burn some ETH and generate and submit a proof:
    `worm-miner burn --rpc http://127.0.0.1:8545 --private-key 0x4f3edf983ac636a65a842ce7c78d9aa706d3b113bce9c46f30d7d21715b23b1d  --amount 1 --spend 0.999 --fee 0.001 --receiver 0x90F8bf6A479f320ead074411a4B0e7944Ea8c9C1 --contract 0xe78A0F7E598Cc8b0Bb87894B0F60dD2a88d6a8Ab`
