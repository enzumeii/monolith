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
        page.validate_after(after.as_deref())
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
pub(crate) struct CursorPage<T> {
    pub nodes: Vec<T>,
    pub has_more: bool,
}

pub(crate) struct NumberedPage<T> {
    pub nodes: Vec<T>,
    pub page: u32,
    pub total_pages: u32,
}

pub(crate) async fn collect_cursor_pages<T, U, E, F, Fut, I, C, CFut, M>(
    mut fetch: F,
    mut item_id: I,
    mut consume: C,
    mut pagination_error: M,
) -> Result<Vec<U>, E>
where
    T: Send,
    U: Send,
    F: FnMut(Option<String>) -> Fut + Send,
    Fut: std::future::Future<Output = Result<CursorPage<T>, E>> + Send,
    I: FnMut(&T) -> String + Send,
    C: FnMut(Vec<T>) -> CFut + Send,
    CFut: std::future::Future<Output = Result<Vec<U>, E>> + Send,
    M: FnMut(ClientError) -> E + Send,
{
    let mut after = None;
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    loop {
        let page = fetch(after).await?;
        let next = page.nodes.last().map(&mut item_id);
        output.extend(consume(page.nodes).await?);
        if !page.has_more {
            return Ok(output);
        }
        let next = next
            .filter(|next| !next.is_empty() && seen.insert(next.clone()))
            .ok_or_else(|| {
                pagination_error(ClientError::new(ClientErrorKind::Protocol(
                    "pagination cursor did not progress",
                )))
            })?;
        after = Some(next);
    }
}

pub(crate) async fn for_each_numbered_page<T, E, F, Fut, C, M>(
    mut fetch: F,
    mut consume: C,
    mut pagination_error: M,
) -> Result<(), E>
where
    T: Send,
    F: FnMut(u32) -> Fut + Send,
    Fut: std::future::Future<Output = Result<NumberedPage<T>, E>> + Send,
    C: FnMut(Vec<T>) -> Result<(), E> + Send,
    M: FnMut(ClientError) -> E + Send,
{
    let mut requested = 1;
    loop {
        let page = fetch(requested).await?;
        if page.page != requested
            || (page.total_pages == 0 && !page.nodes.is_empty())
            || (page.total_pages > 0 && requested > page.total_pages)
            || (requested < page.total_pages && page.nodes.is_empty())
        {
            return Err(pagination_error(ClientError::new(
                ClientErrorKind::Protocol("invalid numbered pagination metadata"),
            )));
        }
        consume(page.nodes)?;
        if requested >= page.total_pages {
            return Ok(());
        }
        requested = requested.checked_add(1).ok_or_else(|| {
            pagination_error(ClientError::new(ClientErrorKind::Protocol(
                "invalid numbered pagination metadata",
            )))
        })?;
    }
}

#[cfg(test)]
#[path = "../tests/services/pagination.rs"]
mod tests;
