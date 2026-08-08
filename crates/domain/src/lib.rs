//! Shared domain types for the whole workspace.
//!
//! Nothing here may depend on axum, dioxus, the api service, or any app crate.
//! Pure data: Money, Product, Order, Inventory, Report aggregates, ids.
//! Serde round-trip is the contract between apps and the api service.

#![allow(dead_code)] // types are populated feature by feature; remove later