// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Import certificate (`cert-import.html`): an uploaded PEM, DER or
//! PKCS#12 file, or a stream file on this system, into a store. The
//! PKCS#12 password goes to the one import call and nowhere else.

use qsweb::frame::Page;
use qsweb::http::Response;
use qsweb::msg::Kind;
use qsweb::partials::{Button, Prop, PropGroup, buttons, checkbox, input, input_cmd, props_table, radio, select, upload_form};

use super::stores::password_field;
use super::{Rq, failed, invalid, store_url, url_q};
use crate::PRODUCT;
use crate::env::{Import, ImportFormat, ImportSource};
use crate::msgs;

/// `/certs/import[?store=]`.
pub fn page(rq: &Rq<'_>) -> Response {
    let q = rq.query();
    if rq.post() {
        let f = rq.form();
        let st = f.get("store").or_else(|| q.get("store")).unwrap_or("*SYSTEM").to_string();
        if f.has("cancel") {
            return Response::redirect(&store_url(&st, ""));
        }
        // Import certificate... on a store page posts here plainly.
        if rq.req.multipart().is_none() {
            return Response::redirect(&url_q("/certs/import", &[("store", &st)]));
        }
        return import(rq, &st);
    }
    let stores = rq.env().certs.stores();
    let names: Vec<&str> = stores.iter().map(|s| s.name.as_str()).collect();
    let chosen = q.get("store").map_or_else(|| rq.get("certimp.store"), ToString::to_string);
    let chosen = if names.first().is_some_and(|f| *f == chosen) { String::new() } else { chosen };
    let fmt = rq.get("certimp.fmt");
    let ca = rq.get("certimp.t") == "ca";
    let groups = [
        PropGroup {
            title: Some("Source".into()),
            rows: vec![
                Prop::new("Import into store", select("store", &names, &chosen)),
                Prop::new(
                    "File",
                    "<input type=\"file\" name=\"file\" size=\"40\"> <span class=\"small grey\">PEM (.pem, .crt, .cer), DER or PKCS#12 (.p12, .pfx)</span>".into(),
                )
                .help("cert-import-file"),
                Prop::new(
                    "Or path on this system",
                    format!(
                        "{} <input type=\"submit\" value=\"Browse...\">",
                        input_cmd("path", &rq.get("certimp.path"), 45)
                    ),
                ),
                Prop::new(
                    "Format",
                    format!(
                        "{} Detect from file<br>{} PEM<br>{} PKCS#12",
                        radio("fmt", "auto", !matches!(fmt.as_str(), "pem" | "p12")),
                        radio("fmt", "pem", fmt == "pem"),
                        radio("fmt", "p12", fmt == "p12")
                    ),
                ),
            ],
        },
        PropGroup {
            title: Some("PKCS#12".into()),
            rows: vec![Prop::new("Password", password_field("pw"))],
        },
        PropGroup {
            title: Some("Options".into()),
            rows: vec![
                Prop::new(
                    "Certificate label",
                    format!(
                        "{} <span class=\"small grey\">blank: use the common name</span>",
                        input("label", &rq.get("certimp.label"), 30)
                    ),
                ),
                Prop::new(
                    "Import type",
                    format!(
                        "{} Server or client certificate (with private key)<br>{} Certificate authority certificate",
                        radio("t", "srv", !ca),
                        radio("t", "ca", ca)
                    ),
                ),
                Prop::new("After import", format!("{} Make default certificate", checkbox("dft", false))),
            ],
        },
    ];
    let body = upload_form(
        &rq.ctx,
        "",
        &format!(
            "\n{}\n{}",
            props_table(&PRODUCT, &groups, true),
            buttons(&[Button::new("import", "Import"), Button::new("cancel", "Cancel")])
        ),
    );
    rq.page(Page {
        title: "Import certificate".into(),
        crumb: vec![
            "Certificates".into(),
            "Certificate stores".into(),
            "Import certificate".into(),
        ],
        h1: "Import certificate".into(),
        body,
        help: "cert-import".into(),
        ..Page::default()
    })
}

fn import(rq: &Rq<'_>, st: &str) -> Response {
    let f = rq.form();
    let parts = rq.req.multipart().unwrap_or_default();
    let file = parts
        .into_iter()
        .find(|p| p.name == "file" && p.filename.as_deref().is_some_and(|n| !n.is_empty()))
        .map(|p| p.data)
        .filter(|d| !d.is_empty());
    let path = f.str("path").trim().to_string();
    let label = f.str("label").trim().to_string();
    let fmt = f.str("fmt").to_string();
    let t = f.str("t").to_string();
    // Remembered for the page if the import fails; never the password.
    rq.set(&[
        ("certimp.store", st),
        ("certimp.path", &path),
        ("certimp.label", &label),
        ("certimp.fmt", &fmt),
        ("certimp.t", &t),
    ]);
    let back = url_q("/certs/import", &[("store", st)]);
    let source = match (file, path.is_empty()) {
        (Some(d), _) => ImportSource::Data(d),
        (None, false) => ImportSource::Path(path.clone()),
        (None, true) => {
            rq.flash(invalid("choose a file to upload or give the path of a stream file"));
            return Response::redirect(&back);
        }
    };
    let pw = f.str("pw");
    let import = Import {
        store: st.to_string(),
        source,
        format: match fmt.as_str() {
            "pem" => ImportFormat::Pem,
            "p12" => ImportFormat::Pkcs12,
            _ => ImportFormat::Auto,
        },
        password: (!pw.is_empty()).then(|| pw.to_string()),
        label: (!label.is_empty()).then(|| label.clone()),
        ca: t == "ca",
        make_default: f.has("dft"),
    };
    match rq.env().certs.import(rq.user(), &import) {
        Ok(labels) => {
            rq.clear("certimp.");
            let shown = labels.join(", ");
            rq.audit_change(st, &format!("certificate {shown} imported"));
            let tab = if import.ca { "cas" } else { "" };
            rq.flash(msgs::message(Kind::Done, "HTP8302", &[&shown, st]));
            Response::redirect(&store_url(st, tab))
        }
        Err(e) => {
            rq.flash(failed(&e));
            Response::redirect(&back)
        }
    }
}
