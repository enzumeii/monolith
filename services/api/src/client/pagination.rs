use crate::{client::ClientError, models::client::Connection};
use std::collections::HashSet;

pub(crate) async fn collect_pages<T, F>(mut fetch: F) -> Result<Vec<T>, ClientError>
where
    F: AsyncFnMut(Option<String>) -> Result<Connection<T>, ClientError>,
{
    let mut nodes = Vec::new();
    let mut after = None;
    let mut seen = HashSet::new();
    loop {
        let page = fetch(after).await?;
        nodes.extend(page.nodes);
        if !page.page_info.has_next_page {
            return Ok(nodes);
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
