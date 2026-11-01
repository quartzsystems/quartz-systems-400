// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/tcp/ifc`: the interfaces; `?add=1` and `?edit=<address>` put the
//! Add (`ADDTCPIFC`) and Change (`CHGTCPIFC`) form under the list.
//! `/tcp/ifc/start?a=` (`STRTCPIFC`) and `/tcp/ifc/confirm?a=&op=end|
//! remove` (`ENDTCPIFC`, `RMVTCPIFC`) are confirmed.

use std::fmt::Write as _;

use qsweb::html::{esc, query};
use qsweb::http::{Form, Response};
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, actions, buttons, form, input, list_table, mono,
    props_table,
};

use super::{GROUP, active, arg, cfg_tabs, confirmed, need, page, yes};
use crate::env::net::Interface;
use crate::routes::Rq;
use crate::routes::work::{Confirm, confirm_in, pick, quote, reply, strings};

/// Route `/tcp/ifc/...`.
pub fn route(rq: &Rq<'_>, rest: &[&str]) -> Response {
    match rest {
        [] => list(rq),
        [op @ ("start" | "confirm")] => {
            let a = rq.query().str("a").to_string();
            match rq
                .env()
                .net
                .interfaces(rq.user())
                .into_iter()
                .find(|i| i.address == a)
            {
                Some(i) => act(rq, &i, op),
                None => rq.not_found(&format!("interface {a}")),
            }
        }
        _ => rq.not_found(&format!("tcp/ifc/{}", rest.join("/"))),
    }
}

fn link(rq: &Rq<'_>, path: &str, pairs: &[(&str, &str)]) -> String {
    rq.ctx.url(&format!("{path}{}", query(pairs)))
}

/// The posted Add or Change command.
fn command(f: &Form, add: bool) -> String {
    let a = f.str("a").trim();
    let mut c = if add {
        format!(
            "ADDTCPIFC INTNETADR({}) LIND({})",
            quote(a),
            f.str("line").trim().to_ascii_uppercase()
        )
    } else {
        format!("CHGTCPIFC INTNETADR({})", quote(a))
    };
    let mask = f.str("mask").trim();
    if !mask.is_empty() {
        let _ = write!(c, " SUBNETMASK({})", arg(mask));
    }
    let mtu = f.str("mtu").trim();
    if !mtu.is_empty() {
        let _ = write!(c, " MTU({mtu})");
    }
    let _ = write!(
        c,
        " AUTOSTART({})",
        if f.str("auto") == "*NO" {
            "*NO"
        } else {
            "*YES"
        }
    );
    let alias = f.str("alias").trim();
    if !alias.is_empty() {
        let _ = write!(c, " ALIASNAME({alias})");
    } else if !add {
        c.push_str(" ALIASNAME(*NONE)");
    }
    c
}

/// `/tcp/ifc?sort=&add=&edit=`; the forms post here.
#[allow(clippy::too_many_lines)]
pub fn list(rq: &Rq<'_>) -> Response {
    if rq.post() {
        let f = rq.form();
        let add = f.has("addifc");
        if add || f.has("chgifc") {
            if let Err(r) = need(
                rq,
                if add {
                    "Add TCP/IP interface"
                } else {
                    "Change TCP/IP interface"
                },
                if add { "ADDTCPIFC" } else { "CHGTCPIFC" },
                "Interfaces",
            ) {
                return r;
            }
            return reply(
                rq,
                rq.env().net.run(rq.user(), &command(&f, add)),
                "/tcp/ifc",
            );
        }
        return rq.redirect(if f.has("add") {
            "/tcp/ifc?add=1"
        } else {
            "/tcp/ifc"
        });
    }
    let q = rq.query();
    let mut ifcs = rq.env().net.interfaces(rq.user());
    match q.str("sort") {
        "a" => ifcs.sort_by(|x, y| x.address.cmp(&y.address)),
        "l" => ifcs.sort_by(|x, y| x.line.cmp(&y.line)),
        "s" => ifcs.sort_by_key(|i| !i.active),
        _ => {}
    }
    let rows = ifcs
        .iter()
        .map(|i| {
            let a = i.address.as_str();
            let mut acts = vec![if i.active {
                (
                    link(rq, "/tcp/ifc/confirm", &[("a", a), ("op", "end")]),
                    "End",
                )
            } else {
                (link(rq, "/tcp/ifc/start", &[("a", a)]), "Start")
            }];
            acts.push((link(rq, "/tcp/ifc", &[("edit", a)]), "Change"));
            if !i.generated {
                acts.push((
                    link(rq, "/tcp/ifc/confirm", &[("a", a), ("op", "remove")]),
                    "Remove",
                ));
            }
            vec![
                Cell::act(actions(&acts)),
                Cell::mono(a),
                Cell::mono(&i.mask),
                Cell::mono(&i.line),
                Cell::mono(&i.line_type),
                Cell::html(active(i.active)),
                Cell::mono(yes(i.autostart)),
                Cell::num(&i.mtu.to_string()),
                Cell::text(&i.alias),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Internet address", "a"),
            Col::plain("Subnet mask"),
            Col::sorted("Line", "l"),
            Col::plain("Line type"),
            Col::sorted("Status", "s"),
            Col::plain("Autostart"),
            Col::plain("MTU"),
            Col::plain("Alias"),
        ],
        rows,
        empty: "No interfaces are configured.".into(),
        keep: Vec::new(),
    };
    let mut body = format!(
        "{}{}{}",
        cfg_tabs(rq, 1),
        list_table(&list),
        form(
            &rq.ctx,
            "",
            &buttons(&[
                Button::new("add", "Add..."),
                Button::new("refresh", "Refresh")
            ])
        )
    );
    let edit = ifcs.iter().find(|i| i.address == q.str("edit"));
    if q.has("add") || edit.is_some() {
        body.push_str(&edit_form(rq, edit));
    }
    page(
        rq,
        &["Interfaces"],
        "TCP/IP interfaces",
        None,
        body,
        "tcpifc",
        Some("/tcp/ifc"),
    )
}

/// The Add form, or the Change form of `i`.
fn edit_form(rq: &Rq<'_>, i: Option<&Interface>) -> String {
    let blank = Interface {
        mtu: 1500,
        autostart: true,
        ..Interface::default()
    };
    let add = i.is_none();
    let i = i.unwrap_or(&blank);
    let mut rows = Vec::new();
    if add {
        rows.push(Prop::new("Internet address", input("a", "", 40)).required());
        rows.push(Prop::new("Line description", input("line", "", 10)).required());
    } else {
        rows.push(Prop::new(
            "Internet address",
            format!(
                "{}<input type=\"hidden\" name=\"a\" value=\"{}\">",
                mono(&i.address),
                esc(&i.address)
            ),
        ));
        rows.push(Prop::new("Line description", mono(&i.line)));
    }
    rows.push(Prop::new("Subnet mask", input("mask", &i.mask, 16)));
    rows.push(Prop::new("MTU", input("mtu", &i.mtu.to_string(), 6)));
    rows.push(Prop::new(
        "Autostart",
        pick("auto", &strings(&["*YES", "*NO"]), yes(i.autostart)),
    ));
    rows.push(Prop::new("Alias", input("alias", &i.alias, 25)));
    let (title, ok) = if add {
        ("Add interface", Button::new("addifc", "Add"))
    } else {
        ("Change interface", Button::new("chgifc", "Change"))
    };
    format!(
        "<h2>{title}</h2>{}",
        form(
            &rq.ctx,
            &rq.ctx.url("/tcp/ifc"),
            &format!(
                "{}{}",
                props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
                buttons(&[ok, Button::new("cancel", "Cancel")])
            )
        )
    )
}

/// Start, or the confirmed End and Remove.
fn act(rq: &Rq<'_>, i: &Interface, op: &str) -> Response {
    let op = if op == "start" {
        "start"
    } else if rq.query().str("op") == "remove" {
        "remove"
    } else {
        "end"
    };
    let a = quote(&i.address);
    let (what, cmd, sentence) = match op {
        "start" => ("Start Interface", "STRTCPIFC", "Press OK to start the interfaces listed below.".to_string()),
        "remove" => (
            "Remove Interface",
            "RMVTCPIFC",
            "Removing an interface deletes its configuration; connections over it end. Press OK to remove the interfaces listed below, or Cancel to return.".to_string(),
        ),
        _ => (
            "End Interface",
            "ENDTCPIFC",
            "Ending an interface ends every connection over it. Press OK to end the interfaces listed below, or Cancel to return.".to_string(),
        ),
    };
    let command = format!("{cmd} INTNETADR({a})");
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect("/tcp/ifc");
        }
        if let Err(r) = need(rq, what, cmd, "Interfaces") {
            return r;
        }
        return confirmed(rq, &command, "/tcp/ifc");
    }
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what,
            crumb: &["Interfaces"],
            sentence,
            cols: &["Internet address", "Line", "Status", "Alias"],
            rows: vec![vec![
                mono(&i.address),
                mono(&i.line),
                active(i.active),
                esc(&i.alias),
            ]],
            options: String::new(),
            command,
        },
    )
}
