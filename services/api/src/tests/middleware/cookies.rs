use super::*;
use axum::{
    http::header::{COOKIE, SET_COOKIE},
    response::IntoResponse,
};

#[test]
fn cookie_settings_match_creation_and_removal() {
    for (insecure_local, name) in [(false, "__Host-backoffice_auth"), (true, "backoffice_auth")] {
        let config = BrowserConfig {
            allowed_origins: vec![
                if insecure_local {
                    "http://127.0.0.1:8084"
                } else {
                    "https://admin.example.test"
                }
                .into(),
                "https://convention.enzumeii.com".into(),
            ],
            insecure_local,
        };
        let tokens = TokenResponse {
            access_token: "signed-access".into(),
            refresh_token: "ab".repeat(32),
            token_type: "Bearer".into(),
            expires_in: 600,
            id: 1,
        };
        let created = response_cookie(save_cookie(CookieJar::new(), &config, &tokens));
        assert_eq!(created.name(), name);
        assert_eq!(created.http_only(), Some(true));
        assert_eq!(
            created.secure(),
            if insecure_local { None } else { Some(true) }
        );
        assert_eq!(created.same_site(), Some(SameSite::Lax));
        assert_eq!(created.path(), Some("/"));
        assert_eq!(created.domain(), None);
        assert!(
            created
                .max_age()
                .expect("persistent credential")
                .whole_seconds()
                > 0
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            format!("{name}={}", created.value())
                .parse()
                .expect("valid cookie header"),
        );
        let removed = response_cookie(clear_cookie(CookieJar::from_headers(&headers), &config));
        assert_eq!(removed.name(), created.name());
        assert_eq!(removed.path(), created.path());
        assert_eq!(removed.domain(), created.domain());
        assert_eq!(removed.secure(), created.secure());
        assert_eq!(removed.http_only(), created.http_only());
        assert_eq!(removed.same_site(), created.same_site());
        assert_eq!(
            removed.max_age().expect("cookie deletion").whole_seconds(),
            0
        );
    }
}

fn response_cookie(jar: CookieJar) -> Cookie<'static> {
    let response = jar.into_response();
    Cookie::parse(
        response
            .headers()
            .get(SET_COOKIE)
            .expect("credential cookie")
            .to_str()
            .expect("valid header"),
    )
    .expect("valid cookie")
    .into_owned()
}
