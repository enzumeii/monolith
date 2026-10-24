use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;
use pulldown_cmark::{Event, Parser};

#[component]
pub fn MarkdownPreview(markdown: String) -> Element {
    let html = render_markdown(&markdown);
    rsx! { div { class: "markdown-preview", dangerous_inner_html: html } }
}

fn render_markdown(markdown: &str) -> String {
    let events = Parser::new(markdown).map(|event| match event {
        Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
        other => other,
    });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events);
    ammonia::Builder::empty()
        .tags(HashSet::from([
            "p",
            "h1",
            "h2",
            "h3",
            "h4",
            "h5",
            "h6",
            "ul",
            "ol",
            "li",
            "em",
            "strong",
            "code",
            "pre",
            "blockquote",
            "br",
            "hr",
            "a",
        ]))
        .tag_attributes(HashMap::from([
            ("a", HashSet::from(["href", "title"])),
            ("ol", HashSet::from(["start"])),
        ]))
        .url_schemes(HashSet::from(["http", "https", "mailto"]))
        .url_relative(ammonia::UrlRelative::Deny)
        .link_rel(Some("noopener noreferrer"))
        .clean(&html)
        .to_string()
}

#[cfg(test)]
#[path = "tests/markdown.rs"]
mod tests;
