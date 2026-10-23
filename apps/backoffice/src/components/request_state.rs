use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RequestState {
    revision: u64,
    pending: Option<u64>,
}
impl RequestState {
    pub fn begin(&mut self) -> Option<u64> {
        if self.pending.is_some() {
            return None;
        }
        self.revision = self.revision.wrapping_add(1);
        self.pending = Some(self.revision);
        self.pending
    }
    pub fn edited(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.pending = None;
    }
    pub fn accepts(&self, ticket: u64) -> bool {
        self.pending == Some(ticket) && self.revision == ticket
    }
    pub fn finish(&mut self, ticket: u64) -> bool {
        if !self.accepts(ticket) {
            return false;
        }
        self.pending = None;
        true
    }
    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }
}

pub fn use_owner() -> Memo<Option<(i64, domain::AdminRole)>> {
    use_memo(move || {
        crate::session::SESSION
            .read()
            .as_ref()
            .map(|viewer| (viewer.admin_id, viewer.role))
    })
}

#[cfg(test)]
#[path = "../tests/components/request_state.rs"]
mod tests;
