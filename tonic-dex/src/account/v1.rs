use std::collections::HashMap;

use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    Timestamp,
};
use once_cell::unsync::OnceCell;
use tonic_sdk::borsh_size::{self, BorshSize};

use crate::*;

#[derive(BorshSerialize, BorshDeserialize)]
pub struct AccountV1 {
    /// ID of this account, initialized at runtime after loading from trie for
    /// convenience. Not directly serialized to trie.
    #[borsh_skip]
    pub id: OnceCell<AccountId>,

    /// Amounts of tokens and native NEAR deposited to this account.
    balances: TokenBalancesMap,

    /// A map of the account's open orders.
    open_orders: OpenOrdersMap,

    /// Amount of NEAR deposited for storage. This is distinct from NEAR
    /// available for trading.
    pub storage_balance: Balance,
}

impl AccountV1 {
    impl_lazy_accessors!(id, unwrap_id, initialize_id, AccountId);
}

#[derive(BorshSerialize, BorshDeserialize)]
struct TokenBalancesMap(HashMap<String, Balance>);

impl BorshSize for TokenBalancesMap {
    fn borsh_size(&self) -> StorageUsage {
        self.0.borsh_size()
    }
}

impl TokenBalancesMap {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Data structure representing an account's open orders. The structure is
/// ```ignore
/// {
///   market id -> {
///     order id -> (original order size, timestamp)
///   }
/// }
/// ```
#[derive(BorshSerialize, BorshDeserialize)]
struct OpenOrdersMap(HashMap<MarketId, HashMap<OrderId, (LotBalance, Timestamp)>>);

impl OpenOrdersMap {
    /// Iterate over open orders, if any exist.
    pub fn market_orders_iter(
        &self,
        market_id: &MarketId,
    ) -> impl Iterator<Item = (OrderId, (LotBalance, Timestamp))> {
        self.0
            .get(market_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
    }
}

impl BorshSize for OpenOrdersMap {
    fn borsh_size(&self) -> StorageUsage {
        // this structure is a map of sets, { market id -> { order info } }
        let n_market_ids = self.0.len() as u64;
        let total_market_keys_size = n_market_ids
            * (borsh_size::HASH_SET_OVERHEAD + size::MARKET_ID + size::MARKET_PAIR_OVERHEAD);

        let n_orders: u64 = self.0.iter().map(|(_, oids)| oids.len() as u64).sum();
        let total_orders_size =
            n_orders * (size::ORDER_ID + size::LOT_BALANCE + size::OPEN_LIMIT_ORDER);

        total_market_keys_size + total_orders_size
    }
}

impl OpenOrdersMap {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl BorshSize for AccountV1 {
    fn borsh_size(&self) -> StorageUsage {
        size::ACCOUNT + self.balances.borsh_size() + self.open_orders.borsh_size()
    }
}

impl AccountV1 {
    pub fn new(_account_id: &AccountId) -> Self {
        AccountV1 {
            id: OnceCell::from(_account_id.clone()),
            balances: TokenBalancesMap(HashMap::new()),
            open_orders: OpenOrdersMap(HashMap::new()),
            storage_balance: 0,
        }
    }

    /// Deposit amount to the balance of given token.
    pub fn deposit(&mut self, token: &TokenType, amount: Balance) {
        let key = token.key();
        if let Some(x) = self.balances.0.get(&key).cloned() {
            self.balances.0.insert(key, amount + x);
        } else {
            self.balances.0.insert(key, amount);
        }
    }

    /// Withdraw amount of `token` from the internal balance.
    /// Panics if `amount` is bigger than the current balance.
    pub fn withdraw(&mut self, token: &TokenType, amount: Balance) {
        let key = token.key();
        if let Some(x) = self.balances.0.get(&key).cloned() {
            if x < amount {
                env::panic_str(errors::INSUFFICIENT_BALANCE);
            }
            if x == amount {
                self.balances.0.remove(&key);
            } else {
                self.balances.0.insert(key, x - amount);
            }
        } else {
            env::panic_str(errors::INSUFFICIENT_BALANCE);
        }
    }

    /// Get account's available token balance (balance not locked in orders).
    pub fn get_balance(&self, token: &TokenType) -> Balance {
        let key = token.key();
        self.balances.0.get(&key).cloned().unwrap_or_default()
    }

    /// Get all account available token balances (balances not locked in orders).
    pub fn get_balances(&self) -> Vec<(String, Balance)> {
        self.balances.0.clone().into_iter().collect()
    }

    /// Depends on DEX having a token
    pub fn get_fee_tier(&self) -> fees::FeeTier {
        0.into()
    }

    /// Return a list of the account's open orders in a market.
    pub fn get_tracked_order_ids(&self, market_id: &MarketId) -> Vec<OrderId> {
        if let Some(ids) = self.open_orders.0.get(market_id).cloned() {
            ids.into_keys().collect()
        } else {
            vec![]
        }
    }

    /// Save an open order ID on the account. Called when an order is posted,
    /// used to get a list of an account's open orders.
    pub fn save_order_info(
        &mut self,
        market_id: &MarketId,
        order_id: OrderId,
        original_qty_lots: LotBalance,
        max_allowed_orders: usize,
    ) {
        let timestamp = env::block_timestamp();
        match self.open_orders.0.get_mut(market_id) {
            Some(orders_in_market) => {
                #[cfg(not(feature = "no_order_limit"))]
                if orders_in_market.len() >= max_allowed_orders {
                    env::panic_str(errors::EXCEEDED_ORDER_LIMIT);
                }
                orders_in_market.insert(order_id, (original_qty_lots, timestamp));
            }
            None => {
                let mut orders_in_market = HashMap::new();
                orders_in_market.insert(order_id, (original_qty_lots, timestamp));
                self.open_orders.0.insert(*market_id, orders_in_market);
            }
        };
    }

    /// Delete all of an account's order IDs for a market.
    pub fn remove_all_order_infos(&mut self, market_id: &MarketId) -> Vec<OrderId> {
        if let Some(existing) = self.open_orders.0.remove(market_id) {
            existing.into_keys().collect()
        } else {
            vec![]
        }
    }

    /// Find information about one of the account's open orders, if it exists.
    pub fn get_order_info(
        &self,
        market_id: &MarketId,
        order_id: &OrderId,
    ) -> Option<(LotBalance, Timestamp)> {
        self.open_orders.0.get(market_id)?.get(order_id).cloned()
    }

    /// Delete all of an account's order IDs for a market.
    pub fn remove_order_info(
        &mut self,
        market_id: &MarketId,
        order_id: OrderId,
    ) -> Option<OrderId> {
        let mut ret = None;
        if let Some(orders) = self.open_orders.0.get_mut(market_id) {
            if orders.remove(&order_id).is_some() {
                if orders.is_empty() {
                    self.open_orders.0.remove(market_id);
                }
                ret = Some(order_id);
            }
        }
        ret
    }

    pub fn open_orders_iter(
        &self,
        market_id: &MarketId,
    ) -> impl Iterator<Item = (OrderId, (LotBalance, Timestamp))> {
        self.open_orders.market_orders_iter(market_id)
    }

    /// Return true if the account is empty, ie, has no open orders and no
    /// exchange balances.
    pub fn is_empty(&self) -> bool {
        self.balances.is_empty() && self.open_orders.is_empty()
    }
}

impl AccountV1 {
    fn storage_balance_locked(&self) -> Balance {
        Balance::from(self.borsh_size()) * env::storage_byte_cost()
    }

    pub fn is_storage_covered(&self) -> bool {
        self.storage_balance_locked() <= self.storage_balance
    }

    pub fn storage_balance_available(&self) -> Balance {
        self.storage_balance - self.storage_balance_locked()
    }
}
