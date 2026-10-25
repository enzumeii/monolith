use dioxus::prelude::*;

#[component]
pub(super) fn PortalFooter() -> Element {
    rsx! {
        footer { class: "portal-footer",
            nav { class: "portal-contact-links", aria_label: "Contact Enzumeii",
                ContactLink { href: "mailto:order@enzumeii.com", label: "Email Enzumeii about your order",
                    svg { view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.6", stroke_linecap: "round", stroke_linejoin: "round", "aria-hidden": "true", "focusable": "false",
                        rect { x: "3", y: "5", width: "18", height: "14", rx: "2" }
                        path { d: "m3 6 9 7 9-7" }
                    }
                }
                ContactLink { href: "https://www.facebook.com/enzumeii", label: "Enzumeii on Facebook",
                    svg { view_box: "0 0 24 24", fill: "currentColor", "aria-hidden": "true", "focusable": "false",
                        path { d: "M14 21v-8h3l.5-3H14V8c0-.9.3-1.5 1.6-1.5H18V3.8c-.4-.1-1.8-.3-3.2-.3-3.1 0-5.1 1.8-5.1 5.2V10H7v3h2.7v8z" }
                    }
                }
                ContactLink { href: "http://instagram.com/enzumeii", label: "Enzumeii on Instagram",
                    svg { view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.6", "aria-hidden": "true", "focusable": "false",
                        rect { x: "3", y: "3", width: "18", height: "18", rx: "5" }
                        circle { cx: "12", cy: "12", r: "4" }
                        circle { cx: "17.5", cy: "6.5", r: "1", fill: "currentColor", stroke: "none" }
                    }
                }
                ContactLink { href: "https://twitter.com/enzumeii", label: "Enzumeii on X (Twitter)",
                    svg { view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.6", stroke_linejoin: "round", "aria-hidden": "true", "focusable": "false",
                        path { d: "M4 3h4l12 18h-4L4 3ZM4 21l7-8M13 11l7-8" }
                    }
                }
                ContactLink { href: "https://www.tiktok.com/@enzumei", label: "Enzumeii on TikTok",
                    svg { view_box: "0 0 24 24", fill: "currentColor", "aria-hidden": "true", "focusable": "false",
                        path { d: "M14 3h3c.3 2.1 1.7 3.7 4 4v3a8 8 0 0 1-4-1.3V16a6 6 0 1 1-6-6v3a3 3 0 1 0 3 3V3Z" }
                    }
                }
            }
        }
    }
}

#[component]
fn ContactLink(href: &'static str, label: &'static str, children: Element) -> Element {
    rsx! {
        a { class: "portal-contact-link", href, aria_label: label, title: label, target: "_blank", rel: "noopener noreferrer",
            {children}
        }
    }
}

#[cfg(test)]
#[path = "../tests/pages/footer.rs"]
mod tests;
