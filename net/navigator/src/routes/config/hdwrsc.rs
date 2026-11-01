// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/hdwrsc?t=&s=`: the hardware resources as an indented tree
//! (`WRKHDWRSC`); `/hdwrsc/<name>`: one resource;
//! `/hdwrsc/<name>/varyon` and `/confirm` (Vary off): `VRYCFG` of the
//! configuration descriptions over the resource, confirmed.

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{
    Button, Cell, Col, FilterField, List, Prop, PropGroup, actions, buttons, filter, form,
    list_table, mono, props_table,
};

use super::{GROUP, page};
use crate::env::config::Resource;
use crate::env::{Need, Target};
use crate::routes::Rq;
use crate::routes::work::{Confirm, confirm_in, reply, strings};

const TYPES: &[&str] = &[
    "All resources",
    "Communications",
    "Storage",
    "Processor",
    "Workstation",
];
const STATUSES: &[&str] = &["All", "Operational", "Not operational", "Not connected"];

/// A resource's route.
fn base(r: &Resource) -> String {
    format!("/hdwrsc/{}", r.name.replace(' ', "%20"))
}

/// The rows `t` and `s` keep, with the parents of each.
fn shown(all: &[Resource], t: &str, s: &str) -> Vec<Resource> {
    let hit = |r: &Resource| (t.is_empty() || r.category == t) && (s.is_empty() || r.status == s);
    let mut keep = vec![false; all.len()];
    for (i, r) in all.iter().enumerate() {
        if !hit(r) {
            continue;
        }
        keep[i] = true;
        let mut depth = r.depth;
        for j in (0..i).rev() {
            if all[j].depth < depth {
                keep[j] = true;
                depth = all[j].depth;
            }
        }
    }
    all.iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(r, _)| r.clone())
        .collect()
}

/// `/hdwrsc?t=&s=`.
pub fn list(rq: &Rq<'_>) -> Response {
    let q = rq.query();
    let t = if TYPES[1..].contains(&q.str("t")) {
        q.str("t")
    } else {
        ""
    };
    let s = if STATUSES[1..].contains(&q.str("s")) {
        q.str("s")
    } else {
        ""
    };
    let rs = shown(&rq.env().config.resources(rq.user()), t, s);
    let rows = rs
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let parent = r.depth < 3 || rs.get(i + 1).is_some_and(|n| n.depth > r.depth);
            let glyph = if parent { "[-]" } else { "&nbsp;&nbsp;&nbsp;" };
            let acts = if r.depth == 0 {
                String::new()
            } else {
                let b = base(r);
                actions(&[
                    (rq.ctx.url(&b), "Properties"),
                    (rq.ctx.url(&format!("{b}/varyon")), "Vary on"),
                    (rq.ctx.url(&format!("{b}/confirm")), "Vary off"),
                ])
            };
            vec![
                Cell::act(acts),
                Cell::mono_html(format!(
                    "<span class=\"ind{}\"><span class=\"glyph\">{glyph}</span> {}</span>",
                    r.depth.min(3),
                    esc(&r.name)
                )),
                Cell::mono(&r.type_model),
                Cell::text(&r.text),
                Cell::text(&r.status),
                Cell::mono(&r.serial),
                Cell::text(&r.location),
            ]
        })
        .collect();
    let list = List {
        cols: [
            "Actions",
            "Resource name",
            "Type-model",
            "Description",
            "Status",
            "Serial",
            "Location",
        ]
        .iter()
        .map(|c| Col::plain(c))
        .collect(),
        rows,
        empty: "No hardware resources match the filter.".into(),
        keep: Vec::new(),
    };
    let f = filter(
        &[
            FilterField::Select("Show".into(), "t".into(), strings(TYPES), t.into()),
            FilterField::Select("Status".into(), "s".into(), strings(STATUSES), s.into()),
        ],
        false,
    );
    page(
        rq,
        &["Hardware resources"],
        "Hardware resources",
        Some("Hardware resources as a tree by I/O processor (WRKHDWRSC)."),
        format!("{f}{}", list_table(&list)),
        "hdwrsc",
        Some("/hdwrsc"),
    )
}

/// Route `/hdwrsc/<name>/...`.
pub fn route(rq: &Rq<'_>, name: &str, rest: &[&str]) -> Response {
    let Some(r) = rq
        .env()
        .config
        .resources(rq.user())
        .into_iter()
        .find(|r| r.name == name && r.depth > 0)
    else {
        return rq.not_found(&format!("hardware resource {name}"));
    };
    match rest {
        [] => props(rq, &r),
        ["varyon"] => vary(rq, &r, true),
        ["confirm"] => vary(rq, &r, false),
        _ => rq.not_found(&format!("page {} of resource {name}", rest.join("/"))),
    }
}

fn props(rq: &Rq<'_>, r: &Resource) -> Response {
    let cfg = if r.cfg.is_empty() {
        "None".to_string()
    } else {
        r.cfg
            .iter()
            .map(|(n, t)| mono(&format!("{n} {t}")))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let rows = vec![
        Prop::new("Resource name", mono(&r.name)),
        Prop::new("Type-model", mono(&r.type_model)),
        Prop::new("Description", esc(&r.text)),
        Prop::new("Status", esc(&r.status)),
        Prop::new("Serial number", mono(&r.serial)),
        Prop::new("Location", esc(&r.location)),
        Prop::new("Configuration descriptions", cfg),
    ];
    let body = format!(
        "{}<p><a href=\"{}\">Hardware resources</a></p>",
        props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
        esc(&rq.ctx.url("/hdwrsc"))
    );
    page(
        rq,
        &["Hardware resources", &r.name],
        &format!("Resource {}", r.name),
        None,
        body,
        "hdwrsc",
        None,
    )
}

/// The `VRYCFG` commands for the resource's descriptions, one per type.
fn commands(r: &Resource, on: bool) -> Vec<String> {
    let mut types: Vec<&str> = r.cfg.iter().map(|(_, t)| t.as_str()).collect();
    types.dedup();
    types
        .iter()
        .map(|t| {
            let objs: Vec<&str> = r
                .cfg
                .iter()
                .filter(|(_, ty)| ty == t)
                .map(|(n, _)| n.as_str())
                .collect();
            format!(
                "VRYCFG CFGOBJ({}) CFGTYPE({t}) STATUS({})",
                objs.join(" "),
                if on { "*ON" } else { "*OFF" }
            )
        })
        .collect()
}

/// Vary on, or the confirmed Vary off.
fn vary(rq: &Rq<'_>, r: &Resource, on: bool) -> Response {
    let what = if on { "Vary On" } else { "Vary Off" };
    let cmds = commands(r, on);
    if rq.post() {
        if rq.form().has("cancel") || cmds.is_empty() {
            return rq.redirect("/hdwrsc");
        }
        let t = Target::obj("QSYS", &r.name, "*HDWRSC");
        if let Err(resp) = rq.require(Need::JobCtl, what, &t, &[GROUP, "Hardware resources"]) {
            return resp;
        }
        let mut last = None;
        for c in &cmds {
            match rq.env().config.run(rq.user(), c) {
                Err(m) => return reply(rq, Err(m), "/hdwrsc"),
                ok => last = Some(ok),
            }
        }
        return match last {
            Some(r) => reply(rq, r, "/hdwrsc"),
            None => rq.redirect("/hdwrsc"),
        };
    }
    if cmds.is_empty() {
        let body = format!(
            "<p>No configuration description uses resource {}; there is nothing to vary {}.</p>{}",
            mono(&r.name),
            if on { "on" } else { "off" },
            form(&rq.ctx, "", &buttons(&[Button::new("cancel", "Cancel")]))
        );
        let title = format!("Confirm {what}");
        return page(
            rq,
            &["Hardware resources", &title],
            &title,
            None,
            body,
            "confirm",
            None,
        );
    }
    let sentence = if on {
        "Press OK to vary on the configuration descriptions over the resource listed below."
            .to_string()
    } else {
        "A line, controller or device varied off cannot be used until it is varied on again; the jobs using it lose it. Press OK to vary off the configuration descriptions listed below, or Cancel to return.".to_string()
    };
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what,
            crumb: &["Hardware resources"],
            sentence,
            cols: &[
                "Configuration description",
                "Type",
                "Resource",
                "Resource status",
            ],
            rows: r
                .cfg
                .iter()
                .map(|(n, t)| vec![mono(n), mono(t), mono(&r.name), esc(&r.status)])
                .collect(),
            options: String::new(),
            command: cmds.join("; "),
        },
    )
}
