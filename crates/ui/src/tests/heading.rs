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
                back: rsx! { a { class: "back-link", href: "/fair", aria_label: "Back to products", "<" } },
                button { r#type: "button", "Save" }
            }
        }
    });
    assert!(html.contains("aria-label=\"Back to products\""), "{html}");
    assert!(html.contains("class=\"back-link\""), "{html}");
    let back = html.find("href=\"/fair\"").expect("back link");
    let title = html.find("<h3>Cart</h3>").expect("section title");
    let actions = html
        .find("class=\"content-heading-actions\"")
        .expect("actions");
    let save = html.find(">Save</button>").expect("action button");
    assert!(back < title && title < actions && actions < save, "{html}");
}
