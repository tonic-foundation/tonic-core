use near_sdk::json_types::{U128, U64};

use tonic_dex::*;

mod util;
use util::*;

#[test]
fn get_order() {
    let mut contract = setup_contract();
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
    contract.internal_deposit(&user_a.clone(), &(&usdc).into(), 100);

    set_predecessor_context(user_a.clone());
    let PlaceOrderResultView { id: order_id, .. } = contract.new_order(
        market_id.into(),
        NewOrderParams {
            limit_price: Some(U128::from(10)),
            max_spend: Some(U128::from(50)),
            quantity: U128::from(5),
            side: Side::Buy,
            order_type: OrderType::Limit,
            client_id: None,
            referrer_id: None,
        },
    );

    let order_view = contract.get_order(market_id.clone(), order_id.clone());
    assert!(order_view.is_some(), "existing order wasn't found");
    assert_eq!(
        order_view.clone().unwrap().original_qty.unwrap(),
        U128(5),
        "original size not returned"
    );
    assert_eq!(
        order_view.unwrap().timestamp.unwrap(),
        U64(0), // no block ts set in this test
        "timestamp not returned"
    );
}
