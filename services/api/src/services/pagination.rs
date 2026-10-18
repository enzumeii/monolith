use crate::{
    client::{ClientError, ClientErrorKind},
    models::shopify::Connection,
};
use std::collections::HashSet;

pub(crate) async fn for_each_page<T: Send, F, Fut, C>(
    mut fetch: F,
    mut consume: C,
) -> Result<(), ClientError>
where
    F: FnMut(Option<String>) -> Fut + Send,
    Fut: std::future::Future<Output = Result<Connection<T>, ClientError>> + Send,
    C: FnMut(Vec<T>) -> Result<(), ClientError> + Send,
{
    let mut after = None;
    let mut seen = HashSet::new();
    loop {
        let page = fetch(after.clone()).await?;
        page.page_info.validate_after(after.as_deref())?;
        consume(page.nodes)?;
        if !page.page_info.has_next_page {
            return Ok(());
        }
        let next = page
            .page_info
            .end_cursor
            .ok_or_else(|| ClientError::new(ClientErrorKind::Protocol("next cursor is missing")))?;
        if !seen.insert(next.clone()) {
            return Err(ClientError::new(ClientErrorKind::Protocol(
                "pagination cursor did not progress",
            )));
        }
        after = Some(next);
    }
}
#[cfg(test)]
#[path = "../tests/services/pagination.rs"]
mod tests;

pub(crate) async fn collect_connection<T: Send, F, Fut>(
    mut page: Connection<T>,
    mut fetch: F,
) -> Result<Vec<T>, ClientError>
where
    F: FnMut(Option<String>) -> Fut + Send,
    Fut: std::future::Future<Output = Result<Connection<T>, ClientError>> + Send,
{
    let mut nodes = Vec::new();
    let mut after = None;
    let mut seen = HashSet::new();
    loop {
        page.page_info.validate_after(after.as_deref())?;
        nodes.extend(page.nodes);
        if !page.page_info.has_next_page {
            return Ok(nodes);
        }
        after = page.page_info.end_cursor;
        if !seen.insert(after.clone()) {
            return Err(ClientError::new(ClientErrorKind::Protocol(
                "nested cursor did not progress",
            )));
        }
        page = fetch(after.clone()).await?;
    }
}
