use tonic_sdk::measure_gas;

use crate::*;

#[near_bindgen]
impl Contract {
    pub fn set_market_state(&mut self, market_id: MarketId, new_state: MarketState) {
        self.assert_is_owner();
        let mut market = self.internal_unwrap_market(&market_id);
        market.set_state(new_state);
        self.internal_save_market(&market.unwrap_id().clone(), market);
    }

    pub fn set_market_bid_window(&mut self, market_id: MarketId, minimum_bid_bps: u32) {
        self.assert_is_owner();
        let mut market = self.internal_unwrap_market(&market_id);
        market.minimum_bid_bps = minimum_bid_bps;
        self.internal_save_market(&market.unwrap_id().clone(), market);
    }

    pub fn set_market_ask_window(&mut self, market_id: MarketId, maximum_ask_bps: u32) {
        self.assert_is_owner();
        let mut market = self.internal_unwrap_market(&market_id);
        market.maximum_ask_bps = maximum_ask_bps;
        self.internal_save_market(&market.unwrap_id().clone(), market);
    }

    /// Delete a market. Market must be uninitialized or paused with no resting
    /// orders. Only callable by the contract owner.
    pub fn admin_delete_market(&mut self, market_id: MarketId) {
        self.assert_is_owner();

        let market = self.internal_unwrap_market(&market_id);
        let can_delete = match market.state {
            MarketState::Uninitialized => true,
            MarketState::Paused => {
                market.orderbook.bids.is_empty() && market.orderbook.asks.is_empty()
            }
            _ => false,
        };

        if !can_delete {
            env::panic_str("Market cannot be deleted");
        }

        if let Some(pos) = self.market_iter_map.iter().position(|id| id == market_id) {
            let deleted_id = self.market_iter_map.swap_remove(pos as u64);
            _assert_eq!(
                deleted_id,
                market_id,
                "bug: deleted market id and passed market id are different"
            );
            self.markets.remove(&market_id);
        }
    }

    /// Cancel the given user's order. Only callable by the contract owner.
    #[measure_gas(feature = "measure_gas")]
    pub fn admin_cancel_order(&mut self, market_id: MarketId, order_id: OrderId) {
        self.assert_is_owner();
        let market = self.internal_unwrap_market(&market_id);
        let order = market.orderbook.get_order(order_id).unwrap();
        self.internal_cancel_order(market_id, order.owner_id, order_id);
    }

    /// Cancel all of the given user's orders in a market. Only callable by the
    /// contract owner.
    #[measure_gas(feature = "measure_gas")]
    pub fn admin_cancel_all_user_orders(
        &mut self,
        market_id: MarketId,
        account_id: AccountId,
    ) -> Vec<OrderId> {
        self.assert_is_owner();
        self.internal_cancel_all_orders(&market_id, account_id)
    }

    /// Cancel all orders in a market. Only callable by the contract owner.
    #[measure_gas(feature = "measure_gas")]
    pub fn admin_clear_orderbook(
        &mut self,
        market_id: MarketId,
        limit: Option<u16>,
    ) -> Vec<OrderId> {
        self.assert_is_owner();
        self.internal_clear_orderbook_orders(&market_id, limit)
    }
}
