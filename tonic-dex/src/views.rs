use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    json_types::{U128, U64},
    serde::{Deserialize, Serialize},
    Timestamp,
};

use crate::market::MarketState;
use crate::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct MarketView {
    pub id: MarketId,
    pub base_token: TokenView,
    pub quote_token: TokenView,
    pub orderbook: OrderbookView,
    pub maker_rebate_base_rate: u8,
    pub taker_fee_base_rate: u8,
    pub fees_accrued: U128,
    pub max_orders_per_account: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_orders: Option<u32>,
    pub state: MarketState,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenView {
    pub token_type: TokenType,
    pub lot_size: U128,
    pub decimals: u8,
}

impl From<Token> for TokenView {
    fn from(t: Token) -> Self {
        Self {
            token_type: t.token_type,
            decimals: t.decimals,
            lot_size: t.lot_size.into(),
        }
    }
}

impl Market {
    pub fn total_orders(&self) -> u32 {
        let total_bids = self.orderbook.bids.iter().map(|_| 1u32).sum::<u32>();
        let total_asks = self.orderbook.asks.iter().map(|_| 1u32).sum::<u32>();
        total_bids + total_asks
    }

    pub fn to_view(&self, price_depth: u8, show_total: bool) -> MarketView {
        MarketView {
            id: self.unwrap_id(),
            base_token: self.base_token.clone().into(),
            quote_token: self.quote_token.clone().into(),
            orderbook: orderbook_to_view(
                &self.orderbook,
                price_depth,
                self.base_token.lot_size,
                self.quote_token.lot_size,
                false,
                false,
                false,
            ),
            maker_rebate_base_rate: self.maker_rebate_base_rate,
            taker_fee_base_rate: self.taker_fee_base_rate,
            fees_accrued: self.fees_accrued.into(),
            max_orders_per_account: self.max_orders_per_account,
            total_orders: if show_total {
                Some(self.total_orders())
            } else {
                None
            },
            state: self.state,
        }
    }
}

#[derive(Clone, Debug, PartialEq, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct OpenLimitOrderView {
    pub id: OrderId,
    pub limit_price: U128,
    pub open_qty: U128,
    /// The original size of an order may or may not be returned, depending on
    /// how it's requested. It's generally not available when fetching orders
    /// straight directly by ID---since the original size isn't relevant to
    /// order matching, it's not stored on the orderbook.
    ///
    /// The [Account] struct keeps track of the original size of its orders; the
    /// original size is known when fetching orders owned by a given account.
    pub original_qty: Option<U128>,
    pub side: Side,
    pub timestamp: Option<U64>,
    pub client_id: Option<ClientId>,
}

pub fn order_to_view(
    order: &OpenLimitOrder,
    base_lot_size: u128,
    quote_lot_size: u128,
    original_qty_lots: Option<LotBalance>,
    timestamp: Option<Timestamp>,
) -> OpenLimitOrderView {
    let original_qty = original_qty_lots.map(|q| U128::from(q as u128 * base_lot_size));
    OpenLimitOrderView {
        id: order.id(),
        limit_price: (order.unwrap_price() as u128 * quote_lot_size).into(),
        open_qty: (order.open_qty_lots as u128 * base_lot_size).into(),
        original_qty,
        timestamp: timestamp.map(|t| t.into()),
        side: order.unwrap_side(),
        client_id: order.client_id,
    }
}

#[derive(Clone, Debug, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct L2OpenLimitOrderView {
    pub limit_price: U128,
    pub open_quantity: U128,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<AccountId>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<OrderId>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<u32>,
}

pub fn order_to_l2_view(
    order: &OpenLimitOrder,
    base_lot_size: u128,
    quote_lot_size: u128,
    show_owner: bool,
    show_order_id: bool,
    show_client_id: bool,
) -> L2OpenLimitOrderView {
    L2OpenLimitOrderView {
        limit_price: (order.unwrap_price() as u128 * quote_lot_size).into(),
        open_quantity: (order.open_qty_lots as u128 * base_lot_size).into(),
        owner: if show_owner {
            Some(order.owner_id.clone())
        } else {
            None
        },
        order_id: if show_order_id {
            Some(order.id())
        } else {
            None
        },
        client_id: if show_client_id {
            order.client_id
        } else {
            None
        },
    }
}

#[derive(Default, Clone, Debug, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct OrderbookView {
    pub bids: Vec<L2OpenLimitOrderView>,
    pub asks: Vec<L2OpenLimitOrderView>,
}

pub fn orderbook_to_view(
    ob: &VecOrderbook,
    price_depth: u8,
    base_lot_size: u128,
    quote_lot_size: u128,
    show_owner: bool,
    show_order_id: bool,
    show_client_id: bool,
) -> OrderbookView {
    OrderbookView {
        bids: ob
            .bids
            .take_depth(price_depth as usize)
            .iter()
            .flat_map(|(_, o)| o.iter())
            .map(|o| {
                order_to_l2_view(
                    o,
                    base_lot_size,
                    quote_lot_size,
                    show_owner,
                    show_order_id,
                    show_client_id,
                )
            })
            .collect::<Vec<L2OpenLimitOrderView>>(),
        asks: ob
            .asks
            .take_depth(price_depth as usize)
            .iter()
            .flat_map(|(_, o)| o.iter())
            .map(|o| {
                order_to_l2_view(
                    o,
                    base_lot_size,
                    quote_lot_size,
                    show_owner,
                    show_order_id,
                    show_client_id,
                )
            })
            .collect::<Vec<L2OpenLimitOrderView>>(),
    }
}

#[near_bindgen]
impl Contract {
    pub fn get_market(&self, market_id: MarketId, show_total: Option<bool>) -> Option<MarketView> {
        self.internal_get_market(&market_id)
            .map(|m| m.to_view(8, show_total.unwrap_or(false)))
    }

    pub fn get_orderbook(
        &self,
        market_id: MarketId,
        depth: u8,
        show_owner: Option<bool>,
        show_order_id: Option<bool>,
        show_client_id: Option<bool>,
    ) -> Option<OrderbookView> {
        self.internal_get_market(&market_id).map(|m| {
            orderbook_to_view(
                &m.orderbook,
                depth,
                m.base_token.lot_size,
                m.quote_token.lot_size,
                show_owner.unwrap_or(false),
                show_order_id.unwrap_or(false),
                show_client_id.unwrap_or(false),
            )
        })
    }

    pub fn get_open_orders(
        &self,
        market_id: MarketId,
        account_id: AccountId,
    ) -> Vec<OpenLimitOrderView> {
        let m = self.internal_unwrap_market(&market_id);
        self.internal_unwrap_account(&account_id)
            .open_orders_iter(&market_id)
            .map(|(oid, (original_qty_lots, timestamp))| {
                let o = m.orderbook.get_order(oid).unwrap();
                order_to_view(
                    &o,
                    m.base_token.lot_size,
                    m.quote_token.lot_size,
                    Some(original_qty_lots),
                    Some(timestamp),
                )
            })
            .collect()
    }

    pub fn get_order(&self, market_id: MarketId, order_id: OrderId) -> Option<OpenLimitOrderView> {
        let market = self.internal_unwrap_market(&market_id);
        let order = market.orderbook.get_order(order_id);

        order.map(|o| {
            let owner = self.internal_unwrap_account(&o.owner_id);
            let (original_quantity_lots, ts) = owner.get_order_info(&market_id, &order_id).unwrap();
            order_to_view(
                &o,
                market.base_denomination(),
                market.quote_denomination(),
                Some(original_quantity_lots),
                Some(ts),
            )
        })
    }

    pub fn get_balance(&self, account_id: &AccountId, token_id: &AccountId) -> U128 {
        self.internal_unwrap_account(account_id)
            .get_balance(&token_id.into())
            .into()
    }

    pub fn get_near_balance(&self, account_id: &AccountId) -> U128 {
        self.internal_unwrap_account(account_id)
            .get_balance(&TokenType::NativeNear)
            .into()
    }

    pub fn get_balances(&self, account_id: &AccountId) -> Vec<(String, U128)> {
        self.internal_unwrap_account(account_id)
            .get_balances()
            .into_iter()
            .map(|(t, b)| (t, U128::from(b)))
            .collect()
    }

    pub fn list_markets(&self, from_index: u64, limit: u64) -> Vec<MarketView> {
        (from_index..std::cmp::min(from_index + limit, self.market_iter_map.len()))
            .map(|index| {
                let id = self.market_iter_map.get(index).unwrap();
                self.internal_get_market(&id).unwrap().to_view(8, false)
            })
            .collect()
    }

    pub fn get_number_of_markets(&self) -> u64 {
        self.market_iter_map.len()
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner_id.clone()
    }

    pub fn get_contract_state(&self) -> ContractState {
        self.state.clone()
    }
}
