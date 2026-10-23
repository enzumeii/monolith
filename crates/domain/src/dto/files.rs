use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UploadFileQuery {
    pub location: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct UploadedFileDTO {
    pub location: String,
    pub url: String,
    pub expires_at: String,
}
