use std::panic;

use near_sdk::json_types::U128;
use tonic_dex::*;

use near_contract_standards::fungible_token::metadata::FungibleTokenMetadata;
use near_sdk::test_utils::VMContextBuilder;
use near_sdk::{testing_env, AccountId, Balance, Gas, PublicKey, VMContext};

fn max_account_id(c: &str) -> String {
    c.repeat(64)
}

pub fn get_balance(contract: &Contract, account: &AccountId, token: TokenType) -> Balance {
    contract
        .internal_unwrap_account(account)
        .get_balance(&token)
        .into()
}

/// Returns a pre-defined account_id from a list of 6.
pub fn accounts(id: usize) -> AccountId {
    accounts_list().get(id).unwrap().clone()
}

pub fn accounts_list() -> Vec<AccountId> {
    [
        &max_account_id("a"),
        &max_account_id("b"),
        &max_account_id("c"),
        "danny",
        "eugene",
        "fargo",
    ]
    .map(|x| x.to_string())
    .map(AccountId::new_unchecked)
    .into()
}

pub mod deposits {
    pub const TENTH_NEAR: u128 = 100_000_000_000_000_000_000_000;
}

const CURRENT_ACCOUNT_ID: &'static str = "contract.testnet";
// const PREDECESSOR_ACCOUNT_ID: &'static str = "alice.testnet";

pub fn get_context(input: Vec<u8>) -> VMContext {
    let random_seed: [u8; 32] = [0; 32];
    let signer_pk: PublicKey = "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp"
        .parse()
        .unwrap();
    VMContext {
        current_account_id: CURRENT_ACCOUNT_ID.parse().unwrap(),
        signer_account_id: accounts(1).to_string().parse().unwrap(),
        signer_account_pk: signer_pk,
        predecessor_account_id: accounts(1).to_string().parse().unwrap(),
        input,
        block_index: 0,
        block_timestamp: 0,
        account_balance: 0,
        account_locked_balance: 0,
        storage_usage: 0,
        attached_deposit: 0,
        prepaid_gas: Gas(10u64.pow(18)),
        random_seed,
        output_data_receivers: vec![],
        epoch_height: 19,
        view_config: None,
    }
}

pub fn setup_contract() -> Contract {
    let context = get_context(vec![]);
    testing_env!(context);
    Contract::new(accounts(0))
}

pub fn set_signer_context(account_id: AccountId) {
    let context = VMContextBuilder::new()
        .signer_account_id(account_id)
        .build();
    testing_env!(context);
}

pub fn set_predecessor_context(account_id: AccountId) {
    let context = VMContextBuilder::new()
        .predecessor_account_id(account_id)
        .build();
    testing_env!(context);
}

pub fn set_deposit_context(signer_id: AccountId, amount: Balance) {
    let context = VMContextBuilder::new()
        .predecessor_account_id(signer_id.clone())
        .signer_account_id(signer_id)
        .attached_deposit(amount)
        .build();
    testing_env!(context);
}

pub fn get_accounts() -> (AccountId, AccountId, AccountId, AccountId) {
    let user_a = accounts(0);
    let user_b = accounts(2);

    let wnear = accounts(3);
    let usdc = accounts(4);
    (user_a, user_b, wnear, usdc)
}

pub fn get_ft_metadata(decimals: u8) -> FungibleTokenMetadata {
    FungibleTokenMetadata {
        name: "".to_string(),
        spec: "".to_string(),
        symbol: "".to_string(),
        icon: None,
        reference: None,
        reference_hash: None,
        decimals,
    }
}

pub fn get_mt_metadata(decimals: u8) -> MTBaseTokenMetadata {
    MTBaseTokenMetadata {
        name: "".to_string(),
        id: "".to_string(),
        symbol: None,
        icon: None,
        decimals: Some(decimals.to_string()),
        base_uri: None,
        reference: None,
        copies: None,
        reference_hash: None,
    }
}

pub fn create_and_init_market(
    contract: &mut Contract,
    args: CreateMarketArgs,
    base_decimals: u8,
    quote_decimals: u8,
) -> MarketId {
    let market_id = contract.create_market(args);

    contract.on_ft_metadata(
        market_id.clone(),
        PairSide::Base,
        Some(get_ft_metadata(base_decimals)),
    );
    contract.on_mt_metadata(
        market_id.clone(),
        PairSide::Quote,
        Some(vec![get_mt_metadata(quote_decimals)]),
    );

    market_id.into()
}

pub fn create_and_init_market_using_multitokens(
    contract: &mut Contract,
    args: CreateMarketArgs,
    base_decimals: u8,
    quote_decimals: u8,
) -> MarketId {
    let market_id = contract.create_market(args);

    contract.on_mt_metadata(
        market_id.clone(),
        PairSide::Base,
        Some(vec![get_mt_metadata(base_decimals)]),
    );
    contract.on_mt_metadata(
        market_id.clone(),
        PairSide::Quote,
        Some(vec![get_mt_metadata(quote_decimals)]),
    );

    market_id.into()
}

// 0.1 NEAR for tests
pub const DEFAULT_STORAGE_BALANCE_YOCTO: u128 = 10u128.pow(23);
pub fn storage_deposit(contract: &mut Contract, account_id: &AccountId) {
    contract.internal_storage_deposit(&account_id, false, DEFAULT_STORAGE_BALANCE_YOCTO);
}
pub fn storage_deposit_registration_only(contract: &mut Contract, account_id: &AccountId) {
    contract.internal_storage_deposit(&account_id, true, DEFAULT_STORAGE_BALANCE_YOCTO);
}

pub fn create_market_and_place_orders(
    contract: &mut Contract,
    market_args: CreateMarketArgs,
    orders: Vec<(AccountId, NewOrderParams)>,
) -> Market {
    let market_id = create_and_init_market(contract, market_args, 16, 18);
    for (user, order) in orders.into_iter() {
        set_predecessor_context(user.clone());
        contract.new_order(market_id.into(), order);
    }
    contract.internal_unwrap_market(&market_id)
}

pub fn new_order_params(
    limit_price_native: u128,
    max_spend: Option<U128>,
    max_qty_native: u128,
    side: Side,
    order_type: OrderType,
    client_id: Option<ClientId>,
    referrer_id: Option<AccountId>,
) -> NewOrderParams {
    NewOrderParams {
        limit_price: Some(limit_price_native.into()),
        max_spend,
        quantity: max_qty_native.into(),
        side,
        order_type,
        client_id,
        referrer_id,
    }
}

pub fn cancel_all_orders(contract: &mut Contract, market: &Market) {
    for account_id in accounts_list() {
        if let Ok(_) = panic::catch_unwind(|| contract.get_balances(&account_id)) {
            contract.internal_cancel_all_orders(market.unwrap_id(), account_id);
        }
    }
}

pub fn assert_balance_invariant(
    contract: &Contract,
    market: Option<&Market>,
    token_amounts: Vec<(&AccountId, u128)>,
) {
    for (token_account, amount) in token_amounts {
        let mut total = 0u128;
        for account_id in accounts_list() {
            if let Ok(balance) =
                panic::catch_unwind(|| contract.get_balance(&account_id, token_account).0)
            {
                total += balance;
            }
        }

        // Add accumulated fees only when it's the quote token for the market
        if let Some(market) = market {
            if market.quote_token.token_type == TokenType::from(token_account.clone()) {
                total += market.fees_accrued;
            }
        }
        assert!(
            amount == total,
            "Expected balances for token {} to sum to {}, found {}",
            token_account,
            amount,
            total
        );
    }
}
