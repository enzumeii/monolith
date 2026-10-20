pub mod accounting;
pub mod admin;
pub mod auth;
pub mod cron;
pub mod error;

pub use accounting::{
    AccountingGateway, AccountingItem, AccountingKind, AccountingReportQuery,
    AccountingReportResponse, AccountingTransaction,
};
pub use admin::{
    AdminDTO, AdminListResponse, AdminRole, CreateAdminRequest, ListAdminsQuery, UpdateAdminRequest,
};
pub use auth::{LoginRequest, RefreshRequest, SessionResponse, TokenResponse};
pub use cron::{
    CreateCronConfiguration, CronConfigurationDTO, CronConfigurationListResponse,
    CronConfigurationPreview, CronOccurrencePreview, CronPeriodPreview, EmailStatus,
    GenerationStatus, ListCronConfigurationsQuery, ListReportResultsQuery, PatchCronConfiguration,
    PatchField, PreviewCronConfiguration, ReportResultDTO, ReportResultDetail, ReportRowsQuery,
    ReportRowsResponse, SchedulePeriodMode, ScheduleReportType, ScheduleStatus,
};
pub use error::ErrorResponse;

pub mod products;
pub use products::ProductReportRow;
pub mod inventories;
pub mod orders;
pub use inventories::{
    InventoryEnableError, InventoryEnableResponse, InventoryQuery, InventoryResponse,
};
pub use orders::{OrderReportQuery, OrderReportResponse};

#[cfg(test)]
#[path = "tests/dto/cron.rs"]
mod cron_tests;

pub mod pagination;

pub use pagination::PageInfo;
