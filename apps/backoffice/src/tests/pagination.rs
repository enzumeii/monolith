use super::*;
use domain::dto::{InventoryResponse, PageInfo};
use futures::executor::block_on;
use std::collections::VecDeque;
fn page(next: bool, cursor: Option<&str>) -> InventoryResponse {
    InventoryResponse {
        products: vec![],
        page_info: PageInfo {
            has_next_page: next,
            end_cursor: cursor.map(str::to_string),
        },
    }
}
#[test]
fn complete_inventory_follows_empty_pages_until_complete() {
    let mut calls = vec![];
    let mut pages = VecDeque::from([
        Ok(PageData::new(
            Vec::<String>::new(),
            page(true, Some("abc+/=")).page_info,
        )),
        Ok(PageData::new(
            Vec::<String>::new(),
            page(false, None).page_info,
        )),
    ]);
    let rows = block_on(collect_pages(|cursor| {
        calls.push(cursor);
        std::future::ready(pages.pop_front().expect("valid test fixture"))
    }))
    .expect("valid test fixture");
    assert_eq!(rows.len(), 0);
    assert_eq!(calls, [None, Some("abc+/=".into())]);
}
#[test]
fn incomplete_inventory_never_succeeds() {
    for mut pages in [
        VecDeque::from([Ok(PageData::new(
            Vec::<String>::new(),
            page(true, None).page_info,
        ))]),
        VecDeque::from([
            Ok(PageData::new(
                Vec::<String>::new(),
                page(true, Some("same")).page_info,
            )),
            Ok(PageData::new(
                Vec::<String>::new(),
                page(true, Some("same")).page_info,
            )),
        ]),
        VecDeque::from([
            Ok(PageData::new(
                Vec::<String>::new(),
                page(true, Some("next")).page_info,
            )),
            Err(crate::api::ApiError::new(
                reqwest::StatusCode::BAD_GATEWAY,
                "upstream failed",
            )),
        ]),
    ] {
        assert!(
            block_on(collect_pages(|_| std::future::ready(
                pages.pop_front().expect("valid test fixture")
            )))
            .is_err()
        );
    }
}

#[test]
fn generic_pagination_collects_rows_across_pages() {
    let mut calls = vec![];
    let mut pages = VecDeque::from([
        Ok(PageData::new(
            vec![1, 2],
            page(true, Some("next")).page_info,
        )),
        Ok(PageData::new(vec![3], page(false, None).page_info)),
    ]);
    let rows = block_on(collect_pages(|cursor| {
        calls.push(cursor);
        std::future::ready(pages.pop_front().expect("valid test fixture"))
    }))
    .expect("valid test fixture");
    assert_eq!(rows, [1, 2, 3]);
    assert_eq!(calls, [None, Some("next".into())]);
}
#[test]
fn cursor_history_returns_to_first_page_and_resets() {
    let mut history = CursorHistory::default();
    history
        .next(&page(true, Some("one")).page_info)
        .expect("valid test fixture");
    history
        .next(&page(true, Some("two")).page_info)
        .expect("valid test fixture");
    history.previous();
    assert_eq!(history.current.as_deref(), Some("one"));
    history.previous();
    assert_eq!(history.current, None);
    history
        .next(&page(true, Some("new")).page_info)
        .expect("valid test fixture");
    history.reset();
    assert_eq!(history.current, None);
    assert!(history.next(&page(true, None).page_info).is_err());
}
