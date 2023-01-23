use near_sdk::{Gas, Promise};

use crate::*;

#[near_bindgen]
impl Contract {
    pub fn version(&self) -> String {
        format!("{}:{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
    }

    #[private]
    #[init(ignore_state)]
    pub fn migrate() -> Self {
        let contract: Contract = env::state_read().expect("Contract is not initialized");
        contract
    }

    pub fn upgrade(&self) -> Promise {
        self.assert_is_owner();

        const CALL_GAS: Gas = Gas(200_000_000_000_000); // 200 TGAS
        const NO_ARGS: Vec<u8> = vec![];

        // Receive the code directly from the input to avoid the
        // GAS overhead of deserializing parameters
        let code = env::input().expect("Error: No input").to_vec();

        // Deploy the contract on self
        Promise::new(env::current_account_id())
            .deploy_contract(code)
            .function_call("migrate".to_string(), NO_ARGS, 0, CALL_GAS)
            .as_return()
    }
}
