use near_contract_standards::fungible_token::metadata::FungibleTokenMetadata;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{ext_contract, Balance, Gas};

use crate::*;

pub const NO_DEPOSIT: Balance = 0;

const TGAS: u64 = 1_000_000_000_000;
pub const GAS_FT_METADATA_READ: Gas = Gas(25 * TGAS);
pub const GAS_FT_METADATA_WRITE: Gas = Gas(25 * TGAS);

pub const GAS_MT_METADATA_READ: Gas = Gas(25 * TGAS);
pub const GAS_MT_METADATA_WRITE: Gas = Gas(25 * TGAS);

#[ext_contract(ext_ft)]
pub trait ExtFT {
    // Get FT metadata.
    fn ft_metadata(&self) -> FungibleTokenMetadata;
}

#[ext_contract(ext_mt)]
pub trait ExtMT {
    // Get MT metadata.
    fn mt_metadata_base_by_token_id(&self, token_ids: Vec<String>) -> Vec<MTBaseTokenMetadata>;
}

#[ext_contract(ext_self)]
pub trait ExtSelf {
    fn after_ft_transfer(&mut self, account_id: AccountId, balance: U128) -> bool;

    // Save FT metadata
    fn on_ft_metadata(
        &mut self,
        market_id: MarketId,
        pair_side: PairSide,
        #[callback] ft_metadata: Option<FungibleTokenMetadata>,
    );

    fn after_mt_transfer(&mut self, account_id: AccountId, balance: U128) -> bool;

    // Save MT metadata
    fn on_mt_metadata(
        &mut self,
        market_id: MarketId,
        pair_side: PairSide,
        #[callback] mt_metadata: Option<MTBaseTokenMetadata>,
    );
}

#[near_bindgen]
impl Contract {
    #[private]
    pub fn on_ft_metadata(
        &mut self,
        market_id: MarketId,
        pair_side: PairSide,
        #[callback] ft_metadata: Option<FungibleTokenMetadata>,
    ) {
        let mut market = self.internal_unwrap_market(&market_id);
        if let Some(ft_metadata_value) = ft_metadata {
            market.set_decimals(pair_side, ft_metadata_value.decimals);
            if market.validate_lots_and_decimals() {
                market.state = MarketState::Active;
            }
            self.internal_save_market(&market_id, market);
        } else {
            debug_log!("Missing metadata for market ID {}", market_id);
            self.internal_remove_market(&market_id);
        }
    }

    #[private]
    pub fn on_mt_metadata(
        &mut self,
        market_id: MarketId,
        pair_side: PairSide,
        #[callback] mt_metadata: Option<Vec<MTBaseTokenMetadata>>,
    ) {
        let mut market = self.internal_unwrap_market(&market_id);
        if let Some(mt_metadata_value) = mt_metadata {
            if let Some(Ok(decimals)) = mt_metadata_value
                .get(0)
                .and_then(|metadata| metadata.decimals.clone())
                .map(|decimals_raw| decimals_raw.parse::<u8>())
            {
                market.set_decimals(pair_side, decimals);
                if market.base_token.decimals != INVALID_DECIMALS
                    && market.quote_token.decimals != INVALID_DECIMALS
                {
                    market.state = MarketState::Active;
                }
            } else {
                debug_log!(
                    "Invalid `decimals` in MT metadata for market ID {}.",
                    market_id
                );
            }
            self.internal_save_market(&market_id, market);
        } else {
            debug_log!("Missing metadata for market ID {}", market_id);
            self.internal_remove_market(&market_id);
        }
    }
}

/// Metadata for the collection of tokens on the contract
/// TODO: remove this and import directly from near_contract_standards after MT is merged.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct MTBaseTokenMetadata {
    pub name: String,
    pub id: String,
    pub symbol: Option<String>,
    pub icon: Option<String>,
    pub decimals: Option<String>,
    pub base_uri: Option<String>,
    pub reference: Option<String>,
    pub copies: Option<u8>,
    pub reference_hash: Option<String>,
}
