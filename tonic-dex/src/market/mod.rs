use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use once_cell::unsync::OnceCell;

use tonic_sdk::measure_gas;

use crate::*;

pub mod v1;
pub use v1::*;

/// Market creation depends on a promise chain. This is used as a default value
/// while waiting for the data to come back.
pub const INVALID_DECIMALS: u8 = 100;

pub const DEFAULT_MAX_ORDERS: u8 = 20;
pub const DEFAULT_MIN_MULTIPLIER_BPS: u32 = 1_000; // 10%
pub const DEFAULT_MAX_MULTIPLIER_BPS: u32 = 300_000; // 3000%

#[derive(BorshDeserialize, BorshSerialize)]
pub enum VMarket {
    Current(Market),
}

impl From<VMarket> for Market {
    fn from(v: VMarket) -> Self {
        match v {
            VMarket::Current(a) => a,
        }
    }
}

impl From<Market> for VMarket {
    fn from(a: Market) -> Self {
        Self::Current(a)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct CreateMarketArgs {
    pub base_token: String,
    pub quote_token: String,
    pub base_token_lot_size: U128,
    pub quote_token_lot_size: U128,
    pub taker_fee_base_rate: u8,
    pub maker_rebate_base_rate: u8,
}

fn gen_market_id(
    base_token: TokenType,
    base_lot_size: u128,
    quote_token: TokenType,
    quote_lot_size: u128,
) -> MarketId {
    let base = format!(
        "{} {} {} {} {}",
        env::current_account_id(),
        base_token.key(),
        base_lot_size,
        quote_token.key(),
        quote_lot_size
    );
    MarketId(near_sdk::env::sha256_array(base.as_bytes()))
}

#[near_bindgen]
impl Contract {
    /// Create a market. Since markets can never be deleted, storage deposits
    /// for markets cannot be recovered.
    #[payable]
    pub fn create_market(&mut self, args: CreateMarketArgs) -> MarketId {
        self.assert_is_owner();
        // NB: if making this permissionless, need to assert contract active here

        _assert!(
            args.maker_rebate_base_rate < args.taker_fee_base_rate
                || (args.maker_rebate_base_rate == 0 && args.taker_fee_base_rate == 0),
            "maker rebate rate must be less than taker fee rate"
        );

        _assert!(
            args.base_token != args.quote_token,
            "base and quote tokens should be different"
        );

        let base_token = TokenType::from_key(&args.base_token);
        let quote_token = TokenType::from_key(&args.quote_token);
        let market_id = gen_market_id(
            base_token.clone(),
            args.base_token_lot_size.0,
            quote_token.clone(),
            args.quote_token_lot_size.0,
        );
        if self.markets.contains_key(&market_id) {
            env::panic_str(errors::MARKET_EXISTS);
        }

        self.assert_valid_lot_sizes(args.base_token_lot_size.0, args.quote_token_lot_size.0);
        // If either token is native NEAR, we know it has 24 decimals. If not,
        // use an invalid default as a placeholder and set the token's decimals
        // in ft_metadata callback.
        let base_decimals = self
            .get_decimals_for_token(&market_id, PairSide::Base, &base_token)
            .unwrap_or(INVALID_DECIMALS);
        let quote_decimals = self
            .get_decimals_for_token(&market_id, PairSide::Quote, &quote_token)
            .unwrap_or(INVALID_DECIMALS);

        let storage_increase = measure_storage_increase!({
            self.internal_save_market(
                &market_id,
                Market {
                    id: OnceCell::new(),
                    base_token: Token {
                        token_type: base_token.clone(),
                        lot_size: args.base_token_lot_size.0,
                        decimals: base_decimals,
                    },
                    quote_token: Token {
                        token_type: quote_token.clone(),
                        lot_size: args.quote_token_lot_size.0,
                        decimals: quote_decimals,
                    },
                    orderbook: Orderbook::default(),
                    state: MarketState::Uninitialized,
                    fees_accrued: 0,
                    taker_fee_base_rate: args.taker_fee_base_rate,
                    maker_rebate_base_rate: args.maker_rebate_base_rate,
                    max_orders_per_account: DEFAULT_MAX_ORDERS,
                    minimum_bid_bps: DEFAULT_MIN_MULTIPLIER_BPS,
                    maximum_ask_bps: DEFAULT_MAX_MULTIPLIER_BPS,
                },
            );
            self.market_iter_map.push(&market_id);
        });

        let attached_deposit = env::attached_deposit();
        let deposit_used = Balance::from(storage_increase) * env::storage_byte_cost();
        let refund = _expect!(
            attached_deposit.checked_sub(deposit_used),
            errors::INSUFFICIENT_MARKET_DEPOSIT
        );
        if refund > 0 {
            Promise::new(env::predecessor_account_id()).transfer(refund);
        }

        emit_event(EventType::NewMarket(NewMarketEvent {
            creator_id: env::predecessor_account_id(),
            market_id,
            base_token,
            quote_token,
        }));

        market_id
    }

    /// Cancel all orders in a given market owned by the account.
    #[measure_gas(feature = "measure_gas")]
    pub fn cancel_all_orders(&mut self, market_id: MarketId) -> Vec<OrderId> {
        self.assert_active();
        self.assert_can_cancel();

        let account_id = env::predecessor_account_id();
        self.internal_cancel_all_orders(&market_id, account_id)
    }

    /// Cancel account's order in a market.
    #[measure_gas(feature = "measure_gas")]
    pub fn cancel_order(&mut self, market_id: MarketId, order_id: OrderId) {
        self.assert_can_cancel();
        let account_id = env::predecessor_account_id();
        self.internal_cancel_order(market_id, account_id, order_id);
    }

    /// Place a new order.
    #[measure_gas(feature = "measure_gas")]
    pub fn new_order(
        &mut self,
        market_id: MarketId,
        order: NewOrderParams,
    ) -> PlaceOrderResultView {
        self.assert_active();
        self.assert_valid_order(&order);
        let mut market = self.internal_unwrap_market(&market_id);
        market.assert_active();

        let taker_account_id = env::predecessor_account_id();
        let mut taker_account = self.internal_unwrap_account(&taker_account_id);

        let result = if order.order_type == OrderType::Market {
            self.internal_place_market_order(
                &mut market,
                taker_account_id.clone(),
                &mut taker_account,
                order,
            )
        } else {
            match order.side {
                Side::Buy => self.internal_place_limit_buy(
                    &mut market,
                    taker_account_id.clone(),
                    &mut taker_account,
                    order,
                ),
                Side::Sell => self.internal_place_limit_sell(
                    &mut market,
                    taker_account_id.clone(),
                    &mut taker_account,
                    order,
                ),
            }
        };

        let ret = result.into_view(market.base_token.lot_size, market.quote_token.lot_size);

        self.internal_save_account(&taker_account_id, taker_account);
        self.internal_save_market(&market_id, market);

        ret
    }
}

impl Contract {
    pub fn internal_save_market(&mut self, id: &MarketId, market: Market) {
        self.markets.insert(id, &market.into());
    }

    pub fn internal_get_market(&self, id: &MarketId) -> Option<Market> {
        self.markets.get(id).map(|o| {
            let mut m: Market = o.into();
            // none of market, orderbook, nor L2 store the market_id on-trie in
            // their own structs, but need them at runtime
            m.initialize_id(*id);
            m
        })
    }

    pub fn internal_unwrap_market(&self, id: &MarketId) -> Market {
        self.internal_get_market(id).unwrap()
    }

    // NB: no longer used; invalid/disabled markets are periodically
    // swept with admin_delete_market
    // pub fn internal_remove_market(&mut self, id: &MarketId) {
    //     let market = self.internal_unwrap_market(id);
    //     require!(
    //         market.state != MarketState::Active,
    //         "Cannot delete active market"
    //     );
    // }

    /// Panic if lot sizes are not powers of 10.
    fn assert_valid_lot_sizes(&self, base_token_lot_size: u128, quote_token_lot_size: u128) {
        require!(
            base_token_lot_size % 10 == 0 || base_token_lot_size == 1,
            "Invalid base lot size"
        );
        require!(
            quote_token_lot_size % 10 == 0 || quote_token_lot_size == 1,
            "Invalid quote lot size"
        );
        if base_token_lot_size == 0 {
            env::panic_str(errors::INVALID_BASE_LOT_SIZE);
        }
        if quote_token_lot_size == 0 {
            env::panic_str(errors::INVALID_QUOTE_LOT_SIZE);
        }
    }

    fn assert_valid_order(&self, order: &NewOrderParams) {
        if let Some(limit_price) = order.limit_price {
            require!(
                u128::from(limit_price) > 0,
                "Limit price must be greater than 0"
            );
        }
        if let Some(max_spend) = order.max_spend {
            require!(
                u128::from(max_spend) > 0,
                "Max spend must be greater than 0"
            );
        }
        require!(
            u128::from(order.quantity) > 0,
            "Quantity must be greater than 0"
        );
    }

    /// Return Some if the decimals are immediately known, None if it'll get set
    /// in a promise callback.
    fn get_decimals_for_token(
        &self,
        market_id: &MarketId,
        pair_side: PairSide,
        token: &TokenType,
    ) -> Option<u8> {
        match token {
            TokenType::NativeNear => Some(24),
            TokenType::FungibleToken { account_id } => {
                ext_ft::ft_metadata(account_id.clone(), NO_DEPOSIT, GAS_FT_METADATA_READ).then(
                    ext_self::on_ft_metadata(
                        *market_id,
                        pair_side,
                        env::current_account_id(),
                        NO_DEPOSIT,
                        GAS_FT_METADATA_WRITE,
                    ),
                );
                None
            }
            TokenType::MultiFungibleToken {
                account_id,
                subtoken_id,
            } => {
                ext_mt::mt_metadata_base_by_token_id(
                    vec![subtoken_id.clone()],
                    account_id.clone(),
                    NO_DEPOSIT,
                    GAS_MT_METADATA_READ,
                )
                .then(ext_self::on_mt_metadata(
                    *market_id,
                    pair_side,
                    env::current_account_id(),
                    NO_DEPOSIT,
                    GAS_MT_METADATA_WRITE,
                ));
                None
            }
        }
    }

    pub fn internal_clear_orderbook_orders(
        &mut self,
        market_id: &MarketId,
        limit: Option<u16>,
    ) -> Vec<OrderId> {
        let limit = limit.unwrap_or(u16::MAX);
        let market = self.internal_unwrap_market(market_id);
        market.assert_can_cancel();
        let orderbook = market.orderbook;
        orderbook
            .asks
            .iter()
            .chain(orderbook.bids.iter())
            .take(limit.into())
            .map(|open_order| {
                let account_id = open_order.clone().owner_id;
                let order_id = open_order.id();
                self.internal_cancel_order(*market_id, account_id, order_id);
                order_id
            })
            .collect()
    }

    /// Cancel all orders owned by account in this market
    pub fn internal_cancel_all_orders(
        &mut self,
        market_id: &MarketId,
        account_id: AccountId,
    ) -> Vec<OrderId> {
        let mut market = self.internal_unwrap_market(market_id);
        market.assert_can_cancel();
        let mut account = self.internal_unwrap_account(&account_id);
        let order_ids = account.remove_all_order_infos(market_id);
        let orders = market.orderbook.cancel_orders(order_ids.clone());

        let cancels = process_refunds(&market, &mut account, orders);
        self.internal_save_market(market_id, market);
        self.internal_save_account(&account_id, account);
        emit_event(EventType::Cancel(NewCancelEvent {
            market_id: *market_id,
            cancels,
        }));

        order_ids
    }

    /// Cancel an order
    pub fn internal_cancel_order(
        &mut self,
        market_id: MarketId,
        account_id: AccountId,
        order_id: OrderId,
    ) {
        let mut market = self.internal_unwrap_market(&market_id);
        market.assert_can_cancel();
        let mut account = self.internal_unwrap_account(&account_id);
        match account.remove_order_info(&market_id, order_id) {
            Some(_) => {
                let order = _expect!(
                    market.orderbook.cancel_order(order_id),
                    errors::ORDER_NOT_FOUND
                ); // should never happen
                let cancels = process_refunds(&market, &mut account, vec![order]);
                self.internal_save_market(&market_id, market);
                self.internal_save_account(&account_id, account);
                emit_event(EventType::Cancel(NewCancelEvent { market_id, cancels }))
            }
            _ => env::panic_str(errors::ORDER_NOT_FOUND),
        }
    }
}

/// Credit the account for any open orders. Does not save the account yet.
fn process_refunds(
    market: &Market,
    account: &mut AccountV1,
    orders: Vec<OpenLimitOrder>,
) -> Vec<CancelEventData> {
    let mut cancels: Vec<CancelEventData> = vec![];

    for order in orders {
        _assert_eq!(
            &order.owner_id,
            account.unwrap_id(),
            "order is not owned by account"
        );
        let (refund_amount, token) = get_refund_amount(market, &order);
        account.deposit(&token, refund_amount);
        account.remove_order_info(market.unwrap_id(), order.id());

        cancels.push(CancelEventData {
            order_id: order.id(),
            refund_amount: refund_amount.into(),
            refund_token: token,
            cancelled_qty: market.base_lots_to_native(order.open_qty_lots).into(),
        });
    }

    cancels
}

/// Return the amount and token type to refund after cancelling the order.
fn get_refund_amount(market: &Market, order: &OpenLimitOrder) -> (Balance, TokenType) {
    match order.unwrap_side() {
        Side::Buy => {
            let base_denomination = market.base_denomination();
            let refund_amount = ({
                U256::from(order.open_qty_lots)
                    * U256::from(*order.unwrap_price())
                    * U256::from(market.quote_token.lot_size)
                    * U256::from(market.base_token.lot_size)
                    / U256::from(base_denomination)
            })
            .as_u128();
            (refund_amount, market.quote_token.token_type.clone())
        }
        Side::Sell => (
            (order.open_qty_lots as u128)
                .checked_mul(market.base_token.lot_size)
                .unwrap(),
            market.base_token.token_type.clone(),
        ),
    }
}
