#![allow(clippy::unwrap_used)]
use super::*;
use crate::models::shopify::PageInfo;
#[tokio::test]
async fn child_collection_rejects_missing_empty_repeated_and_cyclic_cursors() {
    for cursors in [
        vec![None],
        vec![Some("")],
        vec![Some("a"), Some("a")],
        vec![Some("a"), Some("b"), Some("a")],
    ] {
        let mut iter = cursors.into_iter();
        let result = collect_connection(|_| {
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
        })
        .await;
        assert!(result.is_err());
    }
}
#[tokio::test]
async fn child_collection_keeps_nodes_in_order_and_never_returns_partial_success() {
    let mut count = 0;
    let all = collect_connection(|after| {
        count += 1;
        let n = count;
        async move {
            assert_eq!(after.as_deref(), if n == 1 { None } else { Some("a") });
            Ok(Connection {
                nodes: vec![n],
                page_info: PageInfo {
                    has_next_page: n == 1,
                    end_cursor: if n == 1 { Some("a".into()) } else { None },
                },
            })
        }
    })
    .await
    .unwrap();
    assert_eq!(all.nodes, vec![1, 2]);
    let mut count = 0;
    let failed = collect_connection(|_| {
        count += 1;
        let n = count;
        async move {
            if n == 2 {
                return Err(ClientError::Protocol("child failure"));
            }
            Ok(Connection {
                nodes: vec![n],
                page_info: PageInfo {
                    has_next_page: true,
                    end_cursor: Some("a".into()),
                },
            })
        }
    })
    .await;
    assert!(failed.is_err());
}
