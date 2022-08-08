/// Implements an interface for performing batched actions in a single
/// transaction.
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{assert_one_yocto, near_bindgen};

use crate::errors::INVALID_ACTION;
use crate::*;

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct NewOrderAction {
    pub market_id: MarketId,
    pub params: NewOrderParams,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct SwapAction {
    pub market_id: MarketId,
    pub side: Side,
    pub min_output_token: Option<U128>,
    pub referrer_id: Option<AccountId>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct CancelOrdersAction {
    pub market_id: MarketId,
    pub order_ids: Vec<OrderId>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct CancelAllOrdersAction {
    pub market_id: MarketId,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde", tag = "action", content = "params")]
pub enum Action {
    NewOrder(NewOrderAction),
    CancelOrders(CancelOrdersAction),
    CancelAllOrders(CancelAllOrdersAction),
    Swap(Vec<SwapAction>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub enum ActionResult {
    #[serde(rename = "placed")]
    Order(PlaceOrderResultView),

    #[serde(rename = "cancelled")]
    Cancel(Vec<OrderId>),
}

#[near_bindgen]
impl Contract {
    /// Executes a given list actions on behalf of the predecessor account.
    /// - Requires one yoctoNEAR.
    #[payable]
    pub fn execute(&mut self, actions: Vec<Action>) -> Vec<ActionResult> {
        assert_one_yocto();

        let mut results = vec![];
        for action in actions.iter() {
            let result = match action {
                Action::NewOrder(NewOrderAction { market_id, params }) => {
                    let res = self.new_order(*market_id, params.clone());
                    ActionResult::Order(res)
                }
                Action::CancelOrders(CancelOrdersAction {
                    market_id,
                    order_ids,
                }) => {
                    for order_id in order_ids.iter() {
                        self.cancel_order(*market_id, *order_id);
                    }
                    ActionResult::Cancel(order_ids.to_vec())
                }
                Action::CancelAllOrders(CancelAllOrdersAction { market_id }) => {
                    let order_ids = self.cancel_all_orders(*market_id);
                    ActionResult::Cancel(order_ids)
                }
                _ => {
                    env::panic_str(INVALID_ACTION);
                }
            };
            results.push(result);
        }

        results
    }
}
