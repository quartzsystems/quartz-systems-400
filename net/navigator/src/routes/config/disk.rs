// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/disk`: the ASPs and disk units (`WRKDSKSTS`); `?reset=1` restarts
//! the user's I/O counters (`RESET(*YES)`), which Disk activity reads.
//! `/disk/<unit>`: one unit.

use std::cmp::Reverse;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{Cell, Col, List, Prop, PropGroup, actions, list_table, mono, props_table};

use super::{bar, gb, page};
use crate::env::config::DiskUnit;
use crate::routes::Rq;
use crate::util::tenths;

fn status(u: &DiskUnit) -> String {
    let good = match u.status.as_str() {
        "Active" => Some(true),
        "Non-configured" => None,
        _ => Some(false),
    };
    qsweb::partials::status(&u.status, good)
}

/// `HH:MM:SS` of seconds.
pub fn hms(s: u32) -> String {
    format!("{:02}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
}

/// `/disk?sort=&reset=`.
#[allow(clippy::too_many_lines)]
pub fn list(rq: &Rq<'_>) -> Response {
    let q = rq.query();
    let d = rq.env().config.disks(rq.user(), q.has("reset"));
    let asps = List {
        cols: [
            "ASP",
            "Type",
            "Capacity (GB)",
            "Used (GB)",
            "% used",
            "",
            "Protection",
            "Overflow",
            "Threshold",
        ]
        .iter()
        .map(|c| Col::plain(c))
        .collect(),
        rows: d
            .asps
            .iter()
            .map(|a| {
                vec![
                    Cell::num(&a.number.to_string()),
                    Cell::text(&a.kind),
                    Cell::num(&gb(a.capacity)),
                    Cell::num(&gb(a.used)),
                    Cell::num(&tenths(a.used_pct)),
                    Cell::html(bar(a.used_pct / 10)),
                    Cell::text(&a.protection),
                    Cell::text(match a.overflow {
                        Some(true) => "Yes",
                        Some(false) => "No",
                        None => "",
                    }),
                    Cell::text(&format!("{}%", a.threshold)),
                ]
            })
            .collect(),
        empty: "No auxiliary storage pools.".into(),
        keep: Vec::new(),
    };
    let mut units = d.units.clone();
    match q.str("sort") {
        "u" => units.sort_by_key(|u| u.unit),
        "r" => units.sort_by(|a, b| a.resource.cmp(&b.resource)),
        "c" => units.sort_by_key(|u| Reverse(u.capacity)),
        "p" => units.sort_by_key(|u| Reverse(u.used_pct)),
        "s" => units.sort_by(|a, b| a.status.cmp(&b.status)),
        _ => {}
    }
    let reset = rq.ctx.url("/disk?reset=1");
    let rows = units
        .iter()
        .map(|u| {
            vec![
                Cell::act(actions(&[
                    (rq.ctx.url(&format!("/disk/{}", u.unit)), "Properties"),
                    (reset.clone(), "Reset"),
                ])),
                Cell::num(&u.unit.to_string()),
                Cell::mono(&u.resource),
                Cell::mono(&u.kind),
                Cell::mono(&u.model),
                Cell::num(&gb(u.capacity)),
                Cell::num(&tenths(u.used_pct)),
                Cell::html(bar(u.used_pct / 10)),
                Cell::text(&u.protection),
                Cell::html(status(u)),
                Cell::num(&u.asp.map(|a| a.to_string()).unwrap_or_default()),
                Cell::mono(&u.serial),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Unit", "u"),
            Col::sorted("Resource", "r"),
            Col::plain("Type"),
            Col::plain("Model"),
            Col::sorted("Capacity (GB)", "c"),
            Col::sorted("% used", "p"),
            Col::plain(""),
            Col::plain("Protection"),
            Col::sorted("Status", "s"),
            Col::plain("ASP"),
            Col::plain("Serial"),
        ],
        rows,
        empty: "No disk units.".into(),
        keep: Vec::new(),
    };
    let body = format!(
        "<h2>Auxiliary storage pools</h2>{}<h2>Disk units</h2>{}<form method=\"get\"><div class=\"buttons\"><input type=\"submit\" value=\"Refresh\"> &nbsp; <span class=\"small grey\">Elapsed interval {}. Reset clears the I/O counters used on Disk activity.</span></div></form>",
        list_table(&asps),
        list_table(&list),
        hms(d.elapsed)
    );
    page(
        rq,
        &["Disk status"],
        "Disk status",
        Some("Auxiliary storage pools and disk units (WRKDSKSTS)."),
        body,
        "disk",
        Some("/disk"),
    )
}

/// `/disk/<unit>`.
pub fn props(rq: &Rq<'_>, unit: &str) -> Response {
    let d = rq.env().config.disks(rq.user(), false);
    let Some(u) = d.units.iter().find(|u| u.unit.to_string() == unit) else {
        return rq.not_found(&format!("disk unit {unit}"));
    };
    let rows = vec![
        Prop::new("Unit", u.unit.to_string()),
        Prop::new("Resource", mono(&u.resource)),
        Prop::new("Type / model", mono(&format!("{} / {}", u.kind, u.model))),
        Prop::new("Serial number", mono(&u.serial)),
        Prop::new("Capacity", format!("{} GB", gb(u.capacity))),
        Prop::new(
            "Used",
            format!("{}% &nbsp; {}", tenths(u.used_pct), bar(u.used_pct / 10)),
        ),
        Prop::new("Protection", esc(&u.protection)),
        Prop::new("Status", status(u)),
        Prop::new("ASP", u.asp.map(|a| a.to_string()).unwrap_or_default()),
    ];
    let body = format!(
        "{}<p><a href=\"{}\">Disk status</a> &nbsp; <a href=\"{}\">Disk activity</a></p>",
        props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
        esc(&rq.ctx.url("/disk")),
        esc(&rq.ctx.url("/perf/disk"))
    );
    let h1 = format!("Disk unit {}", u.unit);
    page(rq, &["Disk status", &h1], &h1, None, body, "disk", None)
}
