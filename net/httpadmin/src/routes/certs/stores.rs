// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Certificate stores (`cert-stores.html`) with Create, one store's
//! entries by tab (`cert-store-system.html`), its password and its
//! default certificate.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::Kind;
use qsweb::partials::{
    Button, Cell, Col, List, Paging, Prop, PropGroup, actions, buttons, form, list_table, paging, props, props_table,
    tabs,
};

use super::{Rq, failed, invalid, issuer_name, store_url, url_q};
use crate::PRODUCT;
use crate::env::{CertRow, EntryType, StoreInfo, StoreType};
use crate::msgs;

/// Where new stores go unless a path says otherwise.
pub const STORE_DIR: &str = "/QIBM/UserData/ICSS/Cert/";

/// A store page's tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    /// Server and client certificates.
    Server,
    /// Certificate authorities.
    Ca,
    /// Certificate requests.
    Request,
}

fn find(rq: &Rq<'_>, name: &str) -> Option<StoreInfo> {
    rq.env().certs.stores().into_iter().find(|s| s.name.eq_ignore_ascii_case(name))
}

/// Whether `n` is a store name a user may give: 1 to 10 of A-Z, 0-9, `_`,
/// starting with a letter.
fn name_ok(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 10
        && n.starts_with(|c: char| c.is_ascii_uppercase())
        && n.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// `GET /certs/stores`, `POST` creates a store.
pub fn list(rq: &Rq<'_>) -> Response {
    if rq.post() {
        return create_store(rq);
    }
    let q = rq.query();
    let sort = q.str("sort");
    let mut all = rq.env().certs.stores();
    match sort {
        "name" => all.sort_by(|a, b| a.name.cmp(&b.name)),
        "type" => all.sort_by_key(|s| s.kind.label()),
        "n" => all.sort_by_key(|s| std::cmp::Reverse(s.certificates)),
        _ => {}
    }
    let rows = all
        .iter()
        .map(|s| {
            let open = store_url(&s.name, "");
            let acts = if s.kind == StoreType::Other {
                actions(&[
                    (open, "Open"),
                    (url_q("/certs/confirm", &[("op", "dltstore"), ("store", &s.name)]), "Delete"),
                ])
            } else {
                actions(&[(open, "Open"), (store_url(&s.name, "password"), "Change password")])
            };
            vec![
                Cell::act(acts),
                Cell::mono(&s.name),
                Cell::text(s.kind.label()),
                Cell::mono(&s.path),
                Cell::num(&s.certificates.to_string()),
                Cell::text(s.default.as_deref().unwrap_or("")),
            ]
        })
        .collect();
    let table = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Store", "name"),
            Col::sorted("Type", "type"),
            Col::plain("Path"),
            Col::sorted("Certificates", "n"),
            Col::plain("Default certificate"),
        ],
        rows,
        empty: "No certificate stores exist on this system.".into(),
        keep: Vec::new(),
    };
    let create = form(
        &rq.ctx,
        "",
        &format!(
            "<h3>Create new certificate store</h3>{}{}",
            props_table(
                &PRODUCT,
                &[PropGroup {
                    title: None,
                    rows: vec![
                        Prop::new("Store name", qsweb::partials::input("sn", "", 20)),
                        Prop::new("Path", qsweb::partials::input_cmd("sp", STORE_DIR, 45)),
                        Prop::new("Password", password_field("p1")),
                        Prop::new("Confirm password", password_field("p2")),
                    ],
                }],
                false
            ),
            buttons(&[Button::new("create", "Create")])
        ),
    );
    rq.page(Page {
        title: "Certificate stores".into(),
        crumb: vec!["Certificates".into(), "Certificate stores".into()],
        h1: "Certificate stores".into(),
        desc: Some(
            "Certificate stores on this system. The *SYSTEM store holds the server certificates used by TCP servers."
                .into(),
        ),
        body: format!("{}{create}", list_table(&table)),
        help: "cert-stores".into(),
        nav: Some(("Certificates", Some("/certs/stores"))),
        ..Page::default()
    })
}

/// A password field; it never carries a value.
pub fn password_field(name: &str) -> String {
    format!("<input type=\"password\" name=\"{}\" size=\"20\">", esc(name))
}

fn create_store(rq: &Rq<'_>) -> Response {
    let f = rq.form();
    let name = f.str("sn").trim().to_ascii_uppercase();
    let mut path = f.str("sp").trim().to_string();
    if !name_ok(&name) {
        return rq.done("/certs/stores", invalid("a store name is 1 to 10 letters, digits or underscores, starting with a letter"));
    }
    if path.is_empty() {
        path = STORE_DIR.into();
    }
    if !path.starts_with('/') {
        return rq.done("/certs/stores", invalid("the path must start with /"));
    }
    if path.ends_with('/') {
        let _ = write!(path, "{name}.KDB");
    }
    let (p1, p2) = (f.str("p1"), f.str("p2"));
    if p1 != p2 {
        return rq.done("/certs/stores", invalid("the two passwords are not the same"));
    }
    if p1.chars().count() < 6 {
        return rq.done("/certs/stores", invalid("a store password needs at least 6 characters"));
    }
    match rq.env().certs.create_store(rq.user(), &name, &path, p1) {
        Ok(()) => {
            rq.audit_change(&path, &format!("certificate store {name} created"));
            rq.done("/certs/stores", msgs::message(Kind::Done, "HTP8306", &[&name, &path]))
        }
        Err(e) => rq.done("/certs/stores", failed(&e)),
    }
}

/// The store line above a store's pages.
fn store_line(s: &StoreInfo) -> String {
    format!(
        "<p class=\"small\">Store: <b class=\"mono\">{}</b> &nbsp; Path: <span class=\"mono\">{}</span> &nbsp; <a href=\"{}\">Close store</a></p>",
        esc(&s.name),
        esc(&s.path),
        esc(&PRODUCT.url("/certs/stores"))
    )
}

fn cert_url(store: &str, label: &str, export: bool) -> String {
    let mut q = vec![("store", store), ("label", label)];
    if export {
        q.push(("export", "1"));
    }
    url_q("/certs/cert", &q)
}

fn date(rq: &Rq<'_>, secs: u64) -> String {
    rq.stamp(secs).0
}

/// `GET /certs/store/<name>[/cas|/requests]`.
#[allow(clippy::too_many_lines)]
pub fn store(rq: &Rq<'_>, name: &str, tab: Tab) -> Response {
    let Some(s) = find(rq, name) else {
        return rq.not_found(&format!("certificate store {name}"));
    };
    let rows = match rq.env().certs.entries(&s.name) {
        Ok(r) => r,
        Err(e) => return rq.done("/certs/stores", failed(&e)),
    };
    let now = rq.env().system.now();
    let mut shown: Vec<&CertRow> = rows
        .iter()
        .filter(|r| match tab {
            Tab::Server => matches!(r.kind, EntryType::Server | EntryType::Client),
            Tab::Ca => r.kind == EntryType::Ca,
            Tab::Request => r.kind == EntryType::Request,
        })
        .collect();
    // The default first, then in the order they were added.
    shown.sort_by_key(|r| (!r.is_default, r.added));
    let q = rq.query();
    let sort = q.str("sort");
    match sort {
        "label" => shown.sort_by_key(|r| r.label.to_lowercase()),
        "subj" => shown.sort_by_key(|r| super::dn(&r.subject).to_lowercase()),
        "issuer" => shown.sort_by_key(|r| issuer_name(r, &rows).to_lowercase()),
        "exp" => shown.sort_by_key(|r| r.not_after),
        "key" => shown.sort_by_key(|r| r.key.clone()),
        "st" => shown.sort_by_key(|r| super::days_left(r, now)),
        _ => {}
    }
    let keep = if sort.is_empty() { Vec::new() } else { vec![("sort".to_string(), sort.to_string())] };
    let p = Paging::from_query(q.str("page"), q.str("rows"), shown.len(), keep);
    let delete = |r: &CertRow| url_q("/certs/confirm", &[("op", "delete"), ("store", &s.name), ("label", &r.label)]);
    let view = |r: &CertRow| cert_url(&s.name, &r.label, false);
    let export = |r: &CertRow| cert_url(&s.name, &r.label, true);
    let (cols, rows_html): (Vec<Col>, Vec<Vec<Cell>>) = match tab {
        Tab::Server => (
            vec![
                Col::plain("Actions"),
                Col::sorted("Label", "label"),
                Col::sorted("Subject", "subj"),
                Col::sorted("Issuer", "issuer"),
                Col::sorted("Expires", "exp"),
                Col::sorted("Key", "key"),
                Col::sorted("Status", "st"),
            ],
            shown[p.range()]
                .iter()
                .map(|r| {
                    let expired = r.not_after < now;
                    let mut links = vec![(view(r), "View")];
                    if !expired {
                        let renew = url_q("/certs/create/step/1", &[("store", &s.name), ("renew", &r.label)]);
                        links.push((renew, "Renew"));
                    }
                    links.push((export(r), "Export"));
                    links.push((delete(r), "Delete"));
                    let mut st = super::expiry_status(r, now).unwrap_or_default();
                    if r.is_default {
                        st.push_str(" (default)");
                    }
                    vec![
                        Cell::act(actions(&links)),
                        Cell::mono(&r.label),
                        Cell::text(&super::dn(&r.subject)),
                        Cell::text(&issuer_name(r, &rows)),
                        Cell::text(&date(rq, r.not_after)),
                        Cell::text(&r.key),
                        Cell::html(st),
                    ]
                })
                .collect(),
        ),
        Tab::Ca => (
            vec![
                Col::plain("Actions"),
                Col::sorted("Label", "label"),
                Col::sorted("Subject", "subj"),
                Col::sorted("Expires", "exp"),
                Col::sorted("Key", "key"),
                Col::sorted("Status", "st"),
            ],
            shown[p.range()]
                .iter()
                .map(|r| {
                    let st = if r.not_after < now {
                        qsweb::partials::status("Expired", Some(false))
                    } else if r.trusted {
                        qsweb::partials::status("Trusted", Some(true))
                    } else {
                        "Not trusted".into()
                    };
                    vec![
                        Cell::act(actions(&[(view(r), "View"), (export(r), "Export"), (delete(r), "Delete")])),
                        Cell::mono(&r.label),
                        Cell::text(&super::dn(&r.subject)),
                        Cell::text(&date(rq, r.not_after)),
                        Cell::text(&r.key),
                        Cell::html(st),
                    ]
                })
                .collect(),
        ),
        Tab::Request => (
            vec![
                Col::plain("Actions"),
                Col::sorted("Label", "label"),
                Col::sorted("Subject", "subj"),
                Col::sorted("Key", "key"),
                Col::plain("Created"),
            ],
            shown[p.range()]
                .iter()
                .map(|r| {
                    vec![
                        Cell::act(actions(&[
                            (view(r), "View"),
                            (export(r), "Download request"),
                            (delete(r), "Delete"),
                        ])),
                        Cell::mono(&r.label),
                        Cell::text(&super::dn(&r.subject)),
                        Cell::text(&r.key),
                        Cell::text(&date(rq, r.added)),
                    ]
                })
                .collect(),
        ),
    };
    let empty = match tab {
        Tab::Server => "The store holds no server or client certificates.",
        Tab::Ca => "The store holds no certificate authority certificates.",
        Tab::Request => "No certificate requests are waiting in the store.",
    };
    let table = List {
        cols,
        rows: rows_html,
        empty: empty.into(),
        keep: Vec::new(),
    };
    let current = match tab {
        Tab::Server => 0,
        Tab::Ca => 1,
        Tab::Request => 2,
    };
    let tabs_html = tabs(
        &[
            ("Server certificates", store_url(&s.name, "")),
            ("Certificate authorities", store_url(&s.name, "cas")),
            ("Certificate requests", store_url(&s.name, "requests")),
        ],
        current,
    );
    let more = if p.pages() > 1 { paging(&p) } else { String::new() };
    let create = url_q("/certs/create/step/1", &[("store", &s.name)]);
    let import = url_q("/certs/import", &[("store", &s.name)]);
    let dft = store_url(&s.name, "default");
    let buttons_html = form(
        &rq.ctx,
        &create,
        &format!(
            "<div class=\"buttons\"><input type=\"submit\" value=\"Create certificate...\"><input type=\"submit\" value=\"Import certificate...\" formaction=\"{}\"><input type=\"submit\" value=\"Set default\" formaction=\"{}\"></div>",
            esc(&import),
            esc(&dft)
        ),
    );
    rq.page(Page {
        title: format!("Certificate store: {}", s.name),
        crumb: vec!["Certificates".into(), "Certificate stores".into(), s.name.clone()],
        h1: format!("Certificate store: {}", s.name),
        body: format!("{}{tabs_html}{}{more}{buttons_html}", store_line(&s), list_table(&table)),
        help: "cert-store".into(),
        ..Page::default()
    })
}

/// `/certs/store/<name>/password`: change the store's password.
pub fn password(rq: &Rq<'_>, name: &str) -> Response {
    let Some(s) = find(rq, name) else {
        return rq.not_found(&format!("certificate store {name}"));
    };
    let here = format!("/certs/store/{}/password", s.name);
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/certs/stores");
        }
        let (old, new, again) = (f.str("old"), f.str("p1"), f.str("p2"));
        if new != again {
            return rq.done(&here, invalid("the two new passwords are not the same"));
        }
        if new.chars().count() < 6 {
            return rq.done(&here, invalid("a store password needs at least 6 characters"));
        }
        return match rq.env().certs.change_password(rq.user(), &s.name, old, new) {
            Ok(()) => {
                rq.audit_change(&s.path, "certificate store password changed");
                rq.done("/certs/stores", msgs::message(Kind::Done, "HTP8307", &[&s.name]))
            }
            Err(e) => rq.done(&here, failed(&e)),
        };
    }
    let body = format!(
        "{}{}",
        store_line(&s),
        props(
            &rq.ctx,
            &[PropGroup {
                title: None,
                rows: vec![
                    Prop::new("Current password", password_field("old")).required(),
                    Prop::new("New password", password_field("p1")).required().help("cert-password"),
                    Prop::new("Confirm new password", password_field("p2")).required(),
                ],
            }],
            &[Button::new("change", "Change"), Button::new("cancel", "Cancel")],
        )
    );
    rq.page(Page {
        title: format!("Change password: {}", s.name),
        crumb: vec![
            "Certificates".into(),
            "Certificate stores".into(),
            s.name.clone(),
            "Change password".into(),
        ],
        h1: "Change store password".into(),
        desc: Some("The store is sealed again under the new password; servers keep opening it through its stash.".into()),
        body,
        help: "cert-password".into(),
        ..Page::default()
    })
}

/// `/certs/store/<name>/default`: choose the default certificate.
pub fn set_default(rq: &Rq<'_>, name: &str) -> Response {
    let Some(s) = find(rq, name) else {
        return rq.not_found(&format!("certificate store {name}"));
    };
    let here = format!("/certs/store/{}/default", s.name);
    let back = format!("/certs/store/{}", s.name);
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect(&back);
        }
        let label = f.str("label");
        if label.is_empty() || !f.has("ok") {
            return rq.redirect(&here);
        }
        return match rq.env().certs.set_default(rq.user(), &s.name, label) {
            Ok(()) => {
                rq.audit_change(&s.path, &format!("default certificate {label}"));
                rq.done(&back, msgs::message(Kind::Done, "HTP8305", &[label, &s.name]))
            }
            Err(e) => rq.done(&here, failed(&e)),
        };
    }
    let rows = rq.env().certs.entries(&s.name).unwrap_or_default();
    let now = rq.env().system.now();
    let mut ids: Vec<&CertRow> = rows
        .iter()
        .filter(|r| matches!(r.kind, EntryType::Server | EntryType::Client) && r.not_after >= now)
        .collect();
    ids.sort_by_key(|r| r.added);
    let labels: Vec<&str> = ids.iter().map(|r| r.label.as_str()).collect();
    let current = s.default.clone().unwrap_or_default();
    let body = format!(
        "{}{}",
        store_line(&s),
        props(
            &rq.ctx,
            &[PropGroup {
                title: None,
                rows: vec![Prop::new("Default certificate", qsweb::partials::select("label", &labels, &current))],
            }],
            &[Button::new("ok", "OK"), Button::new("cancel", "Cancel")],
        )
    );
    rq.page(Page {
        title: format!("Set default: {}", s.name),
        crumb: vec![
            "Certificates".into(),
            "Certificate stores".into(),
            s.name.clone(),
            "Set default".into(),
        ],
        h1: "Set default certificate".into(),
        desc: Some("Servers that open the store without naming a certificate present the default certificate.".into()),
        body,
        help: "cert-store".into(),
        ..Page::default()
    })
}
