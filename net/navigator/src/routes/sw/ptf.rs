// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/ptf?p=&s=&id=&sort=&page=` (`DSPPTF`), `/ptf/confirm?op=load|apply|
//! remove|order` (`LODPTF`, `INSPTF`, `APYPTF`, `RMVPTF`, `SNDPTFORD`),
//! `/ptf/level` (the groups), the cover letters (`DSPPTFCVR`) and
//! `/fixes`: the update service's last answer and its attributes
//! (`QSCHKUPD`, `QSCHGUPDA`).

use std::fmt::Write as _;

use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::msg::Kind;
use qsweb::pages::confirm_body;
use qsweb::partials::{
    Button, Cell, Col, FilterField, List, Paging, Prop, PropGroup, actions, buttons, filter, form,
    input, list_table, mono, paging, props, props_table, radio,
};

use super::{GROUP, need, page, run_all};
use crate::PRODUCT;
use crate::env::Target;
use crate::env::sw::{Fix, Ptf, PtfStatus, UpdateAttrs};
use crate::routes::Rq;
use crate::routes::common::{pick, word};

fn status(s: PtfStatus) -> String {
    match s {
        PtfStatus::Permanent => word(s.label(), Some(true)),
        PtfStatus::NotApplied => word(s.label(), Some(false)),
        _ => word(s.label(), None),
    }
}

/// The update server's host, from its URL.
fn host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split('/').next().unwrap_or(rest).to_string()
}

/// Fixes not on the system.
fn missing(fixes: &[Fix]) -> usize {
    fixes.iter().filter(|f| f.on_system.is_empty()).count()
}

/// `/ptf`.
#[allow(clippy::too_many_lines)]
pub fn list(rq: &Rq<'_>) -> Response {
    let sw = &rq.env().sw;
    let user = rq.user();
    let all = sw.ptfs(user);
    if rq.post() {
        let f = rq.form();
        if f.has("level") {
            return rq.redirect("/ptf/level");
        }
        if f.has("load") {
            let cats = sw.catalogs(user);
            let c = cats
                .iter()
                .find(|c| c.name.starts_with("QS400PTF"))
                .or(cats.first());
            return rq.redirect(&format!(
                "/ptf/confirm{}",
                query(&[("op", "load"), ("c", c.map_or("", |c| c.name.as_str()))])
            ));
        }
        let mut q: Vec<(&str, &str)> = vec![("op", "apply")];
        for p in all.iter().filter(|p| p.status == PtfStatus::NotApplied) {
            q.push(("id", &p.id));
        }
        if q.len() == 1 {
            return rq.redirect("/ptf");
        }
        return rq.redirect(&format!("/ptf/confirm{}", query(&q)));
    }
    let q = rq.query();
    if q.has("check") {
        return run_all(rq, &["QSCHKUPD".to_string()], "/ptf");
    }
    let mut products = vec!["*ALL".to_string()];
    for p in &all {
        if !products.contains(&p.product) {
            products.push(p.product.clone());
        }
    }
    let prod = products
        .iter()
        .find(|p| **p == q.str("p"))
        .cloned()
        .unwrap_or_else(|| "*ALL".into());
    let st = PtfStatus::ALL.into_iter().find(|s| s.label() == q.str("s"));
    let id = q.str("id").trim().to_ascii_uppercase();
    let mut ps: Vec<Ptf> = all
        .into_iter()
        .filter(|p| prod == "*ALL" || p.product == prod)
        .filter(|p| st.is_none_or(|s| p.status == s))
        .filter(|p| crate::routes::common::matches(&id, &p.id))
        .collect();
    let key = q.str("sort").to_string();
    match key.as_str() {
        "id" => ps.sort_by(|a, b| a.id.cmp(&b.id)),
        "p" => ps.sort_by(|a, b| a.product.cmp(&b.product)),
        "s" => ps.sort_by_key(|p| p.status.label()),
        "ipl" => ps.sort_by(|a, b| a.ipl_action.cmp(&b.ipl_action)),
        "l" => ps.sort_by_key(|p| std::cmp::Reverse(p.loaded)),
        _ => {}
    }
    let st_label = st.map(PtfStatus::label).unwrap_or_default();
    let mut filters: Vec<(String, String)> = Vec::new();
    for (k, v) in [("p", prod.as_str()), ("s", st_label), ("id", id.as_str())] {
        if !v.is_empty() && v != "*ALL" {
            filters.push((k.into(), v.into()));
        }
    }
    let mut kept = filters.clone();
    if !key.is_empty() {
        kept.push(("sort".into(), key.clone()));
    }
    let p = Paging::from_query(q.str("page"), q.str("rows"), ps.len(), kept);
    let mut statuses = vec!["*ALL".to_string()];
    statuses.extend(PtfStatus::ALL.iter().map(|s| s.label().to_string()));
    let chosen = |v: &str| {
        if v == "*ALL" {
            String::new()
        } else {
            v.to_string()
        }
    };
    let f = filter(
        &[
            FilterField::Select("Product".into(), "p".into(), products, chosen(&prod)),
            FilterField::Select("Status".into(), "s".into(), statuses, st_label.to_string()),
            FilterField::Text("PTF ID".into(), "id".into(), id.clone(), 8),
        ],
        true,
    )
    .replace(
        "<input type=\"submit\" value=\"Go\"> ",
        "<input type=\"submit\" value=\"Go\"> <input type=\"submit\" name=\"check\" value=\"Check for fixes\"> ",
    );
    let mut messages = Vec::new();
    let fixes = sw.fixes(user);
    let attrs = sw.update_attrs(user);
    if missing(&fixes) > 0 && attrs.last > 0 {
        messages.push(rq.message(
            Kind::Info,
            "CPI3599",
            &[
                &missing(&fixes).to_string(),
                &host(&attrs.url),
                &rq.when(attrs.last),
            ],
        ));
    }
    let confirm = |op: &str, id: &str, t: Option<&str>| {
        let mut q = vec![("op", op), ("id", id)];
        if let Some(t) = t {
            q.push(("t", t));
        }
        rq.ctx.url(&format!("/ptf/confirm{}", query(&q)))
    };
    let rows = ps[p.range()]
        .iter()
        .map(|x| {
            let cover = (rq.ctx.url(&format!("/ptf/{}/cover", x.id)), "Cover letter");
            let links = match x.status {
                PtfStatus::Permanent => vec![cover],
                PtfStatus::Temporary => vec![
                    (confirm("apply", &x.id, Some("*PERM")), "Apply permanently"),
                    (confirm("remove", &x.id, None), "Remove"),
                    cover,
                ],
                PtfStatus::NotApplied => vec![
                    (confirm("load", &x.id, None), "Load"),
                    (confirm("apply", &x.id, None), "Apply"),
                    cover,
                ],
                PtfStatus::Superseded => vec![(confirm("remove", &x.id, None), "Remove"), cover],
            };
            vec![
                Cell::act(actions(&links)),
                Cell::mono(&x.id),
                Cell::mono(&x.product),
                Cell::mono(&x.release),
                Cell::html(status(x.status)),
                Cell::mono(&x.ipl_action),
                Cell::text(&x.text),
                Cell::text(&if x.loaded == 0 {
                    String::new()
                } else {
                    rq.stamp(x.loaded).0
                }),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("PTF ID", "id"),
            Col::sorted("Product", "p"),
            Col::plain("Release"),
            Col::sorted("Status", "s"),
            Col::sorted("IPL action", "ipl"),
            Col::plain("Description"),
            Col::sorted("Loaded", "l"),
        ],
        rows,
        empty: "No PTFs match the filter.".into(),
        keep: filters,
    };
    let bs = form(
        &rq.ctx,
        "",
        &buttons(&[
            Button::new("load", "Load from image catalog..."),
            Button::new("apply", "Apply all loaded at next IPL"),
            Button::new("level", "Display PTF level"),
        ]),
    );
    let level = cum_level(&rq.env().system.status(user, false).ptf_level);
    let desc = format!("Program temporary fixes (DSPPTF / WRKPTF). Cumulative level {level}.");
    // The update server's note sits under the filter, part of the list.
    let note: String = messages
        .iter()
        .map(|m| qsweb::partials::message_row(&PRODUCT, m))
        .collect();
    let body = format!("{f}{note}{}{}{bs}", list_table(&list), paging(&p));
    page(
        rq,
        &["PTFs"],
        "PTFs",
        Some(&desc),
        Vec::new(),
        body,
        "ptf",
        Some("/ptf"),
    )
}

/// `/ptf/confirm?op=&id=...&c=&t=`.
#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::type_complexity
)]
pub fn confirm(rq: &Rq<'_>) -> Response {
    let q = rq.query();
    let sw = &rq.env().sw;
    let user = rq.user();
    let op = q.str("op").to_string();
    let ids: Vec<String> = q.all("id").into_iter().map(ToString::to_string).collect();
    let post = rq.post();
    let f = rq.form();
    // Ordering a fix, or only its cover letter, starts from Fixes.
    let fixes_op = op == "order" || op == "cover";
    if post && f.has("cancel") {
        return rq.redirect(if fixes_op { "/fixes" } else { "/ptf" });
    }
    // The radio buttons: posted, else the link's, else the defaults.
    let pick_of = |name: &str, opts: &[&str]| {
        let v = if post { f.str(name) } else { q.str(name) };
        opts.iter()
            .find(|o| **o == v)
            .copied()
            .unwrap_or(opts[0])
            .to_string()
    };
    let t = pick_of("t", &["*TEMP", "*PERM"]);
    let d = pick_of("d", &["*NO", "*IPL"]);
    // Choice-dependent pages: the radio buttons' names and, by their
    // values (`*TEMP|*IPL`), the command and the sentence.
    let mut by = "";
    let mut variants: Vec<(String, String, String)> = Vec::new();
    let (what, sentence, cols, rows, commands, options): (
        &str,
        String,
        Vec<&str>,
        Vec<Vec<String>>,
        Vec<String>,
        String,
    );
    if fixes_op {
        let fixes: Vec<Fix> = sw
            .fixes(user)
            .into_iter()
            .filter(|x| ids.contains(&x.id))
            .collect();
        if fixes.is_empty() {
            return rq.not_found("fix");
        }
        let cover = op == "cover";
        if cover {
            what = "Order Cover Letters";
            sentence = "Press OK to get the cover letters of the fixes listed below from the Quartz Systems update server. Only the letters arrive; nothing is loaded or applied.".into();
        } else {
            what = "Order Fixes";
            sentence = "Press OK to order the fixes listed below from the Quartz Systems update server. Each PTF arrives as a save file in library QGPL, ready to load.".into();
        }
        cols = vec!["Fix", "Type", "Product", "Description"];
        rows = fixes
            .iter()
            .map(|x| vec![mono(&x.id), esc(&x.kind), mono(&x.product), esc(&x.text)])
            .collect();
        let items: Vec<String> = fixes
            .iter()
            .map(|x| {
                if x.kind == "PTF" {
                    format!("({})", x.id)
                } else {
                    "(*CUMPKG)".into()
                }
            })
            .collect();
        commands = vec![format!(
            "SNDPTFORD PTFID({}){}",
            items.join(" "),
            if cover { " PTFPART(*CVRLTR)" } else { "" }
        )];
        options = String::new();
    } else if op == "load" && ids.is_empty() {
        let cats = sw.catalogs(user);
        let Some(c) = cats.iter().find(|c| c.name == q.str("c")).or(cats.first()) else {
            return rq.not_found("image catalog");
        };
        what = "Load PTF";
        sentence = "Press OK to load and apply every PTF on the image catalog listed below. PTFs with a delayed IPL action are applied at the next IPL.".into();
        cols = vec!["Image catalog", "Device", "Text"];
        rows = vec![vec![mono(&c.name), mono(&c.device), esc(&c.text)]];
        commands = vec![format!(
            "INSPTF LICPGM(*ALL) DEV({}) INSTYPE(*DLYALL)",
            c.device
        )];
        options = String::new();
    } else {
        let ptfs: Vec<Ptf> = sw
            .ptfs(user)
            .into_iter()
            .filter(|x| ids.contains(&x.id))
            .collect();
        if ptfs.is_empty() || !matches!(op.as_str(), "load" | "apply" | "remove") {
            return rq.not_found("PTF");
        }
        let mut products: Vec<&str> = Vec::new();
        for x in &ptfs {
            if !products.contains(&x.product.as_str()) {
                products.push(&x.product);
            }
        }
        let select = |p: &str| {
            ptfs.iter()
                .filter(|x| x.product == p)
                .map(|x| x.id.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        };
        cols = vec!["PTF ID", "Product", "Status", "IPL action", "Description"];
        rows = ptfs
            .iter()
            .map(|x| {
                vec![
                    mono(&x.id),
                    mono(&x.product),
                    status(x.status),
                    esc(&x.ipl_action),
                    esc(&x.text),
                ]
            })
            .collect();
        match op.as_str() {
            "load" => {
                what = "Load PTF";
                sentence =
                    "Press OK to load the PTFs listed below from their save files in library QGPL."
                        .into();
                commands = ptfs
                    .iter()
                    .map(|x| {
                        format!(
                            "LODPTF LICPGM({}) DEV(*SAVF) SELECT({}) SAVF(QGPL/{})",
                            x.product, x.id, x.id
                        )
                    })
                    .collect();
                options = String::new();
            }
            "apply" => {
                what = "Apply PTF";
                let of = |t: &str, d: &str| -> (Vec<String>, String) {
                    let now = d == "*IPL";
                    let mut cmds: Vec<String> = products
                        .iter()
                        .map(|p| {
                            format!(
                                "APYPTF LICPGM({p}) SELECT({}) APY({t}) DELAYED(*YES)",
                                select(p)
                            )
                        })
                        .collect();
                    // APYPTF's IPLAPY only marks the next IPL; the IPL
                    // itself is a controlled restart.
                    if now {
                        cmds.push(
                            "PWRDWNSYS OPTION(*CNTRLD) DELAY(60) RESTART(*YES) CONFIRM(*NO)".into(),
                        );
                    }
                    let sentence = if now {
                        "Press OK to apply the PTFs listed below and IPL the system: it ends its jobs and restarts in one minute, applying PTFs with a delayed IPL action."
                    } else {
                        "Press OK to apply the PTFs listed below. PTFs with a delayed IPL action are applied at the next IPL."
                    };
                    (cmds, sentence.into())
                };
                (commands, sentence) = of(&t, &d);
                by = "t,d";
                for vt in ["*TEMP", "*PERM"] {
                    for vd in ["*NO", "*IPL"] {
                        let (c, s) = of(vt, vd);
                        variants.push((format!("{vt}|{vd}"), c.join("; "), s));
                    }
                }
                options = format!(
                    "Apply type: {} Temporarily (*TEMP) {} Permanently (*PERM) &nbsp; Delayed PTFs: {} Apply at next IPL {} Schedule IPL now<br>",
                    radio("t", "*TEMP", t == "*TEMP"),
                    radio("t", "*PERM", t == "*PERM"),
                    radio("d", "*NO", d == "*NO"),
                    radio("d", "*IPL", d == "*IPL")
                );
            }
            _ => {
                what = "Remove PTF";
                sentence = "Press OK to remove the PTFs listed below. PTFs with a delayed IPL action are removed at the next IPL.".into();
                let of = |t: &str| -> Vec<String> {
                    products
                        .iter()
                        .map(|p| format!("RMVPTF LICPGM({p}) SELECT({}) RMV({t})", select(p)))
                        .collect()
                };
                commands = of(&t);
                by = "t";
                for vt in ["*TEMP", "*PERM"] {
                    variants.push((vt.to_string(), of(vt).join("; "), sentence.clone()));
                }
                options = format!(
                    "Remove type: {} Temporarily (*TEMP) {} Permanently (*PERM)<br>",
                    radio("t", "*TEMP", t == "*TEMP"),
                    radio("t", "*PERM", t == "*PERM")
                );
            }
        }
    }
    if post {
        let target = Target::obj("QSYS", ids.first().map_or("*ALL", String::as_str), "*PTF");
        if let Err(r) = need(rq, what, &target, &[GROUP, "PTFs"]) {
            return r;
        }
        let back = match (op.as_str(), ids.first()) {
            ("cover", Some(id)) => format!("/fixes/{id}/cover?ordered=1"),
            _ if fixes_op => "/fixes".into(),
            _ => "/ptf".into(),
        };
        return run_all(rq, &commands, &back);
    }
    let table = list_table(&List {
        cols: cols.iter().map(|c| Col::plain(c)).collect(),
        rows: rows
            .into_iter()
            .map(|r| r.into_iter().map(Cell::html).collect())
            .collect(),
        empty: String::new(),
        keep: Vec::new(),
    });
    // The command and sentence of each choice, for the page's script to
    // follow the radio buttons (qsnav.js, `data-by`); without script the
    // current choice's.
    let follow = |pick: usize| -> String {
        if variants.is_empty() {
            return String::new();
        }
        let map: Vec<(String, String)> = variants
            .iter()
            .map(|v| {
                (
                    v.0.clone(),
                    if pick == 1 { v.1.clone() } else { v.2.clone() },
                )
            })
            .collect();
        format!(" data-by=\"{by}\" data-v=\"{}\"", esc(&json_map(&map)))
    };
    let opts = format!(
        "<p class=\"small grey\">{options}Equivalent command: <span class=\"mono\"{}>{}</span></p>",
        follow(1),
        esc(&commands.join("; "))
    );
    let body = confirm_body(&rq.ctx, &sentence, &table, &opts).replacen(
        &format!("<p>{}</p>", esc(&sentence)),
        &format!("<p{}>{}</p>", follow(2), esc(&sentence)),
        1,
    );
    let title = format!("Confirm {what}");
    let back = if fixes_op { "Fixes" } else { "PTFs" };
    page(
        rq,
        &[back, &title],
        &title,
        None,
        Vec::new(),
        body,
        "confirm",
        None,
    )
}

/// `/ptf/level`: the cumulative level and the PTF groups.
pub fn level(rq: &Rq<'_>) -> Response {
    let groups = rq.env().sw.ptf_groups(rq.user());
    let level = cum_level(&rq.env().system.status(rq.user(), false).ptf_level);
    let head = props_table(
        &PRODUCT,
        &[PropGroup {
            title: None,
            rows: vec![
                Prop::new("Cumulative level", mono(&level)),
                Prop::new("Release", mono(&release::RELEASE.to_string())),
            ],
        }],
        true,
    );
    let list = List {
        cols: vec![
            Col::plain("PTF group"),
            Col::plain("Level"),
            Col::plain("Status"),
            Col::plain("Text"),
        ],
        rows: groups
            .iter()
            .map(|g| {
                vec![
                    Cell::mono(&g.name),
                    Cell::num(&g.level.to_string()),
                    Cell::text(&g.status),
                    Cell::text(&g.text),
                ]
            })
            .collect(),
        empty: "The system has no PTF groups.".into(),
        keep: Vec::new(),
    };
    let body = format!("{head}<h2>PTF groups</h2>{}", list_table(&list));
    page(
        rq,
        &["PTFs", "PTF level"],
        "PTF level",
        Some("The cumulative package and the PTF groups on the system (WRKPTFGRP)."),
        Vec::new(),
        body,
        "ptf",
        None,
    )
}

/// `{"key":"value",...}` with JSON string escapes.
fn json_map(pairs: &[(String, String)]) -> String {
    let q = |s: &str| {
        let mut o = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                c if (c as u32) < 0x20 => o.push(' '),
                c => o.push(c),
            }
        }
        o.push('"');
        o
    };
    let body: Vec<String> = pairs
        .iter()
        .map(|(k, v)| format!("{}:{}", q(k), q(v)))
        .collect();
    format!("{{{}}}", body.join(","))
}

/// The cumulative level, `None` when no package is installed.
fn cum_level(level: &str) -> String {
    let l = level.trim();
    if l.is_empty() {
        "None".into()
    } else {
        l.into()
    }
}

/// A cover letter in the help window.
pub fn cover(rq: &Rq<'_>, id: &str) -> Response {
    match rq
        .env()
        .sw
        .cover_letter(rq.user(), &id.to_ascii_uppercase())
    {
        Some(text) => Response::html(
            200,
            qsweb::frame::help(
                &PRODUCT,
                &format!("Cover letter {id}"),
                &format!("<pre class=\"out\">{}</pre>", esc(&text)),
            ),
        ),
        // A fix still on the update server: its letter is ordered first,
        // once.
        None if !rq.query().has("ordered")
            && rq
                .env()
                .sw
                .fixes(rq.user())
                .iter()
                .any(|x| x.id.eq_ignore_ascii_case(id)) =>
        {
            rq.redirect(&format!(
                "/ptf/confirm{}",
                query(&[("op", "cover"), ("id", &id.to_ascii_uppercase())])
            ))
        }
        None => rq.not_found(&format!("cover letter for {id}")),
    }
}

/// The automatic check's choices for `a`: label and `SCHEDULE`.
fn schedules(a: &UpdateAttrs) -> [(String, &'static str); 3] {
    let hhmm = format!(
        "{}:{}",
        a.time.get(..2).unwrap_or("04"),
        a.time.get(2..4).unwrap_or("00")
    );
    [
        (format!("Weekly (Sunday {hhmm})"), "*WEEKLY"),
        ("Daily".into(), "*DAILY"),
        ("Never".into(), "*NONE"),
    ]
}

/// The size of a fix (`1.2 MB`, `412 MB`).
fn size(bytes: u64) -> String {
    let t = (bytes * 10 + (1 << 19)) >> 20;
    if t >= 100 {
        format!("{} MB", t / 10)
    } else {
        format!("{}.{} MB", t / 10, t % 10)
    }
}

/// `/fixes`.
#[allow(clippy::too_many_lines)]
pub fn fixes(rq: &Rq<'_>) -> Response {
    let sw = &rq.env().sw;
    let user = rq.user();
    let attrs = sw.update_attrs(user);
    let choices = schedules(&attrs);
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/ptf");
        }
        let server = f.str("s").trim();
        let url = if server.contains("://") {
            server.to_string()
        } else if server == host(&attrs.url) {
            attrs.url.clone()
        } else {
            format!("https://{server}/qs400")
        };
        let proxy = match f.str("p").trim() {
            "" => "*NONE".to_string(),
            p => format!("'{}'", p.replace('\'', "''")),
        };
        let schedule = choices
            .iter()
            .find(|(l, _)| l == f.str("a"))
            .map_or("*SAME", |(_, v)| v);
        let cmd = format!(
            "QSCHGUPDA URL('{}') PROXY({proxy}) SCHEDULE({schedule})",
            url.replace('\'', "''")
        );
        let t = Target::obj("QSYS", "QSUPDATTR", "*DTAARA");
        if let Err(r) = need(rq, "Change Update Attributes", &t, &[GROUP, "Fixes"]) {
            return r;
        }
        return run_all(rq, &[cmd], if f.has("ok") { "/ptf" } else { "/fixes" });
    }
    let fixes = sw.fixes(user);
    let rows = fixes
        .iter()
        .map(|x| {
            let order = rq.ctx.url(&format!(
                "/ptf/confirm{}",
                query(&[("op", "order"), ("id", &x.id)])
            ));
            let on = if x.on_system.is_empty() {
                word("No", Some(false))
            } else {
                word(&x.on_system, Some(true))
            };
            vec![
                Cell::act(actions(&[
                    (order, "Download"),
                    (
                        rq.ctx.url(&format!("/fixes/{}/cover", x.id)),
                        "Cover letter",
                    ),
                ])),
                Cell::mono(&x.id),
                Cell::text(&x.kind),
                Cell::mono(&x.product),
                Cell::text(&rq.stamp(x.released).0),
                Cell::text(&size(x.size)),
                Cell::text(&x.text),
                Cell::html(on),
            ]
        })
        .collect();
    let list = List {
        cols: [
            "Actions",
            "Fix",
            "Type",
            "Product",
            "Released",
            "Size",
            "Description",
            "On this system",
        ]
        .iter()
        .map(|c| Col::plain(c))
        .collect(),
        rows,
        empty: "The last check found no fixes this system lacks.".into(),
        keep: Vec::new(),
    };
    let chosen = choices
        .iter()
        .find(|(_, v)| *v == attrs.schedule)
        .map_or(String::new(), |(l, _)| l.clone());
    let labels: Vec<String> = choices.iter().map(|(l, _)| l.clone()).collect();
    let mut checked = if attrs.last == 0 {
        "Never".to_string()
    } else {
        rq.when(attrs.last)
    };
    if attrs.last > 0 {
        let n = missing(&fixes);
        let _ = write!(
            checked,
            " - {n} {} available",
            if n == 1 { "fix" } else { "fixes" }
        );
    }
    let settings = props(
        &rq.ctx,
        &[PropGroup {
            title: Some("Quartz Systems update server".into()),
            rows: vec![
                Prop::new("Server", input("s", &host(&attrs.url), 30)),
                Prop::new("Check automatically", pick("a", &labels, &chosen)),
                Prop::new("Proxy", input("p", &attrs.proxy, 30)),
                Prop::new("Last check", esc(&checked)),
            ],
        }],
        &[
            Button::new("apply", "Apply"),
            Button::new("ok", "OK"),
            Button::new("cancel", "Cancel"),
        ],
    );
    let body = format!("{}{settings}", list_table(&list));
    page(
        rq,
        &["Fixes"],
        "Fixes",
        Some(
            "Fixes available from the Quartz Systems update server that are not yet on this system.",
        ),
        Vec::new(),
        body,
        "fixes",
        Some("/fixes"),
    )
}
