use super::Session;

#[cfg(target_arch = "wasm32")]
mod browser {
    use gloo_storage::{LocalStorage, Storage};

    use super::Session;

    const KEY: &str = "backoffice.session";

    pub fn save(session: &Session) {
        let _ = LocalStorage::set(KEY, session);
    }

    pub fn load() -> Option<Session> {
        LocalStorage::get(KEY).ok()
    }

    pub fn clear() {
        LocalStorage::delete(KEY);
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::Session;

    pub fn save(_session: &Session) {}

    pub fn load() -> Option<Session> {
        None
    }

    pub fn clear() {}
}

#[cfg(target_arch = "wasm32")]
pub(super) use browser::{clear, load, save};
#[cfg(not(target_arch = "wasm32"))]
pub(super) use native::{clear, load, save};
