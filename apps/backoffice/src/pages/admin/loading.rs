use domain::dto::AdminListResponse;

#[derive(Default)]
pub(super) struct ListState {
    generation: u64,
    pub data: Option<AdminListResponse>,
    pub error: Option<String>,
    pub loading: bool,
}
impl ListState {
    pub fn begin(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.loading = true;
        self.error = None;
        self.generation
    }
    pub fn finish(&mut self, request: u64, result: Result<AdminListResponse, String>) -> bool {
        if request != self.generation {
            return false;
        }
        self.loading = false;
        match result {
            Ok(data) => {
                self.data = Some(data);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::dto::{AdminDTO, AdminListResponse, AdminRole};
    fn page(number: u32) -> AdminListResponse {
        AdminListResponse {
            admins: vec![AdminDTO {
                id: i64::from(number),
                username: format!("page-{number}"),
                role: AdminRole::Admin,
            }],
            page: number,
            per_page: 25,
            has_next: number < 3,
        }
    }
    #[test]
    fn older_page_success_and_error_cannot_overwrite_latest_results() {
        let mut state = ListState::default();
        let second = state.begin();
        let third = state.begin();
        assert!(state.finish(third, Ok(page(3))));
        assert!(!state.finish(second, Ok(page(2))));
        assert!(!state.finish(second, Err("stale error".into())));
        assert_eq!(state.data.as_ref().expect("latest page").page, 3);
        assert!(!state.data.as_ref().expect("latest page").has_next);
        assert!(state.error.is_none());
        assert!(!state.loading);
    }
}
