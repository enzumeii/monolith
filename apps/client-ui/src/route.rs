use crate::pages::{CartPage, CataloguePage, Directory, NotFound, PortalLayout, ProductPage};
use dioxus::prelude::*;

#[derive(Clone, Debug, PartialEq, Routable)]
pub enum Route {
    #[layout(PortalLayout)]
    #[route("/")]
    Directory {},
    #[route("/:slug")]
    CataloguePage { slug: String },
    #[route("/:slug/products/:product_id")]
    ProductPage { slug: String, product_id: String },
    #[route("/:slug/cart/:checkout_id")]
    CartPage { slug: String, checkout_id: String },
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}
