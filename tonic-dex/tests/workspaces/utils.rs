use anyhow::anyhow;
use near_primitives::views::FinalExecutionStatus;
use near_sdk::json_types::U128;
use near_sdk::serde::Serialize;
use near_sdk::{ONE_NEAR, ONE_YOCTO};
use serde_json::json;
use workspaces::prelude::*;
use workspaces::{Account, Contract, DevNetwork, Worker};

pub const BASE_TOKEN_LOT_SIZE: u128 = 1000000000;
pub const QUOTE_TOKEN_LOT_SIZE: u128 = 100000;

pub const ONE_BASE: u128 = (10 as u128).pow(16);
pub const ONE_QUOTE: u128 = (10 as u128).pow(18);

pub const ONE_TGAS: u64 = 1_000_000_000_000;

/*
Set up DEX, 2 tokens, and 2 users. Mint and deposit some tokens for each user.

return UserA, UserB, QuoteToken, BaseToken, Dex,
*/
pub async fn init_dex_and_deposit(
    worker: &Worker<impl DevNetwork>,
) -> anyhow::Result<(Account, Account, Contract, Contract, Contract)> {

    let (alice, bob, dex) = init_dex(worker).await?;

    let (qt_contract, bt_contract) = deploy_test_tokens(&worker).await?;
    token_storage_deposit(&worker, &alice, &dex.as_account(), &bt_contract).await?;
    token_storage_deposit(&worker, &alice, &dex.as_account(), &qt_contract).await?;
    dex_storage_deposit(&worker, &alice, &dex).await?;
    dex_storage_deposit(&worker, &bob, &dex).await?;

    mint_tokens(&worker, &alice, &qt_contract, ONE_QUOTE * 100).await?;
    mint_tokens(&worker, &bob, &bt_contract, ONE_BASE).await?;
    deposit_tokens(&worker, &alice, &qt_contract, &dex, ONE_QUOTE * 10).await?;
    deposit_tokens(&worker, &bob, &bt_contract, &dex, ONE_BASE / 4).await?;

    Ok((alice, bob, dex, qt_contract, bt_contract))
}

pub async fn init_dex(
    worker: &Worker<impl DevNetwork>,
) -> anyhow::Result<(Account, Account, Contract)> {
    let dex = worker
        .dev_deploy(
            include_bytes!("../../../target/wasm32-unknown-unknown/release/tonic_dex.wasm").to_vec(),
        )
        .await?;

    let alice: Account = worker.dev_create_account().await?;
    let bob: Account = worker.dev_create_account().await?;

    // Instantiate the DEX contract.
    let res = alice
        .call(&worker, dex.id().clone(), "new")
        .args_json(json!({
            "owner_id": alice.id(),
        }))?
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));

    Ok((alice, bob, dex))
}

pub async fn token_storage_deposit(
    worker: &Worker<impl DevNetwork>,
    caller: &Account,
    dex_account: &Account,
    token_contract: &Contract,
) -> anyhow::Result<bool> {
    let res = caller
        .call(&worker, token_contract.id().clone(), "storage_deposit")
        .args_json(json!({
            "account_id": dex_account.id(),
            "registration_only": true,
        }))?
        .deposit(ONE_NEAR)
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));
    Ok(true)
}

pub async fn dex_storage_deposit(
    worker: &Worker<impl DevNetwork>,
    caller: &Account,
    dex_contract: &Contract,
) -> anyhow::Result<bool> {
    let res = caller
        .call(&worker, dex_contract.id().clone(), "storage_deposit")
        .deposit(ONE_NEAR)
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));
    Ok(true)
}

pub async fn deploy_test_token<U: Serialize>(
    worker: &Worker<impl DevNetwork>,
    json: U,
) -> anyhow::Result<Contract> {
    let token_contract = worker
        .dev_deploy(
            include_bytes!("../../../target/wasm32-unknown-unknown/release/test_token.wasm")
                .to_vec(),
        )
        .await?;
    let res = token_contract
        .call(&worker, "new")
        .args_json(json)?
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));
    Ok(token_contract)
}

pub async fn deploy_test_tokens(
    worker: &Worker<impl DevNetwork>,
) -> anyhow::Result<(Contract, Contract)> {
    let quote_token_contract = deploy_test_token(worker,
        json!({
            "decimals": 18,
            "name": "USD Coin (Tonic)",
            "symbol": "USDC",
            "allow_external_transfer": true,
        })
    ).await?;
    let base_token_contract = deploy_test_token(worker,
        json!({
            "decimals": 16,
            "name": "Wrapped Bitcoin (Tonic)",
            "symbol": "WBTC",
            "allow_external_transfer": true,
        })).await?;
    Ok((quote_token_contract, base_token_contract))
}

pub async fn mint_tokens(
    worker: &Worker<impl DevNetwork>,
    receiver: &Account,
    token_contract: &Contract,
    amount: u128,
) -> anyhow::Result<bool> {
    let res = receiver
        .call(&worker, token_contract.id().clone(), "ft_mint")
        .args_json(json!({
            "receiver_id": receiver.id(),
            "amount": U128(amount),
        }))?
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));
    Ok(true)
}

pub async fn deposit_tokens(
    worker: &Worker<impl DevNetwork>,
    caller: &Account,
    token_contract: &Contract,
    dex_contract: &Contract,
    amount: u128,
) -> anyhow::Result<bool> {
    let res = caller
        .call(&worker, token_contract.id().clone(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": dex_contract.id(),
            "amount": U128(amount),
            "msg": "",
        }))?
        .deposit(ONE_YOCTO)
        .gas(300000000000000)
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));
    Ok(true)
}

pub async fn deposit_native_near(
    worker: &Worker<impl DevNetwork>,
    caller: &Account,
    dex_contract: &Contract,
    amount: u128,
) -> anyhow::Result<bool> {
    let res = caller
        .call(&worker, dex_contract.id().clone(), "deposit_near")
        .deposit(amount)
        .gas(300000000000000)
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));
    Ok(true)
}

pub async fn create_ft_market(
    worker: &Worker<impl DevNetwork>,
    caller: &Account,
    dex: &Contract,
    base_token_contract: &Contract,
    quote_token_contract: &Contract,
    taker_fee_base_rate: u8,
    maker_rebate_base_rate: u8,
) -> anyhow::Result<String> {
    let res = caller
        .call(&worker, dex.id().clone(), "create_market")
        .args_json(json!({"args":{
            "base_token": format!("ft:{}", base_token_contract.id()),
            "base_token_lot_size": U128(BASE_TOKEN_LOT_SIZE),
            "quote_token": format!("ft:{}", quote_token_contract.id()),
            "quote_token_lot_size": U128(QUOTE_TOKEN_LOT_SIZE),
            "taker_fee_base_rate": taker_fee_base_rate,
            "maker_rebate_base_rate": maker_rebate_base_rate}
        }))?
        .deposit(ONE_NEAR)
        .gas(300000000000000)
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));

    return if let FinalExecutionStatus::SuccessValue(market_id_encoded) = res.status {
        let market_id_decoded = base64::decode(market_id_encoded).unwrap();
        let market_id_json = String::from_utf8(market_id_decoded).unwrap();
        let market_id: String = serde_json::from_str(&market_id_json).unwrap();
        activate_market(worker, caller, dex, &market_id).await?;
        Ok(market_id)
    } else {
        Err(anyhow!("Failed to create market"))
    };
}

pub async fn create_native_near_market(
    worker: &Worker<impl DevNetwork>,
    caller: &Account,
    dex: &Contract,
    quote_token_contract: &Contract,
) -> anyhow::Result<String> {
    let res = caller
        .call(&worker, dex.id().clone(), "create_market")
        .args_json(json!({"args":{
            "base_token": "NEAR",
            "base_token_lot_size": U128(BASE_TOKEN_LOT_SIZE),
            "quote_token": format!("ft:{}", quote_token_contract.id()),
            "quote_token_lot_size": U128(QUOTE_TOKEN_LOT_SIZE),
            "taker_fee_base_rate": 0u8,
            "maker_rebate_base_rate": 0u8}
        }))?
        .deposit(ONE_NEAR)
        .gas(300000000000000)
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));

    return if let FinalExecutionStatus::SuccessValue(market_id_encoded) = res.status {
        let market_id_decoded = base64::decode(market_id_encoded).unwrap();
        let market_id_json = String::from_utf8(market_id_decoded).unwrap();
        let market_id: String = serde_json::from_str(&market_id_json).unwrap();
        activate_market(worker, caller, dex, &market_id).await?;
        Ok(market_id)
    } else {
        Err(anyhow!("Failed to create market"))
    };
}


pub async fn activate_market(
    worker: &Worker<impl DevNetwork>,
    caller: &Account,
    dex: &Contract,
    market_id: &String,
) -> anyhow::Result<()> {
    let res = caller
        .call(&worker, dex.id().clone(), "set_market_state")
        .args_json(json!({
            "market_id": market_id.clone(),
            "new_state": "Active",
        }))?
        .gas(300000000000000)
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));
    Ok(())
}