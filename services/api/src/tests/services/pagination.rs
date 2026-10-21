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
            None,
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
    let consumed = std::sync::Mutex::new(Vec::<usize>::new());
    let mut count = 0;
    for_each_page(
        None,
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
        None,
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
    use crate::error::{AppError, upstream};
    use axum::{http::StatusCode, response::IntoResponse};
    for failure in ["cursor", "fetch", "consume"] {
        let error = for_each_page(
            None,
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

#[tokio::test]
async fn prefetched_pages_use_the_same_cursor_and_consumer_rules() {
    for continuing in [false, true] {
        let initial = Connection {
            nodes: vec![1],
            page_info: PageInfo {
                has_next_page: continuing,
                end_cursor: continuing.then(|| "next".into()),
            },
        };
        let mut calls = 0;
        let mut collected = Vec::new();
        for_each_page(
            Some(initial),
            |after| {
                calls += 1;
                assert_eq!(after.as_deref(), Some("next"));
                async {
                    Ok(Connection {
                        nodes: vec![2],
                        page_info: PageInfo {
                            has_next_page: false,
                            end_cursor: None,
                        },
                    })
                }
            },
            |nodes| {
                collected.extend(nodes);
                Ok(())
            },
            std::convert::identity,
        )
        .await
        .unwrap();
        assert_eq!(calls, usize::from(continuing));
        assert_eq!(collected, if continuing { vec![1, 2] } else { vec![1] });
    }
    let result = for_each_page(
        Some(Connection {
            nodes: vec![1],
            page_info: PageInfo {
                has_next_page: true,
                end_cursor: None,
            },
        }),
        |_| async { panic!("invalid initial cursor must not fetch") },
        |_| Ok(()),
        std::convert::identity,
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn cursor_pages_consume_before_fetch_and_reject_cycles() {
    let consumed = std::sync::Mutex::new(Vec::<usize>::new());
    let mut calls = 0;
    let values = collect_cursor_pages(
        |after| {
            calls += 1;
            let n = calls;
            assert_eq!(consumed.lock().unwrap().len(), n - 1);
            assert_eq!(after.as_deref(), if n == 1 { None } else { Some("1") });
            async move {
                Ok(CursorPage {
                    nodes: vec![n],
                    has_more: n == 1,
                })
            }
        },
        ToString::to_string,
        |nodes| {
            consumed.lock().unwrap().extend(&nodes);
            async move { Ok(nodes) }
        },
        std::convert::identity,
    )
    .await
    .unwrap();
    assert_eq!(values, vec![1, 2]);
    for ids in [vec![""], vec!["a", "a"], vec!["a", "b", "a"]] {
        let mut ids = ids.into_iter();
        let result = collect_cursor_pages(
            |_| {
                let id = ids.next().unwrap();
                async move {
                    Ok(CursorPage {
                        nodes: vec![id],
                        has_more: true,
                    })
                }
            },
            |id| (*id).to_owned(),
            |nodes| async move { Ok(nodes) },
            std::convert::identity,
        )
        .await;
        assert!(result.is_err());
    }
}

#[tokio::test]
async fn numbered_pages_validate_metadata_and_stop_on_errors() {
    for (page, total_pages, nodes) in [(2, 2, vec![1]), (1, 0, vec![1]), (1, 2, vec![])] {
        let result = for_each_numbered_page(
            |_| {
                let nodes = nodes.clone();
                async move {
                    Ok(NumberedPage {
                        nodes,
                        page,
                        total_pages,
                    })
                }
            },
            |_| Ok(()),
            std::convert::identity,
        )
        .await;
        assert!(result.is_err());
    }
    let mut calls = 0;
    let result = for_each_numbered_page(
        |page| {
            calls += 1;
            async move {
                Ok(NumberedPage {
                    nodes: vec![1],
                    page,
                    total_pages: 2,
                })
            }
        },
        |_| {
            Err(ClientError::new(ClientErrorKind::Protocol(
                "consumer failed",
            )))
        },
        std::convert::identity,
    )
    .await;
    assert!(result.is_err());
    assert_eq!(calls, 1);
    let mut pages = Vec::new();
    for_each_numbered_page(
        |page| async move {
            Ok(NumberedPage {
                nodes: vec![page],
                page,
                total_pages: 2,
            })
        },
        |nodes| {
            pages.extend(nodes);
            Ok(())
        },
        std::convert::identity,
    )
    .await
    .unwrap();
    assert_eq!(pages, vec![1, 2]);
}

#[tokio::test]
async fn id_cursor_driver_rejects_missing_cursor_and_stops_on_fetch_or_consumer_failure() {
    for failure in ["missing", "fetch", "consume"] {
        let mut calls = 0;
        let result = collect_cursor_pages(
            |_| {
                calls += 1;
                async move {
                    if failure == "fetch" {
                        return Err(ClientError::new(ClientErrorKind::Protocol("fetch failed")));
                    }
                    Ok(CursorPage {
                        nodes: if failure == "missing" {
                            vec![]
                        } else {
                            vec![1]
                        },
                        has_more: true,
                    })
                }
            },
            ToString::to_string,
            |nodes| async move {
                if failure == "consume" {
                    Err(ClientError::new(ClientErrorKind::Protocol(
                        "consumer failed",
                    )))
                } else {
                    Ok(nodes)
                }
            },
            std::convert::identity,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }
    let result: Vec<u8> = collect_cursor_pages(
        |_| async {
            Ok(CursorPage {
                nodes: vec![],
                has_more: false,
            })
        },
        ToString::to_string,
        |nodes| async move { Ok::<_, ClientError>(nodes) },
        std::convert::identity,
    )
    .await
    .unwrap();
    assert_eq!(result.len(), 0);
}

#[tokio::test]
async fn numbered_driver_accepts_empty_final_pages_and_rejects_later_failures() {
    for total_pages in [0, 1] {
        for_each_numbered_page(
            |page| async move {
                Ok(NumberedPage {
                    nodes: Vec::<u8>::new(),
                    page,
                    total_pages,
                })
            },
            |_| Ok(()),
            std::convert::identity,
        )
        .await
        .unwrap();
    }
    for failure in ["fetch", "total"] {
        let mut calls = 0;
        let result = for_each_numbered_page(
            |page| {
                calls += 1;
                async move {
                    if page == 2 && failure == "fetch" {
                        return Err(ClientError::new(ClientErrorKind::Protocol(
                            "later fetch failed",
                        )));
                    }
                    Ok(NumberedPage {
                        nodes: vec![1],
                        page,
                        total_pages: if page == 2 { 1 } else { 2 },
                    })
                }
            },
            |_| Ok(()),
            std::convert::identity,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(calls, 2);
    }
}
