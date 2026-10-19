use crate::{
    client::{ClientError, ClientErrorKind},
    models::shopify::Connection,
};
use std::collections::HashSet;

pub(crate) async fn for_each_page<T: Send, E, F, Fut, C, M>(
    mut initial: Option<Connection<T>>,
    mut fetch: F,
    mut consume: C,
    mut pagination_error: M,
) -> Result<(), E>
where
    F: FnMut(Option<String>) -> Fut + Send,
    Fut: std::future::Future<Output = Result<Connection<T>, E>> + Send,
    C: FnMut(Vec<T>) -> Result<(), E> + Send,
    M: FnMut(ClientError) -> E + Send,
{
    let mut after = None;
    let mut seen = HashSet::new();
    loop {
        let page = match initial.take() {
            Some(page) => page,
            None => fetch(after.clone()).await?,
        };
        page.page_info
            .validate_after(after.as_deref())
            .map_err(&mut pagination_error)?;
        consume(page.nodes)?;
        if !page.page_info.has_next_page {
            return Ok(());
        }
        let next = page
            .page_info
            .end_cursor
            .ok_or_else(|| ClientError::new(ClientErrorKind::Protocol("next cursor is missing")))
            .map_err(&mut pagination_error)?;
        if !seen.insert(next.clone()) {
            return Err(pagination_error(ClientError::new(
                ClientErrorKind::Protocol("pagination cursor did not progress"),
            )));
        }
        after = Some(next);
    }
}
#[cfg(test)]
#[path = "../tests/services/pagination.rs"]
mod tests;
