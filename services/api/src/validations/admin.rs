use crate::error::AppError;
use domain::dto::{AdminPageRequest, Claims, ListAdminsQuery};
use domain::entities::AdminRole;

/// Map the raw query params to a validated `AdminPageRequest`.
/// Applies defaults (`page=1`, `per_page=25`) and enforces 1..=100.
pub fn validate_page(query: &ListAdminsQuery) -> Result<AdminPageRequest, AppError> {
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(25);
    if page == 0 || per_page == 0 || per_page > 100 {
        return Err(AppError::BadRequest(
            "page must be positive and per_page must be between 1 and 100".into(),
        ));
    }
    Ok(AdminPageRequest { page, per_page })
}

/// Ensure the caller may update the target account.
///
/// `SuperAdmin` may update any account; a regular `Admin` may only update
/// their own account. Anything else is rejected with `Forbidden`.
pub fn validate_update(claims: &Claims, target_id: i64) -> Result<(), AppError> {
    let allowed = claims.role == AdminRole::SuperAdmin || claims.admin_id == target_id;
    if !allowed {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

/// Ensure the caller is not deleting their own account.
pub fn validate_delete(claims: &Claims, target_id: i64) -> Result<(), AppError> {
    if claims.admin_id == target_id {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use domain::entities::AdminRole;

    fn query(page: Option<u32>, per_page: Option<u32>) -> ListAdminsQuery {
        ListAdminsQuery { page, per_page }
    }

    fn claims(admin_id: i64, role: AdminRole) -> Claims {
        Claims {
            admin_id,
            username: "admin".into(),
            role,
            expiration: 0,
        }
    }

    #[test]
    fn validate_page_applies_defaults_when_omitted() {
        let page = validate_page(&query(None, None)).unwrap();
        assert_eq!(page.page, 1);
        assert_eq!(page.per_page, 25);
    }

    #[test]
    fn validate_page_rejects_zero_page() {
        assert!(matches!(
            validate_page(&query(Some(0), Some(25))),
            Err(AppError::BadRequest(_))
        ));
    }

    #[test]
    fn validate_page_rejects_zero_per_page() {
        assert!(matches!(
            validate_page(&query(Some(1), Some(0))),
            Err(AppError::BadRequest(_))
        ));
    }

    #[test]
    fn validate_page_rejects_per_page_over_100() {
        assert!(matches!(
            validate_page(&query(Some(1), Some(101))),
            Err(AppError::BadRequest(_))
        ));
    }

    #[test]
    fn validate_page_accepts_boundary_values() {
        assert!(validate_page(&query(Some(1), Some(1))).is_ok());
        assert!(validate_page(&query(Some(1), Some(100))).is_ok());
    }

    #[test]
    fn validate_update_passes_when_ids_match() {
        assert!(validate_update(&claims(1, AdminRole::Admin), 1).is_ok());
    }

    #[test]
    fn validate_update_returns_forbidden_when_ids_mismatch() {
        assert!(matches!(
            validate_update(&claims(1, AdminRole::Admin), 2),
            Err(AppError::Forbidden)
        ));
    }

    #[test]
    fn validate_update_super_admin_passes_for_self() {
        assert!(validate_update(&claims(1, AdminRole::SuperAdmin), 1).is_ok());
    }

    #[test]
    fn validate_update_super_admin_passes_for_other_account() {
        assert!(validate_update(&claims(1, AdminRole::SuperAdmin), 99).is_ok());
    }

    #[test]
    fn validate_delete_rejects_self() {
        assert!(matches!(
            validate_delete(&claims(1, AdminRole::SuperAdmin), 1),
            Err(AppError::Forbidden)
        ));
    }

    #[test]
    fn validate_delete_passes_for_other_account() {
        assert!(validate_delete(&claims(1, AdminRole::SuperAdmin), 2).is_ok());
    }
}
