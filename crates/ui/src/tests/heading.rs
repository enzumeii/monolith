use super::*;

fn render(app: fn() -> Element) -> String {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

#[test]
fn page_heading_retains_markup_and_escapes_title_without_empty_actions() {
    let html = render(|| rsx! { Heading { title: "Products <sale>" } });
    assert!(
        html.contains("<header class=\"content-heading\">"),
        "{html}"
    );
    assert!(
        html.contains("<div class=\"content-heading-title\">"),
        "{html}"
    );
    assert!(html.contains("<h2>Products &#60;sale&#62;</h2>"), "{html}");
    assert!(!html.contains("content-heading-actions"), "{html}");
}

#[test]
fn section_heading_keeps_back_link_and_actions_in_their_existing_containers() {
    let html = render(|| {
        rsx! {
            Heading {
                title: "Cart",
                level: HeadingLevel::Section,
                back: rsx! { a { class: "button-link ghost", href: "/fair", "Back" } },
                button { r#type: "button", "Save" }
            }
        }
    });
    let back = html.find("href=\"/fair\"").expect("back link");
    let title = html.find("<h3>Cart</h3>").expect("section title");
    let actions = html
        .find("class=\"content-heading-actions\"")
        .expect("actions");
    let save = html.find(">Save</button>").expect("action button");
    assert!(back < title && title < actions && actions < save, "{html}");
}

#[test]
fn shared_styles_preserve_all_heading_rules() {
    let css = include_str!("../../assets/common.css");
    for rule in [
        ".content-heading { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 1rem; margin-bottom: 1rem; }",
        ".content-heading-title { display: flex; align-items: center; gap: .75rem; min-width: 0; flex: 1; }",
        ".content-heading-title h2, .content-heading-title h3 { margin: 0; min-width: 0; overflow-wrap: anywhere; }",
        ".content-heading-title > .button-link { flex-shrink: 0; min-width: 2.75rem; font-size: 1.25rem; }",
        ".content-heading-actions { display: flex; align-items: center; gap: .75rem; flex-wrap: wrap; }",
        ".content-heading-actions > button, .content-heading-actions > .button-link { margin-top: 0; }",
    ] {
        assert!(css.contains(rule), "missing {rule}");
    }
}
