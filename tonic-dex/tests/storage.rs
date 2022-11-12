mod util;
use near_sdk::AccountId;
use tonic_dex::*;
use util::*;

/// Basically just measures storage use
#[test]
fn storage_measurement_cleanup() {
    let mut contract = setup_contract();
    let storage_increase = measure_storage_increase!({
        // setup
        let account_id = AccountId::new_unchecked("0".repeat(64));
        let account_usage = measure_storage_increase!({
            storage_deposit(&mut contract, &account_id);
        });

        let market_id = MarketId::new_unchecked(&b"m".repeat(64));
        let base_token_id = AccountId::new_unchecked("b".repeat(64));
        let quote_token_id = AccountId::new_unchecked("q".repeat(64));
        contract.internal_save_market(
            &market_id,
            Market {
                id: None,
                base_token: Token {
                    token_type: base_token_id.clone().into(),
                    lot_size: 1,
                    decimals: 0,
                },
                quote_token: Token {
                    token_type: quote_token_id.clone().into(),
                    lot_size: 1,
                    decimals: 0,
                },
                orderbook: Orderbook::default(),
                state: MarketState::Active,
                fees_accrued: 0,
                taker_fee_base_rate: 0,
                maker_rebate_base_rate: 0,
                max_orders_per_account: 10,
                minimum_bid_bps: 1000,
                maximum_ask_bps: 30000,
            },
        );

        // deposit token so we can place an order
        let mut account = contract.internal_unwrap_account(&account_id);
        account.deposit(&(&quote_token_id).into(), 2);
        contract.internal_save_account(&account_id, account);

        let mut market = contract.internal_unwrap_market(&market_id);
        let order_usage = measure_storage_increase!({
            market.orderbook.bids.save_order(OpenLimitOrder {
                client_id: None,
                limit_price_lots: 1.into(),
                open_qty_lots: 1,
                owner_id: account_id.clone(),
                sequence_number: 1,
                side: Side::Buy.into(),
                price_rank: None, // doesn't matter
            });
            contract.internal_save_market(&market_id, market);
        });

        // teardown
        contract.internal_cancel_all_orders(&market_id, account_id.clone());
        contract.internal_unregister_account(&account_id, true);
        contract.markets.remove(&market_id);

        println!(
            "STORAGE USAGE: account {} order {}",
            account_usage, order_usage
        );
    });

    assert_eq!(storage_increase, 0, "storage bug: leak during test");
}

#[test]
#[should_panic(expected = "E12: insufficient storage balance")]
fn storage_registration_only() {
    let mut contract = setup_contract();
    // setup
    let account_id = AccountId::new_unchecked("0".repeat(64));
    storage_deposit_registration_only(&mut contract, &account_id);

    let token_id = AccountId::new_unchecked("q".repeat(64));

    // This should fail since we only deposited minimal storage, which does not cover
    // cost of storing token balances.
    let mut account = contract.internal_unwrap_account(&account_id);
    account.deposit(&(&token_id).into(), 2);
    contract.internal_save_account(&account_id, account);
}
