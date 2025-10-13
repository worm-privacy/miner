# ========= Stage 1: Build rapidsnark =========
FROM debian:bookworm-slim AS rapidsnark-builder
WORKDIR /src

# Install build dependencies for rapidsnark
RUN apt-get update && \
    apt-get install --no-install-recommends -y \
    build-essential \
    cmake \
    libgmp-dev \
    libsodium-dev \
    nasm \
    curl \
    m4 \
    git \
    ca-certificates \
    && apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Clone and build rapidsnark
RUN git clone https://github.com/iden3/rapidsnark.git && \
    cd rapidsnark && \
    git submodule init && \
    git submodule update
WORKDIR /src/rapidsnark

# Build GMP and rapidsnark
RUN ./build_gmp.sh host && \
    make host

# ========= Stage 2: Build circuits from worm-privacy/witness =========
FROM rapidsnark-builder AS circuits-builder
WORKDIR /src

# Install git and nlohmann-json for witness build
RUN apt-get update && \
    apt-get install --no-install-recommends -y git nlohmann-json3-dev pkg-config && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

# Clone and build witness circuits
RUN git clone https://github.com/worm-privacy/witness && \
    cd witness && \
    make all

# ========= Stage 3: Build Rust worm-miner =========
FROM rustlang/rust:nightly-bookworm AS rust-builder
WORKDIR /src

# Install additional dependencies for Rust build
RUN apt-get update && \
    apt-get install --no-install-recommends -y \
    build-essential \
    cmake \
    libgmp-dev \
    libsodium-dev \
    nasm \
    curl \
    m4 \
    git \
    pkg-config \
    libssl-dev \
    libclang-dev \
    nlohmann-json3-dev \
    && apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Copy worm-miner source
COPY . .

# Copy rapidsnark libraries and replace precompiled ones
COPY --from=rapidsnark-builder /src/rapidsnark/package/lib /src/rapidsnark-libs/lib
COPY --from=rapidsnark-builder /src/rapidsnark/package/include /src/rapidsnark-libs/include
COPY --from=circuits-builder /src/witness/libcircuits.a /src/rapidsnark-libs/

# Create symbolic links to our built libraries in the expected location
RUN mkdir -p /tmp/witness/rapidsnark-linux-x86_64-v0.0.7/lib && \
    mkdir -p /tmp/witness/rapidsnark-linux-x86_64-v0.0.7/include && \
    cp /src/rapidsnark-libs/lib/* /tmp/witness/rapidsnark-linux-x86_64-v0.0.7/lib/ && \
    cp /src/rapidsnark-libs/include/* /tmp/witness/rapidsnark-linux-x86_64-v0.0.7/include/ && \
    cp /src/rapidsnark-libs/libcircuits.a /tmp/witness/

# Copy the witness source for the circuits build
COPY --from=circuits-builder /src/witness/proof_of_burn /tmp/witness/proof_of_burn
COPY --from=circuits-builder /src/witness/spend /tmp/witness/spend
COPY --from=circuits-builder /src/witness/fr /tmp/witness/fr
COPY --from=circuits-builder /src/witness/Makefile /tmp/witness/

# Build args for Rust flags configuration
# By default, use conservative flags for wider compatibility
ARG RUSTFLAGS="-C target-cpu=x86-64 -C target-feature=-avx,-avx2,-fma"
ENV RUSTFLAGS="${RUSTFLAGS}"
ENV CARGO_UNSTABLE_EDITION2024=true

# Build the Rust application (release)
RUN cargo +nightly build --release

# ========= Stage 4: Final runtime image =========
FROM debian:bookworm-slim
WORKDIR /app

# Runtime dependencies
RUN apt-get update && \
    apt-get install --no-install-recommends -y \
    ca-certificates \
    libc6-dev \
    libgmp10 \
    libsodium23 \
    libgomp1 \
    libstdc++6 \
    wget \
    make \
    && apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# Copy the compiled binary
COPY --from=rust-builder /src/target/release/worm-miner /usr/local/bin/worm-miner

# Copy Makefile for artifact download helper
COPY Makefile /usr/local/share/worm-miner/Makefile

# Create directories
RUN mkdir -p /root/.worm-miner /usr/local/share/worm-miner

# Create artifact download helper script (includes actual download)
RUN set -e; \
  printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    'echo "🔄 Downloading worm-miner artifacts..."' \
    'cd /usr/local/share/worm-miner' \
    'make WGET_ARGS=-q download_params' \
    'echo "✅ Artifacts downloaded to /root/.worm-miner/"' \
    'echo "📁 Contents:"' \
    'ls -lah /root/.worm-miner/' \
  > /usr/local/bin/worm-miner-download-artifacts && \
  chmod +x /usr/local/bin/worm-miner-download-artifacts

# Auto-download params on first container start if missing (disable with AUTO_DOWNLOAD=0)
RUN set -e; \
  printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    '' \
    '# Auto-download params on first run' \
    'if [ "${AUTO_DOWNLOAD:-1}" = "1" ]; then' \
    '  if [ ! -s /root/.worm-miner/proof_of_burn.zkey ] || [ ! -s /root/.worm-miner/proof_of_burn.dat ]; then' \
    '    echo "🔄 Params missing; downloading...";' \
    '    /usr/local/bin/worm-miner-download-artifacts;' \
    '  else' \
    '    echo "✅ Params present; skipping download.";' \
    '  fi' \
    'fi' \
    '' \
    'exec /usr/local/bin/worm-miner "$@"' \
  > /usr/local/bin/docker-entrypoint.sh && \
  chmod +x /usr/local/bin/docker-entrypoint.sh

# Document the default server port
EXPOSE 8080

# Entrypoint wrapper (auto-download then exec worm-miner)
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["--help"]
