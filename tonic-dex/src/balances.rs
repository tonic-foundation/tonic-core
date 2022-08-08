/// Implements structs and methods for working with exchange balances.
use near_contract_standards::fungible_token::core_impl::ext_fungible_token;
use near_sdk::json_types::U128;
use near_sdk::{ext_contract, Gas, PromiseResult};
use std::string::ToString;

use crate::*;

pub const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas(20_000_000_000_000); // 20 TGas
/// 25m + gas for resolve transfer
// pub const GAS_FOR_FT_TRANSFER_CALL: Gas = Gas(45_000_000_000_000);
/// Amount of gas for fungible token transfers, increased to 20T to support AS token contracts.
pub const GAS_FOR_FT_TRANSFER: Gas = Gas(20_000_000_000_000);

#[ext_contract(ext_self)]
pub trait TonicExchange {
    fn exchange_callback_post_withdraw(
        &mut self,
        token: TokenType,
        receiver_id: AccountId,
        amount: U128,
    );
}

#[near_bindgen]
impl Contract {
    #[payable]
    pub fn deposit_near(&mut self) {
        let amount = env::attached_deposit();
        let sender_id = env::signer_account_id();
        let mut account = self.internal_unwrap_account(&sender_id);
        account.deposit(&TokenType::NativeNear, amount);
        self.internal_save_account(&sender_id, account);
    }

    #[payable]
    pub fn withdraw_near(&mut self, amount: U128) {
        assert_one_yocto();
        let account_id = env::signer_account_id();

        let token = TokenType::from_key("NEAR");
        self.internal_withdraw(&account_id, &token, amount.into());
    }

    #[payable]
    pub fn withdraw_ft(&mut self, token: AccountId, amount: U128) {
        assert_one_yocto();
        let account_id = env::signer_account_id();

        let token = TokenType::from_account_id(token);
        self.internal_withdraw(&account_id, &token, amount.into());
    }

    #[payable]
    pub fn withdraw_mt(&mut self, mt_account_id: AccountId, token_id: TokenId, amount: U128) {
        assert_one_yocto();
        let account_id = env::signer_account_id();
        let key = format!("mft:{}:{}", mt_account_id, token_id);
        let token = TokenType::from_key(&key);
        self.internal_withdraw(&account_id, &token, amount.into());
    }

    #[private]
    pub fn exchange_callback_post_withdraw(
        &mut self,
        token: &TokenType,
        receiver_id: AccountId,
        amount: U128,
    ) {
        debug_log!(
            "exchange cb post withdraw token {}, amount {}",
            token.to_string(),
            u128::from(amount)
        );
        assert_eq!(
            env::promise_results_count(),
            1,
            "{}",
            "expected one promise result post-withdraw"
        );
        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Successful(_) => {}
            PromiseResult::Failed => {
                // This reverts the changes from withdraw function.
                // If account no longer exists, deposit to the token owner's account.
                if self.internal_get_account(&receiver_id).is_some() {
                    self.internal_deposit(&receiver_id, token, amount.0);
                } else {
                    env::log_str(&format!(
                        "Account {} is not registered. Depositing to contract owner.",
                        receiver_id
                    ));
                    self.internal_deposit(&self.owner_id.clone(), token, amount.0);
                }
            }
        };
    }
}

impl Contract {
    pub fn internal_withdraw(
        &mut self,
        account_id: &AccountId,
        token: &TokenType,
        amount: Balance,
    ) -> Promise {
        assert!(amount > 0, "Withdraw amount must be positive");

        let mut account = self.internal_unwrap_account(account_id);
        account.withdraw(token, amount);
        self.internal_save_account(account_id, account);
        self.internal_send(account_id, token, amount)
    }

    pub fn internal_send(
        &self,
        receiver_id: &AccountId,
        token: &TokenType,
        amount: Balance,
    ) -> Promise {
        match token {
            TokenType::NativeNear => Promise::new(receiver_id.clone()).transfer(amount),
            TokenType::FungibleToken { account_id } => {
                self.internal_send_ft(receiver_id, account_id, amount)
            }
            TokenType::MultiFungibleToken {
                account_id,
                subtoken_id,
            } => self.internal_send_mft(receiver_id, account_id, subtoken_id, amount),
        }
        .then(ext_self::exchange_callback_post_withdraw(
            token.clone(),
            receiver_id.clone(),
            U128(amount),
            env::current_account_id(),
            0,
            GAS_FOR_RESOLVE_TRANSFER,
        ))
    }

    pub fn internal_send_ft(
        &self,
        receiver_id: &AccountId,
        token_id: &AccountId,
        amount: Balance,
    ) -> Promise {
        ext_fungible_token::ft_transfer(
            receiver_id.clone(),
            U128(amount),
            None,
            token_id.clone(),
            1,
            GAS_FOR_FT_TRANSFER,
        )
    }

    pub fn internal_send_mft(
        &self,
        receiver_id: &AccountId,
        token_account_id: &AccountId,
        token_id: &str,
        amount: Balance,
    ) -> Promise {
        ext_multi_token::mt_transfer(
            receiver_id.clone(),
            token_id.into(),
            U128(amount),
            None,
            None,
            token_account_id.clone(),
            1,
            GAS_FOR_FT_TRANSFER,
        )
    }
}

// TODO: Once MFT standard impl is merged, remove this and use
// `near_contract_standards::multi_token::core_impl::ext_multi_token::mt_transfer`
#[ext_contract(ext_multi_token)]
pub trait MultiTokenCore {
    fn mt_transfer(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        amount: U128,
        approval: Option<(AccountId, u64)>,
        memo: Option<String>,
    );
}
