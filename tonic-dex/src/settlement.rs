/// Helper methods for settling trades.
use crate::*;

/// Result from settling the maker side of fills. Balance changes due to trading
/// are settled in two stages: makers first, then taker.
#[derive(Debug, Default)]
pub struct MakerSettlementResult {
    /// Total amount of base traded
    pub base_traded: Balance,
    /// Total amount of quote traded
    pub quote_traded: Balance,
    pub total_maker_rebate: Balance,
}

impl Contract {
    /// Settle maker account balances based on matched orders.
    ///
    /// The caller is responsible for settling taker balances. This makes the
    /// code more complex but saves redundant writes to the taker account.
    pub fn internal_settle_maker_fills(
        &mut self,
        market: &Market,
        order_id: OrderId,
        side: Side,
        matches: &[Match],
        taker_account_id: AccountId,
    ) -> MakerSettlementResult {
        if matches.is_empty() {
            return MakerSettlementResult::default();
        }
        let base_lot_size = market.base_token.lot_size as u128;
        let quote_lot_size = market.quote_token.lot_size as u128;

        let mut total_maker_rebate: Balance = 0;
        let mut base_traded: Balance = 0; // amount of base purchased in bid, amount sold in ask
        let mut quote_traded: Balance = 0; // amount of quote spent in bid, amount received in ask

        let mut fills: Vec<FillEventData> = vec![];

        for fill in matches.iter() {
            let native_fill_price = (fill.fill_price_lots as u128) * quote_lot_size;
            let native_fill_qty = (fill.fill_qty_lots as u128) * base_lot_size;
            base_traded += native_fill_qty;
            quote_traded += fill.native_quote_paid;

            let mut maker_account = self.internal_unwrap_account(&fill.maker_user_id);
            if fill.did_remove_maker_order() {
                maker_account.remove_order_info(market.unwrap_id(), fill.maker_order_id);
            }

            let fee_calculator = FeeCalculator::new(&maker_account, market);
            let native_maker_rebate = fee_calculator.maker_rebate(fill.native_quote_paid);
            total_maker_rebate += native_maker_rebate;
            maker_account.deposit(&market.quote_token.token_type, native_maker_rebate);

            match side {
                Side::Buy => {
                    // taker paid quote
                    maker_account.deposit(&market.quote_token.token_type, fill.native_quote_paid);
                }
                Side::Sell => {
                    // taker sold base
                    maker_account.deposit(&market.base_token.token_type, native_fill_qty);
                }
            }

            self.internal_save_account(&fill.maker_user_id, maker_account);

            fills.push(FillEventData {
                fill_qty: native_fill_qty.into(),
                fill_price: native_fill_price.into(),
                quote_qty: fill.native_quote_paid.into(),
                maker_order_id: fill.maker_order_id,
                maker_rebate: native_maker_rebate.into(),
                side,
                taker_account_id: taker_account_id.clone(),
                maker_account_id: fill.maker_user_id.clone(),
            });
        }

        emit_event(EventType::Fill(NewFillEvent {
            fills,
            market_id: *market.unwrap_id(),
            order_id,
        }));

        MakerSettlementResult {
            base_traded,
            quote_traded,
            total_maker_rebate,
        }
    }

    /// Settle referrer rebate. Return amount rebated.
    pub fn internal_settle_referrer_rebate(
        &mut self,
        quote_token: &TokenType,
        referrer_id: Option<AccountId>,
        taker_fee_less_maker_rebate: Balance,
    ) -> Balance {
        let mut referrer_rebate: u128 = 0;
        if let Some(id) = referrer_id {
            if let Some(mut referrer_account) = self.internal_get_account(&id) {
                referrer_rebate = fees::referrer_rebate(taker_fee_less_maker_rebate);
                referrer_account.deposit(quote_token, referrer_rebate);
                if let Err(()) = self.internal_try_save_account(&id, referrer_account) {
                    // Insufficient storage balance to store the rebate token
                    referrer_rebate = 0;
                };
            }
            // leaving referrer rebate as 0 will accrue all fees to the contract
        };
        referrer_rebate
    }
}
