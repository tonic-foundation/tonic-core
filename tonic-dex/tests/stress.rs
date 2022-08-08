use near_sdk::json_types::U128;
use near_sdk::{AccountId, Balance};

use tonic_dex::*;

mod util;
use util::*;

pub fn get_balance(contract: &Contract, account: &AccountId, token: TokenType) -> Balance {
    contract
        .internal_unwrap_account(account)
        .get_balance(&token)
}

pub fn create_market_and_place_orders(
    contract: &mut Contract,
    market_args: CreateMarketArgs,
    orders: Vec<(AccountId, NewOrderParams)>,
) -> Market {
    let market_id = create_and_init_market(contract, market_args, 16, 18);
    for (user, order) in orders.into_iter() {
        set_predecessor_context(user.clone());
        contract.new_order(market_id.into(), order);
    }
    contract.internal_unwrap_market(&market_id)
}

const BASE_TOKEN_LOT_SIZE: u128 = 1000000000;
const QUOTE_TOKEN_LOT_SIZE: u128 = 100000;

fn new_order_params(
    limit_price_native: u128,
    max_spend: Option<U128>,
    max_qty_native: u128,
    side: Side,
    order_type: OrderType,
    client_id: Option<ClientId>,
    referrer_id: Option<AccountId>,
) -> NewOrderParams {
    NewOrderParams {
        limit_price: Some(limit_price_native.into()),
        max_spend,
        quantity: max_qty_native.into(),
        side,
        order_type,
        client_id,
        referrer_id,
    }
}

#[test]
fn test_stress() {
    let mut contract = setup_contract();
    let one_base = (10 as u128).pow(16);
    let one_quote = (10 as u128).pow(18);
    let one_near = 10_u128.pow(24);
    let (market_creator, _, wnear, usdc) = get_accounts();

    set_deposit_context(market_creator.clone(), deposits::TENTH_NEAR);
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

    // place 3000 orders
    for i in 0..120 {
        let mut actions: Vec<Action> = vec![];

        // use a temp user so that a single user doesn't have too many orders
        let user = AccountId::new_unchecked(format!("{}{:04}", "a".repeat(60), i));
        set_deposit_context(user.clone(), deposits::TENTH_NEAR);
        contract.internal_storage_deposit(&user, false, one_near);
        contract.internal_deposit(&user, &(&usdc).into(), 1_000_000 * one_quote);
        contract.internal_deposit(&user, &(&wnear).into(), 1_000_000 * one_base);

        // start at 1 so the smallest price is 1
        for j in 1..26 {
            actions.push(Action::NewOrder(NewOrderAction {
                market_id: market_id.into(),
                params: new_order_params(
                    // make every price unique to take up max possible space
                    (i * 100 + j) * one_quote,
                    Some(U128(one_quote)),
                    one_base,
                    Side::Sell,
                    OrderType::Limit,
                    None,
                    None,
                ),
            }))
        }
        set_deposit_context(user.clone(), 1);
        contract.execute(actions);
    }

    println!(
        "{:?}",
        contract.get_orderbook(market_id.into(), 10, None).unwrap()
    );
}
