// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/tcp/rte`: the routes; `?add=1` and `?d=&m=&n=` (a route's
//! destination, mask and next hop) put the Add (`ADDTCPRTE`) and Change
//! (`CHGTCPRTE`) form under the list; `/tcp/rte/confirm?d=&m=&n=` is
//! Remove (`RMVTCPRTE`), confirmed.

use std::fmt::Write as _;

use qsweb::html::{esc, query};
use qsweb::http::{Form, Response};
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, actions, buttons, form, input, list_table, mono,
    props_table,
};

use super::{GROUP, active, arg, cfg_tabs, confirmed, need, page};
use crate::env::net::Route;
use crate::routes::Rq;
use crate::routes::work::{Confirm, confirm_in, reply};

/// Route `/tcp/rte/...`.
pub fn route(rq: &Rq<'_>, rest: &[&str]) -> Response {
    match rest {
        [] => list(rq),
        ["confirm"] => match find(rq) {
            Some(r) => remove(rq, &r),
            None => rq.not_found("route"),
        },
        _ => rq.not_found(&format!("tcp/rte/{}", rest.join("/"))),
    }
}

/// The route the query names.
fn find(rq: &Rq<'_>) -> Option<Route> {
    let q = rq.query();
    rq.env()
        .net
        .routes(rq.user())
        .into_iter()
        .find(|r| r.dest == q.str("d") && r.mask == q.str("m") && r.next_hop == q.str("n"))
}

/// `?d=&m=&n=` of a route.
fn key(r: &Route) -> String {
    query(&[("d", &r.dest), ("m", &r.mask), ("n", &r.next_hop)])
}

/// The route's identifying parameters.
fn ident(dest: &str, mask: &str, hop: &str) -> String {
    format!(
        "RTEDEST({}) SUBNETMASK({}) NEXTHOP({})",
        arg(dest),
        arg(mask),
        arg(hop)
    )
}

/// The posted Add or Change command.
fn command(f: &Form, add: bool) -> String {
    let mut c = format!(
        "{} {}",
        if add { "ADDTCPRTE" } else { "CHGTCPRTE" },
        ident(f.str("d").trim(), f.str("m").trim(), f.str("n").trim())
    );
    let ifc = f.str("ifc").trim();
    if add && !ifc.is_empty() {
        let _ = write!(c, " BINDIFC({})", arg(ifc));
    }
    let metric = f.str("metric").trim();
    if !metric.is_empty() {
        let _ = write!(c, " METRIC({metric})");
    }
    c
}

/// `/tcp/rte?sort=&add=&d=&m=&n=`; the forms post here.
pub fn list(rq: &Rq<'_>) -> Response {
    if rq.post() {
        let f = rq.form();
        let add = f.has("addrte");
        if add || f.has("chgrte") {
            let cmd = if add { "ADDTCPRTE" } else { "CHGTCPRTE" };
            if let Err(r) = need(
                rq,
                if add {
                    "Add TCP/IP route"
                } else {
                    "Change TCP/IP route"
                },
                cmd,
                "Routes",
            ) {
                return r;
            }
            return reply(
                rq,
                rq.env().net.run(rq.user(), &command(&f, add)),
                "/tcp/rte",
            );
        }
        if f.has("cancel") {
            return rq.redirect("/tcp/rte");
        }
        return rq.redirect("/tcp/rte?add=1");
    }
    let q = rq.query();
    let mut routes = rq.env().net.routes(rq.user());
    match q.str("sort") {
        "d" => routes.sort_by(|a, b| a.dest.cmp(&b.dest)),
        "n" => routes.sort_by(|a, b| a.next_hop.cmp(&b.next_hop)),
        "s" => routes.sort_by_key(|r| !r.active),
        _ => {}
    }
    let rows = routes
        .iter()
        .map(|r| {
            let k = key(r);
            vec![
                Cell::act(actions(&[
                    (rq.ctx.url(&format!("/tcp/rte{k}")), "Change"),
                    (rq.ctx.url(&format!("/tcp/rte/confirm{k}")), "Remove"),
                ])),
                Cell::mono(&r.dest),
                Cell::mono(&r.mask),
                Cell::mono(&r.next_hop),
                Cell::mono(&r.interface),
                Cell::text(&r.kind),
                Cell::num(&r.metric.to_string()),
                Cell::html(active(r.active)),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Destination", "d"),
            Col::plain("Subnet mask"),
            Col::sorted("Next hop", "n"),
            Col::plain("Preferred interface"),
            Col::plain("Type"),
            Col::plain("Metric"),
            Col::sorted("Status", "s"),
        ],
        rows,
        empty: "No routes are configured.".into(),
        keep: Vec::new(),
    };
    let mut body = format!(
        "{}{}{}",
        cfg_tabs(rq, 2),
        list_table(&list),
        form(&rq.ctx, "", &buttons(&[Button::bare("Add...")]))
    );
    let edit = find(rq);
    if q.has("add") || edit.is_some() {
        body.push_str(&edit_form(rq, edit.as_ref()));
    }
    page(
        rq,
        &["Routes"],
        "TCP/IP routes",
        None,
        body,
        "tcprte",
        Some("/tcp/rte"),
    )
}

/// The Add form, or the Change form of `r`.
fn edit_form(rq: &Rq<'_>, r: Option<&Route>) -> String {
    let rows = match r {
        None => vec![
            Prop::new("Destination", input("d", "", 40)).required(),
            Prop::new("Subnet mask", input("m", "", 16)).required(),
            Prop::new("Next hop", input("n", "", 40)).required(),
            Prop::new("Preferred interface", input("ifc", "", 40)),
            Prop::new("Metric", input("metric", "1", 3)),
        ],
        Some(r) => {
            let hidden = |n: &str, v: &str| {
                format!("<input type=\"hidden\" name=\"{n}\" value=\"{}\">", esc(v))
            };
            vec![
                Prop::new(
                    "Destination",
                    format!("{}{}", mono(&r.dest), hidden("d", &r.dest)),
                ),
                Prop::new(
                    "Subnet mask",
                    format!("{}{}", mono(&r.mask), hidden("m", &r.mask)),
                ),
                Prop::new(
                    "Next hop",
                    format!("{}{}", mono(&r.next_hop), hidden("n", &r.next_hop)),
                ),
                Prop::new("Preferred interface", mono(&r.interface)),
                Prop::new("Metric", input("metric", &r.metric.to_string(), 3)),
            ]
        }
    };
    let (title, ok) = if r.is_none() {
        ("Add route", Button::new("addrte", "Add"))
    } else {
        ("Change route", Button::new("chgrte", "Change"))
    };
    format!(
        "<h2>{title}</h2>{}",
        form(
            &rq.ctx,
            &rq.ctx.url("/tcp/rte"),
            &format!(
                "{}{}",
                props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
                buttons(&[ok, Button::new("cancel", "Cancel")])
            )
        )
    )
}

/// Remove, confirmed.
fn remove(rq: &Rq<'_>, r: &Route) -> Response {
    let command = format!("RMVTCPRTE {}", ident(&r.dest, &r.mask, &r.next_hop));
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect("/tcp/rte");
        }
        if let Err(resp) = need(rq, "Remove TCP/IP route", "RMVTCPRTE", "Routes") {
            return resp;
        }
        return confirmed(rq, &command, "/tcp/rte");
    }
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what: "Remove Route",
            crumb: &["Routes"],
            sentence: "Traffic for a removed route's destination goes by the remaining routes, or nowhere. Press OK to remove the routes listed below, or Cancel to return.".into(),
            cols: &["Destination", "Subnet mask", "Next hop", "Status"],
            rows: vec![vec![mono(&r.dest), mono(&r.mask), mono(&r.next_hop), active(r.active)]],
            options: String::new(),
            command,
        },
    )
}
