// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/5250/verify`: the desktop's Verify Connection for a system (`?sys=`,
//! or the Start form's): name lookup, Telnet, Telnet over TLS, QAP and
//! its TLS, QAP Hello, and on request a test sign-on.

use qsweb::frame::Page;
use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::partials::{Cell, Col, List, checkbox, list_table};

use super::{GROUP, ITEM, Rq, start_policy};

/// `GET/POST /5250/verify`.
pub fn page(rq: &Rq<'_>) -> Response {
    let crumb = ["5250 session", "Verify connection"];
    if let Err(r) = start_policy(rq, &crumb) {
        return r;
    }
    let q = rq.query();
    let f = rq.form();
    let me = rq.env().system.name();
    let posted = f
        .get("sys")
        .map(|s| s.trim_end_matches(" (this system)").trim().to_string());
    let system = posted
        .clone()
        .or_else(|| q.get("sys").map(ToString::to_string))
        .unwrap_or(me.clone());
    let system = if system.is_empty() || system == "Other..." {
        me
    } else {
        system.to_ascii_uppercase()
    };
    if rq.post() && f.has("close") {
        return rq.redirect("/5250");
    }
    // From the Start form: come back here with the system in the URL.
    if rq.post() && posted.is_some() {
        return rq.redirect(&format!("/5250/verify{}", query(&[("sys", &system)])));
    }
    let test = rq.post() && f.has("verify") && f.has("ts");
    let checks = rq.env().session5250.verify(rq.user(), &system, test);
    let rows = checks
        .iter()
        .map(|c| {
            let result = match c.ok {
                Some(true) => Cell::html("<span class=\"ok\">OK</span>".into()),
                Some(false) => Cell::html("<span class=\"err\">Failed</span>".into()),
                None => Cell::text("Skipped"),
            };
            vec![Cell::text(&c.name), result, Cell::text(&c.message)]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Check"),
            Col::plain("Result"),
            Col::plain("Message"),
        ],
        rows,
        empty: String::new(),
        keep: Vec::new(),
    };
    let form = format!(
        "<form method=\"post\">{}<table class=\"props\" cellspacing=\"0\">\n<tr><th>Test sign-on</th><td>{} Sign on as {} (counts toward QMAXSIGN and is audited)</td></tr>\n</table>\n<div class=\"buttons\"><input type=\"submit\" name=\"verify\" value=\"Verify\"><input type=\"submit\" name=\"close\" value=\"Close\"></div></form>",
        rq.ctx.csrf_field(),
        checkbox("ts", test),
        esc(rq.user())
    );
    let page = Page {
        title: format!("Verify connection: {system}"),
        crumb: crumb.iter().map(ToString::to_string).collect(),
        h1: format!("Verify connection: {system}"),
        body: format!("{}{form}", list_table(&list)),
        help: "5250verify".into(),
        nav: Some((GROUP, Some(ITEM))),
        ..Page::default()
    };
    rq.page(page)
}
