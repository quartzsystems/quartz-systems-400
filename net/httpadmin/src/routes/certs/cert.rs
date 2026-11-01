// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One certificate: its page (View and Export), the PEM downloads, and
//! the confirmations that delete a certificate, delete a store or remove
//! an application's assignment.

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::Kind;
use qsweb::pages::confirm_body;
use qsweb::partials::{Button, Cell, Col, List, Prop, PropGroup, buttons, form, list_table, props_table};

use super::{Rq, dn, failed, store_url, url_q};
use crate::PRODUCT;
use crate::env::{CertDetail, EntryType};
use crate::msgs;

/// Upper-case hex as spaced bytes (`41 53 34`).
pub fn spaced(hex: &str) -> String {
    hex.as_bytes()
        .chunks(2)
        .map(|c| std::str::from_utf8(c).unwrap_or("??"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The facts and PEM of a certificate, as the view and the create
/// wizard's result show them.
pub fn facts(rq: &Rq<'_>, d: &CertDetail) -> String {
    let r = &d.row;
    let mut rows = vec![Prop::new("Label", esc(&r.label)).mono(), Prop::new("Subject", esc(&dn(&r.subject))).mono()];
    if r.kind == EntryType::Request {
        rows.push(Prop::new("Key", esc(&r.key)));
    } else {
        rows.push(Prop::new("Issuer", esc(&dn(&r.issuer))).mono());
        rows.push(Prop::new("Serial", esc(&spaced(&d.serial))).mono());
        rows.push(Prop::new("Valid", esc(&format!("{} to {}", rq.when(r.not_before), rq.when(r.not_after)))));
        rows.push(Prop::new("Fingerprint (SHA-256)", esc(&d.fingerprint)).mono());
    }
    let heading = if r.kind == EntryType::Request { "Certificate request (PEM)" } else { "Certificate (PEM)" };
    format!(
        "{}<h3>{heading}</h3><pre class=\"out\">{}</pre>",
        props_table(&PRODUCT, &[PropGroup { title: None, rows }], false),
        esc(d.pem.trim_end())
    )
}

/// The download buttons: `download`, `chain` (not for a request) and
/// `assign`.
pub fn download_buttons(d: &CertDetail) -> String {
    let mut b = vec![Button::new("download", "Download")];
    if d.row.kind != EntryType::Request {
        b.push(Button::new("chain", "Download with chain"));
    }
    if matches!(d.row.kind, EntryType::Server | EntryType::Client) {
        b.push(Button::new("assign", "Assign to application..."));
    }
    buttons(&b)
}

/// Where a pressed download button goes: the download route or the
/// Applications page.
pub fn download_target(rq: &Rq<'_>, store: &str, label: &str) -> Option<Response> {
    let f = rq.form();
    if f.has("download") || f.has("chain") {
        let chain = if f.has("chain") { "1" } else { "" };
        return Some(Response::redirect(&url_q(
            "/certs/download",
            &[("store", store), ("label", label), ("chain", chain)],
        )));
    }
    if f.has("assign") {
        return Some(Response::redirect(&url_q("/certs/apps", &[("cert", label)])));
    }
    None
}

/// `/certs/cert?store=&label=[&export=1]`: one entry.
pub fn view(rq: &Rq<'_>) -> Response {
    let q = rq.query();
    let (store, label) = (q.str("store").to_string(), q.str("label").to_string());
    if rq.post() {
        if let Some(r) = download_target(rq, &store, &label) {
            return r;
        }
        return Response::redirect(&store_url(&store, ""));
    }
    let d = match rq.env().certs.detail(&store, &label) {
        Ok(d) => d,
        Err(e) => return rq.done("/certs/stores", failed(&e)),
    };
    let export = q.str("export") == "1";
    let body = format!(
        "{}{}",
        facts(rq, &d),
        form(
            &rq.ctx,
            "",
            &format!(
                "{}{}",
                download_buttons(&d),
                buttons(&[Button::new("close", "Close")])
            )
        )
    );
    let h1 = if export { "Export certificate" } else { "Certificate" };
    rq.page(Page {
        title: format!("{h1}: {label}"),
        crumb: vec!["Certificates".into(), "Certificate stores".into(), store.clone(), label.clone()],
        h1: format!("{h1}: {label}"),
        desc: export.then(|| {
            "Download the certificate as a PEM file, alone or with the certificates that issued it.".to_string()
        }),
        body,
        help: "cert-view".into(),
        ..Page::default()
    })
}

/// A file name for a label.
fn file_name(label: &str, ext: &str) -> String {
    let base: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' })
        .collect();
    format!("{}.{ext}", base.trim_matches('.'))
}

/// `GET /certs/download?store=&label=[&chain=1]`: the PEM file.
pub fn download(rq: &Rq<'_>) -> Response {
    let q = rq.query();
    let (store, label) = (q.str("store"), q.str("label"));
    let chain = q.str("chain") == "1";
    let request = rq
        .env()
        .certs
        .detail(store, label)
        .is_ok_and(|d| d.row.kind == EntryType::Request);
    match rq.env().certs.export(store, label, chain) {
        Ok(b) => {
            let name = if request { file_name(label, "csr") } else { file_name(label, "pem") };
            let ct = if request { "application/pkcs10" } else { "application/x-pem-file" };
            Response::download(&name, ct, b)
        }
        Err(e) => rq.done("/certs/stores", failed(&e)),
    }
}

/// `/certs/confirm?op=delete&store=&label=`, `?op=dltstore&store=`,
/// `?op=unassign&app=`: the confirmation page, and its OK.
#[allow(clippy::too_many_lines)]
pub fn confirm(rq: &Rq<'_>) -> Response {
    let q = rq.query();
    let op = q.str("op").to_string();
    let store = q.str("store").to_string();
    let label = q.str("label").to_string();
    let app = q.str("app").to_string();
    let certs = &rq.env().certs;
    let back = match op.as_str() {
        "delete" => format!("/certs/store/{store}"),
        "unassign" => "/certs/apps".to_string(),
        _ => "/certs/stores".to_string(),
    };
    if rq.post() {
        let f = rq.form();
        if !f.has("ok") {
            return rq.redirect(&back);
        }
        let done = match op.as_str() {
            "delete" => certs.remove(rq.user(), &store, &label).map(|()| {
                rq.audit_change(&store, &format!("certificate {label} removed"));
                msgs::message(Kind::Done, "HTP8304", &[&label, &store])
            }),
            "dltstore" => certs.delete_store(rq.user(), &store).map(|()| {
                rq.audit_change(&store, "certificate store deleted");
                msgs::message(Kind::Done, "HTP8308", &[&store])
            }),
            "unassign" => certs.assign(rq.user(), &app, None).map(|()| {
                rq.audit_change(&app, "certificate assignment removed");
                msgs::message(Kind::Done, "HTP8311", &[&app])
            }),
            _ => return rq.not_found("such confirmation"),
        };
        return match done {
            Ok(m) => rq.done(&back, m),
            Err(e) => rq.done(&back, failed(&e)),
        };
    }
    let (title, crumb, sentence, cols, row): (&str, Vec<String>, &str, Vec<&str>, Vec<Cell>) = match op.as_str() {
        "delete" => {
            let d = match certs.detail(&store, &label) {
                Ok(d) => d,
                Err(e) => return rq.done(&back, failed(&e)),
            };
            let apps: Vec<String> = if store == "*SYSTEM" {
                certs.apps().into_iter().filter(|a| a.label.as_deref() == Some(label.as_str())).map(|a| a.id).collect()
            } else {
                Vec::new()
            };
            (
                "Confirm Delete Certificate",
                vec!["Certificates".into(), "Certificate stores".into(), store.clone()],
                "Deleting a certificate removes it and its private key from the store. Applications assigned the certificate lose it. Press OK to delete the certificate listed below, or Cancel to return without deleting.",
                vec!["Label", "Subject", "Expires", "Assigned to"],
                vec![
                    Cell::mono(&label),
                    Cell::text(&dn(&d.row.subject)),
                    Cell::text(&if d.row.not_after == 0 { String::new() } else { rq.stamp(d.row.not_after).0 }),
                    Cell::mono(&if apps.is_empty() { "(none)".to_string() } else { apps.join(", ") }),
                ],
            )
        }
        "dltstore" => {
            let Some(s) = certs.stores().into_iter().find(|s| s.name == store) else {
                return rq.not_found(&format!("certificate store {store}"));
            };
            (
                "Confirm Delete Certificate Store",
                vec!["Certificates".into(), "Certificate stores".into()],
                "Deleting a certificate store removes the store file, every certificate and private key in it, and its stashed password. Press OK to delete the store listed below, or Cancel to return without deleting.",
                vec!["Store", "Type", "Path", "Certificates"],
                vec![
                    Cell::mono(&s.name),
                    Cell::text(s.kind.label()),
                    Cell::mono(&s.path),
                    Cell::num(&s.certificates.to_string()),
                ],
            )
        }
        "unassign" => {
            let Some(a) = certs.apps().into_iter().find(|a| a.id == app) else {
                return rq.not_found(&format!("application {app}"));
            };
            (
                "Confirm Remove Assignment",
                vec!["Certificates".into(), "Applications".into()],
                "Removing the assignment leaves the application without a certificate of its own; its server presents the store's default certificate the next time it starts. Press OK to remove the assignment listed below, or Cancel to return.",
                vec!["Application ID", "Description", "Assigned certificate"],
                vec![
                    Cell::mono(&a.id),
                    Cell::text(&a.description),
                    Cell::text(a.label.as_deref().unwrap_or("(none)")),
                ],
            )
        }
        _ => return rq.not_found("such confirmation"),
    };
    let table = list_table(&List {
        cols: cols.iter().map(|c| Col::plain(c)).collect(),
        rows: vec![row],
        empty: String::new(),
        keep: Vec::new(),
    });
    let mut crumb = crumb;
    crumb.push(title.into());
    rq.page(Page {
        title: title.into(),
        crumb,
        h1: title.into(),
        body: confirm_body(&rq.ctx, sentence, &table, ""),
        help: "cert-confirm".into(),
        ..Page::default()
    })
}
