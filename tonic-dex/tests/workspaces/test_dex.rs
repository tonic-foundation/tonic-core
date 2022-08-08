use crate::utils::*;
use crate::utils::{ONE_BASE, ONE_QUOTE};
use near_primitives::views::FinalExecutionStatus;
use near_sdk::json_types::U128;
use near_sdk::{ONE_YOCTO, ONE_NEAR};
use serde_json::json;
use tonic_dex::MarketView;
use workspaces::prelude::*;
use workspaces::Account;

#[tokio::test]
async fn test_dex_contract() -> anyhow::Result<()> {
    let worker = workspaces::sandbox();
    let contract = worker
        .dev_deploy(
            include_bytes!("../../../target/wasm32-unknown-unknown/release/tonic_dex.wasm").to_vec(),
        )
        .await?;

    let alice: Account = worker.dev_create_account().await?;

    let res = contract
        .call(&worker, "new")
        .args_json(json!({
            "owner_id": alice.id(),
        }))?
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));

    println!(
        "new() outcome: total_gas_burnt={}TGas",
        res.total_gas_burnt / ONE_TGAS
    );

    Ok(())
}

#[tokio::test]
async fn test_single_maker_order_gas() -> anyhow::Result<()> {
    let worker = workspaces::sandbox();

    let (alice, _bob, dex, quote_token, base_token) = init_dex_and_deposit(&worker).await?;
    let market_id: String =
        create_ft_market(&worker, &alice, &dex, &base_token, &quote_token, 0, 0).await?;

    let res = alice
        .call(&worker, dex.id().clone(), "new_order")
        .args_json(json!({
            "market_id": market_id.clone(),
            "order": {
                "limit_price": U128(5 * ONE_QUOTE as u128),
                "quantity": U128((ONE_BASE / 5) as u128),
                "side": "Buy",
                "order_type": "Limit",
            }
        }))?
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));
    println!(
        "new_order() outcome: total_gas_burnt={}TGas ({})",
        res.total_gas_burnt / ONE_TGAS,
        res.total_gas_burnt
    );

    Ok(())
}

#[tokio::test]
async fn test_swap_ft_buy() -> anyhow::Result<()> {
    let worker = workspaces::sandbox();
    let (alice, bob, dex, quote_token, base_token) = init_dex_and_deposit(&worker).await?;
    let market_id = create_ft_market(&worker, &alice, &dex, &base_token, &quote_token, 20u8, 2u8).await?;

    // Alice starts with 90 QUOTE on the token contract, and 10 QUOTE on the DEX.
    let balance = alice
        .call(&worker, quote_token.id().clone(), "ft_balance_of")
        .args_json(json!({
            "account_id": alice.id(),
        }))?
        .gas(300000000000000)
        .transact()
        .await?
        .json::<U128>()?;
    assert_eq!(balance.0, 90 * ONE_QUOTE, "Wrong initial balance: {}, expected: {}", balance.0, 90 * ONE_QUOTE);
    
    // Bob opens an order, selling 1/5 BASE @ 10 QUOTE
    let res = bob
        .call(&worker, dex.id().clone(), "new_order")
        .args_json(json!({
            "market_id": market_id.clone(),
            "order": {
                "limit_price": U128(10 * ONE_QUOTE as u128),
                "quantity": U128((ONE_BASE / 5) as u128),
                "side": "Sell",
                "order_type": "Limit",
            }
        }))?
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));

    // Alice attempts to swaps 1 QUOTE for 0.2 BASE.
    // The swap fails due to lack of matching order for that quantity.
    let msg = format!(r#"{{"action": "Swap", "params": [{{"market_id": "{}", "side": "Buy", "min_output_token": "{}" }}] }}"#,
        market_id.clone(), 2 * ONE_BASE / 10);
    let amount_accepted = alice
        .call(&worker, quote_token.id().clone(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": dex.id(),
            "amount": U128(ONE_QUOTE as u128),
            "msg": msg,
        }))?
        .deposit(ONE_YOCTO)
        .gas(300000000000000)
        .transact()
        .await?
        .json::<U128>()?;
    assert_eq!(amount_accepted.0, 0);

    let quote_vol = 2 * ONE_QUOTE;
    let amount_sent = quote_vol * 10_000u128 / (10_000u128 - 20_u128); // works out to amount_sent*0.002 = 2 QUOTE
    let msg = format!(r#"{{"action": "Swap", "params": [{{"market_id": "{}", "side": "Buy", "min_output_token": "{}" }}] }}"#,
        market_id, ONE_BASE / 10);
    let amount_accepted = alice
        .call(&worker, quote_token.id().clone(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": dex.id(),
            "amount": U128(amount_sent as u128),
            "msg": msg,
        }))?
        .deposit(ONE_YOCTO)
        .gas(300000000000000)
        .transact()
        .await?
        .json::<U128>()?;
    assert_eq!(amount_accepted.0, amount_sent);

    // Alice now has 0.2 BASE and 90-3 QUOTE
    let balance = alice
        .call(&worker, quote_token.id().clone(), "ft_balance_of")
        .args_json(json!({
            "account_id": alice.id(),
        }))?
        .gas(300000000000000)
        .transact()
        .await?
        .json::<U128>()?;
    assert_eq!(balance.0, 90 * ONE_QUOTE - amount_sent, "Wrong balance: {}, expected: {}", balance.0, 90 * ONE_QUOTE - amount_sent);

    let balance: U128 = alice
        .call(&worker, dex.id().clone(), "get_balance")
        .args_json(json!({
            "token_id": base_token.id(),
            "account_id": alice.id(),
        }))?
        .view()
        .await?
        .json::<U128>()?;
    assert_eq!(balance.0, ONE_BASE / 5);

    // Maker has received their full 2 quote in the trade, plus maker rebate
    let balance: U128 = bob
        .call(&worker, dex.id().clone(), "get_balance")
        .args_json(json!({
            "token_id": quote_token.id(),
            "account_id": bob.id(),
        }))?
        .view()
        .await?
        .json::<U128>()?;
    let expected_maker_rebate = 2u128 * quote_vol / 10_000;
    assert_eq!(balance.0, quote_vol + expected_maker_rebate);

    let market = bob
        .call(&worker, dex.id().clone(), "get_market")
        .args_json(json!({
            "market_id": market_id.clone(),
        }))?
        .view()
        .await?
        .json::<Option<MarketView>>()?;
    let expected_taker_fee = (20u128 * quote_vol / 10_000) - expected_maker_rebate;
    assert_eq!(market.unwrap().fees_accrued.0, expected_taker_fee);
    Ok(())
}

#[tokio::test]
async fn test_swap_ft_sell() -> anyhow::Result<()> {
    let worker = workspaces::sandbox();
    let (alice, bob, dex, quote_token, base_token) = init_dex_and_deposit(&worker).await?;
    let market_id = create_ft_market(&worker, &alice, &dex, &base_token, &quote_token, 20u8, 2u8).await?;

    // Alice starts with 90 QUOTE on the token contract, and 10 QUOTE on the DEX.
    let balance = alice
        .call(&worker, quote_token.id().clone(), "ft_balance_of")
        .args_json(json!({
            "account_id": alice.id(),
        }))?
        .gas(300000000000000)
        .transact()
        .await?
        .json::<U128>()?;
    assert_eq!(balance.0, 90 * ONE_QUOTE, "Wrong initial balance: {}, expected: {}", balance.0, 90 * ONE_QUOTE);


    let quote_vol = 2 * ONE_QUOTE;
    let base_vol = ONE_BASE / 5;
    let expected_taker_fee = 20u128 * quote_vol / 10_000u128;
    let expected_maker_rebate = 2u128 * quote_vol / 10_000u128;
    let raw_limit_price = 10 * ONE_QUOTE;
    let implicit_max_spend = raw_limit_price * base_vol;
    let adjusted_max_spend =
        implicit_max_spend + (implicit_max_spend * 20u128 / ONE_QUOTE);

    // Alice opens an order, buying 1/5 BASE @ 10 QUOTE
    let res = alice
        .call(&worker, dex.id().clone(), "new_order")
        .args_json(json!({
            "market_id": market_id.clone(),
            "order": {
                "limit_price": U128(raw_limit_price),
                "quantity": U128(base_vol),
                "side": "Buy",
                "order_type": "Limit",
                "max_spend": U128(adjusted_max_spend),
            }
        }))?
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));

    // Bob attempts to swap 0.1 BASE for 1 QUOTE.
    // The swap fails due to lack of matching order for that quantity.
    let msg = format!(r#"{{"action": "Swap", "params": [{{"market_id": "{}", "side": "Sell", "min_output_token": "{}" }}] }}"#,
        market_id.clone(), ONE_QUOTE);
    let amount_accepted = bob
        .call(&worker, base_token.id().clone(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": dex.id(),
            "amount": U128(ONE_BASE / 10 as u128),
            "msg": msg,
        }))?
        .deposit(ONE_YOCTO)
        .gas(300000000000000)
        .transact()
        .await?
        .json::<U128>()?;
    assert_eq!(amount_accepted.0, 0);

    // Bob successfully swaps 0.2 BASE for 2 QUOTE.
    let msg = format!(r#"{{"action": "Swap", "params": [{{"market_id": "{}", "side": "Sell", "min_output_token": "{}" }}] }}"#,
        market_id, quote_vol - expected_taker_fee);
    let amount_accepted = bob
        .call(&worker, base_token.id().clone(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": dex.id(),
            "amount": U128(base_vol as u128),
            "msg": msg,
        }))?
        .deposit(ONE_YOCTO)
        .gas(300000000000000)
        .transact()
        .await?
        .json::<U128>()?;
    assert_eq!(amount_accepted.0, base_vol);

    // Bob has 0.2 BASE deducted from his original amount, and has received 2 QUOTE - fee.
    let balance = bob
        .call(&worker, base_token.id().clone(), "ft_balance_of")
        .args_json(json!({
            "account_id": bob.id(),
        }))?
        .gas(300000000000000)
        .transact()
        .await?
        .json::<U128>()?;
    assert_eq!(balance.0,  (3 * ONE_BASE / 4) - base_vol, "Wrong balance: {}, expected: {}", balance.0, 11 * ONE_BASE / 20);

    let balance: U128 = bob
        .call(&worker, dex.id().clone(), "get_balance")
        .args_json(json!({
            "token_id": quote_token.id(),
            "account_id": bob.id(),
        }))?
        .view()
        .await?
        .json::<U128>()?;
    assert_eq!(balance.0, quote_vol - expected_taker_fee);

    // Alice has received their 0.2 base in the trade
    let balance: U128 = alice
        .call(&worker, dex.id().clone(), "get_balance")
        .args_json(json!({
            "token_id": base_token.id(),
            "account_id": alice.id(),
        }))?
        .view()
        .await?
        .json::<U128>()?;
    assert_eq!(balance.0, ONE_BASE / 5);

    let market = bob
        .call(&worker, dex.id().clone(), "get_market")
        .args_json(json!({
            "market_id": market_id.clone(),
        }))?
        .view()
        .await?
        .json::<Option<MarketView>>()?;
    assert_eq!(market.unwrap().fees_accrued.0, expected_taker_fee - expected_maker_rebate);

    Ok(())
}



#[tokio::test]
async fn test_swap_near() -> anyhow::Result<()> {
    let worker = workspaces::sandbox();
    let (alice, bob, dex, quote_token, _) = init_dex_and_deposit(&worker).await?;
    let market_id = create_native_near_market(&worker, &alice, &dex, &quote_token).await?;

    // Alice places order to purchase 4 NEAR @ 2 QUOTE
    let res = alice
        .call(&worker, dex.id().clone(), "new_order")
        .args_json(json!({
            "market_id": market_id.clone(),
            "order": {
                "limit_price": U128(2 * ONE_QUOTE as u128),
                "quantity": U128(4 * ONE_NEAR as u128),
                "side": "Buy",
                "order_type": "Limit",
            }
        }))?
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));

    // Bob swaps 3 NEAR, and receives 6 QUOTE
    let res = bob
        .call(&worker, dex.id().clone(), "swap_near")
        .args_json(json!({
            "swaps": [{
                "market_id": market_id.clone(),
                "side": "Sell",
                "min_output_token": U128(5),
            }]
        }))?
        .gas(300000000000000)
        .deposit(3 * ONE_NEAR)
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));

    // Alice now has 3 NEAR, and Bob has 6 USDC on his exchange balance
    let bob_dex_quote_balance: U128 = bob
        .call(&worker, dex.id().clone(), "get_balance")
        .args_json(json!({
            "account_id": bob.id(),
            "token_id": quote_token.id(),
        }))?
        .view()
        .await?
        .json::<U128>()?;
    assert_eq!(bob_dex_quote_balance.0, 6 * ONE_QUOTE);
    let bob_ft_quote_balance= bob
        .call(&worker, quote_token.id().clone(), "ft_balance_of")
        .args_json(json!({
            "account_id": bob.id(),
        }))?
        .gas(300000000000000)
        .transact()
        .await?
        .json::<U128>()?;
    assert_eq!(bob_ft_quote_balance.0, 0);

    let alice_balance: U128 = alice
        .call(&worker, dex.id().clone(), "get_near_balance")
        .args_json(json!({
            "account_id": alice.id(),
        }))?
        .view()
        .await?
        .json::<U128>()?;
    assert_eq!(alice_balance.0, 3*ONE_NEAR);

    Ok(())
}

#[tokio::test]
async fn test_multi_step_swap_ft() -> anyhow::Result<()> {
    let worker = workspaces::sandbox();
    let (alice, bob, dex) = init_dex(&worker).await?;
    let (usdc, btc) = deploy_test_tokens(&worker).await?;
    let usn = deploy_test_token(&worker,
        json!({
            "decimals": 16,
            "name": "USN Coin",
            "symbol": "USN",
            "allow_external_transfer": true,
        })
    ).await?;
    let ONE_USN = (10 as u128).pow(16);

    // Alice starts with 100 BTC, none deposited to DEX.
    // She is registered on the USN contract, though no balance.
    // Note that she does not need to be registered on the DEX.
    token_storage_deposit(&worker, &alice, &dex.as_account(), &btc).await?;
    token_storage_deposit(&worker, &alice, &dex.as_account(), &usn).await?;
    mint_tokens(&worker, &alice, &btc, ONE_BASE * 100).await?;
    // Following line is necessary to register alice on the usdc contract
    mint_tokens(&worker, &alice, &usn, 0).await?;

    // Bob starts with 1000 USDC and 200 USN.
    dex_storage_deposit(&worker, &bob, &dex).await?;
    token_storage_deposit(&worker, &bob, &dex.as_account(), &usdc).await?;
    token_storage_deposit(&worker, &bob, &dex.as_account(), &usn).await?;
    mint_tokens(&worker, &bob, &usdc, ONE_QUOTE * 1000).await?;
    mint_tokens(&worker, &bob, &usn, ONE_USN * 200).await?;
    deposit_tokens(&worker, &bob, &usdc, &dex, ONE_QUOTE * 1000).await?;
    deposit_tokens(&worker, &bob, &usn, &dex, ONE_USN * 200).await?;

    let btc_market_id = create_ft_market(&worker, &alice, &dex, &btc, &usdc, 0, 0).await?;
    let usn_market_id = create_ft_market(&worker, &alice, &dex, &usn, &usdc, 0, 0).await?;

    // 1. Bob opens an order, buying 5 BTC @ 100 USDC
    let res = bob
        .call(&worker, dex.id().clone(), "new_order")
        .args_json(json!({
            "market_id": btc_market_id.clone(),
            "order": {
                "limit_price": U128(100 * ONE_QUOTE as u128),
                "quantity": U128((5 * ONE_BASE) as u128),
                "side": "Buy",
                "order_type": "Limit",
            }
        }))?
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));

    // 2. Bob opens another order, selling 200 USN @ 2 USDC
    let res = bob
        .call(&worker, dex.id().clone(), "new_order")
        .args_json(json!({
            "market_id": usn_market_id.clone(),
            "order": {
                "limit_price": U128(2 * ONE_QUOTE as u128),
                "quantity": U128((200 * ONE_USN) as u128),
                "side": "Sell",
                "order_type": "Limit",
            }
        }))?
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));

    // 3. Alice swaps 1 BTC for 50 USN.
    // Internally, she first receives 100 USDC, which is then spent to receive 50 USN.
    // She sets slippage parameter so that she receives at least 49 USN.
    let msg = format!(r#"{{"action": "Swap", "params": [{{"market_id": "{}", "side": "Sell" }}, {{"market_id": "{}", "side": "Buy", "min_output_token": "{}" }}] }}"#,
        btc_market_id, usn_market_id, 49 * ONE_USN );
    let amount_accepted = alice
        .call(&worker, btc.id().clone(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": dex.id(),
            "amount": U128(ONE_BASE as u128),
            "msg": msg,
        }))?
        .deposit(ONE_YOCTO)
        .gas(300000000000000)
        .transact()
        .await?
        .json::<U128>()?;
    assert_eq!(amount_accepted.0, ONE_BASE);

    // 4. Alice now has 50 USN on the USN contract.
    let balance = alice
        .call(&worker, usn.id().clone(), "ft_balance_of")
        .args_json(json!({
            "account_id": alice.id(),
        }))?
        .gas(300000000000000)
        .transact()
        .await?
        .json::<U128>()?;
    assert_eq!(balance.0, 50 * ONE_USN, "Wrong balance: {}, expected: {}", balance.0, 50 * ONE_USN);

    // 5. Bob now has 1 BTC in the exchange.
    let balance: U128 = bob
        .call(&worker, dex.id().clone(), "get_balance")
        .args_json(json!({
            "token_id": btc.id(),
            "account_id": bob.id(),
        }))?
        .view()
        .await?
        .json::<U128>()?;
    assert_eq!(balance.0, ONE_BASE);

    Ok(())
}

#[tokio::test]
async fn test_withdraw_ft_reverts_on_failure() -> anyhow::Result<()> {
    let worker = workspaces::sandbox();

    let (alice, _bob, dex, quote_token, _) = init_dex_and_deposit(&worker).await?;

    let res = alice
        .call(&worker, quote_token.id().clone(), "ft_balance_of")
        .args_json(json!({
            "account_id": alice.id(),
        }))?
        .gas(300000000000000)
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));

    // Unregister DEX on the token contract. This is an unusual action that isn't supported on a real FT contract,
    // but is a simple way to force the ft_transfer XCC to fail, as the DEX won't have enough balance.
    let res = alice
        .call(&worker, quote_token.id().clone(), "unregister_account")
        .args_json(json!({
            "account_id": dex.id(),
        }))?
        .gas(300000000000000)
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));

    let res = alice
        .call(&worker, dex.id().clone(), "withdraw_ft")
        .args_json(json!({
            "token": quote_token.id(),
            "amount": U128(5),
        }))?
        .deposit(ONE_YOCTO)
        .gas(300000000000000)
        .transact()
        .await?;
    assert!(matches!(res.status, FinalExecutionStatus::SuccessValue(_)));

    // Alice's internal deposit balance should not have changed, as the failed withdrawal
    // was succesfully reverted.
    let balance: U128 = alice
        .call(&worker, dex.id().clone(), "get_balance")
        .args_json(json!({
            "token_id": quote_token.id(),
            "account_id": alice.id(),
        }))?
        .view()
        .await?
        .json::<U128>()?;
    assert_eq!(balance.0, ONE_QUOTE * 10);

    Ok(())
}
