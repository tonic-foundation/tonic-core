use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};

use once_cell::unsync::OnceCell;

use crate::*;

#[derive(
    Copy, Clone, Debug, PartialEq, BorshDeserialize, BorshSerialize, Serialize, Deserialize,
)]
#[serde(crate = "near_sdk::serde")]
#[repr(u8)]
pub enum MarketState {
    /// Market was created but is pending receipt of token decimal info
    Uninitialized,
    /// Market allows trading
    Active,
    /// Market does not allow any trading operations
    Paused,
    /// Market only allows cancelling existing orders
    CancelOnly,
}

#[derive(Clone, Debug, PartialEq, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Token {
    pub token_type: TokenType,
    pub lot_size: u128,
    pub decimals: u8,
}

/// Let
/// * `L_q` = quote lot size
/// * `L_b` = base lot size
/// * `D_b` = base token decimals
///
/// Audit 5.2: Matching in V1 of the orderbook works when the following
/// condition holds:
///
/// ```md
/// L_q * L_b >= 10^{D_b}
/// ```
///
/// This is validated in the `on_ft_metadata` receiver after market creation.
#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub struct Market {
    /// ID of this market, initialized at runtime after loading from trie for
    /// convenience. Not directly serialized to trie.
    #[borsh_skip]
    pub id: OnceCell<MarketId>,

    pub state: MarketState,
    pub base_token: Token,
    pub quote_token: Token,

    pub orderbook: VecOrderbook,

    /// Integer number bps.
    pub taker_fee_base_rate: u8,

    /// Integer number bps.
    pub maker_rebate_base_rate: u8,

    pub max_orders_per_account: u8,

    /// Net taker fees (ie, after maker and referrer rebates) accrued to the
    /// contract, denominated in the quote currency.
    pub fees_accrued: Balance,

    /// Minimum percent of best bid for a new order price, in bps
    pub minimum_bid_bps: u32,

    /// Maximum percent of best ask for a new order price, in bps
    pub maximum_ask_bps: u32,
}

impl Market {
    impl_lazy_accessors!(id, unwrap_id, initialize_id, MarketId);
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub enum PairSide {
    Base,
    Quote,
}

/// Parameters for a new order. Limit price is ignored for market orders.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct NewOrderParams {
    /// Limit price with decimals.
    pub limit_price: Option<U128>,

    /// Maximum amount to spend with decimals (bids only).
    pub max_spend: Option<U128>,

    /// Quantity to buy/sell with decimals.
    pub quantity: U128,

    pub side: Side,

    pub order_type: OrderType,

    /// Optional ID for caller's own accounting.
    pub client_id: Option<u32>,

    /// Account that receives a portion of taker fees if any part of this order
    /// fills immediately.
    pub referrer_id: Option<AccountId>,
}

pub(crate) fn denomination(decimals: u8) -> u128 {
    10u128.pow(decimals.into())
}

impl Market {
    /// See [Market] struct for discussion on valid lot sizes
    pub fn validate_lots_and_decimals(&self) -> bool {
        self.base_token.decimals != INVALID_DECIMALS
            && self.quote_token.decimals != INVALID_DECIMALS
            && U256::from(self.base_token.lot_size) * U256::from(self.quote_token.lot_size)
                >= U256::from(self.base_denomination())
    }

    pub fn base_denomination(&self) -> Balance {
        denomination(self.base_token.decimals)
    }

    pub fn quote_denomination(&self) -> Balance {
        denomination(self.quote_token.decimals)
    }

    pub fn quote_lots_to_native(&self, lots: LotBalance) -> Balance {
        lots as u128 * self.quote_token.lot_size as u128
    }

    pub fn quote_native_to_lots(&self, amount: Balance) -> LotBalance {
        (amount / self.quote_token.lot_size as u128) as u64
    }

    pub fn base_lots_to_native(&self, lots: LotBalance) -> Balance {
        lots as u128 * self.base_token.lot_size as u128
    }

    pub fn base_native_to_lots(&self, amount: Balance) -> LotBalance {
        (amount / self.base_token.lot_size as u128) as u64
    }

    pub fn fee_calculator(&self, account: &AccountV1) -> FeeCalculator {
        FeeCalculator::new(account, self)
    }

    pub fn set_decimals(&mut self, side: PairSide, decimals: u8) {
        match side {
            PairSide::Base => self.base_token.decimals = decimals,
            PairSide::Quote => self.quote_token.decimals = decimals,
        }
    }

    pub fn incr_fees_accrued(&mut self, amount: Balance) {
        self.fees_accrued += amount
    }

    pub fn best_bid(&self) -> Option<OpenLimitOrder> {
        self.orderbook.find_bbo(Side::Buy)
    }

    pub fn best_ask(&self) -> Option<OpenLimitOrder> {
        self.orderbook.find_bbo(Side::Sell)
    }

    pub fn place_order(
        &mut self,
        sequence_number: SequenceNumber,
        owner_id: &AccountId,
        limit_price_lots: Option<LotBalance>,
        max_qty_lots: LotBalance,
        available_quote_lots: Option<LotBalance>,
        side: Side,
        order_type: OrderType,
        client_id: Option<ClientId>,
    ) -> PlaceOrderResult {
        self.assert_active();

        self.orderbook.place_order(
            owner_id,
            NewOrder {
                sequence_number,
                limit_price_lots,
                max_qty_lots,
                available_quote_lots,
                side,
                order_type,
                quote_lot_size: self.quote_token.lot_size,
                base_denomination: self.base_denomination(),
                base_lot_size: self.base_token.lot_size,
                client_id,
            },
        )
    }

    pub fn assert_active(&self) {
        _assert_eq!(
            self.state,
            MarketState::Active,
            "Market must be active to place an order"
        );
    }

    pub fn assert_can_cancel(&self) {
        _assert!(
            self.state == MarketState::Active || self.state == MarketState::CancelOnly,
            "Market must be active or cancel-only to cancel an order"
        );
    }

    pub fn set_state(&mut self, new_state: MarketState) {
        self.state = new_state;
    }
}
