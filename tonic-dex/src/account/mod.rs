use crate::*;

pub mod v1;
pub use v1::*;

#[derive(BorshSerialize, BorshDeserialize)]
pub enum VAccount {
    V1(AccountV1),
}

impl From<VAccount> for AccountV1 {
    fn from(v: VAccount) -> Self {
        match v {
            VAccount::V1(a) => a,
        }
    }
}

impl From<AccountV1> for VAccount {
    fn from(a: AccountV1) -> Self {
        Self::V1(a)
    }
}

impl Contract {
    /// Save the account. Panics if the account has insufficient storage balance.
    pub fn internal_save_account(&mut self, account_id: &AccountId, account: AccountV1) {
        _assert!(
            account.is_storage_covered(),
            // since there are no near collection fields in AccountV1, it's OK
            // to do this assertion before writing the account
            errors::INSUFFICIENT_STORAGE_BALANCE
        );
        self.accounts.insert(account_id, &account.into());
    }

    pub fn internal_try_save_account(
        &mut self,
        account_id: &AccountId,
        account: AccountV1,
    ) -> Result<(), ()> {
        if !account.is_storage_covered() {
            Err(())
        } else {
            self.internal_save_account(account_id, account);
            Ok(())
        }
    }

    pub fn internal_get_account(&self, account_id: &AccountId) -> Option<AccountV1> {
        self.accounts.get(account_id).map(|a| {
            let mut account: AccountV1 = a.into();
            account.initialize_id(account_id.clone());
            account
        })
    }

    pub fn internal_unwrap_account(&self, account_id: &AccountId) -> AccountV1 {
        _expect!(
            self.internal_get_account(account_id),
            errors::ACCOUNT_NOT_FOUND
        )
    }

    /// Deposit into an account. This may be used in situations where the
    /// account isn't already loaded for another use, eg, in the public deposit
    /// method or the withdraw callback. In most cases, prefer loading the
    /// account and depositing with `account.deposit` to save gas.
    pub fn internal_deposit(&mut self, sender_id: &AccountId, token: &TokenType, amount: Balance) {
        let mut account = self.internal_unwrap_account(sender_id);
        account.deposit(token, amount);
        self.internal_save_account(sender_id, account);
    }
}
