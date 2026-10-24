use dioxus::prelude::*;

#[derive(Clone, Copy, Default, PartialEq)]
pub enum HeadingLevel {
    #[default]
    Page,
    Section,
}

#[component]
pub fn Heading(
    title: String,
    #[props(default)] level: HeadingLevel,
    back: Option<Element>,
    children: Option<Element>,
) -> Element {
    rsx! {
        header { class: "content-heading",
            div { class: "content-heading-title",
                if let Some(back) = back { {back} }
                match level {
                    HeadingLevel::Page => rsx! { h2 { "{title}" } },
                    HeadingLevel::Section => rsx! { h3 { "{title}" } },
                }
            }
            if let Some(children) = children { div { class: "content-heading-actions", {children} } }
        }
    }
}

#[cfg(test)]
#[path = "tests/heading.rs"]
mod tests;
