use dioxus::prelude::*;

use crate::{
    components::NavLayout,
    pages::{Accounting, Admins, Home, NotFound, Orders, Profile, Reports, Triggers},
};

#[derive(Clone, Debug, PartialEq, Routable)]
pub enum Route {
    #[layout(NavLayout)]
    #[route("/")]
    Home {},
    #[route("/admins")]
    Admins {},
    #[route("/reports")]
    Reports {},
    #[route("/accounting")]
    Accounting {},
    #[route("/orders")]
    Orders {},
    #[route("/triggers")]
    Triggers {},
    #[route("/profile")]
    Profile {},
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}
