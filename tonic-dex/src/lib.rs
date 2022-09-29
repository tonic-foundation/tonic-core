#![allow(clippy::ptr_offset_with_cast, clippy::assign_op_pattern)]

use external_tokens::{
    ext_ft, ext_mt, ext_self, GAS_FT_METADATA_READ, GAS_FT_METADATA_WRITE, NO_DEPOSIT,
};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, Vector};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    assert_one_yocto, env, near_bindgen, require, AccountId, Balance, BorshStorageKey,
    PanicOnDefault, Promise, StorageUsage,
};

pub use tonic_sdk::macros::*;
pub use tonic_sdk::prelude::*;

mod account;
mod actions;
mod admin;
mod balances;
mod external_tokens;
mod fees;
mod limit_order;
mod market;
mod market_order;
mod settlement;
mod storage;
mod storage_manager;
mod swap_order;
mod views;

pub use crate::account::*;
pub use crate::actions::*;
pub use crate::admin::*;
pub use crate::balances::*;
pub use crate::external_tokens::*;
pub use crate::external_tokens::*;
pub use crate::fees::*;
pub use crate::limit_order::*;
pub use crate::market::*;
pub use crate::market_id::*;
pub use crate::market_order::*;
pub use crate::order_id::*;
pub use crate::settlement::*;
pub use crate::storage::*;
pub use crate::storage_manager::*;
pub use crate::swap_order::*;
pub use crate::views::*;

#[derive(Clone, Debug, PartialEq, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub enum ContractState {
    /// Allow all operations
    Active,

    /// No operations allowed except admin actions
    Paused,

    /// Only allow cancelling existing orders
    CancelOnly,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    pub owner_id: AccountId,

    pub accounts: LookupMap<AccountId, VAccount>,

    pub markets: LookupMap<MarketId, VMarket>,

    /// An UnorderedMap requires 2 storage reads to get a value and 3 to write
    /// one. Using this list to enumerate markets saves those additional writes
    /// for all operations that require gas other than creating a market, which
    /// incurs an additional write as a result of maintaining this map.
    pub market_iter_map: Vector<MarketId>,

    /// Global order counter
    pub prev_order_sequence_number: SequenceNumber,

    pub state: ContractState,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        Self {
            owner_id,
            accounts: LookupMap::new(StorageKey::Accounts),
            markets: LookupMap::new(StorageKey::Markets),
            market_iter_map: Vector::new(StorageKey::MarketIterMap),
            prev_order_sequence_number: 0,
            state: ContractState::Active,
        }
    }

    pub fn set_contract_state(&mut self, state: ContractState) {
        self.assert_is_owner();
        self.internal_set_state(state);
    }
}

impl Contract {
    pub fn next_sequence_number(&mut self) -> SequenceNumber {
        self.prev_order_sequence_number += 1;
        self.prev_order_sequence_number
    }

    pub fn assert_is_owner(&self) {
        _assert_eq!(
            env::predecessor_account_id(),
            self.owner_id,
            "Method can only be called by contract owner ID"
        );
    }

    pub fn internal_set_state(&mut self, state: ContractState) {
        self.state = state;
    }

    pub fn assert_active(&self) {
        _assert_eq!(
            self.state,
            ContractState::Active,
            "Contract must be active to place a trade"
        );
    }

    pub fn assert_can_cancel(&self) {
        _assert!(
            self.state == ContractState::Active || self.state == ContractState::CancelOnly,
            "Contract must be active or cancel only to cancel an order"
        );
    }
}
