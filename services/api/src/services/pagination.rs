use crate::{client::ClientError, models::shopify::Connection};
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
            .ok_or(ClientError::Protocol("next cursor is missing"))?;
        if !seen.insert(next.clone()) {
            return Err(ClientError::Protocol("pagination cursor did not progress"));
        }
        after = Some(next);
    }
}
#[cfg(test)]
#[path = "../tests/services/pagination.rs"]
mod tests;
