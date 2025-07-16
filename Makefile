.PHONY=download_params

download_params:
	mkdir -p ~/.worm-miner
	echo "Downloading parameter files..."
	cd ~/.worm-miner && wget $(WGET_ARGS) -c https://github.com/worm-privacy/proof-of-burn/releases/download/v0.1.0/params.tar.gz.aa
	cd ~/.worm-miner && wget $(WGET_ARGS) -c https://github.com/worm-privacy/proof-of-burn/releases/download/v0.1.0/params.tar.gz.ab
	cd ~/.worm-miner && wget $(WGET_ARGS) -c https://github.com/worm-privacy/proof-of-burn/releases/download/v0.1.0/params.tar.gz.ac
	cd ~/.worm-miner && wget $(WGET_ARGS) -c https://github.com/worm-privacy/proof-of-burn/releases/download/v0.1.0/params.tar.gz.ad
	cd ~/.worm-miner && wget $(WGET_ARGS) -c https://github.com/worm-privacy/proof-of-burn/releases/download/v0.1.0/params.tar.gz.ae
	echo "Extracting parameter files..."
	cat ~/.worm-miner/params.tar.gz.a* > ~/.worm-miner/params.tar.gz
	rm -rf ~/.worm-miner/params.tar.gz.a*
	cd ~/.worm-miner && tar xzf params.tar.gz
	rm -rf ~/.worm-miner/params.tar.gz
	echo "Done!"
