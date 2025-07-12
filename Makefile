.PHONY=download_params

download_params:
	mkdir ~/.worm-miner
	cd ~/.worm-miner && wget -c https://github.com/worm-privacy/proof-of-burn/releases/download/v0.1.0/params.tar.gz.aa
	cd ~/.worm-miner && wget -c https://github.com/worm-privacy/proof-of-burn/releases/download/v0.1.0/params.tar.gz.ab
	cd ~/.worm-miner && wget -c https://github.com/worm-privacy/proof-of-burn/releases/download/v0.1.0/params.tar.gz.ac
	cd ~/.worm-miner && wget -c https://github.com/worm-privacy/proof-of-burn/releases/download/v0.1.0/params.tar.gz.ad
	cd ~/.worm-miner && wget -c https://github.com/worm-privacy/proof-of-burn/releases/download/v0.1.0/params.tar.gz.ae
	cat ~/.worm-miner/params.tar.gz.a* > ~/.worm-miner/params.tar.gz
	cd ~/.worm-miner && tar xzf params.tar.gz
