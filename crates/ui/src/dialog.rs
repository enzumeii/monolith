use dioxus::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIALOG: AtomicUsize = AtomicUsize::new(1);

#[must_use]
pub fn use_native_dialog(open: bool, dismiss_on_backdrop: bool) -> (String, Callback<()>) {
    let id = use_hook(|| format!("dialog-{}", NEXT_DIALOG.fetch_add(1, Ordering::Relaxed)));
    let mut dialog = use_signal(|| None::<document::Eval>);
    use_effect(use_reactive!(|open| {
        if let Some(dialog) = *dialog.peek() {
            let _ = dialog.send(Some(open));
        }
    }));
    use_drop(move || {
        if let Some(dialog) = *dialog.peek() {
            let _ = dialog.send(None::<bool>);
        }
    });
    let mounted_id = id.clone();
    let mounted = use_callback(move |()| {
        let bridge = document::eval(include_str!("dialog.js"));
        let _ = bridge.send((&mounted_id, Some(open), dismiss_on_backdrop));
        dialog.set(Some(bridge));
    });
    (id, mounted)
}
