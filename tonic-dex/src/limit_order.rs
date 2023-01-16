/// Implements limit orders.
use crate::*;

impl Market {
    /// Return max spend if explicitly provided, otherwise compute it from the
    /// volume implied by the order price * quantity. Note: the fee must come
    /// out of this value, eg, a client placing a bid for 1000 NEAR@1 USN in a
    /// market with a 0.1% taker fee with no specified max spend can actually
    /// buy only 999 NEAR. To buy 1000 NEAR, they would need to specify a max
    /// spend of 1001.001001 USN.
    fn get_max_bid_debit(&self, params: &NewOrderParams) -> Balance {
        params.max_spend.map(|q| q.0).unwrap_or_else(|| {
            let limit_price = _expect!(params.limit_price, errors::MISSING_LIMIT_PRICE).0;
            let quantity = params.quantity.0;
            (U256::from(quantity) * U256::from(limit_price) / self.base_denomination()).as_u128()
        })
    }
}

impl Contract {
    /// Place a limit buy order. This method runs the matching engine, settles
    /// balance transfers between the maker and taker, and adds fees to the
    /// market. The caller is responsible for saving the taker account and the
    /// market to save gas. Maker accounts are saved in the function body.
    ///
    /// The caller optionally specifies the maximum amount they are willing to
    /// spend using the `max_spend` field of [NewOrderParams]. If it's not
    /// specified, it's computed as `limit price * quantity`.
    ///
    /// Note: the max spend must account for the taker fee, ie, if a market has
    /// a 0.1% taker fee, in order to place an order worth 1000 USN, the caller
    /// must specify a max spend of 1001.001001 USN.
    pub fn internal_place_limit_buy(
        &mut self,
        market: &mut Market,
        taker_account_id: AccountId,
        taker_account: &mut AccountV1,
        params: NewOrderParams,
    ) -> PlaceOrderResult {
        let fee_calculator = market.fee_calculator(taker_account);
        let base_lot_size = market.base_token.lot_size;
        let quote_lot_size = market.quote_token.lot_size;
        if let Some(best_bid) = market.best_bid() {
            let limit_price = params.limit_price.unwrap().0;
            require!(
                (limit_price * 10_000) / market.quote_lots_to_native(best_bid.unwrap_price())
                    >= market.minimum_bid_bps.into(),
                "Bid outside of market trading window"
            );
        }

        // Get amount of quote available for matching (ie, max spend less fees)
        let max_quote_debit = market.get_max_bid_debit(&params);
        let available_quote_lots =
            (fee_calculator.withhold_taker_fee(max_quote_debit) / quote_lot_size as u128) as u64;
        if available_quote_lots == 0 {
            env::panic_str(errors::ZERO_ORDER_AMOUNT)
        }

        // Get max buy amount based on quote available for matching
        let NewOrderParams {
            side,
            order_type,
            limit_price,
            quantity,
            client_id,
            referrer_id,
            ..
        } = params;
        let limit_price = _expect!(limit_price, errors::MISSING_LIMIT_PRICE).0;
        let limit_price_lots = market.quote_native_to_lots(limit_price);
        // min(specified buy amount, available quote / price)
        let quantity_lots = market.base_native_to_lots(quantity.0).min(
            (U256::from(market.quote_lots_to_native(available_quote_lots))
                * U256::from(market.base_denomination())
                / U256::from(limit_price)
                / U256::from(base_lot_size))
            .as_u64(),
        );

        // Match orders
        let result = market.place_order(
            self.next_sequence_number(),
            &taker_account_id,
            Some(limit_price_lots),
            quantity_lots,
            Some(available_quote_lots),
            side,
            order_type,
            client_id,
        );

        // Settle maker balance changes first. We do balance transfers in two
        // steps to reduce redundant taker account writes.
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

        let total_taker_fee = fee_calculator.taker_fee(quote_traded);
        let referrer_rebate = self.internal_settle_referrer_rebate(
            &market.quote_token.token_type,
            referrer_id.clone(),
            total_taker_fee - total_maker_rebate,
        );
        _assert!(
            total_taker_fee >= referrer_rebate + total_maker_rebate,
            "bid accounting bug: over-counted fees"
        );
        market.incr_fees_accrued(total_taker_fee - total_maker_rebate - referrer_rebate);

        // Settle taker balance changes. Start by crediting any base purchased
        // due to crossing the spread, then debit the amount of quote spent and
        // amount locked in the open order.
        //
        // Credit base purchased
        taker_account.deposit(&market.base_token.token_type, base_traded);

        // Debit quote spent and quote locked
        let quote_locked = (U256::from(result.open_qty_lots)
            * U256::from(base_lot_size)
            * U256::from(limit_price)
            / U256::from(market.base_denomination()))
        .as_u128();
        let total_quote_debit = quote_locked + quote_traded + total_taker_fee;
        _assert!(total_quote_debit <= max_quote_debit, "bid bug: overspent");
        taker_account.withdraw(&market.quote_token.token_type, total_quote_debit);

        // Save the taker's newly posted order on their account
        if result.is_posted() {
            taker_account.save_new_order_info(
                &market.unwrap_id().clone(),
                result.id,
                quantity_lots,
                market.max_orders_per_account as usize,
            );
        }

        emit_event(EventType::Order(NewOrderEvent {
            account_id: taker_account_id.clone(),
            market_id: market.unwrap_id(),
            order_id: result.id,
            open_quantity: Some(U128::from(market.base_lots_to_native(result.open_qty_lots))),
            limit_price: limit_price.into(),
            price_rank: result.price_rank,
            quantity,
            side,
            order_type,
            taker_fee: total_taker_fee.into(),
            referrer_id,
            referrer_rebate: referrer_rebate.into(),
            is_swap: false,
            client_id: params.client_id,
            best_bid: result
                .best_bid
                .map(|p| U128::from(market.quote_lots_to_native(p))),
            best_ask: result
                .best_ask
                .map(|p| U128::from(market.quote_lots_to_native(p))),
        }));

        result
    }

    /// Place a limit sell order. This method runs the matching engine, settles
    /// balance transfers between the maker and taker, and adds fees to the
    /// market. The caller is responsible for saving the taker account and the
    /// market to save gas. Maker accounts are saved in the function body.
    pub fn internal_place_limit_sell(
        &mut self,
        market: &mut Market,
        taker_account_id: AccountId,
        taker_account: &mut AccountV1,
        params: NewOrderParams,
    ) -> PlaceOrderResult {
        let fee_calculator = market.fee_calculator(taker_account);
        if let Some(best_ask) = market.best_ask() {
            let limit_price = params.limit_price.unwrap().0;
            require!(
                (limit_price * 10_000) / market.quote_lots_to_native(best_ask.unwrap_price())
                    <= market.maximum_ask_bps.into(),
                "Ask outside of market trading window"
            );
        }

        let NewOrderParams {
            side,
            order_type,
            limit_price,
            quantity,
            client_id,
            referrer_id,
            ..
        } = params;
        let max_base_debit = quantity.0;
        if max_base_debit == 0 {
            env::panic_str(errors::ZERO_ORDER_AMOUNT)
        }

        let quantity_lots = market.base_native_to_lots(quantity.0);
        let limit_price = _expect!(limit_price, errors::MISSING_LIMIT_PRICE).0;
        let limit_price_lots = market.quote_native_to_lots(limit_price);

        let result = market.place_order(
            self.next_sequence_number(),
            &taker_account_id,
            Some(limit_price_lots),
            quantity_lots,
            None,
            side,
            order_type,
            client_id,
        );

        // Settle maker balance changes first. We do balance transfers in two
        // steps to reduce redundant taker account writes.
        let MakerSettlementResult {
            quote_traded,
            base_traded,
            total_maker_rebate,
        } = self.internal_settle_maker_fills(
            market,
            result.id,
            side,
            &result.matches,
            taker_account_id.clone(),
        );

        // Transfer referrer rebate
        let total_taker_fee = fee_calculator.taker_fee(quote_traded);
        let referrer_rebate = self.internal_settle_referrer_rebate(
            &market.quote_token.token_type,
            referrer_id.clone(),
            total_taker_fee - total_maker_rebate,
        );

        // Accrue net taker fee to the market
        _assert!(
            total_taker_fee >= referrer_rebate + total_maker_rebate,
            "ask accounting bug: over-counted fees"
        );
        market.incr_fees_accrued(total_taker_fee - total_maker_rebate - referrer_rebate);

        // Settle taker balance changes. Start by crediting taker with quote
        // from crossing the spread, then debit amount of base sold and amount
        // of base locked in the open order.
        //
        // Start with the quote credit
        taker_account.deposit(
            &market.quote_token.token_type,
            quote_traded - total_taker_fee,
        );

        // Debit base sold and base locked in order
        let base_locked = market.base_lots_to_native(result.open_qty_lots);
        let total_base_debit = base_traded + base_locked;
        _assert!(total_base_debit <= max_base_debit, "ask bug: oversold");
        taker_account.withdraw(&market.base_token.token_type, total_base_debit);

        // Save the taker's newly posted order on their account
        if result.is_posted() {
            taker_account.save_new_order_info(
                &market.unwrap_id().clone(),
                result.id,
                quantity_lots,
                market.max_orders_per_account as usize,
            );
        }

        emit_event(EventType::Order(NewOrderEvent {
            account_id: taker_account_id.clone(),
            market_id: market.unwrap_id(),
            order_id: result.id,
            open_quantity: Some(U128::from(market.base_lots_to_native(result.open_qty_lots))),
            limit_price: limit_price.into(),
            price_rank: result.price_rank,
            quantity,
            side,
            order_type,
            taker_fee: total_taker_fee.into(),
            referrer_id,
            referrer_rebate: referrer_rebate.into(),
            is_swap: false,
            client_id: params.client_id,
            best_bid: result
                .best_bid
                .map(|p| U128::from(market.quote_lots_to_native(p))),
            best_ask: result
                .best_ask
                .map(|p| U128::from(market.quote_lots_to_native(p))),
        }));

        result
    }
}
