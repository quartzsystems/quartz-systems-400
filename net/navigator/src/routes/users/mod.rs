// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Users and groups: `/users`, `/user/<name>/...`,
//! `/users/create/step/<n>`, `/groups`, `/autl[/<name>]`. Lists read
//! what `WRKUSRPRF`, `DSPUSRPRF` and `WRKAUTL` show; every change is the
//! security command run as the user
//! ([`crate::env::users::Users::run`]), reached through a form or a
//! confirmation and a POST.

mod autl;
mod create;
mod groups;
mod list;
mod props;

use qsweb::http::Response;

use super::Rq;
use super::common::{Confirm, answered, confirm_in, group_page};
use crate::env::users::Profile;
use crate::env::{Need, Target};

/// The tree's group.
const GROUP: &str = "Users and groups";

/// Route `segs` (the path's segments, the area's first).
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    match segs {
        ["users"] => list::list(rq),
        ["users", "create", "step", n] => create::step(rq, n),
        ["user", name, rest @ ..] => {
            let name = name.to_ascii_uppercase();
            match profile(rq, &name) {
                Some(p) => props::route(rq, &p, rest),
                None => rq.not_found(&format!("user profile {name}")),
            }
        }
        ["groups"] => groups::list(rq),
        ["groups", "create"] => groups::create(rq),
        ["autl", rest @ ..] => autl::route(rq, rest),
        _ => rq.not_found(&segs.join("/")),
    }
}

/// Profile `name`.
fn profile(rq: &Rq<'_>, name: &str) -> Option<Profile> {
    rq.env()
        .users
        .profiles(rq.user())
        .into_iter()
        .find(|p| p.name == name)
}

/// A page of the group; `item` is the tree's current item for a list
/// page, `None` for pages below one.
fn page(
    rq: &Rq<'_>,
    crumb: &[&str],
    h1: &str,
    desc: Option<&str>,
    body: String,
    help: &str,
    item: Option<&'static str>,
) -> Response {
    group_page(rq, GROUP, crumb, h1, desc, body, help, item)
}

/// The confirmation page.
fn confirm_page(rq: &Rq<'_>, c: &Confirm<'_>) -> Response {
    confirm_in(rq, GROUP, c)
}

/// Run `command` as the user, then `303` to `back` with its message.
fn run(rq: &Rq<'_>, command: &str, back: &str) -> Response {
    answered(rq, rq.env().users.run(rq.user(), command), back)
}

/// The profile commands need `*SECADM`: the Not authorized page
/// otherwise.
fn secadm(rq: &Rq<'_>, function: &str, name: &str, crumb: &[&str]) -> Result<(), Response> {
    rq.require(
        Need::SecAdm,
        function,
        &Target::obj("QSYS", name, "*USRPRF"),
        crumb,
    )
}
