use super::*;

#[test]
fn commonmark_structure_and_safe_links_are_preserved() {
    let html = render_markdown(
        "# Convention\n\nA **bold** and *emphasized* description.\n\n- One\n- Two\n\n3. Third\n\n> Quote\n\n`inline`\n\n```\nlet price = 12;\n```\n\n[Shop](https://example.com/store) [Mail](mailto:admin@example.com) [HTTP](http://example.com)",
    );
    for expected in [
        "<h1>Convention</h1>",
        "<strong>bold</strong>",
        "<em>emphasized</em>",
        "<ul>",
        "<li>One</li>",
        "<ol start=\"3\">",
        "<blockquote>",
        "<code>inline</code>",
        "<pre><code>let price = 12;",
        "href=\"https://example.com/store\"",
        "href=\"mailto:admin@example.com\"",
        "href=\"http://example.com\"",
        "rel=\"noopener noreferrer\"",
    ] {
        assert!(html.contains(expected), "missing {expected}: {html}");
    }
}

#[test]
fn raw_html_is_displayed_as_text_without_executable_elements_or_attributes() {
    for markdown in [
        "<script>alert(1)</script>",
        "<img src=x onerror=alert(1)>",
        "<iframe src=\"https://example.com\"></iframe>",
        "<a href=\"javascript:alert(1)\" onclick=\"alert(1)\">Click</a>",
        "<style>body {display:none}</style>",
        "<svg onload=alert(1)><script>alert(1)</script></svg>",
        "<math><mtext><img src=x onerror=alert(1)></mtext></math>",
    ] {
        let html = render_markdown(markdown);
        for forbidden in [
            "<script", "<img", "<iframe", "<style", "<svg", "<math", "<a href",
        ] {
            assert!(!html.contains(forbidden), "{markdown}: {html}");
        }
        assert!(html.contains("&lt;"), "raw HTML should be escaped: {html}");
    }
}

#[test]
fn unsafe_encoded_relative_and_executable_urls_cannot_become_links() {
    for url in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "java&#x73;cript:alert(1)",
        "javascript&#58;alert(1)",
        "java&#9;script:alert(1)",
        "data:text/html,unsafe",
        "vbscript:msgbox(1)",
        "file:///etc/passwd",
        "javascript%3Aalert(1)",
        "%6a%61vascript:alert(1)",
        "/relative",
        "//example.com/path",
        "#fragment",
    ] {
        let html = render_markdown(&format!("[unsafe](<{url}>)"));
        assert!(!html.contains("href="), "unsafe URL {url}: {html}");
    }
}

#[test]
fn markdown_images_are_removed_including_svg_and_data_sources() {
    for source in [
        "https://example.com/image.svg",
        "data:image/svg+xml,unsafe",
        "https://example.com/image.png",
    ] {
        let html = render_markdown(&format!("![logo]({source})"));
        assert!(!html.contains("<img"), "{html}");
        assert!(!html.contains("src="), "{html}");
    }
}

#[test]
fn code_preserves_escaped_markup_and_does_not_execute_it() {
    let html = render_markdown(
        "`<script>alert(1)</script>`\n\n```html\n<img src=x onerror=alert(1)>\n```",
    );
    assert!(html.contains("<code>&lt;script&gt;"), "{html}");
    assert!(html.contains("&lt;img"), "{html}");
    assert!(!html.contains("<script"));
    assert!(!html.contains("class="));
}

#[test]
fn preview_component_renders_sanitized_html() {
    fn app() -> Element {
        rsx! { MarkdownPreview { markdown: "**Preview** <script>bad()</script>" } }
    }
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("class=\"markdown-preview\""));
    assert!(html.contains("<strong>Preview</strong>"));
    assert!(!html.contains("<script"));
}
