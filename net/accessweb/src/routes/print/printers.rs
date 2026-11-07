// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/print/printers`: the printers, their writers' state and output
//! queues. Print to opens Printer output with the printer's queue
//! chosen for Move; Output lists the files on its queue.

use qsweb::frame::Page;
use qsweb::html::query;
use qsweb::http::Response;
use qsweb::partials::{Cell, Col, List, actions, list_table, status};

use super::{GROUP, OUTPUT, PRINTERS};
use crate::routes::Rq;

/// `GET /print/printers`.
pub fn page(rq: &Rq<'_>) -> Response {
    let rows = rq
        .env()
        .print
        .printers(rq.user())
        .into_iter()
        .map(|p| {
            let to = rq
                .ctx
                .url(&format!("{OUTPUT}{}", query(&[("to", &p.outq)])));
            let out = rq.ctx.url(&format!(
                "{OUTPUT}{}",
                query(&[("u", "*ALL"), ("q", &p.outq)])
            ));
            vec![
                Cell::act(actions(&[(to, "Print to"), (out, "Output")])),
                Cell::mono(&p.name),
                Cell::html(if p.started {
                    status("Started", Some(true))
                } else {
                    status("Ended", Some(false))
                }),
                Cell::mono(&p.outq),
                Cell::text(&p.location),
                Cell::text(&p.kind),
            ]
        })
        .collect();
    let list = List {
        cols: [
            "Actions",
            "Printer",
            "Status",
            "Output queue",
            "Location",
            "Type",
        ]
        .iter()
        .map(|c| Col::plain(c))
        .collect(),
        rows,
        empty: "No printers.".into(),
        keep: Vec::new(),
    };
    let body = format!(
        "{}<p class=\"small grey\">Printer drivers for your workstation are on the <a href=\"{}\">Download</a> page.</p>",
        list_table(&list),
        qsweb::html::esc(&rq.ctx.url("/download"))
    );
    rq.page(Page {
        title: "Printers".into(),
        crumb: vec![GROUP.into(), "Printers".into()],
        h1: "Printers".into(),
        body,
        help: "printers".into(),
        nav: Some((GROUP, Some(PRINTERS))),
        ..Page::default()
    })
}
