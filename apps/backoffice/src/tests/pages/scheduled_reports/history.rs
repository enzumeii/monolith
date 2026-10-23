use super::*;
#[test]
fn history_filters_round_trip_supported_global_fields_and_preserve_back_state() {
    let filters = HistoryFilters::parse(
        "report_type=INVENTORY&enabled=false&configuration_id=old&generated_from=2026-10-01T00%3A00%3A00%2B07%3A00&page=3&per_page=50",
    );
    assert_eq!(filters.page, 3);
    assert_eq!(filters.per_page, 50);
    assert_eq!(filters.report_type, Some(ScheduleReportType::Inventory));
    assert_eq!(
        filters.generated_from.as_deref(),
        Some("2026-10-01T00:00:00+07:00")
    );
    assert_eq!(HistoryFilters::parse(&filters.query()), filters);
    let detail = Route::ScheduledReportDetail {
        id: "da41d46b-f1ca-4a76-a774-62c269d28731".into(),
        query: filters.query(),
    };
    let Route::ScheduledReportDetail { query, .. } = detail
        .to_string()
        .parse::<Route>()
        .expect("valid test fixture")
    else {
        panic!("detail route")
    };
    assert_eq!(HistoryFilters::parse(&query), filters);

    assert!(!filters.query().contains("enabled"));
    assert!(!filters.query().contains("configuration"));
    let req = filters.request().expect("valid test fixture");
    assert_eq!(req.page, Some(3));
    assert_eq!(req.generated_to, None);
    let changed = HistoryFilters::apply("TRANSACTION", "", "2026-11-01T00:00", 10)
        .expect("valid test fixture");
    assert_eq!(changed.page, 1);
    assert_eq!(changed.per_page, 10);
    assert_eq!(changed.generated_from, None);
    assert_eq!(
        changed.generated_to.as_deref(),
        Some("2026-11-01T00:00:00+07:00")
    );
}
#[test]
fn invalid_bounds_never_become_unfiltered_history_requests() {
    let filters = HistoryFilters::parse("generated_from=garbage&page=0&per_page=101");
    assert_eq!((filters.page, filters.per_page), (1, 25));
    assert!(filters.request().is_err());
    assert!(HistoryFilters::apply("", "2026-11-01T00:00", "2026-10-01T00:00", 25).is_err());
    assert!(
        HistoryFilters::parse("generated_to=2026-10-01T00%3A00%3A00%2B07%3A00")
            .request()
            .is_ok()
    );
}

#[cfg(not(target_arch = "wasm32"))]
mod mounted {
    use super::*;
    use dioxus::router::RouterContext;
    use dioxus::{
        dioxus_core::{DynamicNode, TemplateNode, VNode},
        history::{MemoryHistory, provide_history_context},
    };
    use futures::FutureExt;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        rc::Rc,
    };

    #[derive(Clone)]
    struct Harness {
        history: Rc<MemoryHistory>,
    }

    fn app() -> Element {
        let harness = use_context::<Harness>();
        use_hook(move || provide_history_context(harness.history));
        rsx! { Router::<crate::route::Route> {} }
    }

    fn mount(listener: &TcpListener) -> (VirtualDom, Harness) {
        let harness = Harness {
            history: Rc::new(MemoryHistory::with_initial_path(
                "/scheduled-reports/history?page=1&per_page=25",
            )),
        };
        let mut dom = VirtualDom::new(app)
            .with_root_context(harness.clone())
            .with_root_context(crate::config::Settings {
                conventions: crate::config::ConventionsConfig {
                    catalogue_location_id: "65761018027".into(),
                },
                api: crate::config::ApiConfig {
                    base_url: format!(
                        "http://{}/v1",
                        listener.local_addr().expect("valid test fixture")
                    ),
                },
            });
        set_owner(&dom, Some(1));
        dom.rebuild_in_place();
        (dom, harness)
    }

    fn set_owner(dom: &VirtualDom, id: Option<i64>) {
        dom.in_scope(ScopeId::ROOT, || {
            *session::SESSION.write() = id.map(|admin_id| session::Session {
                admin_id,
                username: "fixture".into(),
                role: domain::AdminRole::Admin,
                expires_at: session::now_unix() + 10000,
            });
        });
    }

    fn navigate(dom: &VirtualDom, target: String) {
        fn find(dom: &VirtualDom, node: &VNode) -> Option<(ScopeId, RouterContext)> {
            for (index, dynamic) in node.dynamic_nodes.iter().enumerate() {
                match dynamic {
                    DynamicNode::Component(component) => {
                        if let Some(id) = component.mounted_scope_id(index, node, dom) {
                            if let Some(router) =
                                dom.in_scope(id, try_consume_context::<RouterContext>)
                            {
                                return Some((id, router));
                            }
                            if let Some(root) =
                                dom.get_scope(id).and_then(|scope| scope.try_root_node())
                                && let Some(router) = find(dom, root)
                            {
                                return Some(router);
                            }
                        }
                    }
                    DynamicNode::Fragment(nodes) => {
                        for node in nodes {
                            if let Some(router) = find(dom, node) {
                                return Some(router);
                            }
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        let (scope, router) = find(dom, dom.base_scope().root_node()).expect("mounted router");
        dom.in_scope(scope, || {
            router.push(dioxus::router::NavigationTarget::Internal(target));
        });
    }

    fn pump(dom: &mut VirtualDom) {
        for _ in 0..10 {
            let _ = dom.wait_for_work().now_or_never();
            dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);
        }
    }

    async fn request(dom: &mut VirtualDom, listener: &TcpListener) -> (TcpStream, String) {
        for _ in 0..300 {
            pump(dom);
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            if let Ok((mut stream, _)) = listener.accept() {
                stream.set_nonblocking(true).expect("valid test fixture");
                let mut bytes = Vec::new();
                for _ in 0..300 {
                    let mut chunk = [0; 8192];
                    match stream.read(&mut chunk) {
                        Ok(count) if count > 0 => bytes.extend_from_slice(&chunk[..count]),
                        Ok(_) => panic!("request closed before headers"),
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(error) => panic!("request read failed: {error}"),
                    }
                    if bytes.windows(4).any(|part| part == b"\r\n\r\n") {
                        stream.set_nonblocking(false).expect("valid test fixture");
                        return (
                            stream,
                            String::from_utf8(bytes).expect("valid test fixture"),
                        );
                    }
                    pump(dom);
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
                panic!("HTTP headers did not arrive");
            }
        }
        panic!("metadata request did not reach the real controlled HTTP server");
    }

    fn respond(mut stream: TcpStream, status: u16, body: &str) {
        let _ = write!(
            stream,
            "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    }

    fn record(id: &str, parameters: &str) -> String {
        format!(
            r#"{{"id":"{id}","report_type":"INVENTORY","attempted_at":"2026-10-22T00:00:00Z","generated_at":null,"status":"FAILED","error":"Publication failed","parameters":"{parameters}","files":[]}}"#
        )
    }
    fn page(parameters: &str) -> String {
        format!(
            r#"{{"items":[{}],"page":1,"per_page":25,"has_next":false}}"#,
            record("da41d46b-f1ca-4a76-a774-62c269d28731", parameters)
        )
    }

    fn text(dom: &VirtualDom) -> String {
        fn vnode(dom: &VirtualDom, node: &VNode, out: &mut String) {
            fn template(
                dom: &VirtualDom,
                vnode_value: &VNode,
                node: &TemplateNode,
                out: &mut String,
            ) {
                match node {
                    TemplateNode::Text { text } => {
                        out.push_str(text);
                        out.push(' ');
                    }
                    TemplateNode::Element { children, .. } => {
                        for child in *children {
                            template(dom, vnode_value, child, out);
                        }
                    }
                    TemplateNode::Dynamic { id } => match &vnode_value.dynamic_nodes[*id] {
                        DynamicNode::Text(value) => {
                            out.push_str(&value.value);
                            out.push(' ');
                        }
                        DynamicNode::Fragment(nodes) => {
                            for node in nodes {
                                vnode(dom, node, out);
                            }
                        }
                        DynamicNode::Component(component) => {
                            if let Some(scope) = component.mounted_scope(*id, vnode_value, dom)
                                && let Some(root) = scope.try_root_node()
                            {
                                vnode(dom, root, out);
                            }
                        }
                        DynamicNode::Placeholder(_) => {}
                    },
                }
            }
            for root in node.template.roots {
                template(dom, node, root, out);
            }
        }
        let mut out = String::new();
        vnode(dom, dom.base_scope().root_node(), &mut out);
        out
    }

    async fn wait_text(dom: &mut VirtualDom, want: &str) {
        for _ in 0..300 {
            pump(dom);
            if text(dom).contains(want) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("missing rendered text {want:?}: {}", text(dom));
    }

    #[derive(Clone, Default)]
    struct RefreshHarness {
        load: Rc<
            std::cell::Cell<
                Option<Resource<api::ApiResult<api::report_results::ReportResultListResponse>>>,
            >,
        >,
    }
    fn refresh_app() -> Element {
        let harness = use_context::<RefreshHarness>();
        let (load, saved) = use_history(HistoryFilters::parse("page=1&per_page=25"));
        harness.load.set(Some(load));
        let error = load
            .read()
            .as_ref()
            .and_then(|result| result.as_ref().err())
            .map(ToString::to_string);
        rsx! {
            if let Some(page)=saved() { for record in page.items { p { "{record.parameters}" } } }
            if let Some(error)=error { p { "{error}" } }
        }
    }
    #[tokio::test]
    async fn explicit_refresh_retains_last_success_during_pending_and_failure_then_replaces_on_success()
     {
        let listener = TcpListener::bind("127.0.0.1:0").expect("valid test fixture");
        listener.set_nonblocking(true).expect("valid test fixture");
        let harness = RefreshHarness::default();
        let mut dom = VirtualDom::new(refresh_app)
            .with_root_context(harness.clone())
            .with_root_context(crate::config::Settings {
                conventions: crate::config::ConventionsConfig {
                    catalogue_location_id: "65761018027".into(),
                },
                api: crate::config::ApiConfig {
                    base_url: format!(
                        "http://{}/v1",
                        listener.local_addr().expect("valid test fixture")
                    ),
                },
            });
        set_owner(&dom, Some(1));
        dom.rebuild_in_place();
        let (initial, _) = request(&mut dom, &listener).await;
        respond(initial, 200, &page("LAST_SUCCESS"));
        wait_text(&mut dom, "LAST_SUCCESS").await;
        dom.in_scope(ScopeId::ROOT, || {
            harness.load.get().expect("valid test fixture").restart();
        });
        let (failed, _) = request(&mut dom, &listener).await;
        assert!(
            text(&dom).contains("LAST_SUCCESS"),
            "refresh retains previous rows while pending"
        );
        respond(failed, 503, r#"{"error":"Fixture API unavailable"}"#);
        wait_text(&mut dom, "Fixture API unavailable").await;
        assert!(
            text(&dom).contains("LAST_SUCCESS"),
            "failed refresh keeps previous successful rows"
        );
        dom.in_scope(ScopeId::ROOT, || {
            harness.load.get().expect("valid test fixture").restart();
        });
        let (retry, _) = request(&mut dom, &listener).await;
        respond(retry, 200, &page("REPLACED_SUCCESS"));
        wait_text(&mut dom, "REPLACED_SUCCESS").await;
        assert!(!text(&dom).contains("LAST_SUCCESS"));
        assert!(!text(&dom).contains("Fixture API unavailable"));
    }

    #[tokio::test]
    async fn real_history_and_detail_scopes_clear_on_filter_navigation_logout_and_account_changes()
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("valid test fixture");
        listener.set_nonblocking(true).expect("valid test fixture");
        let (mut dom, _h) = mount(&listener);
        let (old, old_request) = request(&mut dom, &listener).await;
        assert!(old_request.starts_with("GET /v1/admin/cron/reports?page=1&per_page=25 "));
        navigate(
            &dom,
            "/scheduled-reports/history?report_type=INVENTORY&page=2&per_page=10".into(),
        );
        let (fresh, fresh_request) = request(&mut dom, &listener).await;
        assert!(
            fresh_request.starts_with(
                "GET /v1/admin/cron/reports?report_type=INVENTORY&page=2&per_page=10 "
            )
        );
        respond(fresh, 200, &page("CURRENT_FILTER"));
        wait_text(&mut dom, "CURRENT_FILTER").await;
        respond(old, 200, &page("STALE_FILTER"));
        pump(&mut dom);
        assert!(!text(&dom).contains("STALE_FILTER"));
        navigate(
            &dom,
            "/scheduled-reports/history/da41d46b-f1ca-4a76-a774-62c269d28731?page=2&per_page=10"
                .into(),
        );
        let (old_detail, request1) = request(&mut dom, &listener).await;
        assert!(
            request1
                .starts_with("GET /v1/admin/cron/reports/da41d46b-f1ca-4a76-a774-62c269d28731 ")
        );
        assert!(!text(&dom).contains("CURRENT_FILTER"));
        navigate(
            &dom,
            "/scheduled-reports/history/da41d46b-f1ca-4a76-a774-62c269d28732?page=2&per_page=10"
                .into(),
        );
        let (fresh_detail, request2) = request(&mut dom, &listener).await;
        assert!(
            request2
                .starts_with("GET /v1/admin/cron/reports/da41d46b-f1ca-4a76-a774-62c269d28732 ")
        );
        respond(
            fresh_detail,
            200,
            &record("da41d46b-f1ca-4a76-a774-62c269d28732", "CURRENT_DETAIL"),
        );
        wait_text(&mut dom, "CURRENT_DETAIL").await;
        let rendered = text(&dom);
        assert!(rendered.contains("Not generated"));
        assert!(rendered.contains("Attempted:"));
        assert!(rendered.contains("Files unavailable"));
        assert!(rendered.contains("Publication failed"));
        respond(
            old_detail,
            200,
            &record("da41d46b-f1ca-4a76-a774-62c269d28731", "STALE_DETAIL"),
        );
        pump(&mut dom);
        assert!(!text(&dom).contains("STALE_DETAIL"));
        set_owner(&dom, Some(2));
        pump(&mut dom);
        assert!(!text(&dom).contains("CURRENT_DETAIL"));
        let (account, _) = request(&mut dom, &listener).await;
        set_owner(&dom, None);
        pump(&mut dom);
        respond(
            account,
            200,
            &record("da41d46b-f1ca-4a76-a774-62c269d28732", "LOGGED_OUT_RESULT"),
        );
        pump(&mut dom);
        assert!(!text(&dom).contains("LOGGED_OUT_RESULT"));
        assert!(!text(&dom).contains("CURRENT_DETAIL"));
    }
}
