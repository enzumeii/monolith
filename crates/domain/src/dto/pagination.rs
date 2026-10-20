use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PageInfo {
    #[serde(alias = "hasNextPage")]
    pub has_next_page: bool,
    #[serde(alias = "endCursor")]
    pub end_cursor: Option<String>,
}
