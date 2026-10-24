use crate::checkout::{CheckoutCancelPage, CheckoutReturnPage};
use crate::pages::{CartPage, CataloguePage, Directory, NotFound, PortalLayout};
use dioxus::prelude::*;

#[derive(Clone, Debug, PartialEq, Routable)]
pub enum Route {
    #[layout(PortalLayout)]
    #[route("/")]
    Directory {},
    #[route("/:slug")]
    CataloguePage { slug: String },
    #[route("/:slug/cart/:checkout_id")]
    CartPage { slug: String, checkout_id: String },
    #[route("/:slug/checkout/return?:..query")]
    CheckoutReturnPage { slug: String, query: String },
    #[route("/:slug/checkout/cancel?:..query")]
    CheckoutCancelPage { slug: String, query: String },
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}
