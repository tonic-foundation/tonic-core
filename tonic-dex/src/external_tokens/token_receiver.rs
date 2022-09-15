use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_contract_standards::non_fungible_token::TokenId;
use near_sdk::json_types::U128;
use near_sdk::{serde_json, PromiseOrValue};

use crate::*;
use errors::INVALID_ACTION;

#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.assert_active();

        let amount: u128 = amount.into();
        let token_id = env::predecessor_account_id();
        let token = TokenType::FungibleToken {
            account_id: token_id,
        };
        if msg.is_empty() {
            self.internal_deposit(&sender_id, &token, amount);
            return PromiseOrValue::Value(U128(0));
        }
        let message = serde_json::from_str::<Action>(&msg).expect("Invalid message");
        match message {
            Action::Swap(swaps) => self.execute_swaps(swaps, token, amount, sender_id),
            _ => env::panic_str(INVALID_ACTION),
        }
    }
}

// TODO: Use whatever is in near_contract_standards when this is merged there
// https://github.com/near/NEPs/issues/246
pub trait MultiTokenReceiver {
    /// Take some action after receiving a MultiToken-tokens token
    ///
    /// Requirements:
    /// * Contract MUST restrict calls to this function to a set of whitelisted MultiToken
    ///   contracts
    ///
    /// Arguments:
    /// * `sender_id`: the sender of `mt_transfer_call`
    /// * `previous_owner_ids`: the accounts that owned the token(s) prior to it/them being
    ///   transferred to this contract, which can differ from `sender_id` if using
    ///   Approval Management extension
    /// * `token_ids`: the `token_ids` argument given to `mt_transfer_call`
    /// * `msg`: information necessary for this contract to know how to process the
    ///   request. This may include method names and/or arguments.
    ///
    /// Returns true if tokens should be returned to `sender_id`
    fn mt_on_transfer(
        &mut self,
        sender_id: AccountId,
        previous_owner_ids: Vec<AccountId>,
        token_ids: Vec<TokenId>,
        amounts: Vec<U128>,
        msg: String,
    ) -> PromiseOrValue<Vec<U128>>;
}

impl MultiTokenReceiver for Contract {
    fn mt_on_transfer(
        &mut self,
        _sender_id: AccountId,
        previous_owner_ids: Vec<AccountId>,
        token_ids: Vec<TokenId>,
        amounts: Vec<U128>,
        _msg: String,
    ) -> PromiseOrValue<Vec<U128>> {
        self.assert_active();

        let account_id = env::predecessor_account_id();
        _assert_eq!(
            token_ids.len(),
            amounts.len(),
            "Token list length does not match amounts"
        );
        _assert_eq!(
            token_ids.len(),
            previous_owner_ids.len(),
            "Token list length does not match previous_owner_ids"
        );
        let mut results: Vec<U128> = vec![];
        let it = token_ids
            .iter()
            .zip(amounts.iter())
            .zip(previous_owner_ids.iter());
        for (_i, ((subtoken_id, &amount), prev_owner_id)) in it.enumerate() {
            self.internal_deposit(
                prev_owner_id,
                &TokenType::MultiFungibleToken {
                    account_id: account_id.clone(),
                    subtoken_id: subtoken_id.to_string(),
                },
                amount.into(),
            );
            results.push(U128(0));
        }
        PromiseOrValue::Value(results)
    }
}
