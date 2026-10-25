#![cfg(not(target_arch = "wasm32"))]

use super::*;
use crate::pages::tests::{Screen, TestDocument, empty_cart, mounted};

#[test]
fn contact_links_expose_accessible_names_and_safe_external_destinations() {
    let mut dom = VirtualDom::new(PortalFooter);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains(r#"aria-label="Contact Enzumeii""#));
    let links: Vec<_> = html.split("<a ").skip(1).collect();
    assert_eq!(links.len(), 5);
    for (link, (href, label)) in links.iter().zip([
        (
            "mailto:order@enzumeii.com",
            "Email Enzumeii about your order",
        ),
        ("https://www.facebook.com/enzumeii", "Enzumeii on Facebook"),
        ("http://instagram.com/enzumeii", "Enzumeii on Instagram"),
        ("https://twitter.com/enzumeii", "Enzumeii on X (Twitter)"),
        ("https://www.tiktok.com/@enzumei", "Enzumeii on TikTok"),
    ]) {
        let anchor = link.split("</a>").next().expect("contact anchor");
        assert!(anchor.contains(&format!(r#"href="{href}""#)));
        assert!(anchor.contains(&format!(r#"aria-label="{label}""#)));
        assert!(anchor.contains(&format!(r#"title="{label}""#)));
        assert!(anchor.contains(r#"target="_blank""#));
        assert!(anchor.contains(r#"rel="noopener noreferrer""#));
        assert!(anchor.contains(r#"aria-hidden="true""#));
        assert!(anchor.contains(r#"focusable="false""#));
        assert!(!anchor.contains("<img"));
    }
}

#[tokio::test]
async fn shared_layout_keeps_one_contact_footer_across_portal_routes() {
    for path in [
        "/",
        "/fair-a",
        "/fair-a/products/1",
        "/fair-a/cart/unknown",
        "/fair-a/checkout/return?checkout_id=invalid",
        "/fair-a/checkout/cancel?checkout_id=invalid",
        "/missing/page/route",
    ] {
        let (dom, _) = mounted(
            Screen::Route(path.into()),
            empty_cart(),
            TestDocument::new(vec![]),
        );
        let html = dioxus_ssr::render(&dom);
        assert_eq!(html.matches("<footer").count(), 1, "{path}");
        assert_eq!(
            html.matches(r#"href="mailto:order@enzumeii.com""#).count(),
            1,
            "{path}"
        );
        assert!(
            html.find("<footer").expect("contact footer")
                > html.rfind("</main>").expect("routed main content"),
            "{path}"
        );
    }
}
