# show help
default:
	just --list

# Boneless version of official minify.sh example
build-minified: build
    #!/bin/bash
    set -eu

    # see https://github.com/near/near-sdk-rs/blob/master/minifier/minify.sh

    source scripts/prelude
    require_command wasm-gc
    require_command wasm-strip
    require_command wasm-opt

    ORIGINAL=target/wasm32-unknown-unknown/release/tonic_dex.wasm
    WORKING_COPY=res/temp-dex.wasm
    FINAL_COPY=res/tonic-dex.wasm

    mkdir -p res
    cp ${ORIGINAL} ${WORKING_COPY}
    wasm-gc ${WORKING_COPY}
    wasm-strip ${WORKING_COPY}
    wasm-opt -Oz ${WORKING_COPY} --output ${FINAL_COPY}
    rm ${WORKING_COPY}
    echo ${ORIGINAL} `stat -c "%s" ${ORIGINAL}` "bytes ->" `stat -c "%s" ${FINAL_COPY}` "bytes, see ${FINAL_COPY}"

# Build contract with specific features
build-features *FEATURES:
    #!/bin/bash

    if [ -z "{{FEATURES}}" ]; then
        cargo build --release --target wasm32-unknown-unknown -p tonic-dex
    else
        cargo build --release --target wasm32-unknown-unknown -p tonic-dex --features {{FEATURES}}
    fi

# Build release
build:
    #!/bin/bash
    set -eu

    reset() {
        perl -i -pe 's/\["cdylib"\]/\["cdylib", "rlib"\]/' tonic-dex/Cargo.toml
    }

    hack_skip_rlib() {
        perl -i -pe 's/\["cdylib", "rlib"\]/\["cdylib"\]/' tonic-dex/Cargo.toml
    }

    trap reset EXIT;

    # Don't build rlib in production (saves about 1.4MB)
    hack_skip_rlib
    just build-features

# Build with debug logging enabled
build-debug:
	RUSTFLAGS="-C debug-assertions" cargo build --release --target wasm32-unknown-unknown -p tonic-dex --features debug_log

build-test-token:
    cargo build --release --target wasm32-unknown-unknown -p test-token

# Remove res and target
clean:
    rm -fr res target

# Build docker container for building the contract
build-contract-builder:
    docker buildx build -t tonic-foundation/contract-builder build

# Build contract in docker container
build-reproducible: build-contract-builder
    #!/bin/sh

    HOST_DIR="${HOST_DIR:-$(pwd)}"

    docker run \
        --mount type=bind,source=$HOST_DIR,target=/host \
        --cap-add=SYS_PTRACE --security-opt seccomp=unconfined \
        -i -t tonic-foundation/contract-builder \
        bash -c "cd /host; just build-minified"

# Run clippy
lint:
    cargo clippy -p tonic-dex

# Build docs and open in the default browser
view-docs:
    cargo rustdoc -p tonic-dex --open

# Measure sizes for hard-coded storage constants
measure-storage-usage:
    just test-no-emit storage

# Test DEX
test *TESTS: build-test-token build
    cargo test -p tonic-dex {{TESTS}} --features debug_log -- --skip test_stress

# Test DEX, skip workspace tests
test-no-sim *TESTS:
    cargo test -p tonic-dex {{TESTS}} --features debug_log -- --skip test_dex --skip test_stress

# Test and show all logs
test-with-logs *TESTS: build-test-token build
    cargo test -p tonic-dex {{TESTS}} --features debug_log -- --nocapture --skip test_stress

# Test and show logs with events disabled
test-no-emit *TESTS: build-test-token build
    cargo test -p tonic-dex {{TESTS}} --features debug_log,no_emit -- --nocapture --skip test_stress

deploy-file FILE:
    near deploy --wasmFile {{FILE}} --accountId $TONIC_CONTRACT_ID

deploy-debug:
    just deploy-file target/wasm32-unknown-unknown/release/tonic_dex.wasm 

deploy:
    just deploy-file res/tonic-dex.wasm

# Initialize the DEX and create the default test market
init-dex:
    #!/bin/bash
    source scripts/prelude

    require_env TONIC_CONTRACT_ID
    require_env NEAR_ACCOUNT_ID
    require_command near
    
    quote_token_id=${DEV_QUOTE_TOKEN_ID:-usdc.faucet.orderbook.testnet}
    quote_token_lot_size=${DEV_QUOTE_TOKEN_LOT_SIZE:-"1000"}
    base_token_id=${DEV_BASE_TOKEN_ID:-wbtc.faucet.orderbook.testnet}
    base_token_lot_size=${DEV_BASE_TOKEN_LOT_SIZE:-"100000"}

    info Initializing DEX
    scripts/init-dex

    info Registering DEX with tokens
    scripts/token-storage-deposit $NEAR_ACCOUNT_ID $quote_token_id $TONIC_CONTRACT_ID
    scripts/token-storage-deposit $NEAR_ACCOUNT_ID $base_token_id $TONIC_CONTRACT_ID

    info Registering $NEAR_ACCOUNT_ID with DEX
    near call $TONIC_CONTRACT_ID storage_deposit --deposit 0.1 --accountId $NEAR_ACCOUNT_ID

    info Creating default market with dev tokens
    scripts/create-ft-market $TONIC_CONTRACT_ID \
        $base_token_id \
        $base_token_lot_size \
        $quote_token_id \
        $quote_token_lot_size

# Mint test tokens
mint:
    #!/bin/bash
    source scripts/prelude

    require_env NEAR_ACCOUNT_ID
    require_command near

    quote_token_id=${DEV_QUOTE_TOKEN_ID:-usdc.faucet.orderbook.testnet}
    quote_mint_amount=${DEV_QUOTE_TOKEN_MINT_AMOUNT:-10000000000}

    base_token_id=${DEV_BASE_TOKEN_ID:-wbtc.faucet.orderbook.testnet}
    base_mint_amount=${DEV_BASE_TOKEN_MINT_AMOUNT:-1000000000}

    info Minting quote tokens
    scripts/mint $NEAR_ACCOUNT_ID $quote_token_id $quote_mint_amount

    info Minting base tokens
    scripts/mint $NEAR_ACCOUNT_ID $base_token_id $base_mint_amount

    success Done. Use 'just deposit' to deposit into the exchange.

# Deposit test tokens into the DEX
deposit:
    #!/bin/bash
    source scripts/prelude

    require_env NEAR_ACCOUNT_ID
    require_env TONIC_CONTRACT_ID
    require_command near

    quote_token_id=${DEV_QUOTE_TOKEN_ID:-usdc.faucet.orderbook.testnet}
    quote_mint_amount=${DEV_QUOTE_TOKEN_MINT_AMOUNT:-10000000000}

    base_token_id=${DEV_BASE_TOKEN_ID:-wbtc.faucet.orderbook.testnet}
    base_mint_amount=${DEV_BASE_TOKEN_MINT_AMOUNT:-1000000000}

    info Depositing quote tokens
    scripts/deposit $NEAR_ACCOUNT_ID $quote_token_id $TONIC_CONTRACT_ID $quote_mint_amount

    info Depositing base tokens
    scripts/deposit $NEAR_ACCOUNT_ID $base_token_id $TONIC_CONTRACT_ID $base_mint_amount

# Delete and recreate the testnet contract account
reset-testnet-contract:
    #!/bin/bash
    source scripts/prelude

    require_env NEAR_ACCOUNT_ID
    require_env TONIC_CONTRACT_ID
    require_command near

    warn 'Delete and recreate contract account '$TONIC_CONTRACT_ID'? [ENTER] to continue...'
    read

    warn Deleting $TONIC_CONTRACT_ID
    near delete $TONIC_CONTRACT_ID $NEAR_ACCOUNT_ID

    warn Recreating $TONIC_CONTRACT_ID
    near create-account $TONIC_CONTRACT_ID --masterAccount $NEAR_ACCOUNT_ID


init-localnet: build build-test-token
    #!/bin/bash
    source scripts/prelude
    require_command docker
    require_command near
    require_command nearup
    require_env NEAR_ACCOUNT_ID # must end with ".node0" or account creation will fail

    # sensible defaults since these won't change
    export NEAR_ENV=localnet
    export DEV_QUOTE_TOKEN_ID="quote_token.node0"
    export DEV_BASE_TOKEN_ID="base_token.node0"
    export TONIC_CONTRACT_ID="tonic_dex.node0"
    export LOCALNET_KEY_PATH=~/.near/localnet/node0/validator_key.json

    # Launch docker image containing localnet
    docker run -d -v $HOME/.near:/root/.near -p 3030:3030 --name tonic-localnet nearprotocol/nearup run localnet

    # Set up user, token, and dex accounts
    near create_account $NEAR_ACCOUNT_ID --masterAccount node0 --initialBalance 1000 --keyPath $LOCALNET_KEY_PATH
    near create_account $DEV_QUOTE_TOKEN_ID --masterAccount node0 --initialBalance 1000 --keyPath $LOCALNET_KEY_PATH
    near create_account $DEV_BASE_TOKEN_ID --masterAccount node0 --initialBalance 1000 --keyPath $LOCALNET_KEY_PATH
    near create_account $TONIC_CONTRACT_ID --masterAccount node0 --initialBalance 1000 --keyPath $LOCALNET_KEY_PATH

    # Deploy token and Tonic DEX contracts
    info Deploying contracts
    near deploy --wasmFile target/wasm32-unknown-unknown/release/test_token.wasm --accountId $DEV_QUOTE_TOKEN_ID --keyPath $LOCALNET_KEY_PATH
    near deploy --wasmFile target/wasm32-unknown-unknown/release/test_token.wasm --accountId $DEV_BASE_TOKEN_ID --keyPath $LOCALNET_KEY_PATH
    near deploy --wasmFile target/wasm32-unknown-unknown/release/tonic_dex.wasm --accountId $TONIC_CONTRACT_ID --keyPath $LOCALNET_KEY_PATH

    # Instantiate the contracts
    info Initializing Tokens
    scripts/init-tokens
    just init-dex

    # Register accounts
    info Registering accounts
    scripts/register-accounts-on-dex
    scripts/register-self-with-tokens

    # Mint and deposit some tokens
    just mint
    just deposit


clean-localnet:
    #!/bin/bash
    source scripts/prelude

    info Shutting down localnet container
    docker kill tonic-localnet
    docker rm tonic-localnet

    info Cleaning localnet accounts
    rm -rf ~/.near/localnet/

    info Done cleanup
