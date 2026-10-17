use crate::{client::ClientError, models::shopify::Connection};
use std::collections::HashSet;

pub(crate) async fn collect_connection<T: Send, F, Fut>(
    mut fetch: F,
) -> Result<Connection<T>, ClientError>
where
    F: FnMut(Option<String>) -> Fut + Send,
    Fut: std::future::Future<Output = Result<Connection<T>, ClientError>> + Send,
{
    let mut nodes = Vec::new();
    let mut after = None;
    let mut seen = HashSet::new();
    loop {
        let page = fetch(after.clone()).await?;
        page.page_info.validate_after(after.as_deref())?;
        nodes.extend(page.nodes);
        if !page.page_info.has_next_page {
            return Ok(Connection {
                nodes,
                page_info: page.page_info,
            });
        }
        let next = page
            .page_info
            .end_cursor
            .filter(|value| !value.is_empty())
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
