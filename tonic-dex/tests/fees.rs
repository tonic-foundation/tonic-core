use near_sdk::{json_types::U128, AccountId};

use tonic_dex::*;

mod util;
use util::*;

const BASE_TOKEN_LOT_SIZE: u128 = 1000000000;
const QUOTE_TOKEN_LOT_SIZE: u128 = 100000;

#[test]
fn fees() {
    fn check_fees_for_order_type(taker_order_type: OrderType) {
        // Reset on-chain data to clear contract state.
        near_sdk::mock::with_mocked_blockchain(|b| b.take_storage());

        let mut contract = setup_contract();
        let one_base = 10_u128.pow(16);
        let one_quote = 10_u128.pow(18);
        let (maker, taker, wnear, usdc) = get_accounts();

        let maker_inital_quote_balance = one_quote * 10;
        let taker_inital_base_balance = one_base / 2;

        let raw_base_volume = one_base / 5;
        let raw_limit_price = 5 * one_quote;

        storage_deposit(&mut contract, &maker);
        storage_deposit(&mut contract, &taker);

        // 10 USD
        contract.internal_deposit(&maker, &usdc.clone().into(), maker_inital_quote_balance);
        // 0.5 NEAR
        contract.internal_deposit(&taker, &wnear.clone().into(), taker_inital_base_balance);

        set_deposit_context(maker.clone(), deposits::TENTH_NEAR);

        assert_balance_invariant(
            &contract,
            None,
            vec![
                (&wnear, taker_inital_base_balance),
                (&usdc, maker_inital_quote_balance),
            ],
        );

        // Normally, implicit_max_spend is given by implicit = price * quantity.
        // To account for fees, set max_spend parameter to be a slightly higher value using
        // the formula: adjusted = implicit + implicit*fee_rate .
        // If we don't pass this parameter, when the contract withholds a portion
        // of the quote token to account for fees, the orderbook will receive a smaller
        // order than the user intended.
        // In this case, the orderbook is empty so the extra amount will be returned to the maker.
        let taker_fee_base_rate = 20_u128;
        let maker_rebate_base_rate = 2_u128;
        let implicit_max_spend = raw_limit_price * raw_base_volume;
        let adjusted_max_spend =
            implicit_max_spend + (implicit_max_spend * taker_fee_base_rate / one_quote);

        // Buy 0.2 BASE @ 5 QUOTE
        // Sell 0.4 BASE @ 5 QUOTE (only 0.2 will be filled)
        // 1 QUOTE volume
        let market = create_market_and_place_orders(
            &mut contract,
            CreateMarketArgs {
                base_token: TokenType::from_account_id(wnear.clone()).key(),
                base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
                quote_token: TokenType::from_account_id(usdc.clone()).key(),
                quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
                taker_fee_base_rate: taker_fee_base_rate as u8,
                maker_rebate_base_rate: maker_rebate_base_rate as u8,
            },
            vec![
                (
                    maker.clone(),
                    new_order_params(
                        raw_limit_price,
                        Some(U128(adjusted_max_spend)),
                        raw_base_volume,
                        Side::Buy,
                        OrderType::Limit,
                        None,
                        None,
                    ),
                ),
                (
                    taker.clone(),
                    new_order_params(
                        raw_limit_price,
                        None,
                        raw_base_volume,
                        Side::Sell,
                        taker_order_type,
                        None,
                        None,
                    ),
                ),
            ],
        );

        let expected_fee_amount = taker_fee_base_rate * one_quote / 10000; // 20 bps
        let expected_rebate_amount = maker_rebate_base_rate * one_quote / 10000; // 2 bps
        assert_eq!(
            market.fees_accrued,
            expected_fee_amount - expected_rebate_amount,
            "wrong fees accrued"
        );

        // Taker sold 0.2 @ 5 (1 quote volume)
        // Fee of 20bps = 0.0002 quote
        // Net credit = 1 - 0.0002 quote
        // Expected balance = 1 - 0.0002
        let taker_quote_balance = get_balance(&contract, &taker, usdc.clone().into());
        assert_eq!(
            taker_quote_balance,
            one_quote - expected_fee_amount,
            "wrong taker fee debit"
        );
        let taker_base_balance = get_balance(&contract, &taker, wnear.clone().into());
        assert_eq!(
            taker_base_balance,
            taker_inital_base_balance - 1 * raw_base_volume,
            "wrong taker base balance"
        );

        // Maker bought 0.2 @ 5 (1 quote volume)
        // rebate of 0 = 0.0002 quote
        let maker_quote_balance = get_balance(&contract, &maker, usdc.clone().into());
        assert_eq!(
            maker_quote_balance,
            (10 - 1) * one_quote + expected_rebate_amount,
            "wrong maker rebate credit"
        );
        let maker_base_balance = get_balance(&contract, &maker, wnear.clone().into());
        assert_eq!(
            maker_base_balance, raw_base_volume,
            "wrong maker base balance"
        );
        cancel_all_orders(&mut contract, &market);
        assert_balance_invariant(
            &contract,
            Some(&market),
            vec![
                (&wnear, taker_inital_base_balance),
                (&usdc, maker_inital_quote_balance),
            ],
        );
    }

    check_fees_for_order_type(OrderType::Limit);
    check_fees_for_order_type(OrderType::Market);
}

// Similar test to `fees()`, but now the Taker side is performing a Buy.
// In this case, they adjust up their max spend and the added portion goes to the contract as fees.
#[test]
fn fees_taker_performs_buy() {
    for taker_order_type in vec![OrderType::Limit, OrderType::Market] {
        // Reset on-chain data to clear contract state.
        near_sdk::mock::with_mocked_blockchain(|b| b.take_storage());

        let mut contract = setup_contract();
        let one_base = 10_u128.pow(16);
        let one_quote = 10_u128.pow(18);
        let (maker, taker, wnear, usdc) = get_accounts();

        let maker_inital_base_balance = one_base / 4;
        let taker_inital_quote_balance = one_quote * 10;

        let raw_base_volume = one_base / 5;
        let raw_limit_price = 5 * one_quote;

        storage_deposit(&mut contract, &maker);
        storage_deposit(&mut contract, &taker);

        // 10 USD
        contract.internal_deposit(&taker, &usdc.clone().into(), taker_inital_quote_balance);
        // 0.25 NEAR
        contract.internal_deposit(&maker, &wnear.clone().into(), maker_inital_base_balance);

        set_deposit_context(maker.clone(), deposits::TENTH_NEAR);

        let taker_fee_base_rate = 20_u128;
        let maker_rebate_base_rate = 2_u128;
        let implicit_max_spend = raw_limit_price * raw_base_volume;
        let adjusted_max_spend =
            2 * implicit_max_spend + (implicit_max_spend * taker_fee_base_rate / one_quote);

        // Buy 0.2 BASE @ 5 QUOTE
        // Sell 0.2 BASE @ 5 QUOTE
        // 1 QUOTE volume
        let market = create_market_and_place_orders(
            &mut contract,
            CreateMarketArgs {
                base_token: TokenType::from_account_id(wnear.clone()).key(),
                base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
                quote_token: TokenType::from_account_id(usdc.clone()).key(),
                quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
                taker_fee_base_rate: taker_fee_base_rate as u8,
                maker_rebate_base_rate: maker_rebate_base_rate as u8,
            },
            vec![
                (
                    maker.clone(),
                    new_order_params(
                        raw_limit_price,
                        None,
                        raw_base_volume,
                        Side::Sell,
                        OrderType::Limit,
                        None,
                        None,
                    ),
                ),
                (
                    taker.clone(),
                    new_order_params(
                        raw_limit_price,
                        Some(U128(adjusted_max_spend)),
                        raw_base_volume,
                        Side::Buy,
                        taker_order_type,
                        None,
                        None,
                    ),
                ),
            ],
        );

        let expected_fee_amount = taker_fee_base_rate * one_quote / 10000; // 20 bps
        let expected_rebate_amount = maker_rebate_base_rate * one_quote / 10000; // 2 bps
        assert_eq!(
            market.fees_accrued,
            expected_fee_amount - expected_rebate_amount,
            "wrong fees accrued"
        );

        // Taker bought 0.2 @ 5 (1 quote volume)
        // Fee of 20bps = 0.0002 quote
        // Net credit =  -(1 + 0.0002) quote
        // Expected balance = 9 - 0.0002
        let taker_quote_balance = get_balance(&contract, &taker, usdc.clone().into());
        assert_eq!(
            taker_quote_balance,
            (10 - 1) * one_quote - expected_fee_amount,
            "wrong taker fee debit"
        );
        let taker_base_balance = get_balance(&contract, &taker, wnear.clone().into());
        assert_eq!(
            taker_base_balance, raw_base_volume,
            "wrong taker base balance"
        );

        // Maker sold 0.2 @ 5 (1 quote volume)
        // rebate of 0 = 0.0002 quote
        let maker_quote_balance = get_balance(&contract, &maker, usdc.clone().into());
        assert_eq!(
            maker_quote_balance,
            one_quote + expected_rebate_amount,
            "wrong maker rebate credit"
        );
        let maker_base_balance = get_balance(&contract, &maker, wnear.clone().into());
        assert_eq!(
            maker_base_balance,
            maker_inital_base_balance - raw_base_volume,
            "wrong maker base balance"
        );
    }
}

/// Test that attempting to deposit fees into a referrer account with
/// insufficient storage balance doesn't error.
#[test]
fn referrer_fee_storage() {
    let mut contract = setup_contract();
    let one_base = 10_u128.pow(16);
    let one_quote = 10_u128.pow(18);
    let (maker, taker, base, quote) = get_accounts();

    storage_deposit(&mut contract, &maker);
    storage_deposit(&mut contract, &taker);

    contract.internal_deposit(&maker, &base.clone().into(), one_base);
    contract.internal_deposit(&taker, &quote.clone().into(), one_quote * 2);
    // give taker enough balance to cleanly buy 1 BASE @ 1 QUOTE and also pay
    // fees on top; makes the math cleaner

    set_deposit_context(maker.clone(), deposits::TENTH_NEAR);

    // referrer account doesn't have storage balance to cover any token balances
    let referrer = AccountId::new_unchecked("r".repeat(64));
    storage_deposit_registration_only(&mut contract, &referrer);
    {
        // Sanity check: depositing into the referrer account at this stage
        // should be impossible
        let mut ref_acc = contract.internal_unwrap_account(&referrer);
        ref_acc.deposit(&(&base).into(), one_base);
        if Err(()) != contract.internal_try_save_account(&referrer, ref_acc) {
            panic!("Depositing tokens into account with minimum storage deposit succeeded")
        }
    }

    let m = create_market_and_place_orders(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(base.clone()).key(),
            base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
            quote_token: TokenType::from_account_id(quote.clone()).key(),
            quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
            taker_fee_base_rate: 10 as u8,
            maker_rebate_base_rate: 5 as u8,
        },
        vec![
            (
                maker.clone(),
                new_order_params(
                    one_quote,
                    None,
                    one_base,
                    Side::Sell,
                    OrderType::Limit,
                    None,
                    None,
                ),
            ),
            (
                taker.clone(),
                new_order_params(
                    one_quote,
                    Some(U128(one_quote * 2)),
                    one_base,
                    Side::Buy,
                    OrderType::Limit,
                    None,
                    Some(referrer.clone()),
                ),
            ),
        ],
    );

    // since referrer fee didn't get deposited to referrer, check that it
    // accrued to the contract instead
    let market = contract.internal_get_market(&m.unwrap_id()).unwrap();

    // traded 1 BASE @ 1 QUOTE with a 10 bps taker fee, 5 bps maker rebate, no
    // referrer rebate due to insufficient storage balance.
    // Net should be 10 - 5 = 5 bps accrued to market
    assert_eq!(
        market.fees_accrued,
        one_quote * 5 / 10_000,
        "wrong fees accrued to market"
    )
}
