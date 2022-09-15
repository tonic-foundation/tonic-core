/// Implements the NEAR storage standard.
///
/// https://docs.near.org/docs/concepts/storage-staking
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};
use near_sdk::env;
use tonic_sdk::borsh_size::BorshSize;

use crate::*;

impl Contract {
    pub fn internal_unwrap_storage_balance(&self, account_id: &AccountId) -> StorageBalance {
        let account = self.internal_unwrap_account(account_id);
        StorageBalance {
            available: account.storage_balance_available().into(),
            total: account.storage_balance.into(),
        }
    }

    pub fn internal_unregister_account(&mut self, account_id: &AccountId, _force: bool) {
        // let mut account = self.internal_unwrap_account(account_id);
        self.accounts.remove(account_id);
    }

    /// Do storage deposit. Create account if it doesn't exist.
    pub fn internal_storage_deposit(
        &mut self,
        account_id: &AccountId,
        registration_only: bool,
        amount: Balance,
    ) -> Balance {
        let mut refund: Balance = 0;

        let account = self.internal_get_account(account_id);
        if let Some(mut account) = account {
            if registration_only && amount > 0 {
                refund = amount;
            } else {
                account.storage_balance += amount;
                self.internal_save_account(account_id, account);
            }
        } else {
            // Making a new account
            let mut account = AccountV1::new(account_id);

            let min_balance = Balance::from(account.borsh_size()) * env::storage_byte_cost();
            _assert!(amount >= min_balance, errors::INSUFFICIENT_STORAGE_BALANCE);

            if registration_only {
                refund = amount - min_balance;
                account.storage_balance = min_balance;
            } else {
                account.storage_balance = amount;
            }

            self.internal_save_account(account_id, account);
        }

        refund
    }

    pub fn internal_storage_balance_of(&self, account_id: &AccountId) -> Option<StorageBalance> {
        self.internal_get_account(account_id)
            .map(|account| StorageBalance {
                total: account.storage_balance.into(),
                available: U128(
                    account.storage_balance
                        - std::cmp::max(
                            Balance::from(account.borsh_size()) * env::storage_byte_cost(),
                            self.storage_balance_bounds().min.0,
                        ),
                ),
            })
    }
}

#[near_bindgen]
impl StorageManagement for Contract {
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        self.assert_active();

        let amount = env::attached_deposit();
        let account_id = account_id.unwrap_or_else(env::predecessor_account_id);
        let registration_only = registration_only.unwrap_or(false);

        let refund = self.internal_storage_deposit(&account_id, registration_only, amount);
        if refund > 0 {
            Promise::new(env::predecessor_account_id()).transfer(refund);
        }

        self.internal_unwrap_storage_balance(&account_id)
    }

    #[payable]
    fn storage_withdraw(&mut self, amount: Option<U128>) -> StorageBalance {
        self.assert_active();

        assert_one_yocto();
        let account_id = env::predecessor_account_id();
        if let Some(storage_balance) = self.internal_storage_balance_of(&account_id) {
            let amount = amount.unwrap_or(storage_balance.available).0;
            if amount > storage_balance.available.0 {
                env::panic_str(errors::INSUFFICIENT_STORAGE_BALANCE);
            }
            if amount > 0 {
                let mut account = self.internal_unwrap_account(&account_id);
                account.storage_balance -= amount;
                self.internal_save_account(&account_id, account);
                Promise::new(account_id.clone()).transfer(amount);
            }
            self.internal_storage_balance_of(&account_id).unwrap()
        } else {
            env::panic_str(errors::ACCOUNT_NOT_FOUND);
        }
    }

    /// Unregister the account. Panics if the account still has open orders or
    /// exchange balances. Clients should implement UI for withdrawing all
    /// balances.
    #[payable]
    fn storage_unregister(&mut self, _force: Option<bool>) -> bool {
        self.assert_active();

        assert_one_yocto();
        let account_id = env::predecessor_account_id();
        let account = self.internal_unwrap_account(&account_id);
        if !account.is_empty() {
            env::panic_str("account not empty");
        } else {
            self.accounts.remove(&account_id);
            Promise::new(account_id.clone()).transfer(account.storage_balance);
            true
        }
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        StorageBalanceBounds {
            min: U128(size::ACCOUNT as u128 * near_sdk::env::storage_byte_cost()),
            max: None,
        }
    }

    fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.internal_storage_balance_of(&account_id)
    }
}
