use dioxus::prelude::*;

use crate::{
    components::NavLayout,
    pages::{
        Admins, ConventionCreate, ConventionDetail, Conventions, Home, NotFound, Orders, Profile,
        Reports, ScheduledReportDetail, ScheduledReportHistory, ScheduledReports, Triggers,
    },
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
    #[route("/orders")]
    Orders {},
    #[route("/triggers")]
    Triggers {},
    #[route("/scheduled-reports?:..query")]
    ScheduledReports { query: String },
    #[route("/scheduled-reports/history?:..query")]
    ScheduledReportHistory { query: String },
    #[route("/scheduled-reports/history/:id?:..query")]
    ScheduledReportDetail { id: String, query: String },
    #[route("/conventions?:..query")]
    Conventions { query: String },
    #[route("/conventions/new")]
    ConventionCreate {},
    #[route("/conventions/:id")]
    ConventionDetail { id: String },
    #[route("/profile")]
    Profile {},
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}
