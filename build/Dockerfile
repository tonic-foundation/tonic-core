# NEAR contract builder with rust 1.62.0
# Based on https://github.com/near/near-sdk-rs/tree/master/contract-builder
FROM rust:1.62.0

LABEL description="Container for builds"

RUN rustup default 1.62.0
RUN rustup target add wasm32-unknown-unknown

RUN apt-get -y update && apt-get install -y \
    binaryen \
    clang \
    git \
    less \
    llvm \
    vim \
    wabt \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install just wasm-snip wasm-gc
