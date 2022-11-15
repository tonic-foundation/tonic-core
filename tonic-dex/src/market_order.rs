/// Implements market orders.
use crate::*;

impl Contract {
    pub fn internal_place_market_order(
        &mut self,
        market: &mut Market,
        taker_account_id: AccountId,
        taker_account: &mut AccountV1,
        params: NewOrderParams,
    ) -> PlaceOrderResult {
        let NewOrderParams {
            side,
            order_type,
            quantity,
            client_id,
            referrer_id,
            max_spend,
            ..
        } = params;
        let max_qty_lots = market.base_native_to_lots(quantity.0);
        if max_qty_lots == 0 {
            env::panic_str(errors::ZERO_ORDER_AMOUNT)
        }
        let fee_calculator = market.fee_calculator(taker_account);
        let available_quote_lots = match side {
            Side::Buy => {
                if let Some(max_spend) = max_spend {
                    let max_spend_after_fees = fee_calculator.withhold_taker_fee(max_spend.into());
                    Some(market.quote_native_to_lots(max_spend_after_fees))
                } else {
                    Some(u64::MAX)
                }
            }
            _ => None,
        };

        // Match orders
        let result = market.place_order(
            self.next_sequence_number(),
            &taker_account_id,
            None,
            max_qty_lots,
            available_quote_lots,
            side,
            order_type,
            client_id,
        );

        let MakerSettlementResult {
            base_traded,
            quote_traded,
            total_maker_rebate,
        } = self.internal_settle_maker_fills(
            market,
            result.id,
            side,
            &result.matches,
            taker_account_id.clone(),
        );
        // taker fee is always collected in quote
        let total_taker_fee = fee_calculator.taker_fee(quote_traded);
        let referrer_rebate = self.internal_settle_referrer_rebate(
            &market.quote_token.token_type,
            referrer_id.clone(),
            total_taker_fee - total_maker_rebate,
        );
        _assert!(
            total_taker_fee >= referrer_rebate + total_maker_rebate,
            "accounting bug: over-counted fees"
        );

        // Settle taker balance changes due to trades
        //
        // Taker fee always comes from the quote token, meaning
        // - when selling, taker receives slightly less of the fill token
        // - when buying, taker spends slightly more of the funding token
        let (input_token, output_token) = match side {
            Side::Buy => (
                market.quote_token.token_type.clone(),
                market.base_token.token_type.clone(),
            ),
            Side::Sell => (
                market.base_token.token_type.clone(),
                market.quote_token.token_type.clone(),
            ),
        };
        let input_debit = match side {
            Side::Buy => quote_traded + total_taker_fee,
            Side::Sell => base_traded,
        };
        let output_credit = match side {
            Side::Buy => base_traded,
            Side::Sell => quote_traded - total_taker_fee,
        };

        if side == Side::Buy {
            _assert!(
                input_debit <= max_spend.map(|o| o.0).unwrap_or(u128::MAX),
                "market order overspent"
            )
        }

        taker_account.withdraw(&input_token, input_debit);
        taker_account.deposit(&output_token, output_credit);
        market.incr_fees_accrued(total_taker_fee - total_maker_rebate - referrer_rebate);

        emit_event(EventType::Order(NewOrderEvent {
            account_id: taker_account_id.clone(),
            market_id: market.unwrap_id(),
            order_id: result.id,
            limit_price: 0.into(),
            price_rank: None,
            quantity,
            side,
            order_type,
            taker_fee: total_taker_fee.into(),
            referrer_id,
            referrer_rebate: referrer_rebate.into(),
            is_swap: false,
            client_id: params.client_id,
        }));

        result
    }
}
