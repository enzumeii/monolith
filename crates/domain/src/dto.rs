pub mod common;
pub use common::PatchField;
pub mod conventions;
pub use conventions::{
    ConventionDTO, ConventionListResponse, ConventionPaymentMethod, ConventionVariantDTO,
    ConventionVariantInput, ConventionVariantListResponse, ConventionVariantsResponse,
    CreateConvention, ListConventionVariantsQuery, ListConventionsQuery, PatchConvention,
    ReplaceConventionVariants,
};
pub mod accounting;
pub mod admin;
pub mod auth;
pub mod cron;
pub mod error;
pub mod report_results;

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
    CronConfigurationPreview, CronOccurrencePreview, CronPeriodPreview,
    ListCronConfigurationsQuery, PatchCronConfiguration, PreviewCronConfiguration,
    SchedulePeriodMode, ScheduleReportType,
};
pub use error::ErrorResponse;
pub use report_results::{
    ListReportResultsQuery, ReportDownloadUrlDTO, ReportFileDTO, ReportResultDTO, ReportStatus,
};

pub mod products;
pub use products::ProductReportRow;
pub mod inventories;
pub mod orders;
pub use inventories::{
    InventoryEnableError, InventoryEnableResponse, InventoryQuery, InventoryResponse,
};
pub use orders::{OrderReportQuery, OrderReportResponse};

pub mod catalogue;
pub use catalogue::{
    CatalogueImage, CatalogueInventoryPolicy, CatalogueOption, CataloguePrice, CatalogueProduct,
    CatalogueQuery, CatalogueResponse, CatalogueVariant,
};

pub mod pagination;

pub use pagination::PageInfo;

#[cfg(test)]
#[path = "tests/dto/cron.rs"]
mod cron_tests;

#[cfg(test)]
#[path = "tests/dto/report_results.rs"]
mod report_results_tests;

#[cfg(test)]
#[path = "tests/dto/conventions.rs"]
mod convention_tests;
