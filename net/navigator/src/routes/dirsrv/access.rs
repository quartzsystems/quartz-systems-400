// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/dirsrv/inst/<name>/access?dn=`: an entry's access control list and
//! owners, its own or the ones it inherits (and from where), and the
//! change of its own (`aclEntry`, `aclPropagate`, `entryOwner`,
//! `ownerPropagate`), one value a line.

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{Button, Prop, PropGroup, buttons, form, input, link, mono, props_table};

use super::{changed, enc, fail, inst_route, inst_tabs, inst_url, need, page};
use crate::env::dirsrv::{Access, InstanceInfo, SuffixKind, message};
use crate::routes::Rq;
use crate::routes::common::{pick, strings};

fn yn(name: &str, b: bool) -> String {
    pick(
        name,
        &strings(&["*NO", "*YES"]),
        if b { "*YES" } else { "*NO" },
    )
}

fn area(name: &str, v: &[String]) -> String {
    format!(
        "<textarea name=\"{name}\" rows=\"4\" cols=\"90\" class=\"conf\">{}</textarea>",
        esc(&v.join("\n"))
    )
}

fn lines(s: &str) -> Vec<String> {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn source(rq: &Rq<'_>, i: &InstanceInfo, s: &str, own: bool) -> String {
    if own {
        esc("This entry")
    } else if s == "default" {
        esc("The default (no entry above sets one)")
    } else {
        link(&format!("{}?dn={}", inst_url(rq, i, "access"), enc(s)), s)
    }
}

/// `/dirsrv/inst/<name>/access` and its posts.
#[allow(clippy::many_single_char_names)]
pub fn route(rq: &Rq<'_>, i: &InstanceInfo) -> Response {
    let d = &rq.env().dirsrv;
    let dn = rq.query().str("dn").trim().to_string();
    let here = format!("{}?dn={}", inst_route(i, "access"), enc(&dn));
    let function = "Change directory entry access";
    if rq.post() && !dn.is_empty() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect(&inst_route(i, "access"));
        }
        if let Err(r) = need(rq, Some(i), function, &["Instances", &i.name]) {
            return r;
        }
        let a = Access {
            dn: dn.clone(),
            explicit_acl: f.str("ownacl") == "*YES",
            acl: lines(f.str("acl")),
            acl_propagate: f.str("aclprop") == "*YES",
            explicit_owner: f.str("ownowner") == "*YES",
            owners: lines(f.str("owners")),
            owner_propagate: f.str("ownerprop") == "*YES",
            ..Access::default()
        };
        return match d.set_access(rq.user(), &i.name, &a) {
            Ok(()) => changed(rq, i, &here),
            Err(e) => fail(rq, &e, function, &["Instances", &i.name], &here),
        };
    }
    let find = format!(
        "<form method=\"get\"><p>Entry: {} <input type=\"submit\" value=\"Show\"></p></form>",
        input("dn", &dn, 60)
    );
    let mut body = format!("{}{find}", inst_tabs(rq, i, 4));
    if dn.is_empty()
        && let Ok(all) = d.suffixes(rq.user(), &i.name)
    {
        let roots: Vec<String> = all
            .iter()
            .filter(|s| matches!(s.kind, SuffixKind::User | SuffixKind::Local))
            .map(|s| {
                link(
                    &format!("{}?dn={}", inst_url(rq, i, "access"), enc(&s.dn)),
                    &s.dn,
                )
            })
            .collect();
        let _ = write!(body, "<p>Suffixes: {}</p>", roots.join(" | "));
    }
    if !dn.is_empty() {
        match d.access(rq.user(), &i.name, &dn) {
            Ok(a) => {
                let groups = [
                    PropGroup {
                        title: Some("Access control list".into()),
                        rows: vec![
                            Prop::new("Entry", mono(&a.dn)),
                            Prop::new(
                                "In effect from",
                                source(rq, i, &a.acl_source, a.explicit_acl),
                            ),
                            Prop::new("Own ACL", yn("ownacl", a.explicit_acl)),
                            Prop::new("ACL values", area("acl", &a.acl)),
                            Prop::new("Applies below", yn("aclprop", a.acl_propagate)),
                        ],
                    },
                    PropGroup {
                        title: Some("Owners".into()),
                        rows: vec![
                            Prop::new(
                                "In effect from",
                                source(rq, i, &a.owner_source, a.explicit_owner),
                            ),
                            Prop::new("Own owners", yn("ownowner", a.explicit_owner)),
                            Prop::new("Owner values", area("owners", &a.owners)),
                            Prop::new("Applies below", yn("ownerprop", a.owner_propagate)),
                        ],
                    },
                ];
                body.push_str(&form(
                    &rq.ctx,
                    "",
                    &format!(
                        "{}{}",
                        props_table(rq.ctx.product, &groups, true),
                        buttons(&[Button::new("ok", "OK"), Button::new("cancel", "Cancel")])
                    ),
                ));
            }
            Err(e) if e.kind == crate::env::dirsrv::ErrorKind::NotAuthorized => {
                return fail(rq, &e, "Directory entry access", &[], "/dirsrv");
            }
            Err(e) => {
                let m = message(&e);
                body.push_str(&qsweb::partials::message_row(rq.ctx.product, &m));
            }
        }
    }
    let title = format!("Access control of {}", i.name);
    page(
        rq,
        &["Instances", &i.name, "Access"],
        &title,
        body,
        "dsaccess",
        false,
    )
}
