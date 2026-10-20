use super::*;
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
    let mut pages = VecDeque::from([Ok(page(true, Some("abc+/="))), Ok(page(false, None))]);
    let rows = block_on(collect_inventory(|cursor| {
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
        VecDeque::from([Ok(page(true, None))]),
        VecDeque::from([Ok(page(true, Some("same"))), Ok(page(true, Some("same")))]),
        VecDeque::from([
            Ok(page(true, Some("next"))),
            Err(crate::api::ApiError::new(
                reqwest::StatusCode::BAD_GATEWAY,
                "upstream failed",
            )),
        ]),
    ] {
        assert!(
            block_on(collect_inventory(|_| std::future::ready(
                pages.pop_front().expect("valid test fixture")
            )))
            .is_err()
        );
    }
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
