/// Defines constants and convenience functions for working with storage.
// TODO: can remove all of this if we use stores from the unstable sdk or write
// our own collections
use near_sdk::borsh::{self, BorshSerialize};

use crate::*;

#[derive(BorshStorageKey, BorshSerialize)]
pub enum StorageKey {
    Accounts,
    Markets,
    MarketIterMap,
}

/// Measured sizes of structs and helper functions for calculating required
/// storage balance.
///
/// Measured with `just measure-storage-usage`.
pub mod size {
    use near_sdk::StorageUsage;

    /// The maximum possible size of an account, ie, one with a 64-byte ID.
    /// Measured using `just test-storage`.
    pub const ACCOUNT: StorageUsage = 134;

    /// The size of an order owned by an account with a 64-byte ID, measured
    /// with just test-storage. This value includes the size of a new price
    /// level. We can technically measure more precisely than this, but the
    /// complexity isn't worth it.
    pub const OPEN_LIMIT_ORDER: StorageUsage = 93;

    /// u128 is 16 bytes. This is serialized as-is by Borsh
    pub const ORDER_ID: StorageUsage = 16;

    pub const LOT_BALANCE: StorageUsage = 8;

    /// The size of a market ID.
    pub const MARKET_ID: StorageUsage = 32;

    /// Fixed amount of storage to lock per market in which an account has open
    /// orders. Ensures that the account has enough storage balance to hold both
    /// base and quote token balances.
    ///
    /// Size represents account id + balance + account id + balance
    pub const MARKET_PAIR_OVERHEAD: StorageUsage = 64 + 16 + 64 + 16;
}
