use crate::api::{ApiError, ApiResult};
use domain::dto::{InventoryResponse, PageInfo, ProductReportRow};
use std::{collections::HashSet, future::Future};

pub async fn collect_inventory<F, Fut>(mut fetch: F) -> ApiResult<Vec<ProductReportRow>>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: Future<Output = ApiResult<InventoryResponse>>,
{
    let mut rows = Vec::new();
    let mut cursor = None;
    let mut seen = HashSet::new();
    loop {
        let page = fetch(cursor.clone()).await?;
        rows.extend(page.products);
        if !page.page_info.has_next_page {
            return Ok(rows);
        }
        let next = continuation(&page.page_info)?;
        if !seen.insert(next.clone()) {
            return Err(incomplete());
        }
        cursor = Some(next);
    }
}

#[derive(Clone, Default, PartialEq)]
pub struct CursorHistory {
    pub current: Option<String>,
    previous: Vec<Option<String>>,
}
impl CursorHistory {
    pub fn next(&mut self, info: &PageInfo) -> ApiResult<()> {
        let next = continuation(info)?;
        if self.current.as_ref() == Some(&next)
            || self
                .previous
                .iter()
                .any(|cursor| cursor.as_ref() == Some(&next))
        {
            return Err(incomplete());
        }
        self.previous.push(self.current.take());
        self.current = Some(next);
        Ok(())
    }
    pub fn previous(&mut self) {
        if let Some(cursor) = self.previous.pop() {
            self.current = cursor;
        }
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn has_previous(&self) -> bool {
        !self.previous.is_empty()
    }
}
fn continuation(info: &PageInfo) -> ApiResult<String> {
    if !info.has_next_page {
        return Err(incomplete());
    }
    info.end_cursor
        .clone()
        .filter(|cursor| !cursor.trim().is_empty())
        .ok_or_else(incomplete)
}
fn incomplete() -> ApiError {
    ApiError::new(
        reqwest::StatusCode::BAD_GATEWAY,
        "Incomplete report: invalid pagination cursor",
    )
}
#[cfg(test)]
#[path = "../../tests/pages/reports/data.rs"]
mod tests;
