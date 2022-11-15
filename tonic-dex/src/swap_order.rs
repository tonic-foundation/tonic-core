use near_sdk::PromiseOrValue;

/// Implements swaps. Swaps are market orders using the taker's wallet balance.
/// The taker doesn't need an exchange account to swap.
use crate::{errors::EXCEEDED_SLIPPAGE_TOLERANCE, *};

#[derive(Debug)]
pub struct SwapResult {
    pub output_token: TokenType,
    pub output_amount: Balance,
    pub input_refund: Balance,
}

#[near_bindgen]
impl Contract {
    #[payable]
    pub fn swap_near(&mut self, swaps: Vec<SwapAction>) -> PromiseOrValue<U128> {
        self.assert_active();

        let sender_id = env::predecessor_account_id();
        let amount = env::attached_deposit();
        self.execute_swaps(swaps, TokenType::NativeNear, amount, sender_id)
    }
}

impl Contract {
    // Unwraps swap parameters and handles ft_transfers
    pub fn execute_swaps(
        &mut self,
        swaps: Vec<SwapAction>,
        input_token: TokenType,
        input_amount: Balance,
        sender_id: AccountId,
    ) -> PromiseOrValue<U128> {
        _assert!(!swaps.is_empty(), "At least 1 swap action must be provided");

        if swaps.last().unwrap().min_output_token.is_none() {
            env::panic_str("Slippage tolerance must be provided");
        }

        let mut amount = input_amount;
        let mut token = input_token;
        for swap in swaps {
            let result = self.execute_swap_action(swap, token, amount);
            let SwapResult {
                input_refund: _,
                output_token,
                output_amount,
            } = result;
            token = output_token;
            amount = output_amount;
        }
        if amount > 0 {
            self.internal_send(&sender_id, &token, amount);
        }
        PromiseOrValue::Value(U128(0))
    }

    pub fn execute_swap_action(
        &mut self,
        swap: SwapAction,
        token: TokenType,
        amount: Balance,
    ) -> SwapResult {
        let SwapAction {
            market_id,
            side,
            min_output_token,
            referrer_id,
        } = swap;
        let mut market = self.internal_unwrap_market(&market_id);

        if side == Side::Buy {
            assert_eq!(token, market.quote_token.token_type);
        } else {
            assert_eq!(token, market.base_token.token_type);
        }

        let result = self.internal_swap(&mut market, side, amount, referrer_id);

        if let Some(min_out) = min_output_token {
            let amount: u128 = min_out.into();
            _assert!(amount <= result.output_amount, EXCEEDED_SLIPPAGE_TOLERANCE);
        }

        self.internal_save_market(&market_id, market);

        result
    }

    pub fn internal_swap(
        &mut self,
        market: &mut Market,
        side: Side,
        input_amount: u128,
        referrer_id: Option<AccountId>,
    ) -> SwapResult {
        market.assert_active();
        let taker_account_id = env::signer_account_id();
        let fee_calculator = FeeCalculator::new_with_base_rate(market);
        let (quantity, available_quote) = if side == Side::Buy {
            (
                u128::MAX,
                Some(fee_calculator.withhold_taker_fee(input_amount)),
            )
        } else {
            (input_amount, None)
        };

        let result = market.place_order(
            self.next_sequence_number(),
            &taker_account_id,
            None,
            market.base_native_to_lots(quantity),
            available_quote.map(|q| market.quote_native_to_lots(q)),
            side,
            OrderType::Market,
            None,
        );

        let MakerSettlementResult {
            total_maker_rebate,
            base_traded,
            quote_traded,
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
        let net_fees_accrued = total_taker_fee - total_maker_rebate - referrer_rebate;
        market.incr_fees_accrued(net_fees_accrued);

        let output_amount = match side {
            Side::Buy => base_traded,
            Side::Sell => quote_traded - total_taker_fee,
        };
        let input_refund = match side {
            Side::Buy => input_amount - quote_traded - total_taker_fee,
            Side::Sell => quantity - base_traded,
        };

        let output_token = if side == Side::Buy {
            &market.base_token.token_type
        } else {
            &market.quote_token.token_type
        };

        emit_event(EventType::Order(NewOrderEvent {
            account_id: taker_account_id.clone(),
            market_id: market.unwrap_id(),
            price_rank: None,
            order_id: result.id,
            limit_price: 0.into(),
            quantity: quantity.into(),
            side,
            order_type: OrderType::Market,
            taker_fee: total_taker_fee.into(),
            referrer_id,
            referrer_rebate: referrer_rebate.into(),
            is_swap: true,
            client_id: None,
        }));

        SwapResult {
            output_token: output_token.clone(),
            output_amount,
            input_refund,
        }
    }
}
