// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/print/props/<id>`: every attribute of a spooled file, as
//! `WRKSPLFA` shows them. OK returns to the list.

use qsweb::frame::Page;
use qsweb::http::Response;
use qsweb::partials::{Button, Prop, PropGroup, mono, props};

use super::{GROUP, OUTPUT, find};
use crate::routes::Rq;

fn yes(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}

/// `GET` and `POST /print/props/<id>`.
pub fn page(rq: &Rq<'_>, id: &str) -> Response {
    let f = match find(rq, id) {
        Ok(f) => f,
        Err(r) => return r,
    };
    if rq.post() {
        return rq.redirect(OUTPUT);
    }
    let (d, t) = rq.stamp(f.created);
    let rows: Vec<(&str, String)> = vec![
        ("File", f.file.clone()),
        ("File number", f.number.to_string()),
        ("Job", f.job.clone()),
        ("User data", f.user_data.clone()),
        ("Status", f.status.clone()),
        ("Output queue", f.outq.clone()),
        ("Pages", f.pages.to_string()),
        ("Current page", f.current_page.to_string()),
        ("Copies", f.copies.to_string()),
        ("Form type", f.form_type.clone()),
        ("Priority", f.priority.to_string()),
        ("Created", format!("{d} {t}")),
        ("Device type", f.device_type.clone()),
        ("Page size", format!("{} x {}", f.page_len, f.page_width)),
        ("Lines per inch", f.lpi.to_string()),
        ("Characters per inch", f.cpi.to_string()),
        ("Overflow line", f.overflow.to_string()),
        ("Hold", yes(f.hold).into()),
        ("Save", yes(f.save).into()),
        ("Record length", f.record_len.to_string()),
        ("Total records", f.records.to_string()),
        ("Printer device", f.printer.clone()),
        ("Writer", if f.writer.is_empty() { "*NONE".into() } else { f.writer.clone() }),
    ];
    let body = props(
        &rq.ctx,
        &[PropGroup {
            title: None,
            rows: rows.into_iter().map(|(l, v)| Prop::new(l, mono(&v))).collect(),
        }],
        &[Button::new("ok", "OK")],
    );
    let title = format!("Spooled file properties: {}", f.file);
    rq.page(Page {
        title: title.clone(),
        crumb: vec![GROUP.into(), "Printer output".into(), f.file.clone()],
        h1: title,
        body,
        help: "splf-props".into(),
        nav: Some((GROUP, Some(OUTPUT))),
        ..Page::default()
    })
}
