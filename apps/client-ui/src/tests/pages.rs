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
    pub sent: Rc<RefCell<Vec<serde_json::Value>>>,
}
impl TestDocument {
    pub fn new(replies: Vec<serde_json::Value>) -> Rc<Self> {
        Rc::new(Self {
            owner: dioxus::dioxus_core::internal::generational_box::Owner::default(),
            replies: RefCell::new(replies.into()),
            scripts: RefCell::new(vec![]),
            sent: Rc::new(RefCell::new(vec![])),
        })
    }
}
struct Message {
    values: VecDeque<serde_json::Value>,
    pending: bool,
    sent: Rc<RefCell<Vec<serde_json::Value>>>,
}
impl Evaluator for Message {
    fn send(&self, value: serde_json::Value) -> Result<(), EvalError> {
        self.sent.borrow_mut().push(value);
        Ok(())
    }
    fn poll_recv(&mut self, _: &mut Context<'_>) -> Poll<Result<serde_json::Value, EvalError>> {
        match self.values.pop_front() {
            Some(value) => Poll::Ready(Ok(value)),
            None if self.pending => Poll::Pending,
            None => Poll::Ready(Err(EvalError::Unsupported)),
        }
    }
    fn poll_join(&mut self, cx: &mut Context<'_>) -> Poll<Result<serde_json::Value, EvalError>> {
        self.poll_recv(cx)
    }
}
impl Document for TestDocument {
    fn eval(&self, script: String) -> Eval {
        self.scripts.borrow_mut().push(script);
        let next = self.replies.borrow_mut().pop_front();
        let pending = next
            .as_ref()
            .and_then(|value| value.get("__eval_pending"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let messages = if pending {
            VecDeque::new()
        } else if let Some(values) = next
            .as_ref()
            .and_then(|value| value.get("__eval_messages"))
            .and_then(serde_json::Value::as_array)
        {
            values.clone().into()
        } else {
            next.into_iter().collect()
        };
        Eval::new(self.owner.insert(Box::new(Message {
            values: messages,
            pending,
            sent: self.sent.clone(),
        })))
    }
}
#[derive(Clone)]
pub(crate) enum Screen {
    Route(String),
    Catalogue(domain::dto::PublicCatalogueResponse),
    Product(domain::dto::PublicCatalogueResponse, String),
    Cart(domain::dto::PublicCatalogueResponse, String),
    Provider(domain::dto::PublicCatalogueResponse),
    Checkout(Cart, Option<domain::dto::ConventionQuote>),
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
        Screen::Product(catalogue, product_id) => {
            rsx! { ConventionContent { catalogue, view: ConventionView::Product { product_id }, notice: None } }
        }
        Screen::Cart(catalogue, checkout_id) => {
            rsx! {crate::cart::CartView {catalogue,checkout_id}}
        }
        Screen::Provider(catalogue) => {
            rsx! {crate::cart::CartProvider {catalogue,view:ConventionView::Catalogue}}
        }
        Screen::Checkout(snapshot, quote) => {
            let busy = use_signal(|| false);
            rsx! {crate::checkout::CheckoutControls {slug:"fair-a", snapshot, quote, busy}}
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
    let settings = dioxus::dioxus_core::Runtime::try_current()
        .and_then(|_| try_consume_context::<crate::config::Settings>())
        .unwrap_or_else(crate::config::tests::settings);
    dom = dom.with_root_context(settings);
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

#[test]
fn convention_header_displays_country_names_and_safe_unknown_metadata() {
    for (code, expected) in [
        ("TH", "Thailand"),
        ("TR", "Türkiye"),
        ("CI", "Côte d’Ivoire"),
        ("ZZ", "Unknown country"),
    ] {
        let mut convention = crate::api::tests::convention("a");
        convention.country_code = code.into();
        let mut dom =
            VirtualDom::new_with_props(ConventionHeader, ConventionHeaderProps { convention });
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert!(html.contains(&format!("{expected} ·")), "{html}");
        assert!(!html.contains(&format!("{code} ·")), "{html}");
    }
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
                    serde_json::json!(true),
                ]),
            );
            let html = until(&mut dom, "There are no upcoming conventions").await;
            assert!(!html.contains("Retry conventions"));
            assert!(!html.contains("Directory unavailable"));
        },
    )
    .await;
}

#[tokio::test]
async fn catalogue_search_type_and_pagination_events_show_products() {
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
    let (mut dom, _) = mounted(
        Screen::Catalogue(data),
        empty_cart(),
        TestDocument::new(vec![]),
    );
    assert!(dioxus_ssr::render(&dom).contains("26 products"));
    let html = dioxus_ssr::render(&dom);
    assert!(
        html.find("portal-products").expect("product grid")
            < html.find("id=\"page-size\"").expect("page-size selector")
    );
    input(&mut dom, "page-size", "change", "12");
    assert!(dioxus_ssr::render(&dom).contains("Page 1 of 3"));
    click(&mut dom, "__text", "Next");
    assert!(dioxus_ssr::render(&dom).contains("Page 2 of 3"));
    input(&mut dom, "product-type", "change", "Pins");
    assert!(dioxus_ssr::render(&dom).contains("13 products"));
    input(&mut dom, "product-search", "input", "Product 02");
    assert!(dioxus_ssr::render(&dom).contains("1 products"));
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("View variants"));
    assert!(!html.contains("Add to cart"));
    assert!(!html.contains("quantity-"));
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
        assert!(!html.contains("Retry convention"));
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
                    serde_json::json!(true),
                    serde_json::json!({"raw":null,"checkout_id":CHECKOUT,"blocked":false}),
                    serde_json::json!(true),
                ]),
            );
            let html = until(&mut dom, "Cart unavailable").await;
            assert!(!html.contains("Retry convention"));
            assert!(!html.contains("Provider offline"));
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

#[tokio::test]
async fn terminal_browser_and_protocol_failures_do_not_schedule_customer_retries() {
    let ((), requests) = responses(&[], async {
        let document = TestDocument::new(vec![]);
        let (mut dom, _) = mounted(Screen::Route("/".into()), empty_cart(), document.clone());
        let html = until(&mut dom, "Unable to determine your local date").await;
        assert!(!html.contains("Retry"));
        assert!(!html.contains("reload"));
        assert_eq!(document.scripts.borrow().len(), 1);
    })
    .await;
    assert_eq!(requests, Vec::<String>::new());
    for path in ["/", "/fair-a"] {
        let document = TestDocument::new(if path == "/" {
            vec![serde_json::json!(["2099-01-01", null])]
        } else {
            vec![]
        });
        let ((), requests) = responses(&[(200, "invalid response")], async {
            let (mut dom, _) = mounted(Screen::Route(path.into()), empty_cart(), document.clone());
            let html = until(&mut dom, "The shop returned an invalid response").await;
            assert!(!html.contains("Retry"));
            assert!(
                !document
                    .scripts
                    .borrow()
                    .iter()
                    .any(|script| script.contains("setTimeout"))
            );
        })
        .await;
        assert_eq!(requests.len(), 1);
    }
}
