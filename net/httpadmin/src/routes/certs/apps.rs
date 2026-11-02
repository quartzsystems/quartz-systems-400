// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Certificate applications (`cert-apps.html`): each application ID
//! with the `*SYSTEM` certificate assigned to it, Assign and Remove.

use qsweb::frame::Page;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{Button, Cell, Col, List, Prop, PropGroup, actions, buttons, form, grey, list_table, props_table, select};

use super::{Rq, WARN_DAYS, days_left, expiry_status, failed, invalid, url_q};
use crate::PRODUCT;
use crate::env::{AppInfo, CertFailure, CertRow, EntryType};
use crate::msgs;

/// The certificates an application can be given: `*SYSTEM` identities
/// that are not about to expire, in the order they were added.
fn assignable(rows: &[CertRow], now: u64) -> Vec<&CertRow> {
    let mut v: Vec<&CertRow> = rows
        .iter()
        .filter(|r| matches!(r.kind, EntryType::Server | EntryType::Client) && days_left(r, now) > WARN_DAYS)
        .collect();
    v.sort_by_key(|r| r.added);
    v
}

/// The status of an application's certificate, and a sort key.
fn status(a: &AppInfo, rows: &[CertRow], now: u64) -> (String, i64) {
    let Some(l) = &a.label else {
        return ("No certificate".into(), i64::MIN);
    };
    match rows.iter().find(|r| &r.label == l) {
        Some(r) => (expiry_status(r, now).unwrap_or_default(), days_left(r, now)),
        None => (qsweb::partials::status("Not in store", Some(false)), i64::MIN + 1),
    }
}

/// `GET /certs/apps[?app=&cert=]`, `POST` assigns.
pub fn page(rq: &Rq<'_>) -> Response {
    if rq.post() {
        return assign(rq);
    }
    let certs = &rq.env().certs;
    let now = rq.env().system.now();
    let system = certs.entries("*SYSTEM");
    let rows = system.clone().unwrap_or_default();
    let mut apps = certs.apps();
    let q = rq.query();
    let sort = q.str("sort");
    match sort {
        "id" => apps.sort_by(|a, b| a.id.cmp(&b.id)),
        "type" => apps.sort_by_key(|a| !a.server),
        "cert" => apps.sort_by_key(|a| a.label.clone().unwrap_or_default().to_lowercase()),
        "st" => apps.sort_by_key(|a| status(a, &rows, now).1),
        _ => {}
    }
    let table_rows = apps
        .iter()
        .map(|a| {
            let assign = url_q("/certs/apps", &[("app", &a.id)]);
            let remove = url_q("/certs/confirm", &[("op", "unassign"), ("app", &a.id)]);
            vec![
                Cell::act(actions(&[(assign, "Assign"), (remove, "Remove")])),
                Cell::mono(&a.id),
                Cell::text(if a.server { "Server" } else { "Client" }),
                Cell::text(&a.description),
                a.label.as_deref().map_or_else(|| Cell::html(grey("(none)")), Cell::text),
                Cell::mono("*SYSTEM"),
                Cell::html(status(a, &rows, now).0),
            ]
        })
        .collect();
    let table = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Application ID", "id"),
            Col::sorted("Type", "type"),
            Col::plain("Description"),
            Col::sorted("Assigned certificate", "cert"),
            Col::plain("Store"),
            Col::sorted("Status", "st"),
        ],
        rows: table_rows,
        empty: "No application IDs are registered.".into(),
        keep: Vec::new(),
    };
    // Those that need a certificate: none, or one about to expire.
    let want = q.str("app");
    let mut ids: Vec<&str> = apps
        .iter()
        .filter(|a| a.id == want || status(a, &rows, now).1 <= WARN_DAYS)
        .map(|a| a.id.as_str())
        .collect();
    ids.sort_unstable();
    let options: Vec<String> = assignable(&rows, now)
        .iter()
        .map(|r| format!("{} (expires {})", r.label, rq.stamp(r.not_after).0))
        .collect();
    let opts: Vec<&str> = options.iter().map(String::as_str).collect();
    let cert = q.str("cert");
    let chosen = options.iter().find(|o| label_of(o) == cert).map_or("", String::as_str);
    let (messages, copy_html) = why_none(rq, &system, options.is_empty());
    let form_html = form(
        &rq.ctx,
        "",
        &format!(
            "<h3>Assign certificate</h3>{}{}",
            props_table(
                &PRODUCT,
                &[PropGroup {
                    title: None,
                    rows: vec![
                        Prop::new("Application", select("app", &ids, want)),
                        Prop::new("Certificate", select("cert", &opts, chosen)),
                    ],
                }],
                false
            ),
            buttons(&[Button::new("assign", "Assign")])
        ),
    );
    rq.page(Page {
        title: "Certificate applications".into(),
        crumb: vec!["Certificates".into(), "Applications".into()],
        h1: "Certificate applications".into(),
        desc: Some(
            "Each TCP server and HTTP instance has an application ID. The certificate assigned to the ID is the one the server presents."
                .into(),
        ),
        messages,
        body: format!("{}{copy_html}{form_html}", list_table(&table)),
        help: "cert-apps".into(),
        nav: Some(("Certificates", Some("/certs/apps"))),
        ..Page::default()
    })
}

/// Why the Certificate list is short, as message rows, and the Copy to
/// `*SYSTEM` form for a certificate of another store (`?cert=&store=`,
/// from its page's Assign to application).
fn why_none(rq: &Rq<'_>, system: &Result<Vec<CertRow>, CertFailure>, empty: bool) -> (Vec<Message>, String) {
    let q = rq.query();
    let (label, store) = (q.str("cert"), q.str("store"));
    let mut out = Vec::new();
    let mut copy = String::new();
    let elsewhere = !store.is_empty() && !store.eq_ignore_ascii_case("*SYSTEM") && !label.is_empty();
    match system {
        Err(e) => out.push(msgs::message(Kind::Error, "HTP8313", &[&e.to_string()])),
        Ok(rows) if elsewhere && rows.iter().all(|r| r.label != label) => {
            out.push(msgs::message(Kind::Info, "HTP8314", &[label, store]));
            copy = form(
                &rq.ctx,
                &url_q("/certs/cert", &[("store", store), ("label", label)]),
                &buttons(&[Button::new("copy", "Copy to *SYSTEM")]),
            );
        }
        Ok(_) if empty => out.push(msgs::message(Kind::Info, "HTP8312", &[&WARN_DAYS.to_string()])),
        Ok(_) => {}
    }
    (out, copy)
}

/// The label in a certificate option (`label (expires 01/02/28)`).
pub fn label_of(option: &str) -> &str {
    option.rsplit_once(" (expires ").map_or(option, |(l, _)| l)
}

fn assign(rq: &Rq<'_>) -> Response {
    let f = rq.form();
    let app = f.str("app").trim();
    let label = label_of(f.str("cert").trim());
    if app.is_empty() || label.is_empty() {
        return rq.done("/certs/apps", invalid("choose an application and a certificate"));
    }
    match rq.env().certs.assign(rq.user(), app, Some(label)) {
        Ok(()) => {
            rq.audit_change(app, &format!("certificate {label} assigned"));
            rq.done("/certs/apps", msgs::message(Kind::Done, "HTP8310", &[label, app]))
        }
        Err(e) => rq.done("/certs/apps", failed(&e)),
    }
}
