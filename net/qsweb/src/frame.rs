// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The page frame every signed-on page shares: header band, navigation
//! tree, content (breadcrumb, title, description, messages, body),
//! footer. Also the sign-on page and the help window.

use askama::Template;

use crate::html::esc;
use crate::http::{Request, Response};
use crate::msg::Message;
use crate::partials;
use crate::session::{self, Sessions};

/// The stylesheet's path, the same for every product.
pub const CSS_PATH: &str = "/qs400/shared/qs400.css";

/// A navigation group and its items (label, route under the prefix).
/// A group whose one item has the group's own label is a leaf: a plain
/// link with no `[+]` glyph (Access for Web's `My home page`).
#[derive(Clone, Copy, Debug)]
pub struct NavGroup {
    /// `Manage`.
    pub label: &'static str,
    /// `("Server instances", "/instances")`.
    pub items: &'static [(&'static str, &'static str)],
}

/// A web product: its name, route prefix and navigation tree.
#[derive(Clone, Copy, Debug)]
pub struct Product {
    /// The header band's wordmark and the title suffix.
    pub wordmark: &'static str,
    /// The breadcrumb's first entry.
    pub crumb_root: &'static str,
    /// Every route starts with this (`/qs400/httpadmin`).
    pub prefix: &'static str,
    /// The tree.
    pub nav: &'static [NavGroup],
    /// The header's label before the system name (`System`).
    pub system_label: &'static str,
}

impl NavGroup {
    /// The route of a leaf entry, `None` for a group.
    pub fn leaf(&self) -> Option<&'static str> {
        match self.items {
            [(label, route)] if *label == self.label => Some(route),
            _ => None,
        }
    }
}

impl Product {
    /// `route` under the prefix.
    pub fn url(&self, route: &str) -> String {
        format!("{}{route}", self.prefix)
    }

    /// The help window for `key`.
    pub fn help(&self, key: &str) -> String {
        self.url(&format!("/help/{key}"))
    }

    /// The second-level text of `msgid`.
    pub fn msg(&self, msgid: &str) -> String {
        self.url(&format!("/msg/{msgid}"))
    }
}

/// Who is looking: the request's view of the product.
#[derive(Clone, Debug)]
pub struct Ctx {
    /// The product.
    pub product: &'static Product,
    /// The system name.
    pub system: String,
    /// The signed-on user ("" on the sign-on page).
    pub user: String,
    /// The session's CSRF token.
    pub csrf: String,
    /// The navigation groups the user has open ([`crate::session::nav_open`]);
    /// `None` opens the page's own group only.
    pub open: Option<Vec<String>>,
    /// This page's URL, where the tree's `[+]` and `[-]` return.
    pub here: String,
}

impl Ctx {
    /// `route` under the prefix.
    pub fn url(&self, route: &str) -> String {
        self.product.url(route)
    }

    /// The hidden CSRF field every POST form carries.
    pub fn csrf_field(&self) -> String {
        format!(
            "<input type=\"hidden\" name=\"{}\" value=\"{}\">",
            crate::session::CSRF_FIELD,
            esc(&self.csrf)
        )
    }
}

/// One page's content.
#[derive(Clone, Debug, Default)]
pub struct Page {
    /// The `<title>` before the product's name.
    pub title: String,
    /// The breadcrumb after the product's root.
    pub crumb: Vec<String>,
    /// The `h1`.
    pub h1: String,
    /// The grey line under the title.
    pub desc: Option<String>,
    /// Message rows.
    pub messages: Vec<Message>,
    /// The body, HTML.
    pub body: String,
    /// The help key behind the header's Help.
    pub help: String,
    /// The navigation group to open and its current item's route.
    pub nav: Option<(&'static str, Option<&'static str>)>,
    /// Seconds between automatic reloads.
    pub refresh: Option<u32>,
    /// Render as a generic QS/400 page (shared pages): wordmark and
    /// title suffix `QS/400`.
    pub generic: bool,
}

struct NavLink {
    label: &'static str,
    href: String,
    current: bool,
}

struct NavView {
    label: &'static str,
    href: String,
    open: bool,
    items: Vec<NavLink>,
    leaf: Option<NavLink>,
}

#[derive(Template)]
#[template(path = "frame.html")]
struct FrameT<'a> {
    title: &'a str,
    suffix: &'a str,
    css: &'a str,
    wordmark: &'a str,
    system_label: &'a str,
    system: &'a str,
    user: &'a str,
    signoff: String,
    help: String,
    nav: Vec<NavView>,
    crumb: String,
    h1: &'a str,
    desc: Option<&'a str>,
    messages: String,
    body: &'a str,
    footer: String,
    refresh: Option<u32>,
    note: Option<&'a str>,
}

/// The URL of `req` (path and query), as [`Ctx::here`].
pub fn here(req: &Request) -> String {
    if req.query.is_empty() { req.path.clone() } else { format!("{}?{}", req.path, req.query) }
}

/// `/nav?open=G` or `?close=G`: open or close a group of the tree for
/// session `id`, then back to `back=` (a page of the product).
pub fn nav_toggle(product: &Product, sessions: &Sessions, id: &str, req: &Request) -> Response {
    let q = req.query();
    session::nav_toggle(sessions, id, q.get("open"), q.get("close"));
    match q.get("back").filter(|b| b.starts_with(product.prefix) && !b.contains("//")) {
        Some(b) => Response::redirect(b),
        None => Response::redirect(&product.url("/")),
    }
}

/// The footer line.
pub fn footer() -> String {
    format!("QS/400 {} \u{b7} Quartz Systems", release::RELEASE)
}

/// Render `page` in the frame.
pub fn render(ctx: &Ctx, page: &Page) -> String {
    render_with(ctx, page, None)
}

/// [`render`] with `note`, HTML, in the header band before Sign off
/// (Access for Web's `Messages waiting: N`).
pub fn render_with(ctx: &Ctx, page: &Page, note: Option<&str>) -> String {
    let p = ctx.product;
    let wordmark = if page.generic { "QS/400" } else { p.wordmark };
    let nav = p
        .nav
        .iter()
        .map(|g| {
            if let Some(route) = g.leaf() {
                return NavView {
                    label: g.label,
                    href: p.url(route),
                    open: false,
                    items: Vec::new(),
                    leaf: Some(NavLink {
                        label: g.label,
                        href: p.url(route),
                        current: page.nav.is_some_and(|(l, r)| l == g.label && r == Some(route)),
                    }),
                };
            }
            let (open, href) = match &ctx.open {
                Some(set) => {
                    let open = set.iter().any(|l| l == g.label);
                    let verb = if open { "close" } else { "open" };
                    (open, p.url(&format!("/nav{}", crate::html::query(&[(verb, g.label), ("back", &ctx.here)]))))
                }
                None => (
                    page.nav.is_some_and(|(l, _)| l == g.label),
                    p.url(g.items.first().map_or("", |i| i.1)),
                ),
            };
            NavView {
                label: g.label,
                href,
                open,
                items: g
                    .items
                    .iter()
                    .map(|(label, route)| NavLink {
                        label,
                        href: p.url(route),
                        current: open
                            && page.nav.is_some_and(|(l, _)| l == g.label)
                            && page.nav.and_then(|n| n.1) == Some(*route),
                    })
                    .collect(),
                leaf: None,
            }
        })
        .collect();
    let mut crumb = vec![p.crumb_root.to_string()];
    crumb.extend(page.crumb.iter().cloned());
    FrameT {
        title: &page.title,
        suffix: wordmark,
        css: CSS_PATH,
        wordmark,
        system_label: p.system_label,
        system: &ctx.system,
        user: &ctx.user,
        signoff: p.url("/signoff"),
        help: p.help(&page.help),
        nav,
        crumb: crumb.join(" > "),
        h1: &page.h1,
        desc: page.desc.as_deref(),
        messages: partials::messages(p, &page.messages),
        body: &page.body,
        footer: footer(),
        refresh: page.refresh,
        note,
    }
    .render()
    .expect("frame renders")
}

#[derive(Template)]
#[template(path = "signon.html")]
struct SignonT<'a> {
    title: String,
    css: &'a str,
    wordmark: &'a str,
    system_label: &'a str,
    system: &'a str,
    notice: Option<&'a str>,
    messages: String,
    action: String,
    user: &'a str,
    release: String,
    footer: String,
}

/// The product's sign-on page: User and Password, nothing else.
/// `user` refills the field after a refusal; `notice` is the one line
/// some products show above the form.
pub fn signon(product: &Product, system: &str, user: &str, messages: &[Message], notice: Option<&str>) -> String {
    SignonT {
        title: format!("Sign on - {0} - {0}", product.wordmark),
        css: CSS_PATH,
        wordmark: product.wordmark,
        system_label: product.system_label,
        system,
        notice,
        messages: partials::messages(product, messages),
        action: product.url("/signon"),
        user,
        release: release::RELEASE.to_string(),
        footer: footer(),
    }
    .render()
    .expect("sign-on renders")
}

#[derive(Template)]
#[template(path = "help.html")]
struct HelpT<'a> {
    topic: &'a str,
    css: &'a str,
    contents: String,
    index: String,
    print: &'a str,
    body: &'a str,
    footer: String,
    close: &'a str,
}

/// The help window for `topic`, its body HTML.
pub fn help(product: &Product, topic: &str, body: &str) -> String {
    HelpT {
        topic,
        css: CSS_PATH,
        contents: product.url("/help/contents"),
        index: product.url("/help/index"),
        print: "#",
        body,
        footer: footer(),
        close: "#",
    }
    .render()
    .expect("help renders")
}
