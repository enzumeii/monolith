#![allow(clippy::unwrap_used)]
use super::*;
use crate::models::shopify::PageInfo;
#[tokio::test]
async fn root_pages_reject_missing_empty_repeated_and_cyclic_cursors() {
    for cursors in [
        vec![None],
        vec![Some("")],
        vec![Some("a"), Some("a")],
        vec![Some("a"), Some("b"), Some("a")],
    ] {
        let mut iter = cursors.into_iter();
        let result = for_each_page(
            |_| {
                let cursor = iter.next().unwrap();
                async move {
                    Ok(Connection {
                        nodes: vec![1],
                        page_info: PageInfo {
                            has_next_page: true,
                            end_cursor: cursor.map(str::to_owned),
                        },
                    })
                }
            },
            |_| Ok(()),
        )
        .await;
        assert!(result.is_err());
    }
}
#[tokio::test]
async fn consumes_each_root_page_before_fetching_the_next() {
    let consumed = std::sync::Mutex::new(Vec::new());
    let mut count = 0;
    for_each_page(
        |after| {
            count += 1;
            let n = count;
            assert_eq!(consumed.lock().unwrap().len(), n - 1);
            async move {
                assert_eq!(after.as_deref(), if n == 1 { None } else { Some("next") });
                Ok(Connection {
                    nodes: vec![n],
                    page_info: PageInfo {
                        has_next_page: n == 1,
                        end_cursor: if n == 1 { Some("next".into()) } else { None },
                    },
                })
            }
        },
        |nodes| {
            consumed.lock().unwrap().extend(nodes);
            Ok(())
        },
    )
    .await
    .unwrap();
    assert_eq!(*consumed.lock().unwrap(), vec![1, 2]);
}
#[tokio::test]
async fn consumer_failure_stops_before_the_next_fetch() {
    let mut count = 0;
    let result = for_each_page(
        |_| {
            count += 1;
            async {
                Ok(Connection {
                    nodes: vec![1],
                    page_info: PageInfo {
                        has_next_page: true,
                        end_cursor: Some("next".into()),
                    },
                })
            }
        },
        |_| {
            Err(ClientError::new(ClientErrorKind::Protocol(
                "invalid nested data",
            )))
        },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(count, 1);
}
