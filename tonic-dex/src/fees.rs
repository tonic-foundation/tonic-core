/// Defines structs and functions for working with fees.
///
/// NB: fee tiers have been removed
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};

use crate::*;

/// 1 bp = 1/100th of a percent
pub const FEE_TO_BPS_DIVISOR: u128 = 10_000;

pub const MAX_MAKER_REBATE_BOOST: u8 = 4;
pub const MAX_TAKER_FEE_DISCOUNT: u8 = 5;

#[derive(Copy, Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize)]
#[repr(u8)]
pub enum FeeTier {
    Base,
}

/// Struct containing context used for fee calculations.
#[derive(Debug, Clone, PartialEq)]
pub struct FeeCalculator {
    pub fee_tier: FeeTier,
    pub taker_fee_base_rate: u8,
    pub maker_rebate_base_rate: u8,
}

impl From<u128> for FeeTier {
    fn from(_: u128) -> Self {
        Self::Base
    }
}

impl FeeCalculator {
    pub fn new_with_base_rate(market: &Market) -> Self {
        Self {
            fee_tier: FeeTier::Base,
            taker_fee_base_rate: market.taker_fee_base_rate,
            maker_rebate_base_rate: market.maker_rebate_base_rate,
        }
    }

    pub fn new(account: &AccountV1, market: &Market) -> Self {
        Self {
            fee_tier: account.get_fee_tier(),
            taker_fee_base_rate: market.taker_fee_base_rate,
            maker_rebate_base_rate: market.maker_rebate_base_rate,
        }
    }

    fn maker_rebate_rate(&self) -> u128 {
        let boost = 0;
        (self.maker_rebate_base_rate + boost) as u128
    }

    fn taker_fee_rate(&self) -> u128 {
        let discount = 0;
        self.taker_fee_base_rate.saturating_sub(discount) as u128
    }

    pub fn maker_rebate(&self, quote_quantity: u128) -> u128 {
        let rate_bps = self.maker_rebate_rate();
        quote_quantity.checked_mul(rate_bps).unwrap() / FEE_TO_BPS_DIVISOR
    }

    pub fn taker_fee(&self, quote_quantity: u128) -> u128 {
        let rate_bps = self.taker_fee_rate();
        quote_quantity.checked_mul(rate_bps).unwrap() / FEE_TO_BPS_DIVISOR
    }

    /// Return quote quantity with max possible taker fee subtracted.
    pub fn withhold_taker_fee(&self, quote_quantity: u128) -> u128 {
        quote_quantity.saturating_sub(self.taker_fee(quote_quantity))
    }
}

pub fn referrer_rebate(taker_fee: u128) -> u128 {
    taker_fee / 5
}
