use crate::{components::Confirm, route::Route, session};
use dioxus::{
    history::{History, history, provide_history_context},
    prelude::*,
};
use serde::{Deserialize, Serialize};
use std::{rc::Rc, sync::Arc};

type Owner = Option<(i64, domain::AdminRole)>;
static NAVIGATION: GlobalSignal<DraftNavigation> = GlobalSignal::new(DraftNavigation::default);

#[derive(Clone, Debug, Default, PartialEq)]
struct DraftNavigation {
    owner: Owner,
    source: Option<Route>,
    pending: Option<Route>,
    native_key: Option<String>,
    dirty: bool,
    restoration_required: bool,
    native_confirming: bool,
}

pub fn use_draft_history() {
    use_hook(|| {
        let inner = history();
        provide_history_context(Rc::new(DraftHistory { inner }));
    });
}

struct DraftHistory {
    inner: Rc<dyn History>,
}
impl History for DraftHistory {
    fn current_route(&self) -> String {
        let actual = self.inner.current_route();
        let owner = session::SESSION
            .peek()
            .as_ref()
            .map(|viewer| (viewer.admin_id, viewer.role));
        if let Ok(target) = actual.parse::<Route>()
            && let Some(source) = NAVIGATION.peek().retained_source(owner, &target)
        {
            return source.to_string();
        }
        actual
    }
    fn current_prefix(&self) -> Option<String> {
        self.inner.current_prefix()
    }
    fn can_go_back(&self) -> bool {
        self.inner.can_go_back()
    }
    fn can_go_forward(&self) -> bool {
        self.inner.can_go_forward()
    }
    fn go_back(&self) {
        self.inner.go_back();
    }
    fn go_forward(&self) {
        self.inner.go_forward();
    }
    fn push(&self, route: String) {
        if guarded_target(&route).is_none() {
            self.inner.push(route);
        }
    }
    fn replace(&self, route: String) {
        if guarded_target(&route).is_none() {
            self.inner.replace(route);
        }
    }
    fn external(&self, url: String) -> bool {
        self.inner.external(url)
    }
    fn updater(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.inner.updater(callback);
    }
    fn include_prevent_default(&self) -> bool {
        self.inner.include_prevent_default()
    }
}

#[component]
pub fn DraftDepartureGuard(
    dirty: bool,
    #[props(default = "Your unsaved schedule changes will be lost if you leave this page.".to_owned())]
    warning: String,
) -> Element {
    let owner = super::request_state::use_owner();
    let route = use_route::<Route>();
    let navigator = use_navigator();
    let mut bridge = use_hook(|| document::eval(include_str!("draft_navigation.js")));
    let mut native_error = use_signal(|| None::<String>);
    let mut restoration_pending = use_signal(|| false);
    use_future(move || async move {
        while let Ok(notice) = bridge.recv::<NativeNotice>().await {
            let current_owner = session::SESSION
                .peek()
                .as_ref()
                .map(|viewer| (viewer.admin_id, viewer.role));
            if let Some(required) = notice.restoration {
                NAVIGATION
                    .write()
                    .restoration_notice(current_owner, required);
                restoration_pending.set(required && notice.error.is_none());
                if !required {
                    native_error.set(None);
                }
            }
            if let Some(error) = notice.error {
                native_error.set(Some(error));
                NAVIGATION.write().native_confirming = false;
            }
            if let (Some(key), Some(route)) = (notice.key, notice.route)
                && let Ok(target) = route.parse::<Route>()
            {
                NAVIGATION
                    .write()
                    .native_departure(current_owner, key, target);
            }
        }
    });
    use_effect(use_reactive((&dirty, &route), move |(dirty, route)| {
        let owner = owner();
        NAVIGATION.write().observe(owner, route.clone(), dirty);
        before_unload(dirty && owner.is_some());
        let _ = bridge.send(NativeCommand::Observe {
            dirty: dirty && owner.is_some(),
            prefix: history().current_prefix(),
            protected_path: route_path(&route),
        });
        if !dirty {
            native_error.set(None);
        }
    }));
    use_drop(move || {
        *NAVIGATION.write() = DraftNavigation::default();
        before_unload(false);
        let _ = bridge.send(NativeCommand::Stop);
    });
    rsx! {
        crate::components::ErrorMessage { message: native_error() }
        if NAVIGATION.read().restoration_required {
            p { role: "status", "Restore the editor location before continuing to another page." }
            button { r#type: "button", class: "ghost", disabled: restoration_pending(),
                onclick: move |_| {
                    if restoration_pending() { return; }
                    restoration_pending.set(true);
                    if bridge.send(NativeCommand::RetryRestore).is_err() { restoration_pending.set(false); }
                },
                if restoration_pending() { "Restoring editor…" } else { "Retry restoring editor" }
            }
        }
        Confirm {
            open: NAVIGATION.read().pending.is_some() && !NAVIGATION.read().restoration_required,
            title: "Discard unsaved changes?",
            message: native_error().unwrap_or_else(|| warning.clone()),
            confirm_label: "Discard changes and leave", danger: true,
            on_cancel: move |()| {
                if NAVIGATION.peek().native_confirming { return; }
                NAVIGATION.write().cancel();
                let _ = bridge.send(NativeCommand::Cancel);
            },
            on_confirm: move |()| {
                if NAVIGATION.peek().restoration_required { return; }
                let native_key = NAVIGATION.peek().native_key.clone();
                if let Some(key) = native_key {
                    if NAVIGATION.peek().native_confirming { return; }
                    NAVIGATION.write().native_confirming = true;
                    if bridge.send(NativeCommand::Confirm { key }).is_err() {
                        NAVIGATION.write().native_confirming = false;
                        native_error.set(Some("Unable to open that history entry. Your draft remains open.".into()));
                    }
                } else {
                    confirm_in_app_departure(|target| { navigator.push(target); });
                }
            },
        }
    }
}

impl DraftNavigation {
    fn observe(&mut self, owner: Owner, source: Route, dirty: bool) {
        if owner.is_none() || self.owner != owner {
            *self = Self::default();
        }
        if dirty
            && self.dirty
            && self.owner == owner
            && (self.restoration_required || !self.same_editor(&source))
        {
            return;
        }
        if self.source.as_ref() != Some(&source) {
            self.pending = None;
            self.native_key = None;
        }
        self.owner = owner;
        self.source = Some(source);
        self.dirty = dirty && owner.is_some();
        if !self.dirty {
            self.pending = None;
            self.native_key = None;
            self.restoration_required = false;
            self.native_confirming = false;
        }
    }
    fn retained_source(&self, owner: Owner, target: &Route) -> Option<Route> {
        if !self.dirty || owner.is_none() || self.owner != owner {
            return None;
        }
        if self.native_confirming
            && !self.restoration_required
            && self.pending.as_ref() == Some(target)
        {
            return None;
        }
        if !self.restoration_required && self.same_editor(target) {
            return None;
        }
        self.source.clone()
    }
    fn restoration_notice(&mut self, owner: Owner, required: bool) {
        if self.dirty && owner.is_some() && self.owner == owner {
            self.restoration_required = required;
            self.native_confirming = false;
        }
    }
    fn native_departure(&mut self, owner: Owner, key: String, target: Route) {
        if self.dirty && owner.is_some() && self.owner == owner {
            self.pending = Some(target);
            self.native_key = Some(key);
        }
    }
    fn navigate(&mut self, owner: Owner, target: Route) -> Option<Route> {
        if owner.is_none() || owner != self.owner {
            *self = Self::default();
            return None;
        }
        let source = self.source.clone()?;
        if self.dirty && self.restoration_required {
            return Some(source);
        }
        if !self.dirty || source == target {
            return None;
        }
        if self.same_editor(&target) {
            self.source = Some(target);
            self.pending = None;
            self.native_key = None;
            return None;
        }
        self.pending = Some(target);
        self.native_key = None;
        Some(source)
    }
    fn same_editor(&self, target: &Route) -> bool {
        self.source
            .as_ref()
            .is_some_and(|source| route_path(source) == route_path(target))
    }
    fn cancel(&mut self) {
        self.pending = None;
        self.native_key = None;
    }
    fn confirm(&mut self) -> Option<Route> {
        if self.restoration_required {
            return None;
        }
        let target = self.pending.take();
        if target.is_some() {
            *self = Self::default();
        }
        target
    }
}

#[derive(Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum NativeCommand {
    Observe {
        dirty: bool,
        prefix: Option<String>,
        protected_path: String,
    },
    Cancel,
    RetryRestore,
    Confirm {
        key: String,
    },
    Stop,
}

#[derive(Deserialize)]
struct NativeNotice {
    key: Option<String>,
    route: Option<String>,
    error: Option<String>,
    restoration: Option<bool>,
}

fn confirm_in_app_departure(navigate: impl FnOnce(Route)) {
    let target = NAVIGATION.write().confirm();
    if let Some(target) = target {
        before_unload(false);
        navigate(target);
    }
}

fn guarded_target(raw: &str) -> Option<Route> {
    let target = raw.parse::<Route>().ok()?;
    let owner = session::SESSION
        .peek()
        .as_ref()
        .map(|viewer| (viewer.admin_id, viewer.role));
    {
        let guard = NAVIGATION.peek();
        let changed_owner = guard.owner.is_some() && guard.owner != owner;
        let changed_route =
            guard.dirty && (guard.restoration_required || guard.source.as_ref() != Some(&target));
        if !changed_owner && !changed_route {
            return None;
        }
    }
    NAVIGATION.write().navigate(owner, target)
}

fn route_path(route: &Route) -> String {
    route
        .to_string()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn before_unload(dirty: bool) {
    document::eval(&format!(
        r"
        if (window.enzDraftBeforeUnload) {{
            window.removeEventListener('beforeunload', window.enzDraftBeforeUnload);
            delete window.enzDraftBeforeUnload;
        }}
        if ({dirty}) {{
            window.enzDraftBeforeUnload = event => {{ event.preventDefault(); event.returnValue = ''; }};
            window.addEventListener('beforeunload', window.enzDraftBeforeUnload);
        }}
    "
    ));
}

#[cfg(test)]
#[path = "../tests/components/draft_navigation.rs"]
mod tests;
