use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider,
};
use near_contract_standards::fungible_token::FungibleToken;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::{env, near_bindgen, AccountId, PanicOnDefault, PromiseOrValue};

#[near_bindgen]
#[derive(BorshSerialize, BorshDeserialize, PanicOnDefault)]
pub struct Contract {
    token: FungibleToken,
    decimals: u8,
    name: String,
    symbol: String,
    icon: Option<String>,
    max_mint: Option<U128>,
    allow_external_transfer: bool,
}

near_contract_standards::impl_fungible_token_core!(Contract, token);
near_contract_standards::impl_fungible_token_storage!(Contract, token);

pub const TONIC_TOKEN_CONTRACT_ID: &str = std::env!("TONIC_CONTRACT_ID");

fn tonic_contract() -> AccountId {
    AccountId::new_unchecked(TONIC_TOKEN_CONTRACT_ID.to_string())
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(
        decimals: u8,
        name: String,
        symbol: String,
        icon: Option<String>,
        max_mint: Option<U128>,
        allow_external_transfer: Option<bool>,
    ) -> Self {
        Self {
            token: FungibleToken::new(b"t".to_vec()),
            decimals,
            name,
            symbol,
            icon,
            max_mint,
            allow_external_transfer: allow_external_transfer.unwrap_or(false),
        }
    }

    pub fn set_name(&mut self, name: String) {
        self.assert_caller_allowed();
        self.name = name
    }

    pub fn set_icon(&mut self, icon: Option<String>) {
        self.assert_caller_allowed();
        self.icon = icon
    }

    pub fn set_symbol(&mut self, symbol: String) {
        self.assert_caller_allowed();
        self.symbol = symbol
    }

    pub fn set_max_mint(&mut self, max_mint: Option<U128>) {
        self.assert_caller_allowed();
        self.max_mint = max_mint;
    }

    pub fn set_external_transfer(&mut self, allow: bool) {
        self.assert_caller_allowed();
        self.allow_external_transfer = allow;
    }

    /// Naming this ft_* allows the NEAR wallet to discover this token for you
    pub fn ft_mint(&mut self, receiver_id: AccountId, amount: U128) {
        if let Some(max_mint) = self.max_mint {
            let amount: u128 = amount.into();
            if amount > max_mint.into() {
                env::panic_str("Mint amount exceeds maximum");
            }
        }
        if self.token.accounts.get(&receiver_id).is_none() || self.is_owner() {
            self.token.internal_register_account(&receiver_id);
            self.token.internal_deposit(&receiver_id, amount.into());
        };
    }

    pub fn admin_mint(&mut self, receiver_id: AccountId, amount: U128) {
        if self.is_owner() {
            self.token.internal_register_account(&receiver_id);
            self.token.internal_deposit(&receiver_id, amount.into());
        } else {
            env::panic_str("admin only!");
        }
    }

    pub fn burn(&mut self, account_id: AccountId, amount: U128) {
        self.token.internal_withdraw(&account_id, amount.into());
    }

    pub fn unregister_account(&mut self, account_id: &AccountId) {
        if self.token.accounts.remove(account_id).is_none() {
            env::panic_str("The account does not exist");
        }
    }

    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        self.assert_tonic_transfer(&receiver_id);
        self.token.ft_transfer(receiver_id, amount, memo)
    }

    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.assert_tonic_transfer(&receiver_id);
        self.token.ft_transfer_call(receiver_id, amount, memo, msg)
    }
}

impl Contract {
    fn assert_caller_allowed(&self) {
        if !self.is_owner() {
            env::panic_str("Caller not allowed")
        }
    }

    fn is_owner(&self) -> bool {
        env::signer_account_id() == env::current_account_id()
    }

    fn assert_tonic_transfer(&self, receiver_id: &AccountId) {
        if !self.allow_external_transfer
            && !(receiver_id.clone() == tonic_contract()
                || env::signer_account_id() == tonic_contract())
        {
            env::panic_str("Can only transfer test tokens to Tonic");
        }
    }
}

#[near_bindgen]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        FungibleTokenMetadata {
            spec: "ft-1.0.0".to_string(),
            reference: None,
            reference_hash: None,
            decimals: self.decimals,
            name: self.name.clone(),
            symbol: self.symbol.clone(),
            icon: self.icon.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{env, testing_env, MockedBlockchain};

    use super::*;

    #[test]
    fn test_basics() {
        let mut context = VMContextBuilder::new();
        testing_env!(context.build());
        let mut contract = Contract::new(
            8,
            "test".to_string(),
            "TEST".to_string(),
            None,
            None,
            Some(false),
        );
        testing_env!(context
            .attached_deposit(125 * env::storage_byte_cost())
            .build());
        contract.ft_mint(accounts(0), 1_000_000.into());
        assert_eq!(contract.ft_balance_of(accounts(0)), 1_000_000.into());

        testing_env!(context
            .attached_deposit(125 * env::storage_byte_cost())
            .build());
        contract.storage_deposit(Some(tonic_contract()), None);
        testing_env!(context
            .attached_deposit(1)
            .predecessor_account_id(accounts(0))
            .build());
        contract.ft_transfer(tonic_contract(), 1_000.into(), None);
        assert_eq!(contract.ft_balance_of(tonic_contract()), 1_000.into());

        contract.burn(tonic_contract(), 500.into());
        assert_eq!(contract.ft_balance_of(tonic_contract()), 500.into());
    }

    #[test]
    #[should_panic]
    fn test_send_outside_tonic() {
        let mut context = VMContextBuilder::new();
        testing_env!(context.build());
        let mut contract = Contract::new(
            8,
            "test".to_string(),
            "TEST".to_string(),
            None,
            None,
            Some(false),
        );

        testing_env!(context
            .attached_deposit(125 * env::storage_byte_cost())
            .build());
        contract.ft_mint(accounts(0), 1_000_000.into());
        assert_eq!(contract.ft_balance_of(accounts(0)), 1_000_000.into());

        contract.ft_transfer(accounts(1), 1_000.into(), None);
    }
}
