use super::*;
use domain::dto::orders::{Courier, FulfillOrderResponse, OrderSummary};
use futures::executor::block_on;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

fn order(id: u64, courier: Courier) -> FulfillmentOrder {
    FulfillmentOrder {
        order: OrderSummary {
            id,
            number: i64::try_from(id).expect("test ID fits"),
            full_name: None,
            email: None,
            shipping_country_code: None,
        },
        courier,
    }
}

#[test]
fn duplicate_courier_results_keep_the_first_courier() {
    let rows = merge_courier_orders(
        vec![OrderSummary {
            id: 1,
            number: 1001,
            full_name: None,
            email: None,
            shipping_country_code: None,
        }],
        vec![OrderSummary {
            id: 1,
            number: 1001,
            full_name: None,
            email: None,
            shipping_country_code: None,
        }],
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].courier, Courier::Fastship);
}

#[test]
fn fulfillment_skips_blank_tracking_and_continues_after_failures_in_order() {
    let orders = vec![
        order(1, Courier::Fastship),
        order(2, Courier::ThaiPost),
        order(3, Courier::Fastship),
    ];
    let tracking = HashMap::from([
        (1, "  ".into()),
        (2, "THAI123".into()),
        (3, "FAST123".into()),
    ]);
    let calls = Rc::new(RefCell::new(Vec::new()));
    let seen = calls.clone();
    let results = block_on(fulfill_selected(
        orders,
        tracking,
        move |id, _, tracking| {
            seen.borrow_mut().push((id, tracking));
            std::future::ready(if id == 2 {
                Err(crate::api::ApiError::new(
                    reqwest::StatusCode::BAD_GATEWAY,
                    "Upstream failure",
                ))
            } else {
                Ok(FulfillOrderResponse {
                    success: true,
                    tracking_url: "https://track.test/FAST123".into(),
                })
            })
        },
    ))
    .expect("fulfillment loop");
    assert_eq!(
        calls.borrow().as_slice(),
        [(2, "THAI123".into()), (3, "FAST123".into())]
    );
    assert_eq!(results.len(), 2);
    assert!(results[0].result.is_err());
    assert_eq!(results[1].order_number, 3);
}
