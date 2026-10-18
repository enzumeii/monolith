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
            std::convert::identity,
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
        std::convert::identity,
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
        std::convert::identity,
    )
    .await;
    assert!(result.is_err());
    assert_eq!(count, 1);
}

#[tokio::test]
async fn mapped_pagination_errors_do_not_become_internal_server_errors() {
    use crate::{error::AppError, services::common::upstream};
    use axum::{http::StatusCode, response::IntoResponse};
    for failure in ["cursor", "fetch", "consume"] {
        let error = for_each_page(
            |_| async move {
                if failure == "fetch" {
                    return Err(AppError::new(
                        StatusCode::NOT_FOUND,
                        "Shopify location not found",
                    ));
                }
                Ok(Connection {
                    nodes: vec![1],
                    page_info: PageInfo {
                        has_next_page: failure == "cursor",
                        end_cursor: None,
                    },
                })
            },
            |_| {
                if failure == "consume" {
                    Err(AppError::new(
                        StatusCode::BAD_REQUEST,
                        "Invalid consumer input",
                    ))
                } else {
                    Ok(())
                }
            },
            |error| upstream(error),
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.into_response().status(),
            match failure {
                "cursor" => 502,
                "fetch" => 404,
                "consume" => 400,
                _ => unreachable!(),
            }
        );
    }
}
