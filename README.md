# WORM miner

Test on Debian/Ubuntu systems:

1. Install requirements:
   ```
   sudo apt install -y build-essential cmake libgmp-dev libsodium-dev nasm curl m4 git wget unzip nlohmann-json3-dev pkg-config libssl-dev libclang-dev
   ```
3. Install Rust toolchain:
   ```
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
5. Clone the repo:
   ```
   git clone https://github.com/worm-privacy/miner && cd miner
   ```
7. Download parameters files:
   ```
   make download_params
   ```
9. Install `worm-miner`:
   ```
   cargo install --path .
   ```
11. Run Anvil
    ```
    anvil --mnemonic "myth like bonus scare over problem client lizard pioneer submit female collect"
    ```
13. Deploy BETH on Anvil:
    ```
    git clone https://github.com/worm-privacy/worm && cd worm && forge create --rpc-url http://127.0.0.1:8545 src/BETH.sol:BETH --private-key 0x4f3edf983ac636a65a842ce7c78d9aa706d3b113bce9c46f30d7d21715b23b1d --broadcast
    ```
15. Make sure the contract is deployed at: `0xe78A0F7E598Cc8b0Bb87894B0F60dD2a88d6a8Ab`
16. Burn some ETH and generate and submit a proof:
    ```
    worm-miner burn --network anvil --private-key 0x4f3edf983ac636a65a842ce7c78d9aa706d3b113bce9c46f30d7d21715b23b1d  --amount 1 --spend 0.999 --fee 0.001
    ```
18. Congrats! 0.999 BETH has been minted for `0x90F8bf6A479f320ead074411a4B0e7944Ea8c9C1`! Check by running:
    ```
    worm-miner info --network anvil --private-key 0x4f3edf983ac636a65a842ce7c78d9aa706d3b113bce9c46f30d7d21715b23b1d
    ```
19. Now run the miner:
   ```
   worm-miner mine --network anvil --private-key 0x4f3edf983ac636a65a842ce7c78d9aa706d3b113bce9c46f30d7d21715b23b1d --amount-per-epoch 0.0001 --num-epochs 3 --claim-interval 3
   ```
   Where:
      - `--amount-per-epoch` is the amount of BETH you will consume in each epoch.
      - `--num-epochs` is the number of epochs you would like to participate in in advance.
      - `--claim-interval` is the number of epochs you would like to wait before initiating WORM claims.
      - `--custom-rpc` is an optional parameter that takes in an rpc-url.
      
20. Alternatively, you can participate in participate/claim process without running a live miner:
    You first have to participate in epochs through the `participate` operation:
    ```
    worm-miner participate --network anvil --private-key 0x4f3edf983ac636a65a842ce7c78d9aa706d3b113bce9c46f30d7d21715b23b1d --num-epochs 3 --amount-per-epoch 0.0001
    ```
    And then claim your WORM tokens later by running:
    ```
    worm-miner claim --private-key 0x4f3edf983ac636a65a842ce7c78d9aa706d3b113bce9c46f30d7d21715b23b1d --from-epoch 0 --num-epochs 3
    ```
