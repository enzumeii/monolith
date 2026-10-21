use super::*;
#[test]
fn only_absolute_http_storage_links_are_eligible_without_rewriting_signed_queries() {
    for url in [
        "https://storage.test/transactions.csv?token=a%2Fb&x=1+2",
        "http://localhost:13131/inventory.csv",
    ] {
        assert!(eligible_url(url), "{url}");
    }
    for url in [
        "javascript:alert(1)",
        "data:text/csv,content",
        "file:///tmp/report.csv",
        "/relative.csv",
        "//storage.test/file.csv",
        "not a URL",
    ] {
        assert!(!eligible_url(url), "{url}");
    }
}
#[test]
fn activation_rejects_rapid_duplicates_and_allows_retry_without_completion_signal() {
    let mut guard = Activation::default();
    assert!(guard.begin(Duration::ZERO));
    assert!(!guard.begin(Duration::from_millis(100)));
    assert!(guard.begin(Duration::from_millis(600)));
    assert!(!guard.begin(Duration::from_millis(650)));
}

#[test]
fn rendered_file_links_keep_signed_hrefs_and_reject_unsafe_schemes() {
    use dioxus::dioxus_core::{AttributeValue, DynamicNode, VNode};
    fn links(dom: &VirtualDom, node: &VNode, out: &mut Vec<String>) {
        for attributes in &node.dynamic_attrs {
            for attribute in attributes {
                if attribute.name == "href"
                    && let AttributeValue::Text(value) = &attribute.value
                {
                    out.push(value.clone());
                }
            }
        }
        for (index, dynamic) in node.dynamic_nodes.iter().enumerate() {
            match dynamic {
                DynamicNode::Component(component) => {
                    if let Some(scope) = component.mounted_scope(index, node, dom)
                        && let Some(root) = scope.try_root_node()
                    {
                        links(dom, root, out);
                    }
                }
                DynamicNode::Fragment(nodes) => {
                    for node in nodes {
                        links(dom, node, out);
                    }
                }
                _ => {}
            }
        }
    }
    let signed = "https://storage.test/transactions.csv?token=a%2Fb&x=1+2";
    let mut dom = VirtualDom::new_with_props(
        ReportFiles,
        ReportFilesProps {
            compact: false,
            files: vec![
                ReportFileDTO {
                    filename: "transactions.csv".into(),
                    content_type: "text/csv".into(),
                    download_url: signed.into(),
                },
                ReportFileDTO {
                    filename: "unsafe.csv".into(),
                    content_type: "text/csv".into(),
                    download_url: "javascript:alert(1)".into(),
                },
            ],
        },
    );
    dom.rebuild_in_place();
    let mut hrefs = vec![];
    links(&dom, dom.base_scope().root_node(), &mut hrefs);
    assert_eq!(hrefs, vec![signed]);
}
