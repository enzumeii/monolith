use super::*;

fn render(app: fn() -> Element) -> String {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

#[test]
fn field_preserves_label_input_and_autocomplete_contract() {
    let html = render(|| {
        rsx! {
            FormField {
                label: "Email", id: "email", input_type: "email", autocomplete: "username",
                value: "admin@example.com".to_string(), on_input: move |_| {},
            }
        }
    });
    for expected in [
        "for=\"email\"",
        "id=\"email\"",
        "type=\"email\"",
        "autocomplete=\"username\"",
        "value=\"admin@example.com\"",
    ] {
        assert!(html.contains(expected), "missing {expected}: {html}");
    }
}

#[test]
fn error_is_optional_and_escaped_with_alert_semantics() {
    let absent = render(|| rsx! { ErrorMessage { message: None } });
    assert!(!absent.contains("role=\"alert\""));
    let present =
        render(|| rsx! { ErrorMessage { message: Some("<script>bad()</script>".to_string()) } });
    assert!(present.contains("role=\"alert\""));
    assert!(present.contains("class=\"error\""));
    assert!(present.contains("&#60;script&#62;"), "{present}");
    assert!(!present.contains("<script"));
}

#[test]
fn loading_announces_escaped_status() {
    let html = render(|| rsx! { LoadingMessage { message: "Loading <settings>" } });
    assert!(html.contains("role=\"status\""));
    assert!(html.contains("Loading &#60;settings&#62;"), "{html}");
}
