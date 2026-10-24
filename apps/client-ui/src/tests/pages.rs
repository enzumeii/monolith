#![cfg(not(target_arch = "wasm32"))]
use super::*;
use crate::{
    api::tests::{catalogue, responses},
    cart::{Cart, CartContext, browser},
};
use dioxus::{
    document::{Document, Eval, EvalError, Evaluator},
    history::{MemoryHistory, provide_history_context},
};
use futures::FutureExt;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    task::{Context, Poll},
};

#[path = "events.rs"]
pub(crate) mod events;
pub(crate) const CHECKOUT: &str = "f79b7e64-8e36-4aca-9e21-ecb7cb513700";
pub(crate) struct TestDocument {
    owner: dioxus::dioxus_core::internal::generational_box::Owner,
    pub replies: RefCell<VecDeque<serde_json::Value>>,
    pub scripts: RefCell<Vec<String>>,
}
impl TestDocument {
    pub fn new(replies: Vec<serde_json::Value>) -> Rc<Self> {
        Rc::new(Self {
            owner: dioxus::dioxus_core::internal::generational_box::Owner::default(),
            replies: RefCell::new(replies.into()),
            scripts: RefCell::new(vec![]),
        })
    }
}
struct Message(Option<serde_json::Value>);
impl Evaluator for Message {
    fn send(&self, _: serde_json::Value) -> Result<(), EvalError> {
        Ok(())
    }
    fn poll_recv(&mut self, _: &mut Context<'_>) -> Poll<Result<serde_json::Value, EvalError>> {
        Poll::Ready(self.0.take().ok_or(EvalError::Unsupported))
    }
    fn poll_join(&mut self, cx: &mut Context<'_>) -> Poll<Result<serde_json::Value, EvalError>> {
        self.poll_recv(cx)
    }
}
impl Document for TestDocument {
    fn eval(&self, script: String) -> Eval {
        self.scripts.borrow_mut().push(script);
        Eval::new(
            self.owner
                .insert(Box::new(Message(self.replies.borrow_mut().pop_front()))),
        )
    }
}
#[derive(Clone)]
pub(crate) enum Screen {
    Route(String),
    Catalogue(domain::dto::PublicCatalogueResponse),
    Cart(domain::dto::PublicCatalogueResponse, String),
    Provider(domain::dto::PublicCatalogueResponse),
}
#[derive(Clone)]
pub(crate) struct Harness {
    pub screen: Screen,
    pub initial: Cart,
    pub captured: Rc<Cell<Option<CartContext>>>,
}
#[derive(Clone, PartialEq, Routable)]
enum TestRoute {
    #[route("/")]
    Content {},
}
fn root() -> Element {
    let harness = use_context::<Harness>();
    let path = match &harness.screen {
        Screen::Route(path) => path.clone(),
        _ => "/".into(),
    };
    use_hook(move || provide_history_context(Rc::new(MemoryHistory::with_initial_path(path))));
    let cache = use_context_provider(|| Signal::new(BTreeMap::<String, browser::Restored>::new()));
    let cart = use_signal(|| harness.initial);
    use_context_provider(|| crate::cart::tests::context(cart, cache));
    harness
        .captured
        .set(Some(crate::cart::tests::context(cart, cache)));
    if matches!(harness.screen, Screen::Route(_)) {
        rsx! {Router::<Route>{}}
    } else {
        rsx! {Router::<TestRoute>{}}
    }
}
#[component]
fn Content() -> Element {
    match use_context::<Harness>().screen {
        Screen::Catalogue(catalogue) => rsx! {catalogue::Catalogue {catalogue}},
        Screen::Cart(catalogue, checkout_id) => {
            rsx! {crate::cart::CartView {catalogue,checkout_id}}
        }
        Screen::Provider(catalogue) => {
            rsx! {crate::cart::CartProvider {catalogue,checkout_id:None}}
        }
        Screen::Route(_) => unreachable!(),
    }
}
pub(crate) fn mounted(
    screen: Screen,
    cart: Cart,
    document: Rc<TestDocument>,
) -> (VirtualDom, Rc<Cell<Option<CartContext>>>) {
    dioxus::html::set_event_converter(Box::new(events::FormEventConverter));
    let captured = Rc::new(Cell::new(None));
    let mut dom = VirtualDom::new(root)
        .with_root_context(Harness {
            screen,
            initial: cart,
            captured: captured.clone(),
        })
        .with_root_context(document as Rc<dyn Document>);
    if let Some(settings) = dioxus::dioxus_core::Runtime::try_current()
        .and_then(|_| try_consume_context::<crate::config::Settings>())
    {
        dom = dom.with_root_context(settings);
    }
    dom.rebuild_in_place();
    (dom, captured)
}
pub(crate) async fn until(dom: &mut VirtualDom, expected: &str) -> String {
    for _ in 0..200 {
        let _ = dom.wait_for_work().now_or_never();
        dom.render_immediate_to_vec();
        let html = dioxus_ssr::render(dom);
        if html.contains(expected) {
            return html;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("Missing {expected}: {}", dioxus_ssr::render(dom));
}
pub(crate) fn click(dom: &mut VirtualDom, attribute: &str, value: &str) {
    let id = events::by_attribute(dom, attribute, value);
    events::form_event(dom, id, "click", "");
    events::render_changes(dom);
}
pub(crate) fn input(dom: &mut VirtualDom, id: &str, name: &str, value: &str) {
    let id = events::element_id(dom, id);
    events::form_event(dom, id, name, value);
    events::render_changes(dom);
}
pub(crate) fn empty_cart() -> Cart {
    Cart::new("a".into(), CHECKOUT.into())
}

#[tokio::test]
async fn customer_routes_render_directory_resources_and_safe_convention_metadata() {
    let mut event = crate::api::tests::convention("a");
    event.logo_preview_url = Some("https://images.test/logo.svg".into());
    event.description_markdown = "**Welcome** <script>danger()</script>".into();
    let list=serde_json::json!({"items":[event],"page":1,"per_page":100,"has_next":false,"visitor_country":"TH"}).to_string();
    let ((), requests) = responses(&[(200, &list)], async {
        let (mut dom, _) = mounted(
            Screen::Route("/".into()),
            empty_cart(),
            TestDocument::new(vec![serde_json::json!(["2099-01-01", "TH"])]),
        );
        assert!(dioxus_ssr::render(&dom).contains("Loading conventions"));
        let html = until(&mut dom, "Browse products").await;
        assert!(html.contains("Enzumeii"));
        assert!(html.contains("https://images.test/logo.svg"));
        assert!(html.contains("Welcome"));
        assert!(!html.contains("<script>danger()"));
    })
    .await;
    assert!(requests[0].contains("locale_country=TH"));
    let list=serde_json::json!({"items":[],"page":1,"per_page":100,"has_next":false,"visitor_country":null}).to_string();
    responses(
        &[(503, r#"{"error":"Directory unavailable"}"#), (200, &list)],
        async {
            let (mut dom, _) = mounted(
                Screen::Route("/".into()),
                empty_cart(),
                TestDocument::new(vec![
                    serde_json::json!(["2099-01-01", null]),
                    serde_json::json!(["2099-01-01", null]),
                ]),
            );
            until(&mut dom, "Directory unavailable").await;
            click(&mut dom, "__text", "Retry conventions");
            until(&mut dom, "There are no upcoming conventions").await;
        },
    )
    .await;
}

#[tokio::test]
async fn catalogue_quantity_search_type_and_pagination_events_preserve_cart() {
    let mut data = catalogue();
    data.products = (1..=26)
        .map(|n| {
            let mut p = crate::tests::product();
            p.id = format!("gid://shopify/Product/{n}");
            p.title = format!("Product {n:02}");
            p.featured_image = Some(domain::dto::CatalogueImage {
                url: "https://images.test/product.png".into(),
            });
            p.variants[0].image = Some(domain::dto::CatalogueImage {
                url: "https://images.test/variant.png".into(),
            });
            p.product_type = if n % 2 == 0 { "Pins" } else { "Prints" }.into();
            p.variants[0].id = format!("gid://shopify/ProductVariant/{n}");
            p
        })
        .collect();
    let (mut dom, captured) = mounted(
        Screen::Catalogue(data),
        empty_cart(),
        TestDocument::new(vec![]),
    );
    assert!(dioxus_ssr::render(&dom).contains("26 products"));
    input(&mut dom, "page-size", "change", "12");
    assert!(dioxus_ssr::render(&dom).contains("Page 1 of 3"));
    click(&mut dom, "__text", "Next");
    assert!(dioxus_ssr::render(&dom).contains("Page 2 of 3"));
    input(&mut dom, "product-type", "change", "Pins");
    assert!(dioxus_ssr::render(&dom).contains("13 products"));
    input(&mut dom, "product-search", "input", "Product 02");
    assert!(dioxus_ssr::render(&dom).contains("1 products"));
    input(&mut dom, "quantity-2", "input", "bad");
    click(
        &mut dom,
        "aria-label",
        "Add Product 02 / Blue / Large to cart",
    );
    assert!(dioxus_ssr::render(&dom).contains("whole-number quantity"));
    input(&mut dom, "quantity-2", "input", "2");
    click(
        &mut dom,
        "aria-label",
        "Add Product 02 / Blue / Large to cart",
    );
    assert!(dioxus_ssr::render(&dom).contains("View cart (2)"));
    dom.in_scope(ScopeId::ROOT, || {
        assert_eq!(
            captured
                .get()
                .expect("mounted cart context")
                .cart
                .peek()
                .items[0]
                .quantity,
            2
        );
    });
    input(&mut dom, "quantity-2", "input", "10");
    click(
        &mut dom,
        "aria-label",
        "Add Product 02 / Blue / Large to cart",
    );
    assert!(dioxus_ssr::render(&dom).contains("at most 10"));
    input(&mut dom, "product-search", "input", "no match");
    assert!(dioxus_ssr::render(&dom).contains("No products match"));
}

#[tokio::test]
async fn direct_convention_routes_hide_unavailable_metadata_and_retry_network_errors() {
    responses(&[(404, r#"{"error":"Private metadata"}"#)], async {
        let (mut dom, _) = mounted(
            Screen::Route("/fair-a".into()),
            empty_cart(),
            TestDocument::new(vec![]),
        );
        assert!(dioxus_ssr::render(&dom).contains("Loading convention"));
        let html = until(&mut dom, "not available for preorders").await;
        assert!(!html.contains("Private metadata"));
    })
    .await;
    let data = serde_json::to_string(&catalogue()).expect("serialize catalogue fixture");
    responses(
        &[(502, r#"{"error":"Provider offline"}"#), (200, &data)],
        async {
            let (mut dom, _) = mounted(
                Screen::Route("/fair-a/cart/unknown".into()),
                empty_cart(),
                TestDocument::new(vec![
                    serde_json::json!({"raw":null,"checkout_id":CHECKOUT,"blocked":false}),
                    serde_json::json!(true),
                ]),
            );
            until(&mut dom, "Provider offline").await;
            click(&mut dom, "__text", "Retry convention");
            until(&mut dom, "Cart unavailable").await;
        },
    )
    .await;
    let (mut dom, _) = mounted(
        Screen::Route("/a/b/c".into()),
        empty_cart(),
        TestDocument::new(vec![]),
    );
    assert!(dioxus_ssr::render(&dom).contains("Page unavailable"));
    events::render_changes(&mut dom);
}
