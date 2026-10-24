use super::*;
use std::{cell::Cell, rc::Rc};

const CHECKOUT: &str = "f79b7e64-8e36-4aca-9e21-ecb7cb513700";
type Captured = Rc<Cell<Option<CartContext>>>;

fn harness() -> Element {
    use_context_provider(|| Signal::new(BTreeMap::<String, browser::Restored>::new()));
    let screen = use_signal(|| 0_u32);
    use_context::<Rc<Cell<Option<Signal<u32>>>>>().set(Some(screen));
    rsx! { Session { key: "{screen}", screen: *screen.read() } }
}

#[component]
fn Session(screen: u32) -> Element {
    let cache = use_context::<Signal<BTreeMap<String, browser::Restored>>>();
    let captured = use_context::<Captured>();
    let cart = use_signal(|| {
        cache.peek().get("event").map_or_else(
            || Cart::new("event".into(), CHECKOUT.into()),
            |saved| saved.cart.clone(),
        )
    });
    captured.set(Some(CartContext { cart, cache }));
    rsx! { p { "Screen {screen}: {cart.read().items.len()} items" } }
}

#[test]
fn same_tab_cache_survives_page_remounts_before_any_browser_save() {
    let captured = Captured::default();
    let screen_capture = Rc::new(Cell::new(None::<Signal<u32>>));
    let mut dom = VirtualDom::new(harness)
        .with_root_context(captured.clone())
        .with_root_context(screen_capture.clone());
    dom.rebuild_in_place();
    dom.in_scope(ScopeId::ROOT, || {
        captured
            .get()
            .expect("cart context")
            .update(|cart| cart.add(&crate::tests::variant(), 2))
            .expect("add");
        let mut screen = screen_capture.get().expect("screen");
        screen.set(1);
    });
    dom.render_immediate_to_vec();
    assert!(dioxus_ssr::render(&dom).contains("Screen 1: 1 items"));
    dom.in_scope(ScopeId::ROOT, || {
        let mut context = captured.get().expect("remounted context");
        assert_eq!(context.cart.peek().checkout_id, CHECKOUT);
        context.update(|cart| cart.set_quantity(&crate::tests::variant().id, 3));
        let mut screen = screen_capture.get().expect("screen");
        screen.set(2);
    });
    dom.render_immediate_to_vec();
    dom.in_scope(ScopeId::ROOT, || {
        assert_eq!(
            captured.get().expect("returned context").cart.peek().items[0].quantity,
            3
        );
    });
}
