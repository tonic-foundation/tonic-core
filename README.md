## Related

Building with Tonic? Check out these examples.

- [Tonic trading bot example][trading-bot-example]
- [Tonic UI example][ui-example]
- [Tonic indexer example][indexer-example]
- [Tonic charting service example][data-api-example]

[trading-bot-example]: https://github.com/tonic-foundation/tonic-market-maker-example
[ui-example]: https://github.com/tonic-foundation/tonic-ui-example
[indexer-example]: https://github.com/tonic-foundation/tonic-indexer-example
[data-api-example]: https://github.com/tonic-foundation/data-api-example

## Getting started

- Get [Just](https://github.com/casey/just)
- Get [direnv](https://direnv.net/)
- Get the [Tonic CLI](https://docs.tonic.foundation/developers/api-reference#cli)

<details>
<summary>
Just commands
</summary>

```
just --list
Available recipes:
    build                    # Build release
    build-contract-builder   # Build docker container for building the contract
    build-debug              # Build with debug logging enabled
    build-features *FEATURES # Build contract with specific features
    build-minified           # Boneless version of official minify.sh example
    build-reproducible       # Build contract in docker container
    build-test-token
    clean                    # Remove res and target
    clean-localnet
    default                  # show help
    deploy
    deploy-debug
    deploy-file FILE
    deposit                  # Deposit test tokens into the DEX
    init-dex                 # Initialize the DEX and create the default test market
    init-localnet
    lint                     # Run clippy
    measure-storage-usage    # Measure sizes for hard-coded storage constants
    mint                     # Mint test tokens
    reset-testnet-contract   # Delete and recreate the testnet contract account
    test *TESTS              # Test DEX
    test-no-emit *TESTS      # Test and show logs with events disabled
    test-no-sim *TESTS       # Test DEX, skip workspace tests
    test-with-logs *TESTS    # Test and show all logs
    view-docs                # Build docs and open in the default browser
```

</details>

## Run tests

```
just test
```

## Build for deployment

```
just build-reproducible
```

This builds and minifies the contract in a Docker container. For more
fine-grained control over build output, check out the features in
[`tonic-dex/Cargo.toml`](./tonic-dex/Cargo.toml).

## Deploy to testnet

- Fill out the following environment variables to use Just commands. If you don't have an account to deploy to, you can create one from an existing account with `near create-account --master-account $your_account_id orderbook.$your_account_id`

```bash
# Account to deploy the contract to
export TONIC_CONTRACT_ID=
# A user to test with
export NEAR_ACCOUNT_ID=
```

<details>

<summary>
Optional config (defaults shown)
</summary>

```bash
# Default dev tokens
export DEV_QUOTE_TOKEN_ID=usdc.faucet.orderbook.testnet
export DEV_BASE_TOKEN_ID=wbtc.faucet.orderbook.testnet

# Default mint/deposit amounts
# 10000 USDC (6 decimals)
export DEV_QUOTE_TOKEN_MINT_AMOUNT=10000000000
# 10 WBTC (8 decimals)
export DEV_BASE_TOKEN_MINT_AMOUNT=1000000000

# Default lot sizes for market (created in init-dex)
# 0.01 USDC
export DEV_QUOTE_TOKEN_LOT_SIZE=1000
# 0.001 WBTC
export DEV_BASE_TOKEN_LOT_SIZE=100000
```

</details>

Build, deploy, and create a default market

```
just build-reproducible deploy-file res/tonic-dex.wasm init-dex
```

Deposit exchange balances for the test account

```
just mint deposit
```

Place an order

```
export MARKET_ID=<your market ID from above>
tonic place-order $MARKET_ID --buy --price 1.23 --quantity 1
```

## Test tokens

Test tokens are deployed to the following accounts

```
usdc.faucet.orderbook.testnet.testnet (6 decimals) (used in just commands by default)
wbtc.faucet.orderbook.testnet.testnet (8 decimals) (used in just commands by default)
```

Build and deploy your own

```bash
export contract_id=contract.example.testnet
just build-test-token
near deploy --wasmFile target/wasm32-unknown-unknown/release/test_token.wasm --accountId $contract_id
```

The test token's metadata is set at initialization, eg

```bash
export contract_id=usdc.example.testnet
near call $contract_id new \
    '{"decimals": 6, "name": "USDC Coin (Tonic)", "symbol": "USDC", "icon": "data:image/svg+xml,%3Csvg width=\'32\' height=\'32\' viewBox=\'0 0 32 32\' xmlns=\'http://www.w3.org/2000/svg\'%3E%3Cg fill=\'none\'%3E%3Ccircle cx=\'16\' cy=\'16\' r=\'16\' fill=\'%232775C9\'/%3E%3Cpath d=\'M15.75 27.5C9.26 27.5 4 22.24 4 15.75S9.26 4 15.75 4 27.5 9.26 27.5 15.75A11.75 11.75 0 0115.75 27.5zm-.7-16.11a2.58 2.58 0 00-2.45 2.47c0 1.21.74 2 2.31 2.33l1.1.26c1.07.25 1.51.61 1.51 1.22s-.77 1.21-1.77 1.21a1.9 1.9 0 01-1.8-.91.68.68 0 00-.61-.39h-.59a.35.35 0 00-.28.41 2.73 2.73 0 002.61 2.08v.84a.705.705 0 001.41 0v-.85a2.62 2.62 0 002.59-2.58c0-1.27-.73-2-2.46-2.37l-1-.22c-1-.25-1.47-.58-1.47-1.14 0-.56.6-1.18 1.6-1.18a1.64 1.64 0 011.59.81.8.8 0 00.72.46h.47a.42.42 0 00.31-.5 2.65 2.65 0 00-2.38-2v-.69a.705.705 0 00-1.41 0v.74zm-8.11 4.36a8.79 8.79 0 006 8.33h.14a.45.45 0 00.45-.45v-.21a.94.94 0 00-.58-.87 7.36 7.36 0 010-13.65.93.93 0 00.58-.86v-.23a.42.42 0 00-.56-.4 8.79 8.79 0 00-6.03 8.34zm17.62 0a8.79 8.79 0 00-6-8.32h-.15a.47.47 0 00-.47.47v.15a1 1 0 00.61.9 7.36 7.36 0 010 13.64 1 1 0 00-.6.89v.17a.47.47 0 00.62.44 8.79 8.79 0 005.99-8.34z\' fill=\'%23FFF\'/%3E%3C/g%3E%3C/svg%3E"}' \
    --accountId $contract_id
```

There's no need to do storage deposit with the test token

```bash
export contract_id=usdc.example.testnet
export receiver_id=other.example.testnet

near call $contract_id ft_mint '{"receiver_id": "'receiver_id'", "amount": "1000000000"} --accountId $receiver_id
```

## Developer docs

Please see https://docs.tonic.foundation

## Needs help
- builder image isn't pinned
- scripts don't know about owner account
