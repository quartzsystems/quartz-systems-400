// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/netsvr/disabled`: the users NetServer disabled after failed
//! sign-ons (`QMAXSIGN`); `/<user>/confirm` Enable (`QZLSCHSI`
//! `ZLSS0200`), which needs `*OBJMGT` and `*USE` to the profile, as the
//! API checks.

use std::fmt::Write as _;

use qsweb::http::Response;
use qsweb::partials::{Cell, Col, List, actions, list_table, mono};

use super::{GROUP, answer, ns_tabs, page, when};
use crate::env::netsvr::Disabled;
use crate::routes::Rq;
use crate::routes::work::{Confirm, confirm_in};

/// Route `/netsvr/disabled/...`.
pub fn route(rq: &Rq<'_>, rest: &[&str]) -> Response {
    match rest {
        [] => list(rq),
        [user, "confirm"] => {
            let found = rq
                .env()
                .netsvr
                .disabled(rq.user())
                .into_iter()
                .find(|d| d.user.eq_ignore_ascii_case(user));
            match found {
                Some(d) => enable(rq, &d),
                None => rq.not_found(&format!("disabled user {user}")),
            }
        }
        _ => rq.not_found(&format!("netsvr/disabled/{}", rest.join("/"))),
    }
}

fn list(rq: &Rq<'_>) -> Response {
    let rows = rq
        .env()
        .netsvr
        .disabled(rq.user())
        .iter()
        .map(|d| {
            let u = rq.ctx.url(&format!("/netsvr/disabled/{}/confirm", d.user));
            vec![
                Cell::act(actions(&[(u, "Enable")])),
                Cell::mono(&d.user),
                Cell::text(&when(rq, d.since)),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::plain("User"),
            Col::plain("Disabled since"),
        ],
        rows,
        empty: "No users are disabled for NetServer.".into(),
        keep: Vec::new(),
    };
    let body = format!(
        "{}<p class=\"small grey\">A user is disabled for NetServer after as many failed sign-ons as system value QMAXSIGN allows; other sign-ons still work. Starting NetServer enables every user again.</p>{}",
        ns_tabs(rq, 4),
        list_table(&list)
    );
    page(
        rq,
        &["Disabled users"],
        "Users disabled for NetServer",
        None,
        body,
        "nsdisabled",
        Some("/netsvr"),
    )
}

/// The `ZLSS0200` request for one user, as hex.
fn request(user: &str) -> String {
    let mut s = String::from("00000001");
    for b in netsvrcfg::fmt::text::fixed(user, 10) {
        let _ = write!(s, "{b:02X}");
    }
    s
}

fn enable(rq: &Rq<'_>, d: &Disabled) -> Response {
    let command = format!(
        "CALL PGM(QSYS/QZLSCHSI) PARM(X'{}' X'0000000E' 'ZLSS0200' X'0000000000000000')",
        request(&d.user)
    );
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect("/netsvr/disabled");
        }
        let r = rq.env().netsvr.enable(rq.user(), &d.user);
        return answer(
            rq,
            r,
            "Enable user for NetServer",
            "QZLSCHSI",
            "Disabled users",
            "/netsvr/disabled",
        );
    }
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what: "Enable User",
            crumb: &["NetServer", "Disabled users"],
            sentence: "The user can sign on to NetServer again. Press OK to enable the user listed below, or Cancel to return.".into(),
            cols: &["User", "Disabled since"],
            rows: vec![vec![mono(&d.user), when(rq, d.since)]],
            options: String::new(),
            command,
        },
    )
}
