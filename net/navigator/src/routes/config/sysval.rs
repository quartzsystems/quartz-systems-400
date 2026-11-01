// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/sysval?cat=`: the system values of a category (`WRKSYSVAL`);
//! `/sysval/<name>`: Change (`CHGSYSVAL`), checked against the values
//! the system value accepts before it runs. The command writes the `SV`
//! audit entry.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, actions, input_cmd, list_table, mono, props,
};

use super::page;
use crate::env::config::{CATEGORIES, SysVal};
use crate::env::{Need, Target};
use crate::routes::Rq;
use crate::routes::work::{page_in, quote, reply};

/// The category of `key`, Security when unknown.
fn category(key: &str) -> (&'static str, &'static str) {
    CATEGORIES
        .iter()
        .copied()
        .find(|(k, _)| *k == key)
        .unwrap_or(CATEGORIES[0])
}

/// Where a category's list is: Security's under the System group is
/// `/sysval`.
fn list_route(key: &str) -> String {
    if key == "security" {
        "/sysval".into()
    } else {
        format!("/sysval?cat={key}")
    }
}

/// `/sysval?cat=&sort=`; `cat=security` is the Security group's
/// Security values.
pub fn list(rq: &Rq<'_>) -> Response {
    let q = rq.query();
    let security_page = q.str("cat") == "security";
    let (key, title) = category(q.str("cat"));
    let mut vals: Vec<SysVal> = rq
        .env()
        .config
        .sysvals(rq.user())
        .into_iter()
        .filter(|v| v.category == key)
        .collect();
    if q.str("sort") == "n" {
        vals.sort_by(|a, b| a.name.cmp(&b.name));
    }
    let mut cats = String::from("<div class=\"small\"><b>Categories</b></div>");
    for (k, t) in CATEGORIES {
        if *k == key {
            let _ = write!(cats, "<div class=\"navitem small\"><b>{}</b></div>", esc(t));
        } else {
            let _ = write!(
                cats,
                "<div class=\"navitem small\"><a href=\"{}\">{}</a></div>",
                esc(&rq.ctx.url(&list_route(k))),
                esc(t)
            );
        }
    }
    let details = rq.ctx.product.help("sysval");
    let rows = vals
        .iter()
        .map(|v| {
            let text = if v.ipl {
                format!("{} <span class=\"small grey\">(IPL)</span>", esc(&v.text))
            } else {
                esc(&v.text)
            };
            vec![
                Cell::act(actions(&[
                    (rq.ctx.url(&format!("/sysval/{}", v.name)), "Change"),
                    (details.clone(), "Details"),
                ])),
                Cell::mono(&v.name),
                Cell::mono(&v.value),
                Cell::html(text),
            ]
        })
        .collect();
    let keep = if key == "security" {
        Vec::new()
    } else {
        vec![("cat".to_string(), key.to_string())]
    };
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Name", "n"),
            Col::plain("Current value"),
            Col::plain("Description"),
        ],
        rows,
        empty: "No system values in this category.".into(),
        keep,
    };
    let body = format!(
        "<table width=\"100%\" cellspacing=\"0\"><tr><td width=\"150\" valign=\"top\" style=\"background:#f0f0f0; border-right:1px solid #c0c0c0; padding:4px\">{cats}</td><td valign=\"top\" style=\"padding-left:8px\"><h2>{}</h2>{}<p class=\"small grey\">{} values. Changes to values marked with an IPL action take effect at the next IPL.</p></td></tr></table>",
        esc(title),
        list_table(&list),
        vals.len()
    );
    if security_page {
        page_in(
            rq,
            "Security",
            &["Security values"],
            "Security values",
            Some("The security-related system values (same table as System values > Security)."),
            body,
            "sysval",
            Some("/sysval?cat=security"),
        )
    } else {
        page(
            rq,
            &["System values"],
            "System values",
            Some("System values by category (WRKSYSVAL)."),
            body,
            "sysval",
            Some("/sysval"),
        )
    }
}

/// `/sysval/<name>`: Change; OK runs `CHGSYSVAL`.
pub fn change(rq: &Rq<'_>, name: &str) -> Response {
    let name = name.to_ascii_uppercase();
    let Some(v) = rq
        .env()
        .config
        .sysvals(rq.user())
        .into_iter()
        .find(|v| v.name == name)
    else {
        return rq.not_found(&format!("system value {name}"));
    };
    let back = list_route(&v.category);
    if !rq.post() {
        return form(rq, &v, &v.value, None);
    }
    let f = rq.form();
    if f.has("cancel") {
        return rq.redirect(&back);
    }
    let value = f.str("v").trim().to_string();
    if !v.allowed.accepts(&value) {
        let m = rq.message(Kind::Error, "CPD9B8D", &[&v.name, &value]);
        return form(rq, &v, &value, Some(m));
    }
    if v.category == "security" {
        let t = Target {
            shown: v.name.clone(),
            name: "CHGSYSVAL".into(),
            lib: "QSYS".into(),
            typ: "*CMD".into(),
        };
        if let Err(r) = rq.require(
            Need::SecAdm,
            "Change system value",
            &t,
            &["System", "System values"],
        ) {
            return r;
        }
    }
    match rq.env().config.run(rq.user(), &command(&v.name, &value)) {
        Err(m) => form(rq, &v, &value, Some(m)),
        ok => reply(rq, ok, &back),
    }
}

/// The command a change runs.
fn command(name: &str, value: &str) -> String {
    format!("CHGSYSVAL SYSVAL({name}) VALUE({})", quote(value))
}

/// The Change page with `value` in the field and `m` above it.
fn form(rq: &Rq<'_>, v: &SysVal, value: &str, m: Option<Message>) -> Response {
    let (_, cat) = category(&v.category);
    let changed = v.changed.as_ref().map_or_else(
        || "Not changed since installation".to_string(),
        |(at, by)| format!("{} by {}", rq.stamp(*at).0, esc(by)),
    );
    let size = u32::try_from(value.len().max(v.value.len()) + 2)
        .unwrap_or(60)
        .clamp(8, 60);
    let hint = v.allowed.hint();
    let field = if hint.is_empty() {
        input_cmd("v", value, size)
    } else {
        format!(
            "{} &nbsp; <span class=\"small grey\">{}</span>",
            input_cmd("v", value, size),
            esc(&hint)
        )
    };
    let rows = vec![
        Prop::new("System value", mono(&v.name)),
        Prop::new("Description", esc(&v.text)),
        Prop::new("Current value", mono(&v.value)),
        Prop::new("Shipped value", mono(&v.shipped)),
        Prop::new("New value", field).help("sysval"),
        Prop::new(
            "Takes effect",
            if v.ipl {
                "At the next IPL"
            } else {
                "Immediately"
            }
            .into(),
        ),
        Prop::new("Last changed", changed),
    ];
    let body = format!(
        "{}<p class=\"small grey\">Equivalent command: <span class=\"mono\">{}</span>. The change is audited (entry type SV).</p>",
        props(
            &rq.ctx,
            &[PropGroup { title: None, rows }],
            &[Button::new("ok", "OK"), Button::new("cancel", "Cancel")]
        ),
        esc(&command(&v.name, value))
    );
    let h1 = format!("Change system value: {}", v.name);
    rq.page(Page {
        title: h1.clone(),
        crumb: vec![
            "System".into(),
            "System values".into(),
            cat.into(),
            v.name.clone(),
        ],
        h1,
        messages: m.into_iter().collect(),
        body,
        help: "sysval".into(),
        ..Page::default()
    })
}
