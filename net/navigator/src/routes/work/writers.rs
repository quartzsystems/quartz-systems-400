// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/writers`: the printers and their writers (`WRKWTR`): Start writer
//! (`STRPRTWTR`, `STRRMTWTR`), End writer (`ENDWTR`, confirmed), Hold and
//! Release (`HLDWTR`, `RLSWTR`), Vary on and Vary off (`VRYCFG`, off
//! confirmed), Properties.

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, actions, buttons, form, input, input_cmd, list_table,
    props_table, radio,
};

use super::{Confirm, confirm_page, confirmed, mono, page, pick, run, word};
use crate::env::work::Writer;
use crate::env::{Need, Target};
use crate::routes::Rq;

/// Route `/writers/...`.
pub fn route(rq: &Rq<'_>, rest: &[&str]) -> Response {
    let [dev, more @ ..] = rest else {
        return list(rq);
    };
    let Some(w) = rq
        .env()
        .work
        .writers(rq.user())
        .into_iter()
        .find(|w| w.device == *dev)
    else {
        return rq.not_found(&format!("printer {dev}"));
    };
    match more {
        [] => props(rq, &w),
        ["start"] => start(rq, &w),
        [op @ ("hold" | "release" | "varyon" | "confirm")] => act(rq, &w, op),
        _ => rq.not_found(&format!("page {} of printer {dev}", more.join("/"))),
    }
}

fn wtr_status(w: &Writer) -> String {
    match (w.started, w.held) {
        (true, false) => word("Started", Some(true)),
        (true, true) => word("Held", Some(false)),
        _ => word("Ended", Some(false)),
    }
}

fn dev_status(w: &Writer) -> String {
    match w.varied_on {
        Some(true) => word("Varied on", Some(true)),
        Some(false) => word("Varied off", Some(false)),
        None => String::new(),
    }
}

/// `/writers?sort=`; `?start=1` adds the Start writer form. The buttons
/// post here.
#[allow(clippy::too_many_lines)]
fn list(rq: &Rq<'_>) -> Response {
    let ws = rq.env().work.writers(rq.user());
    if rq.post() {
        let f = rq.form();
        if f.has("strwtr") {
            let dev = f.str("dev").to_string();
            return match ws.iter().find(|w| w.device == dev) {
                Some(w) => start_run(rq, w, &f),
                None => rq.redirect("/writers?start=1"),
            };
        }
        return rq.redirect(if f.has("start") {
            "/writers?start=1"
        } else {
            "/writers"
        });
    }
    let mut sorted = ws.clone();
    match rq.query().str("sort") {
        "dev" => sorted.sort_by(|a, b| a.device.cmp(&b.device)),
        "st" => sorted.sort_by_key(|w| !w.started),
        "dst" => sorted.sort_by_key(|w| w.varied_on.map_or(2, |v| u8::from(!v))),
        "oq" => sorted.sort_by(|a, b| a.outq.cmp(&b.outq)),
        _ => {}
    }
    let rows = sorted
        .iter()
        .map(|w| {
            let u = |t: &str| rq.ctx.url(&format!("/writers/{}{t}", w.device));
            let mut acts = Vec::new();
            if w.started {
                acts.push((u("/confirm?op=end"), "End writer"));
                if w.varied_on.is_some() {
                    acts.push(if w.held {
                        (u("/release"), "Release")
                    } else {
                        (u("/hold"), "Hold")
                    });
                }
            } else {
                acts.push((u("/start"), "Start writer"));
            }
            match w.varied_on {
                Some(true) => acts.push((u("/confirm?op=varyoff"), "Vary off")),
                Some(false) => acts.push((u("/varyon"), "Vary on")),
                None => {}
            }
            acts.push((u(""), "Properties"));
            vec![
                Cell::act(actions(&acts)),
                Cell::mono(&w.device),
                Cell::html(wtr_status(w)),
                Cell::html(dev_status(w)),
                Cell::mono(&w.outq),
                Cell::mono(&w.file),
                Cell::mono(&w.user),
                Cell::num(&w.pages_left.map(|p| p.to_string()).unwrap_or_default()),
                Cell::mono(&w.form_type),
                Cell::text(&w.model),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Device", "dev"),
            Col::sorted("Writer status", "st"),
            Col::sorted("Device status", "dst"),
            Col::sorted("Output queue", "oq"),
            Col::plain("Current file"),
            Col::plain("User"),
            Col::plain("Pages left"),
            Col::plain("Form type"),
            Col::plain("Type / model"),
        ],
        rows,
        empty: "No printers.".into(),
        keep: Vec::new(),
    };
    let mut body = list_table(&list);
    body.push_str(&form(
        &rq.ctx,
        "",
        &buttons(&[
            Button::new("start", "Start writer..."),
            Button::new("refresh", "Refresh"),
        ]),
    ));
    if rq.query().has("start") {
        let ended: Vec<String> = ws
            .iter()
            .filter(|w| !w.started)
            .map(|w| w.device.clone())
            .collect();
        let rows = vec![
            Prop::new("Printer", pick("dev", &ended, "")).required(),
            Prop::new("Output queue", input_cmd("outq", "*DEV", 21)),
            Prop::new("Form type", input("form", "*ALL", 10)),
        ];
        let _ = write!(
            body,
            "<h2>Start writer</h2>{}",
            form(
                &rq.ctx,
                "",
                &format!(
                    "{}{}",
                    props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
                    buttons(&[
                        Button::new("strwtr", "Start"),
                        Button::new("cancel", "Cancel")
                    ])
                )
            )
        );
    }
    page(
        rq,
        &["Printers"],
        "Printers",
        Some("Printer devices and their writers (WRKWTR)."),
        body,
        "writers",
        Some("/writers"),
    )
}

/// The writer's start command from the fields: a device's `STRPRTWTR`,
/// a remote output queue's `STRRMTWTR`.
fn strwtr(w: &Writer, f: &qsweb::http::Form) -> String {
    if w.varied_on.is_none() {
        return format!("STRRMTWTR OUTQ({})", w.outq);
    }
    let outq = f
        .get("outq")
        .map_or_else(|| "*DEV".to_string(), |o| o.trim().to_ascii_uppercase());
    let form = f
        .get("form")
        .map_or_else(|| "*ALL".to_string(), |o| o.trim().to_ascii_uppercase());
    let mut c = format!("STRPRTWTR DEV({})", w.device);
    if !outq.is_empty() && outq != "*DEV" {
        let _ = write!(c, " OUTQ({outq})");
    }
    if !form.is_empty() && form != "*ALL" {
        let _ = write!(c, " FORMTYPE({form})");
    }
    c
}

fn start_run(rq: &Rq<'_>, w: &Writer, f: &qsweb::http::Form) -> Response {
    let t = Target::obj("QSYS", &w.device, "*DEVD");
    if let Err(r) = rq.require(
        Need::JobCtl,
        "Start writer",
        &t,
        &["Work management", "Printers"],
    ) {
        return r;
    }
    run(rq, &strwtr(w, f), "/writers")
}

fn row(w: &Writer) -> Vec<String> {
    vec![
        mono(&w.device),
        wtr_status(w),
        dev_status(w),
        mono(&w.outq),
        esc(&w.model),
    ]
}

const COLS: &[&str] = &[
    "Printer",
    "Writer status",
    "Device status",
    "Output queue",
    "Type / model",
];

/// `/writers/<dev>/start`: Start writer, with its output queue and form
/// type.
fn start(rq: &Rq<'_>, w: &Writer) -> Response {
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/writers");
        }
        return start_run(rq, w, &f);
    }
    let options = if w.varied_on.is_none() {
        String::new()
    } else {
        let rows = vec![
            Prop::new("Output queue", input_cmd("outq", "*DEV", 21)),
            Prop::new("Form type", input("form", "*ALL", 10)),
        ];
        props_table(rq.ctx.product, &[PropGroup { title: None, rows }], false)
    };
    confirm_page(
        rq,
        &Confirm {
            what: "Start Writer",
            crumb: &["Printers"],
            sentence: "Press OK to start writers for the printers listed below.".into(),
            cols: COLS,
            rows: vec![row(w)],
            options,
            command: strwtr(w, &qsweb::http::Form::new(Vec::new())),
        },
    )
}

/// Hold, Release, Vary on, or the confirmed End writer and Vary off
/// (`confirm?op=end|varyoff`).
#[allow(clippy::many_single_char_names)]
fn act(rq: &Rq<'_>, w: &Writer, op: &str) -> Response {
    let q = rq.query();
    let op = if op == "confirm" {
        if q.str("op") == "varyoff" {
            "varyoff"
        } else {
            "end"
        }
    } else {
        op
    };
    let d = &w.device;
    let f = rq.form();
    let (what, command, sentence) = match op {
        "hold" => ("Hold Writer", format!("HLDWTR WTR({d})"), "Press OK to hold the writers listed below.".to_string()),
        "release" => ("Release Writer", format!("RLSWTR WTR({d})"), "Press OK to release the writers listed below.".to_string()),
        "varyon" => (
            "Vary On",
            format!("VRYCFG CFGOBJ({d}) CFGTYPE(*DEV) STATUS(*ON)"),
            "Press OK to vary on the printers listed below.".to_string(),
        ),
        "varyoff" => (
            "Vary Off",
            format!("VRYCFG CFGOBJ({d}) CFGTYPE(*DEV) STATUS(*OFF)"),
            "A printer varied off cannot print until it is varied on again. Press OK to vary off the printers listed below, or Cancel to return.".to_string(),
        ),
        _ => {
            let opt = match f.str("opt") {
                "*IMMED" => "*IMMED",
                "*PAGEEND" => "*PAGEEND",
                _ => "*CNTRLD",
            };
            (
                "End Writer",
                format!("ENDWTR WTR({d}) OPTION({opt})"),
                "Ending a writer stops printing on the printer; the file printing now is finished first when the end is controlled. Press OK to end the writers listed below, or Cancel to return.".to_string(),
            )
        }
    };
    if rq.post() {
        let t = Target::obj("QSYS", d, "*DEVD");
        if let Err(r) = rq.require(Need::JobCtl, what, &t, &["Work management", "Printers"]) {
            return r;
        }
        return confirmed(rq, &command, "/writers");
    }
    let options = if op == "end" {
        let rows = vec![Prop::new(
            "How to end",
            format!(
                "{} Controlled (*CNTRLD) - after the current file<br>{} At the end of the page (*PAGEEND)<br>{} Immediate (*IMMED)",
                radio("opt", "*CNTRLD", true),
                radio("opt", "*PAGEEND", false),
                radio("opt", "*IMMED", false)
            ),
        )];
        props_table(rq.ctx.product, &[PropGroup { title: None, rows }], false)
    } else {
        String::new()
    };
    confirm_page(
        rq,
        &Confirm {
            what,
            crumb: &["Printers"],
            sentence,
            cols: COLS,
            rows: vec![row(w)],
            options,
            command,
        },
    )
}

/// `/writers/<dev>`: the printer's and writer's attributes.
fn props(rq: &Rq<'_>, w: &Writer) -> Response {
    let rows = vec![
        Prop::new("Printer", mono(&w.device)),
        Prop::new("Type / model", esc(&w.model)),
        Prop::new("Device status", dev_status(w)),
        Prop::new("Writer status", wtr_status(w)),
        Prop::new("Output queue", mono(&w.outq)),
        Prop::new("Form type", mono(&w.form_type)),
        Prop::new("Current file", mono(&w.file)),
        Prop::new("User", mono(&w.user)),
        Prop::new(
            "Pages left",
            w.pages_left.map(|p| p.to_string()).unwrap_or_default(),
        ),
    ];
    let body = format!(
        "{}<p><a href=\"{}\">All printers</a></p>",
        props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
        esc(&rq.ctx.url("/writers"))
    );
    let h1 = format!("Printer: {}", w.device);
    page(
        rq,
        &["Printers", &w.device],
        &h1,
        None,
        body,
        "writers",
        None,
    )
}
