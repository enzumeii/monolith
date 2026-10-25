use dioxus::prelude::*;

#[derive(Clone, Copy)]
struct LoadingState(Signal<usize>);

#[must_use = "Keep the guard alive until the operation completes"]
pub struct LoadingGuard(Option<Signal<usize>>);

pub fn loading_guard() -> LoadingGuard {
    if dioxus::dioxus_core::Runtime::try_current()
        .and_then(|runtime| runtime.try_current_scope_id())
        .is_none()
    {
        return LoadingGuard(None);
    }
    try_consume_context::<LoadingState>()
        .map_or(LoadingGuard(None), |state| LoadingGuard::start(state.0))
}

#[component]
pub fn LoadingBoundary(children: Element) -> Element {
    let active = use_signal(|| 0_usize);
    use_context_provider(|| LoadingState(active));
    let pending = active() > 0;
    rsx! {
        div { class: "loading-page", "inert": pending.then_some("inert"), aria_busy: pending,
            {children}
        }
        if pending { LoadingOverlay {} }
    }
}

#[component]
pub fn LoadingOverlay() -> Element {
    let (id, mounted) = crate::dialog::use_native_dialog(true, false);
    rsx! {
        dialog { id, class: "page-loading", aria_label: "Loading", tabindex: "-1",
            onmounted: move |_| mounted(()),
            oncancel: move |event| event.prevent_default(),
            onkeydown: move |event| event.prevent_default(),
            onwheel: move |event| event.prevent_default(),
            div { class: "page-loading-content", role: "status", aria_live: "polite",
                span { class: "page-loading-spinner", aria_hidden: "true" }
                p { "Working… Please wait." }
            }
        }
    }
}

impl LoadingGuard {
    #[cfg_attr(not(target_arch = "wasm32"), allow(clippy::unused_async))]
    pub async fn paint(&self) {
        #[cfg(target_arch = "wasm32")]
        if self.0.is_some() {
            let _ = document::eval(include_str!("loading.js"))
                .recv::<bool>()
                .await;
        }
    }

    fn start(mut active: Signal<usize>) -> Self {
        let started = if let Ok(mut count) = active.try_write() {
            *count += 1;
            true
        } else {
            false
        };
        Self(started.then_some(active))
    }
}

impl Drop for LoadingGuard {
    fn drop(&mut self) {
        if let Some(active) = self.0.as_mut()
            && let Ok(mut count) = active.try_write()
        {
            *count = count.saturating_sub(1);
        }
    }
}

#[cfg(test)]
#[path = "tests/loading.rs"]
mod tests;
