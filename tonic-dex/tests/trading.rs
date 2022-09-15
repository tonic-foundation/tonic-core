use near_sdk::json_types::U128;
use near_sdk::test_utils::accounts;

use tonic_dex::*;

mod util;
use util::*;

const BASE_TOKEN_LOT_SIZE: u128 = 1000000000;
const QUOTE_TOKEN_LOT_SIZE: u128 = 100000;

fn market_order_params(max_spend: Option<U128>, quantity: U128, side: Side) -> NewOrderParams {
    NewOrderParams {
        limit_price: None,
        max_spend,
        quantity,
        side,
        order_type: OrderType::Market,
        client_id: None,
        referrer_id: None,
    }
}

#[test]
fn get_open_orders() {
    let mut contract = setup_contract();
    let (user_a, user_b, wnear, usdc) = get_accounts();

    set_deposit_context(user_a.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: 1.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: 1.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        0,
        0,
    );
    contract.on_ft_metadata(market_id.into(), PairSide::Base, Some(get_ft_metadata(0)));
    contract.on_ft_metadata(market_id.into(), PairSide::Quote, Some(get_ft_metadata(0)));

    storage_deposit(&mut contract, &user_a);
    storage_deposit(&mut contract, &user_b);
    contract.internal_deposit(&user_a.clone(), &(&usdc).into(), 100);
    contract.internal_deposit(&user_b.clone(), &(&usdc).into(), 100);

    set_predecessor_context(user_a.clone());
    let PlaceOrderResultView { id: order_id, .. } = contract.new_order(
        market_id.into(),
        NewOrderParams {
            limit_price: Some(U128::from(10)),
            max_spend: Some(U128::from(35)),
            quantity: U128::from(5),
            side: Side::Buy,
            order_type: OrderType::Limit,
            client_id: None,
            referrer_id: None,
        },
    );

    set_predecessor_context(user_b.clone());
    contract.new_order(
        market_id.into(),
        NewOrderParams {
            limit_price: Some(U128::from(10)),
            max_spend: None,
            quantity: U128::from(5),
            side: Side::Buy,
            order_type: OrderType::Limit,
            client_id: None,
            referrer_id: None,
        },
    );

    assert_eq!(
        contract
            .get_open_orders(market_id.into(), user_a.clone())
            .len(),
        1,
        "Expected 1 open order owned by user A"
    );

    assert_eq!(
        contract.get_open_orders(market_id.into(), user_a.clone())[0].id,
        order_id,
        "user A wrong open order ID"
    );

    // Setting max spend of U128(35) only allows a true original_qty of 3 (because 3*10 < 35)
    assert_eq!(
        contract
            .get_open_orders(market_id.into(), user_a.clone())
            .get(0)
            .unwrap()
            .original_qty,
        Some(U128(3)),
        "user A wrong original_qty"
    );
}

#[test]
fn balance_invariants() {
    let mut contract = setup_contract();
    let (alice, bob, wnear, usdc) = get_accounts();
    storage_deposit(&mut contract, &alice);
    storage_deposit(&mut contract, &bob);

    let one_base = 10_u128.pow(16);
    let one_quote = 10_u128.pow(18);
    let total_quote = one_quote * 1000;
    let total_base = one_base * 60000;
    let raw_base_volume = one_base / 5;
    let raw_limit_price = 5 * one_quote;
    let taker_fee_base_rate = 20_u128;
    let maker_rebate_base_rate = 2_u128;
    let implicit_max_spend = raw_limit_price * raw_base_volume;
    let adjusted_max_spend =
        implicit_max_spend + (implicit_max_spend * taker_fee_base_rate / one_quote);

    contract.internal_deposit(&alice, &usdc.clone().into(), total_quote);
    contract.internal_deposit(&bob, &wnear.clone().into(), total_base);

    assert_balance_invariant(
        &contract,
        None,
        vec![(&wnear, total_base), (&usdc, total_quote)],
    );

    set_deposit_context(alice.clone(), deposits::TENTH_NEAR);

    let mut market = create_market_and_place_orders(
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
                alice.clone(),
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
                bob.clone(),
                new_order_params(
                    raw_limit_price,
                    None,
                    2 * raw_base_volume,
                    Side::Sell,
                    OrderType::Limit,
                    None,
                    None,
                ),
            ),
            (
                alice.clone(),
                new_order_params(
                    raw_limit_price,
                    None,
                    2 * raw_base_volume,
                    Side::Buy,
                    OrderType::Market,
                    None,
                    None,
                ),
            ),
            (
                bob.clone(),
                new_order_params(
                    raw_limit_price,
                    None,
                    2 * raw_base_volume,
                    Side::Sell,
                    OrderType::Market,
                    None,
                    None,
                ),
            ),
        ],
    );

    contract.internal_swap(
        &mut market,
        Side::Buy,
        one_quote * 5 + QUOTE_TOKEN_LOT_SIZE as u128,
        None,
    );
    cancel_all_orders(&mut contract, &market);
    assert_balance_invariant(
        &contract,
        Some(&market),
        vec![(&wnear, total_base), (&usdc, total_quote)],
    );
}

#[test]
fn test_get_market() {
    let mut contract = setup_contract();
    let one_base = (10 as u128).pow(16);
    let one_quote = (10 as u128).pow(18);
    let (user_a, user_b, wnear, usdc) = get_accounts();

    set_deposit_context(user_a.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        16,
        18,
    );

    storage_deposit(&mut contract, &user_a);
    storage_deposit(&mut contract, &user_b);
    // 10 USD
    contract.internal_deposit(&user_a, &(&usdc).into(), 10 * one_quote);
    // 0.25 NEAR
    contract.internal_deposit(&user_b, &(&wnear).into(), one_base / 4);

    set_predecessor_context(user_a.clone());
    // Buy 0.2 NEAR @ 5USD each -> 1 USD total
    contract.new_order(
        market_id.into(),
        new_order_params(
            5 * one_quote,
            None,
            one_base / 5,
            Side::Buy,
            OrderType::Limit,
            None,
            None,
        ),
    );

    assert!(
        !contract
            .internal_unwrap_market(&market_id)
            .orderbook
            .bids
            .is_empty(),
        "orderbook bids empty"
    );

    assert!(
        !contract
            .internal_unwrap_market(&market_id)
            .to_view(1, false)
            .orderbook
            .bids
            .is_empty(),
        "orderbook view bids empty"
    );
}

#[test]
fn test_trade() {
    let mut contract = setup_contract();
    let one_base = (10 as u128).pow(16);
    let one_quote = (10 as u128).pow(18);
    let (user_a, user_b, wnear, usdc) = get_accounts();

    set_deposit_context(user_a.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        16,
        18,
    );

    storage_deposit(&mut contract, &user_a);
    storage_deposit(&mut contract, &user_b);
    // 10 USD
    contract.internal_deposit(&user_a, &(&usdc).into(), 10 * one_quote);
    // 0.25 NEAR
    contract.internal_deposit(&user_b, &(&wnear).into(), one_base / 4);

    set_predecessor_context(user_a.clone());
    // Buy 0.2 NEAR @ 5USD each -> 1 USD total
    contract.new_order(
        market_id.into(),
        new_order_params(
            5 * one_quote,
            None,
            one_base / 5,
            Side::Buy,
            OrderType::Limit,
            None,
            None,
        ),
    );
    let balance = get_balance(&contract, &user_a, usdc.clone().into());
    assert_eq!(balance, 9 * one_quote);

    set_predecessor_context(user_b.clone());
    // Sell 0.2 NEAR @ 4 USD each
    contract.new_order(
        market_id.into(),
        new_order_params(
            4 * one_quote,
            None,
            one_base / 5,
            Side::Sell,
            OrderType::Limit,
            None,
            None,
        ),
    );
    let buyer_near_balance = get_balance(&contract, &user_a, wnear.clone().into());
    let buyer_usdc_balance = get_balance(&contract, &user_a, usdc.clone().into());
    let seller_near_balance = get_balance(&contract, &user_b, wnear.into());
    let seller_usdc_balance = get_balance(&contract, &user_b, usdc.into());

    assert_eq!(buyer_near_balance, one_base / 5);
    assert_eq!(buyer_usdc_balance, one_quote * 9);
    // 0.05 near left
    assert_eq!(seller_near_balance, one_base / 20);
    assert_eq!(seller_usdc_balance, one_quote);
}

#[test]
fn taker_gets_best_buy_price() {
    let mut contract = setup_contract();
    let one_base = (10 as u128).pow(16);
    let one_quote = (10 as u128).pow(18);
    let (maker, taker, wnear, usdc) = get_accounts();

    set_deposit_context(maker.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        16,
        18,
    );

    storage_deposit(&mut contract, &maker);
    storage_deposit(&mut contract, &taker);
    // 0.5 NEAR
    contract.internal_deposit(&maker, &(&wnear).into(), one_base / 2);
    // 10 USD
    contract.internal_deposit(&taker, &(&usdc).into(), 10 * one_quote);

    // Sell 0.2 NEAR @ 5 USD each -> 1 USD to fill the order
    set_predecessor_context(maker.clone());
    contract.new_order(
        market_id.into(),
        new_order_params(
            5 * one_quote,
            None,
            one_base / 5,
            Side::Sell,
            OrderType::Limit,
            None,
            None,
        ),
    );

    // Sell 0.2 NEAR @ 6 USD each -> 1.2 USD to fill the order
    contract.new_order(
        market_id.into(),
        new_order_params(
            6 * one_quote,
            None,
            one_base / 5,
            Side::Sell,
            OrderType::Limit,
            None,
            None,
        ),
    );

    // Buy 0.4 NEAR @ 6 USD each -> 2.4 USD max spend
    // Should fill like this:
    //   0.2 NEAR @ 5 USD   1.0 USD
    // + 0.2 NEAR @ 6 USD   1.2 USD
    // ----------------------------
    //   Actual spend       2.2 USD
    //
    // Since we deposited 10 USD, we should see 7.8 USD left after both fill.
    //
    // Using two maker orders lets us test that the refund is calculated
    // correctly for each matched maker order.
    set_predecessor_context(taker.clone());
    contract.new_order(
        market_id.into(),
        new_order_params(
            6 * one_quote,
            None,
            one_base / 5 * 2,
            Side::Buy,
            OrderType::Limit,
            None,
            None,
        ),
    );
    let maker_near_balance = get_balance(&contract, &maker, wnear.clone().into());
    let maker_usdc_balance = get_balance(&contract, &maker, usdc.clone().into());
    let taker_near_balance = get_balance(&contract, &taker, wnear.into());
    let taker_usdc_balance = get_balance(&contract, &taker, usdc.into());

    // Buyer should get better price than their limit price and only spend 1 USD
    assert_eq!(taker_near_balance, one_base / 5 * 2, "Wrong taker fill");
    assert_eq!(
        taker_usdc_balance,
        one_quote * 78 / 10, // 7.8 USD
        "Wrong taker quote balance (probably didn't refund excess correctly)"
    );

    assert_eq!(maker_near_balance, one_base / 10);
    assert_eq!(maker_usdc_balance, one_quote * 22 / 10); // 2.2 USD
}

#[test]
fn taker_gets_best_sell_price() {
    let mut contract = setup_contract();
    let one_base = (10 as u128).pow(16);
    let one_quote = (10 as u128).pow(18);
    let (maker, taker, wnear, usdc) = get_accounts();

    set_deposit_context(maker.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        16,
        18,
    );

    storage_deposit(&mut contract, &maker);
    storage_deposit(&mut contract, &taker);
    // 10 USD
    contract.internal_deposit(&maker, &(&usdc).into(), 10 * one_quote);
    // 0.5 NEAR
    contract.internal_deposit(&taker, &(&wnear).into(), one_base / 2);

    // Buy 0.2 NEAR @ 5 USD each -> should pay 1 USD
    set_predecessor_context(maker.clone());
    contract.new_order(
        market_id.into(),
        new_order_params(
            5 * one_quote,
            None,
            one_base / 5,
            Side::Buy,
            OrderType::Limit,
            None,
            None,
        ),
    );

    // Buy 0.2 NEAR @ 6 USD each -> should pay 1.2 USD
    contract.new_order(
        market_id.into(),
        new_order_params(
            6 * one_quote,
            None,
            one_base / 5,
            Side::Buy,
            OrderType::Limit,
            None,
            None,
        ),
    );
    // If both maker orders fill at their specified limit prices, the maker
    // should expect to spend 2.2 USD total and have 7.8 USD left after

    // Sell 0.4 NEAR @ 5 USD each -> taker expects to receive USD 2.0
    // Should fill like this:
    //   0.2 NEAR @ 5 USD   1.0 USD
    // + 0.2 NEAR @ 6 USD   1.2 USD <- taker gets a better price for second order
    // ----------------------------
    //   Actual proceeds    2.2 USD
    set_predecessor_context(taker.clone());
    contract.new_order(
        market_id.into(),
        new_order_params(
            5 * one_quote,
            None,
            one_base / 5 * 2,
            Side::Sell,
            OrderType::Limit,
            None,
            None,
        ),
    );
    let maker_near_balance = get_balance(&contract, &maker, wnear.clone().into());
    let maker_usdc_balance = get_balance(&contract, &maker, usdc.clone().into());
    let taker_near_balance = get_balance(&contract, &taker, wnear.into());
    let taker_usdc_balance = get_balance(&contract, &taker, usdc.into());

    // Buyer should get better price than their limit price and only spend 1 USD
    assert_eq!(taker_near_balance, one_base / 10, "Wrong taker fill");
    assert_eq!(
        taker_usdc_balance,
        one_quote * 22 / 10, // 2.2 USD
        "Wrong taker quote balance (probably didn't refund excess correctly)"
    );

    assert_eq!(maker_near_balance, one_base / 5 * 2);
    assert_eq!(maker_usdc_balance, one_quote * 78 / 10); // 7.8 USD
}

#[test]
fn test_fill_or_kill() {
    let mut contract = setup_contract();
    let one_base = (10 as u128).pow(16);
    let one_quote = (10 as u128).pow(18);
    let (user_a, user_b, wnear, usdc) = get_accounts();

    set_deposit_context(user_a.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        16,
        18,
    );

    storage_deposit(&mut contract, &user_a);
    storage_deposit(&mut contract, &user_b);

    // 10 USD
    contract.internal_deposit(&user_a, &(&usdc).into(), 10 * one_quote);
    // 0.25 NEAR
    contract.internal_deposit(&user_b, &(&wnear).into(), one_base / 4);

    set_predecessor_context(user_b.clone());
    // Sell 0.25 NEAR @ 5USD each -> 1 USD total
    contract.new_order(
        market_id.into(),
        new_order_params(
            4 * one_quote,
            None,
            one_base / 4,
            Side::Sell,
            OrderType::Limit,
            None,
            None,
        ),
    );

    set_predecessor_context(user_a.clone());
    // Sell 0.2 NEAR @ 4 USD each
    let result = contract.new_order(
        market_id.into(),
        new_order_params(
            4 * one_quote,
            None,
            one_base,
            Side::Buy,
            OrderType::FillOrKill,
            None,
            None,
        ),
    );
    let buyer_usdc_balance = get_balance(&contract, &user_a, usdc.clone().into());
    assert_eq!(result.outcome, OrderOutcome::Rejected);
    // nothing should be deducted for a cancelled order
    assert_eq!(buyer_usdc_balance, 10 * one_quote);

    let result = contract.new_order(
        market_id.into(),
        new_order_params(
            4 * one_quote,
            None,
            one_base / 4,
            Side::Buy,
            OrderType::FillOrKill,
            None,
            None,
        ),
    );
    let buyer_usdc_balance = get_balance(&contract, &user_a, usdc.clone().into());
    assert_eq!(result.outcome, OrderOutcome::Filled);
    assert_eq!(buyer_usdc_balance, 9 * one_quote);
}

#[test]
fn test_immediate_or_canel() {
    let mut contract = setup_contract();
    let one_base = (10 as u128).pow(16);
    let one_quote = (10 as u128).pow(18);
    let (user_a, user_b, wnear, usdc) = get_accounts();

    set_deposit_context(user_a.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        16,
        18,
    );

    storage_deposit(&mut contract, &user_a);
    storage_deposit(&mut contract, &user_b);

    // 10 USD
    contract.internal_deposit(&user_a, &(&usdc).into(), 10 * one_quote);
    // 0.25 NEAR
    contract.internal_deposit(&user_b, &(&wnear).into(), one_base / 4);

    set_predecessor_context(user_b.clone());
    // Sell 0.2 NEAR @ 5USD each -> 1 USD total
    contract.new_order(
        market_id.into(),
        new_order_params(
            5 * one_quote,
            None,
            one_base / 5,
            Side::Sell,
            OrderType::Limit,
            None,
            None,
        ),
    );

    set_predecessor_context(user_a.clone());
    // Buy 1 NEAR @ 5 USD each
    let result = contract.new_order(
        market_id.into(),
        new_order_params(
            5 * one_quote,
            None,
            one_base,
            Side::Buy,
            OrderType::ImmediateOrCancel,
            None,
            None,
        ),
    );
    let buyer_usdc_balance = get_balance(&contract, &user_a, usdc.clone().into());
    assert_eq!(result.outcome, OrderOutcome::PartialFill);
    // Only the amount filled should be deducted
    assert_eq!(buyer_usdc_balance, 9 * one_quote);
}

#[test]
fn test_cancel_order() {
    let mut contract = setup_contract();
    let owner = util::accounts(0);
    let user = accounts(1);
    let base_token = accounts(2);
    let quote_token = accounts(3);

    set_deposit_context(owner, deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(base_token.clone()).key(),
            base_token_lot_size: 1.into(),
            quote_token: TokenType::from_account_id(quote_token.clone()).key(),
            quote_token_lot_size: 1.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        0,
        0,
    );
    contract.on_ft_metadata(market_id.into(), PairSide::Base, Some(get_ft_metadata(0)));
    contract.on_ft_metadata(market_id.into(), PairSide::Quote, Some(get_ft_metadata(0)));

    storage_deposit(&mut contract, &user);

    contract.internal_deposit(&accounts(1).into(), &(&base_token).into(), 100);
    contract.internal_deposit(&accounts(1).into(), &(&quote_token).into(), 1000);

    set_predecessor_context(user.clone());
    let PlaceOrderResultView { id: order_id, .. } = contract.new_order(
        market_id.into(),
        NewOrderParams {
            limit_price: Some(U128::from(10)),
            max_spend: None,
            quantity: U128::from(5),
            side: Side::Buy,
            order_type: OrderType::Limit,
            client_id: None,
            referrer_id: None,
        },
    );
    assert_eq!(
        get_balance(&contract, &user, quote_token.clone().into()),
        950
    );
    contract.cancel_order(market_id.into(), order_id);
    assert_eq!(get_balance(&contract, &user, quote_token.into()), 1000);
    let open_orders = contract.get_open_orders(market_id.into(), user.clone());
    assert_eq!(open_orders.len(), 0);
}

#[test]
#[should_panic(expected = "E24: order not found")]
fn test_signer_cannot_cancel_other_users_order() {
    let mut contract = setup_contract();
    let (user_a, user_b, wnear, usdc) = get_accounts();

    set_deposit_context(user_a.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: 1.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: 1.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        0,
        0,
    );
    contract.on_ft_metadata(market_id.into(), PairSide::Base, Some(get_ft_metadata(0)));
    contract.on_ft_metadata(market_id.into(), PairSide::Quote, Some(get_ft_metadata(0)));

    storage_deposit(&mut contract, &user_a);
    storage_deposit(&mut contract, &user_b);
    contract.internal_deposit(&user_a.clone(), &(&usdc).into(), 100);

    set_predecessor_context(user_a.clone());
    let PlaceOrderResultView { id: order_id, .. } = contract.new_order(
        market_id.into(),
        NewOrderParams {
            limit_price: Some(U128::from(10)),
            max_spend: None,
            quantity: U128::from(5),
            side: Side::Buy,
            order_type: OrderType::Limit,
            client_id: None,
            referrer_id: None,
        },
    );

    set_predecessor_context(user_b);
    contract.cancel_order(market_id.into(), order_id); // should panic
}

#[test]
fn test_cancel_all() {
    let mut contract = setup_contract();
    let owner = util::accounts(0);
    let user = accounts(1);
    let base_token = accounts(2);
    let quote_token = accounts(3);

    set_deposit_context(owner.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(base_token.clone()).key(),
            base_token_lot_size: 1.into(),
            quote_token: TokenType::from_account_id(quote_token.clone()).key(),
            quote_token_lot_size: 1.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        0,
        0,
    );
    contract.on_ft_metadata(market_id.into(), PairSide::Base, Some(get_ft_metadata(0)));
    contract.on_ft_metadata(market_id.into(), PairSide::Quote, Some(get_ft_metadata(0)));

    storage_deposit(&mut contract, &user);
    contract.internal_deposit(&accounts(1).into(), &(&base_token).into(), 100);
    contract.internal_deposit(&accounts(1).into(), &(&quote_token).into(), 1000);

    set_predecessor_context(user.clone());
    contract.new_order(
        market_id.into(),
        NewOrderParams {
            limit_price: Some(U128::from(10)),
            max_spend: None,
            quantity: U128::from(5),
            side: Side::Buy,
            order_type: OrderType::Limit,
            client_id: None,
            referrer_id: None,
        },
    );
    assert_eq!(
        get_balance(&contract, &user, quote_token.clone().into()),
        950
    );
    contract.cancel_all_orders(market_id.into());
    assert_eq!(get_balance(&contract, &user, quote_token.into()), 1000);
    let open_orders = contract.get_open_orders(market_id.into(), user.clone());
    assert_eq!(open_orders.len(), 0);
}

#[test]
#[should_panic(expected = "can only be called by contract owner")]
fn test_admin_cancel_panics_if_not_admin() {
    let mut contract = setup_contract();
    let user = accounts(1);
    let base_token = accounts(2);
    let quote_token = accounts(3);

    set_deposit_context(user.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(base_token.clone()).key(),
            base_token_lot_size: 1.into(),
            quote_token: TokenType::from_account_id(quote_token.clone()).key(),
            quote_token_lot_size: 1.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        0,
        0,
    );

    storage_deposit(&mut contract, &user);
    contract.internal_deposit(&accounts(1).into(), &(&base_token).into(), 100);
    contract.internal_deposit(&accounts(1).into(), &(&quote_token).into(), 1000);

    let order = contract.new_order(
        market_id.into(),
        NewOrderParams {
            limit_price: Some(U128::from(10)),
            max_spend: None,
            quantity: U128::from(5),
            side: Side::Buy,
            order_type: OrderType::Limit,
            client_id: None,
            referrer_id: None,
        },
    );

    // caller is user not admin, so should panic
    contract.admin_cancel_order(market_id.into(), order.id.into());
}

#[test]
fn test_admin_cancel() {
    let mut contract = setup_contract();
    let admin = util::accounts(0);
    let user = accounts(1);
    let base_token = accounts(2);
    let quote_token = accounts(3);

    set_deposit_context(admin.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(base_token.clone()).key(),
            base_token_lot_size: 1.into(),
            quote_token: TokenType::from_account_id(quote_token.clone()).key(),
            quote_token_lot_size: 1.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        0,
        0,
    );

    storage_deposit(&mut contract, &user);
    contract.internal_deposit(&accounts(1).into(), &(&base_token).into(), 100);
    contract.internal_deposit(&accounts(1).into(), &(&quote_token).into(), 1000);

    set_predecessor_context(user.clone());
    let order = contract.new_order(
        market_id.into(),
        NewOrderParams {
            limit_price: Some(U128::from(10)),
            max_spend: None,
            quantity: U128::from(5),
            side: Side::Buy,
            order_type: OrderType::Limit,
            client_id: None,
            referrer_id: None,
        },
    );

    set_predecessor_context(admin);
    contract.admin_cancel_order(market_id.into(), order.id.into());
    assert_eq!(get_balance(&contract, &user, quote_token.into()), 1000);
    let open_orders = contract.get_open_orders(market_id.into(), user.clone());
    assert_eq!(open_orders.len(), 0);
}

#[test]
fn test_admin_cancel_all_user_orders() {
    let mut contract = setup_contract();
    let admin = util::accounts(0);

    let user_a = accounts(0);
    let user_b = accounts(1);
    let base_token = accounts(2);
    let quote_token = accounts(3);

    set_deposit_context(admin.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(base_token.clone()).key(),
            base_token_lot_size: 1.into(),
            quote_token: TokenType::from_account_id(quote_token.clone()).key(),
            quote_token_lot_size: 1.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        0,
        0,
    );

    for user in [user_a.clone(), user_b.clone()] {
        set_predecessor_context(user.clone());
        storage_deposit(&mut contract, &user.clone());
        contract.internal_deposit(&user.clone().into(), &(&base_token).into(), 100);
        contract.internal_deposit(&user.into(), &(&quote_token).into(), 1000);

        contract.new_order(
            market_id.into(),
            NewOrderParams {
                limit_price: Some(U128::from(10)),
                max_spend: None,
                quantity: U128::from(5),
                side: Side::Buy,
                order_type: OrderType::Limit,
                client_id: None,
                referrer_id: None,
            },
        );
    }

    // Admin cancels all user_a's orders & leaves those of user_b.
    set_predecessor_context(admin);
    contract.admin_cancel_all_user_orders(market_id.into(), user_a.clone());
    let open_orders = contract.get_open_orders(market_id.into(), user_a.clone());
    assert_eq!(open_orders.len(), 0);
    let open_orders = contract.get_open_orders(market_id.into(), user_b.clone());
    assert_eq!(open_orders.len(), 1);
}

#[test]
fn test_admin_clear_orderbook() {
    let mut contract = setup_contract();
    let admin = util::accounts(0);
    let user = accounts(1);
    let base_token = accounts(2);
    let quote_token = accounts(3);

    set_deposit_context(admin.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(base_token.clone()).key(),
            base_token_lot_size: 1.into(),
            quote_token: TokenType::from_account_id(quote_token.clone()).key(),
            quote_token_lot_size: 1.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        0,
        0,
    );

    storage_deposit(&mut contract, &user);
    contract.internal_deposit(&accounts(1).into(), &(&base_token).into(), 10000);
    contract.internal_deposit(&accounts(1).into(), &(&quote_token).into(), 100000);

    set_predecessor_context(user.clone());

    let num_orders = 10;
    for i in 0..num_orders {
        contract.new_order(
            market_id.into(),
            NewOrderParams {
                limit_price: Some(U128::from(1 + i)),
                max_spend: None,
                quantity: U128::from(5),
                side: Side::Buy,
                order_type: OrderType::Limit,
                client_id: None,
                referrer_id: None,
            },
        );
    }

    set_predecessor_context(admin);
    // Clear all but 1 order
    contract.admin_clear_orderbook(market_id.into(), Some((num_orders - 1) as u16));

    let open_orders = contract.get_open_orders(market_id.into(), user.clone());
    assert_eq!(open_orders.len(), 1);
}

#[test]
fn test_decimals() {
    let mut contract = setup_contract();
    let (user_a, _, wnear, usdc) = get_accounts();

    set_deposit_context(user_a.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: 10.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: 100.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        2,
        4,
    );

    storage_deposit(&mut contract, &user_a);
    contract.internal_deposit(&user_a, &(&usdc).into(), 5000);

    contract.new_order(
        market_id.into(),
        NewOrderParams {
            limit_price: Some(U128::from(4000)),
            max_spend: None,
            quantity: U128::from(50),
            side: Side::Buy,
            order_type: OrderType::Limit,
            client_id: None,
            referrer_id: None,
        },
    );
    let balance = get_balance(&contract, &user_a, usdc.into());
    assert_eq!(balance, 3000);
}

// Equivalent to test_decimals test, only using Multitokens.
#[test]
fn test_decimals_mt_token() {
    let mut contract = setup_contract();
    let (user_a, _, wnear_mt_account, usdc_mt_account) = get_accounts();

    let wnear_subtoken_id = "wnear";
    let wnear_key = format!("mft:{}:{}", wnear_subtoken_id, wnear_mt_account);
    let wnear = TokenType::from_key(&wnear_key);

    let usdc_subtoken_id = "usdc";
    let usdc_key = format!("mft:{}:{}", usdc_subtoken_id, usdc_mt_account);
    let usdc = TokenType::from_key(&usdc_key);

    set_deposit_context(user_a.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market_using_multitokens(
        &mut contract,
        CreateMarketArgs {
            base_token: wnear.key(),
            base_token_lot_size: 10.into(),
            quote_token: usdc.key(),
            quote_token_lot_size: 100.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        2,
        4,
    );

    storage_deposit(&mut contract, &user_a);
    contract.internal_deposit(&user_a, &usdc, 5000);

    contract.new_order(
        market_id.into(),
        NewOrderParams {
            limit_price: Some(U128::from(4000)),
            max_spend: None,
            quantity: U128::from(50),
            side: Side::Buy,
            order_type: OrderType::Limit,
            client_id: None,
            referrer_id: None,
        },
    );
    let balance = get_balance(&contract, &user_a, usdc);
    assert_eq!(balance, 3000);
}

#[test]
fn test_market_buy() {
    let mut contract = setup_contract();
    let one_base = (10 as u128).pow(16);
    let one_quote = (10 as u128).pow(18);
    let (maker, taker, wnear, usdc) = get_accounts();

    set_deposit_context(maker.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        16,
        18,
    );

    storage_deposit(&mut contract, &maker);
    storage_deposit(&mut contract, &taker);

    // 10 USD
    contract.internal_deposit(&taker, &(&usdc).into(), 10 * one_quote);
    // 3 NEAR
    contract.internal_deposit(&maker, &(&wnear).into(), one_base * 3);

    set_predecessor_context(maker.clone());
    // Sell 1 NEAR @ 2USD each
    contract.new_order(
        market_id.into(),
        new_order_params(
            2 * one_quote,
            None,
            one_base,
            Side::Sell,
            OrderType::Limit,
            None,
            None,
        ),
    );
    // Sell 1 NEAR @ 3USD each
    contract.new_order(
        market_id.into(),
        new_order_params(
            3 * one_quote,
            None,
            one_base,
            Side::Sell,
            OrderType::Limit,
            None,
            None,
        ),
    );
    // Sell 1 NEAR @ 4USD each
    contract.new_order(
        market_id.into(),
        new_order_params(
            4 * one_quote,
            None,
            one_base,
            Side::Sell,
            OrderType::Limit,
            None,
            None,
        ),
    );

    set_predecessor_context(taker.clone());
    // Market buy 5 USD worth of NEAR + 1 lots, should fill 2 and refund 1 lot
    let result = contract.new_order(
        market_id.into(),
        market_order_params(None, U128::from(one_base * 2), Side::Buy),
    );
    assert_eq!(result.outcome, OrderOutcome::Filled);

    let maker_near_balance = get_balance(&contract, &maker, wnear.clone().into());
    let maker_usdc_balance = get_balance(&contract, &maker, usdc.clone().into());
    let taker_near_balance = get_balance(&contract, &taker, wnear.into());
    let taker_usdc_balance = get_balance(&contract, &taker, usdc.into());
    assert_eq!(maker_usdc_balance, 5 * one_quote);
    // has remaining 1 NEAR on the orderbook
    assert_eq!(maker_near_balance, 0);
    assert_eq!(taker_usdc_balance, 5 * one_quote);
    assert_eq!(taker_near_balance, 2 * one_base);
}

#[test]
fn test_swap_buy() {
    let mut contract = setup_contract();
    let one_base = (10 as u128).pow(16);
    let one_quote = (10 as u128).pow(18);
    let (user_a, user_b, wnear, usdc) = get_accounts();

    set_deposit_context(user_a.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        16,
        18,
    );

    storage_deposit(&mut contract, &user_a);
    storage_deposit(&mut contract, &user_b);

    // 10 USD
    contract.internal_deposit(&user_a, &(&usdc).into(), 10 * one_quote);
    // 3 NEAR
    contract.internal_deposit(&user_b, &(&wnear).into(), one_base * 3);

    set_predecessor_context(user_b.clone());
    // Sell 1 NEAR @ 2USD each
    contract.new_order(
        market_id.into(),
        new_order_params(
            2 * one_quote,
            None,
            one_base,
            Side::Sell,
            OrderType::Limit,
            None,
            None,
        ),
    );
    // Sell 1 NEAR @ 3USD each
    contract.new_order(
        market_id.into(),
        new_order_params(
            3 * one_quote,
            None,
            one_base,
            Side::Sell,
            OrderType::Limit,
            None,
            None,
        ),
    );
    // Sell 1 NEAR @ 4USD each
    contract.new_order(
        market_id.into(),
        new_order_params(
            4 * one_quote,
            None,
            one_base,
            Side::Sell,
            OrderType::Limit,
            None,
            None,
        ),
    );

    let mut market = contract.internal_unwrap_market(&market_id);

    set_predecessor_context(user_a.clone());
    // Market buy 5 USD worth of NEAR + 1 lots, should fill 2 and refund 1 lot
    let result = contract.internal_swap(
        &mut market,
        Side::Buy,
        one_quote * 5 + QUOTE_TOKEN_LOT_SIZE as u128,
        None,
    );
    assert_eq!(result.output_amount, one_base * 2);
    assert_eq!(
        result.input_refund, QUOTE_TOKEN_LOT_SIZE as u128,
        "Wrong refund calculation"
    );

    // let buyer_near_balance = get_balance(&contract, &user_a, wnear.clone().into());
    // assert_eq!(buyer_near_balance, one_base * 2);

    let seller_usdc_balance = get_balance(&contract, &user_b, usdc.clone().into());
    assert_eq!(seller_usdc_balance, 5 * one_quote);
}

#[test]
fn test_market_sell() {
    let mut contract = setup_contract();
    let one_base = (10 as u128).pow(16);
    let one_quote = (10 as u128).pow(18);
    let (maker, taker, wnear, usdc) = get_accounts();

    set_deposit_context(maker.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        16,
        18,
    );

    storage_deposit(&mut contract, &maker);
    storage_deposit(&mut contract, &taker);

    // 3 NEAR
    contract.internal_deposit(&taker, &(&wnear).into(), one_base * 3);

    // 10 USD
    contract.internal_deposit(&maker, &(&usdc).into(), 10 * one_quote);

    set_predecessor_context(maker.clone());
    // Buy 1 NEAR @ 2USD each
    contract.new_order(
        market_id.into(),
        new_order_params(
            2 * one_quote,
            None,
            one_base,
            Side::Buy,
            OrderType::Limit,
            None,
            None,
        ),
    );
    // Buy 1 NEAR @ 3USD each
    contract.new_order(
        market_id.into(),
        new_order_params(
            3 * one_quote,
            None,
            one_base,
            Side::Buy,
            OrderType::Limit,
            None,
            None,
        ),
    );
    // Buy 1 NEAR @ 4USD each
    contract.new_order(
        market_id.into(),
        new_order_params(
            4 * one_quote,
            None,
            one_base,
            Side::Buy,
            OrderType::Limit,
            None,
            None,
        ),
    );

    set_predecessor_context(taker.clone());
    // Market sell 2 NEAR, should output 7 USDC
    let result = contract.new_order(
        market_id.into(),
        market_order_params(None, U128::from(one_base * 2), Side::Sell),
    );
    assert_eq!(result.outcome, OrderOutcome::Filled);

    let maker_near_balance = get_balance(&contract, &maker, wnear.clone().into());
    let maker_usdc_balance = get_balance(&contract, &maker, usdc.clone().into());
    let taker_near_balance = get_balance(&contract, &taker, wnear.into());
    let taker_usdc_balance = get_balance(&contract, &taker, usdc.into());
    assert_eq!(maker_near_balance, 2 * one_base);
    assert_eq!(maker_usdc_balance, 1 * one_quote);
    assert_eq!(taker_near_balance, 1 * one_base);
    assert_eq!(taker_usdc_balance, 7 * one_quote);
}

#[test]
fn test_swap_sell() {
    let mut contract = setup_contract();
    let one_base = (10 as u128).pow(16);
    let one_quote = (10 as u128).pow(18);
    let (user_a, user_b, wnear, usdc) = get_accounts();

    set_deposit_context(user_a.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        16,
        18,
    );
    storage_deposit(&mut contract, &user_a);
    storage_deposit(&mut contract, &user_b);

    // 3 NEAR
    contract.internal_deposit(&user_a, &(&wnear).into(), one_base * 3);

    // 10 USD
    contract.internal_deposit(&user_b, &(&usdc).into(), 10 * one_quote);

    set_predecessor_context(user_b.clone());
    // Buy 1 NEAR @ 2USD each
    contract.new_order(
        market_id.into(),
        new_order_params(
            2 * one_quote,
            None,
            one_base,
            Side::Buy,
            OrderType::Limit,
            None,
            None,
        ),
    );
    // Buy 1 NEAR @ 3USD each
    contract.new_order(
        market_id.into(),
        new_order_params(
            3 * one_quote,
            None,
            one_base,
            Side::Buy,
            OrderType::Limit,
            None,
            None,
        ),
    );
    // Buy 1 NEAR @ 4USD each
    contract.new_order(
        market_id.into(),
        new_order_params(
            4 * one_quote,
            None,
            one_base,
            Side::Buy,
            OrderType::Limit,
            None,
            None,
        ),
    );

    let mut market = contract.internal_unwrap_market(&market_id);

    set_predecessor_context(user_a.clone());
    // Market sell 2 NEAR, should output 7 USDC
    let result = contract.internal_swap(&mut market, Side::Sell, one_base * 2, None);
    assert_eq!(result.output_amount, one_quote * 7);
    assert_eq!(
        result.input_refund, 0,
        "Attempted to refund base when should have completely sold"
    );

    // Market sell 1 NEAR + 1 lot, should output 2 USDC and refund 1 lot of NEAR
    let result = contract.internal_swap(
        &mut market,
        Side::Sell,
        one_base + BASE_TOKEN_LOT_SIZE as u128,
        None,
    );
    assert_eq!(result.output_amount, one_quote * 2);
    assert_eq!(
        result.input_refund, BASE_TOKEN_LOT_SIZE as u128,
        "Wrong refund calculation"
    );

    // let seller_usdc_balance = get_balance(&contract, &user_a, usdc.clone().into());
    // assert_eq!(seller_usdc_balance, one_quote * 7);

    let buyer_near_balance = get_balance(&contract, &user_b, wnear.clone().into());
    assert_eq!(buyer_near_balance, 3 * one_base);
}

#[test]
fn test_batch_operation() {
    let mut contract = setup_contract();
    let one_base = 10_u128.pow(16);
    let one_quote = 10_u128.pow(18);
    let (user_a, _, wnear, usdc) = get_accounts();

    set_deposit_context(user_a.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: 1.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: 1.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        0,
        0,
    );
    contract.on_ft_metadata(market_id.into(), PairSide::Base, Some(get_ft_metadata(0)));
    contract.on_ft_metadata(market_id.into(), PairSide::Quote, Some(get_ft_metadata(0)));

    storage_deposit(&mut contract, &user_a);
    contract.internal_deposit(&user_a, &usdc.clone().into(), one_quote * 10);
    contract.internal_deposit(&user_a, &wnear.clone().into(), one_base * 10);

    set_predecessor_context(user_a.clone());
    let order = contract.new_order(
        market_id.into(),
        NewOrderParams {
            limit_price: Some(U128::from(10)),
            max_spend: None,
            quantity: U128::from(5),
            side: Side::Buy,
            order_type: OrderType::Limit,
            client_id: None,
            referrer_id: None,
        },
    );
    let open_orders = contract.get_open_orders(market_id.into(), user_a.clone());
    assert_eq!(open_orders.len(), 1);

    // Sets predecessor and deposit
    set_deposit_context(user_a.clone(), 1);
    contract.execute(vec![
        Action::CancelAllOrders(CancelAllOrdersAction {
            market_id: market_id.into(),
        }),
        Action::NewOrder(NewOrderAction {
            market_id: market_id.into(),
            params: NewOrderParams {
                limit_price: Some(U128::from(12)),
                max_spend: None,
                quantity: U128::from(8),
                side: Side::Buy,
                order_type: OrderType::Limit,
                client_id: None,
                referrer_id: None,
            },
        }),
    ]);

    let open_orders = contract.get_open_orders(market_id.into(), user_a.clone());
    assert!(open_orders[0].id != order.id);
}

#[test]
#[should_panic(expected = "trading window")]
fn test_trading_window_ask() {
    let mut contract = setup_contract();
    let one_base = (10 as u128).pow(16);
    let one_quote = (10 as u128).pow(18);
    let (user_a, _, wnear, usdc) = get_accounts();

    set_deposit_context(user_a.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        16,
        18,
    );

    storage_deposit(&mut contract, &user_a);
    // 10 USD
    contract.internal_deposit(&user_a, &(&usdc).into(), 100 * one_quote);
    // 10 NEAR
    contract.internal_deposit(&user_a, &(&wnear).into(), one_base * 10);

    set_predecessor_context(user_a.clone());

    // Bids and asks with an empty orderbook should work at any price
    contract.new_order(
        market_id.into(),
        new_order_params(
            5 * one_quote,
            None,
            one_base / 5,
            Side::Sell,
            OrderType::Limit,
            None,
            None,
        ),
    );

    contract.new_order(
        market_id.into(),
        new_order_params(
            5 * one_quote * 5,
            None,
            one_base / 5,
            Side::Sell,
            OrderType::Limit,
            None,
            None,
        ),
    );
    assert_eq!(
        contract
            .internal_unwrap_market(&market_id)
            .orderbook
            .asks
            .unique_prices_count(),
        2
    );

    contract.new_order(
        market_id.into(),
        new_order_params(
            5 * one_quote * 500,
            None,
            one_base / 5,
            Side::Sell,
            OrderType::Limit,
            None,
            None,
        ),
    );
}

#[test]
#[should_panic(expected = "trading window")]
fn test_trading_window_bid() {
    let mut contract = setup_contract();
    let one_base = (10 as u128).pow(16);
    let one_quote = (10 as u128).pow(18);
    let (user_a, _, wnear, usdc) = get_accounts();

    set_deposit_context(user_a.clone(), deposits::TENTH_NEAR);
    let market_id = create_and_init_market(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        16,
        18,
    );

    storage_deposit(&mut contract, &user_a);
    // 10 USD
    contract.internal_deposit(&user_a, &(&usdc).into(), 100 * one_quote);
    // 10 NEAR
    contract.internal_deposit(&user_a, &(&wnear).into(), one_base * 10);

    set_predecessor_context(user_a.clone());

    // Bids and asks with an empty orderbook should work at any price
    contract.new_order(
        market_id.into(),
        new_order_params(
            5 * one_quote,
            None,
            one_base / 5,
            Side::Buy,
            OrderType::Limit,
            None,
            None,
        ),
    );

    contract.new_order(
        market_id.into(),
        new_order_params(
            one_quote,
            None,
            one_base / 5,
            Side::Buy,
            OrderType::Limit,
            None,
            None,
        ),
    );
    assert_eq!(
        contract
            .internal_unwrap_market(&market_id)
            .orderbook
            .bids
            .unique_prices_count(),
        2
    );

    contract.new_order(
        market_id.into(),
        new_order_params(
            one_quote / 25,
            None,
            one_base / 5,
            Side::Buy,
            OrderType::Limit,
            None,
            None,
        ),
    );
}

#[test]
#[should_panic(expected = "self trade")]
fn self_trade_should_panic() {
    let mut contract = setup_contract();
    let one_base = 10_u128.pow(16);
    let one_quote = 10_u128.pow(18);
    let (user_a, user_b, wnear, usdc) = get_accounts();
    set_deposit_context(user_a.clone(), deposits::TENTH_NEAR);
    let market = create_market_and_place_orders(
        &mut contract,
        CreateMarketArgs {
            base_token: TokenType::from_account_id(wnear.clone()).key(),
            base_token_lot_size: BASE_TOKEN_LOT_SIZE.into(),
            quote_token: TokenType::from_account_id(usdc.clone()).key(),
            quote_token_lot_size: QUOTE_TOKEN_LOT_SIZE.into(),
            taker_fee_base_rate: 0,
            maker_rebate_base_rate: 0,
        },
        vec![],
    );
    storage_deposit(&mut contract, &user_a);
    storage_deposit(&mut contract, &user_b);
    contract.internal_deposit(&user_a, &usdc.clone().into(), one_quote * 10);
    contract.internal_deposit(&user_a, &wnear.clone().into(), one_base * 10);
    contract.internal_deposit(&user_b, &usdc.clone().into(), one_quote * 10);
    contract.internal_deposit(&user_b, &wnear.clone().into(), one_base * 10);
    // buy 1 BASE @ 1 QUOTE
    let res1 = contract.new_order(
        *market.unwrap_id(),
        NewOrderParams {
            limit_price: Some(U128::from(one_quote)),
            max_spend: None,
            quantity: U128::from(one_base),
            side: Side::Buy,
            order_type: OrderType::Limit,
            client_id: None,
            referrer_id: None,
        },
    );
    // self trade 1 BASE @ 1 QUOTE, should cause the whole resting order to be cancelled
    contract.new_order(
        *market.unwrap_id(),
        NewOrderParams {
            limit_price: Some(U128::from(one_quote)),
            max_spend: None,
            quantity: U128::from(one_base),
            side: Side::Sell,
            order_type: OrderType::Limit,
            client_id: None,
            referrer_id: None,
        },
    );
}
