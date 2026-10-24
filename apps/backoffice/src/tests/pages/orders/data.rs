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
    let summary = order(1, Courier::Fastship).order;
    let rows = merge_courier_orders([
        (Courier::Fastship, vec![summary.clone()]),
        (Courier::ThaiPost, vec![summary.clone()]),
        (Courier::Best, vec![summary.clone()]),
        (Courier::Teleport, vec![summary]),
    ]);
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

#[test]
fn fulfillment_forwards_best_and_teleport_couriers() {
    let calls = RefCell::new(Vec::new());
    let results = block_on(fulfill_selected(
        vec![order(1, Courier::Best), order(2, Courier::Teleport)],
        HashMap::from([(1, "VP123".into()), (2, "TP123".into())]),
        |id, courier, tracking| {
            calls.borrow_mut().push((id, courier, tracking));
            std::future::ready(Ok(FulfillOrderResponse {
                success: true,
                tracking_url: String::new(),
            }))
        },
    ))
    .expect("batch");
    assert_eq!(
        calls.into_inner(),
        [
            (1, Courier::Best, "VP123".into()),
            (2, Courier::Teleport, "TP123".into())
        ]
    );
    assert_eq!(results.len(), 2);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn fulfillment_loads_all_couriers_and_pages_and_deduplicates_overlaps() {
    use dioxus::prelude::*;
    use futures::FutureExt;
    type Loaded = Rc<RefCell<Option<ApiResult<Vec<FulfillmentOrder>>>>>;
    fn app() -> Element {
        let result = use_context::<Loaded>();
        use_future(move || {
            let result = result.clone();
            async move {
                *result.borrow_mut() = Some(load_fulfillment_orders().await);
            }
        });
        rsx! { div {} }
    }
    let (address, server) = serve_order_pages();
    let result: Loaded = Rc::default();
    let mut dom = VirtualDom::new(app)
        .with_root_context(result.clone())
        .with_root_context(crate::config::Settings {
            conventions: crate::config::ConventionsConfig {
                catalogue_location_id: "65761018027".into(),
            },
            api: crate::config::ApiConfig {
                base_url: format!("http://{address}/v1"),
            },
        });
    dom.rebuild_in_place();
    for _ in 0..200 {
        let _ = dom.wait_for_work().now_or_never();
        dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);
        if result.borrow().is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let requests = server.join().expect("server");
    let rows = result
        .borrow_mut()
        .take()
        .expect("finished load")
        .expect("orders");
    assert_eq!(
        rows.iter()
            .map(|row| (row.order.id, row.courier))
            .collect::<Vec<_>>(),
        [
            (42, Courier::Fastship),
            (101, Courier::Fastship),
            (102, Courier::ThaiPost),
            (103, Courier::Best),
            (104, Courier::Teleport)
        ]
    );
    let expected = [
        "SHIPPING,FASTSHIP",
        "SHIPPING,THAI_POST",
        "SHIPPING,BEST",
        "SHIPPING,TELEPORT",
    ]
    .into_iter()
    .flat_map(|tag| {
        [
            (tag.to_owned(), None),
            (tag.to_owned(), Some("next+/=".into())),
        ]
    })
    .collect::<Vec<_>>();
    assert_eq!(requests, expected);
}

#[cfg(not(target_arch = "wasm32"))]
type RecordedRequests = Vec<(String, Option<String>)>;

#[cfg(not(target_arch = "wasm32"))]
fn serve_order_pages() -> (
    std::net::SocketAddr,
    std::thread::JoinHandle<RecordedRequests>,
) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
    listener.set_nonblocking(true).expect("nonblocking");
    let address = listener.local_addr().expect("address");
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while requests.len() < 8 && std::time::Instant::now() < deadline {
            let Ok((mut stream, _)) = listener.accept() else {
                std::thread::sleep(std::time::Duration::from_millis(5));
                continue;
            };
            stream.set_nonblocking(false).expect("blocking stream");
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(1)))
                .expect("timeout");
            let mut data = vec![0; 8192];
            let length = stream.read(&mut data).expect("request");
            let request = String::from_utf8_lossy(&data[..length]);
            let path = request.split_whitespace().nth(1).expect("path");
            let url = reqwest::Url::parse(&format!("http://localhost{path}")).expect("URL");
            let query = url.query_pairs().into_owned().collect::<HashMap<_, _>>();
            let tag = query.get("tag").expect("tag").clone();
            let cursor = query.get("cursor").cloned();
            assert_eq!(query.get("page_size").map(String::as_str), Some("250"));
            assert_eq!(
                query.get("fulfillment").map(String::as_str),
                Some("UNFULFILLED")
            );
            let id = if cursor.is_none() {
                42
            } else {
                match tag.as_str() {
                    "SHIPPING,FASTSHIP" => 101,
                    "SHIPPING,THAI_POST" => 102,
                    "SHIPPING,BEST" => 103,
                    "SHIPPING,TELEPORT" => 104,
                    _ => panic!("unexpected courier"),
                }
            };
            let (next, end) = if cursor.is_none() {
                ("true", "\"next+/=\"")
            } else {
                ("false", "null")
            };
            let body = format!(
                r#"{{"orders":[{{"id":{id},"number":{id},"full_name":null,"email":null,"shipping_country_code":null}}],"page_info":{{"has_next_page":{next},"end_cursor":{end}}}}}"#
            );
            requests.push((tag, cursor));
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).expect("response");
        }
        requests
    });
    (address, server)
}
