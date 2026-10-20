pub mod admin;
pub mod accounting;
pub mod auth;
pub mod error;

pub use admin::{
    AdminDTO, AdminListResponse, AdminRole, CreateAdminRequest, ListAdminsQuery, UpdateAdminRequest,
};
pub use accounting::{
    AccountingGateway, AccountingItem, AccountingKind, AccountingReportQuery,
    AccountingReportResponse, AccountingTransaction,
};
pub use auth::{LoginRequest, RefreshRequest, SessionResponse, TokenResponse};
pub use error::ErrorResponse;

pub mod products;
pub use products::ProductReportRow;
pub mod inventories;
pub mod orders;
pub use inventories::{
    InventoryEnableError, InventoryEnableResponse, InventoryQuery, InventoryResponse,
};
pub use orders::{OrderReportQuery, OrderReportResponse};

pub mod pagination;

pub use pagination::PageInfo;
