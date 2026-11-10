// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/netsvr/shares` (file shares) and `/netsvr/prtshr` (print shares):
//! the list, `/add` (`QZLSADFS`, `QZLSADPS`), `/<name>` the share's
//! properties (`QZLSCHFS`, `QZLSCHPS`) and `/<name>/confirm` Remove
//! (`QZLSRMS`). Whether the user may share a directory or an output
//! queue is the API's to say (`*IOSYSCFG`, or owning it).

use qsweb::html::esc;
use qsweb::http::{Form, Response};
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, actions, buttons, form, input, list_table, mono,
    props_table,
};

use super::{GROUP, Parm, answer, call, ns_tabs, page, refused, yes};
use crate::env::netsvr::{Session, Share};
use crate::routes::Rq;
use crate::routes::work::{Confirm, confirm_in, pick, strings};
use netsvrcfg::{Kind, Permission, Qualified, SpoolType};

/// What the two lists differ in.
struct Area {
    print: bool,
    base: &'static str,
    title: &'static str,
    one: &'static str,
    tab: usize,
    help: &'static str,
}

fn area(print: bool) -> Area {
    if print {
        Area {
            print,
            base: "/netsvr/prtshr",
            title: "Print shares",
            one: "print share",
            tab: 2,
            help: "nsprtshr",
        }
    } else {
        Area {
            print,
            base: "/netsvr/shares",
            title: "File shares",
            one: "file share",
            tab: 1,
            help: "nsshares",
        }
    }
}

/// Route `/netsvr/shares/...` or `/netsvr/prtshr/...`.
pub fn route(rq: &Rq<'_>, print: bool, rest: &[&str]) -> Response {
    let a = area(print);
    match rest {
        [] => list(rq, &a),
        ["add"] => edit(rq, &a, None),
        [name, more @ ..] => {
            let shares = match rq.env().netsvr.shares(rq.user()) {
                Ok(s) => s,
                Err(m) => return rq.done(a.base, m),
            };
            let Some(s) = shares
                .into_iter()
                .find(|s| s.name.eq_ignore_ascii_case(name) && s.as_print().is_some() == print)
            else {
                return rq.not_found(&format!("{} {name}", a.one));
            };
            match more {
                [] => edit(rq, &a, Some(&s)),
                ["confirm"] => remove(rq, &a, &s),
                _ => rq.not_found(&format!("{} {name}", a.one)),
            }
        }
    }
}

/// Connections to share `name` now.
fn users(sessions: &[Session], name: &str) -> usize {
    sessions
        .iter()
        .flat_map(|s| s.connections.iter())
        .filter(|c| c.share.eq_ignore_ascii_case(name))
        .count()
}

fn max_users(n: i32) -> String {
    if n == -1 {
        "*NOMAX".into()
    } else {
        n.to_string()
    }
}

fn permission(p: Permission) -> &'static str {
    match p {
        Permission::ReadOnly => "Read only",
        Permission::ReadWrite => "Read/write",
    }
}

fn qualified(q: &Qualified) -> String {
    if q.is_empty() {
        "*NONE".into()
    } else {
        format!("{}/{}", q.library, q.name)
    }
}

/// `/netsvr/shares`, `/netsvr/prtshr`; Add... posts here.
fn list(rq: &Rq<'_>, a: &Area) -> Response {
    if rq.post() {
        if rq.form().has("add") {
            return rq.redirect(&format!("{}/add", a.base));
        }
        return rq.redirect(a.base);
    }
    let shares = match rq.env().netsvr.shares(rq.user()) {
        Ok(s) => s,
        Err(m) => return rq.done("/netsvr", m),
    };
    let sessions = rq.env().netsvr.sessions(rq.user());
    let rows = shares
        .iter()
        .filter(|s| s.as_print().is_some() == a.print)
        .map(|s| {
            let u = |t: &str| rq.ctx.url(&format!("{}/{}{t}", a.base, s.name));
            let mut row = vec![
                Cell::act(actions(&[(u(""), "Properties"), (u("/confirm"), "Remove")])),
                Cell::mono(&s.name),
            ];
            match &s.kind {
                Kind::File(f) => row.extend([
                    Cell::mono(&f.path),
                    Cell::text(permission(f.permission)),
                    Cell::num(&max_users(f.max_users)),
                ]),
                Kind::Print(p) => row.extend([
                    Cell::mono(&qualified(&p.outq)),
                    Cell::mono(p.spool_type.special()),
                    Cell::text(&p.driver),
                ]),
            }
            row.push(Cell::num(&users(&sessions, &s.name).to_string()));
            row.push(Cell::text(&s.text));
            row
        })
        .collect();
    let mid: [&str; 3] = if a.print {
        ["Output queue", "Spooled file type", "Printer driver"]
    } else {
        ["Path", "Permission", "Maximum users"]
    };
    let mut cols = vec![Col::plain("Actions"), Col::plain("Share")];
    cols.extend(mid.iter().map(|l| Col::plain(l)));
    cols.push(Col::plain("Current users"));
    cols.push(Col::plain("Text"));
    let list = List {
        cols,
        rows,
        empty: format!("No {}s.", a.one),
        keep: Vec::new(),
    };
    let body = format!(
        "{}{}{}",
        ns_tabs(rq, a.tab),
        list_table(&list),
        form(
            &rq.ctx,
            "",
            &buttons(&[
                Button::new("add", &format!("Add {}...", a.one)),
                Button::new("refresh", "Refresh")
            ])
        )
    );
    page(rq, &[a.title], a.title, None, body, a.help, Some("/netsvr"))
}

fn upper(f: &Form, name: &str) -> String {
    f.str(name).trim().to_ascii_uppercase()
}

fn none_empty(s: String) -> String {
    if s == "*NONE" { String::new() } else { s }
}

/// `LIB/NAME`; a name alone has library `*LIBL`, which the API refuses.
fn posted_qualified(s: &str) -> Qualified {
    match s.split_once('/') {
        Some((l, n)) => Qualified::new(n.trim(), l.trim()),
        None if s.is_empty() || s == "*NONE" => Qualified::default(),
        None => Qualified::new(s, "*LIBL"),
    }
}

/// The share the form describes, from `base` (the share, or a new one).
fn posted(f: &Form, a: &Area, base: Option<&Share>) -> Share {
    let name = base.map_or_else(|| upper(f, "share"), |s| s.name.clone());
    let mut s = base.cloned().unwrap_or_else(|| {
        if a.print {
            Share::print(&name, Qualified::default(), "")
        } else {
            Share::file(&name, "", "")
        }
    });
    s.text = f.str("text").trim().to_string();
    s.encryption_required = f.str("encrypt") == "*YES";
    s.autl = none_empty(upper(f, "autl"));
    match &mut s.kind {
        Kind::File(fs) => {
            fs.path = f.str("path").trim().to_string();
            fs.permission = if f.str("perm") == "Read/write" {
                Permission::ReadWrite
            } else {
                Permission::ReadOnly
            };
            let m = upper(f, "max");
            fs.max_users = if m == "*NOMAX" {
                -1
            } else {
                m.parse().unwrap_or(i32::MIN)
            };
            fs.text_conversion = f.str("conv") == "*YES";
            fs.text_ccsid = f.str("ccsid").trim().parse().unwrap_or(i32::MIN);
            fs.extensions = f
                .str("ext")
                .split([' ', ','])
                .filter(|e| !e.is_empty())
                .map(|e| e.trim_start_matches('.').to_ascii_uppercase())
                .collect();
        }
        Kind::Print(ps) => {
            ps.outq = posted_qualified(&upper(f, "outq"));
            ps.printer_file = posted_qualified(&upper(f, "prtf"));
            ps.driver = f.str("driver").trim().to_string();
            ps.spool_type = match f.str("type") {
                "*USERASCII" => SpoolType::UserAscii,
                "*AFP" => SpoolType::Afp,
                "*SCS" => SpoolType::Scs,
                _ => SpoolType::Auto,
            };
        }
    }
    s
}

/// The form's rows for share `s`; `add` asks for the name too.
fn rows(s: &Share, add: bool) -> Vec<Prop> {
    let yn = |name: &str, b: bool| pick(name, &strings(&["*NO", "*YES"]), yes(b));
    let mut rows = vec![if add {
        Prop::new("Share name", input("share", &s.name, 12)).required()
    } else {
        Prop::new("Share name", mono(&s.name))
    }];
    match &s.kind {
        Kind::File(f) => {
            rows.push(Prop::new("Path", input("path", &f.path, 60)).required());
            rows.push(Prop::new("Text", input("text", &s.text, 50)));
            rows.push(Prop::new(
                "Access",
                pick(
                    "perm",
                    &strings(&["Read only", "Read/write"]),
                    permission(f.permission),
                ),
            ));
            rows.push(Prop::new(
                "Maximum users",
                format!(
                    "{} <span class=\"small grey\">*NOMAX or a number</span>",
                    input("max", &max_users(f.max_users), 7)
                ),
            ));
            rows.push(Prop::new("Text conversion", yn("conv", f.text_conversion)));
            rows.push(Prop::new(
                "Text CCSID",
                format!(
                    "{} <span class=\"small grey\">0: the client's</span>",
                    input("ccsid", &f.text_ccsid.to_string(), 5)
                ),
            ));
            rows.push(Prop::new(
                "Extensions converted",
                format!(
                    "{} <span class=\"small grey\">separated by blanks</span>",
                    input("ext", &f.extensions.join(" "), 40)
                ),
            ));
        }
        Kind::Print(p) => {
            rows.push(
                Prop::new(
                    "Output queue",
                    format!(
                        "{} <span class=\"small grey\">LIBRARY/QUEUE</span>",
                        input("outq", &qualified(&p.outq).replace("*NONE", ""), 21)
                    ),
                )
                .required(),
            );
            rows.push(Prop::new("Text", input("text", &s.text, 50)));
            rows.push(Prop::new("Printer driver", input("driver", &p.driver, 50)));
            rows.push(Prop::new(
                "Spooled file type",
                pick(
                    "type",
                    &strings(&["*AUTO", "*USERASCII", "*AFP", "*SCS"]),
                    p.spool_type.special(),
                ),
            ));
            rows.push(Prop::new(
                "Printer file",
                format!(
                    "{} <span class=\"small grey\">LIBRARY/FILE or *NONE</span>",
                    input("prtf", &qualified(&p.printer_file), 21)
                ),
            ));
        }
    }
    rows.push(Prop::new(
        "Encryption required",
        yn("encrypt", s.encryption_required),
    ));
    rows.push(Prop::new(
        "Authorization list",
        format!(
            "{} <span class=\"small grey\">*NONE or a list users need *USE to</span>",
            input(
                "autl",
                if s.autl.is_empty() { "*NONE" } else { &s.autl },
                10
            )
        ),
    ));
    rows
}

fn api(a: &Area, add: bool) -> &'static str {
    match (a.print, add) {
        (false, true) => "QZLSADFS",
        (false, false) => "QZLSCHFS",
        (true, true) => "QZLSADPS",
        (true, false) => "QZLSCHPS",
    }
}

/// The Add form (`s` none) or a share's properties; `typed` what a
/// refused post asked for.
fn form_page(rq: &Rq<'_>, a: &Area, s: Option<&Share>, typed: Option<&Share>) -> Response {
    let blank = if a.print {
        Share::print("", Qualified::default(), "")
    } else {
        let mut b = Share::file("", "", "");
        if let Kind::File(f) = &mut b.kind {
            f.permission = Permission::ReadOnly;
        }
        b
    };
    let add = s.is_none();
    let shown = typed.or(s).unwrap_or(&blank);
    let bs = if add {
        vec![Button::new("ok", "OK"), Button::new("cancel", "Cancel")]
    } else {
        vec![
            Button::new("apply", "Apply"),
            Button::new("ok", "OK"),
            Button::new("cancel", "Cancel"),
        ]
    };
    let body = form(
        &rq.ctx,
        "",
        &format!(
            "{}{}",
            props_table(
                rq.ctx.product,
                &[PropGroup {
                    title: None,
                    rows: rows(shown, add)
                }],
                true
            ),
            buttons(&bs)
        ),
    );
    let title = match s {
        None => format!("Add {}", a.one),
        Some(s) => format!("Share {}", s.name),
    };
    page(rq, &[a.title, &title], &title, None, body, a.help, None)
}

/// The Add form or a share's properties, and their posts.
#[allow(clippy::many_single_char_names)]
fn edit(rq: &Rq<'_>, a: &Area, s: Option<&Share>) -> Response {
    if !rq.post() {
        return form_page(rq, a, s, None);
    }
    let f = rq.form();
    if f.has("cancel") {
        return rq.redirect(a.base);
    }
    let add = s.is_none();
    let to = posted(&f, a, s);
    let function = if add {
        format!("Add {}", a.one)
    } else {
        format!("Change {}", a.one)
    };
    let api = api(a, add);
    let ns = &rq.env().netsvr;
    let r = if add {
        ns.add_share(rq.user(), &to)
    } else {
        ns.change_share(rq.user(), &to)
    };
    match r {
        Ok(()) if f.has("apply") => rq.redirect(&format!("{}/{}", a.base, to.name)),
        Ok(()) => rq.redirect(a.base),
        Err(m) => {
            if let Some(r) = refused(rq, &m, &function, api, a.title) {
                return r;
            }
            rq.flash(m);
            form_page(rq, a, s, Some(&to))
        }
    }
}

/// Remove, confirmed.
fn remove(rq: &Rq<'_>, a: &Area, s: &Share) -> Response {
    let command = call("QZLSRMS", &[Parm::Char(&s.name)], &[]);
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect(a.base);
        }
        let r = rq.env().netsvr.remove_share(rq.user(), &s.name);
        return answer(rq, r, "Remove share", "QZLSRMS", a.title, a.base);
    }
    let what = match &s.kind {
        Kind::File(f) => mono(&f.path),
        Kind::Print(p) => mono(&qualified(&p.outq)),
    };
    let n = users(&rq.env().netsvr.sessions(rq.user()), &s.name);
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what: "Remove Share",
            crumb: &["NetServer", a.title],
            sentence: "Removing a share stops clients from reaching it; what it shares stays on the system. Press OK to remove the share listed below, or Cancel to return.".into(),
            cols: &["Share", if a.print { "Output queue" } else { "Path" }, "Current users", "Text"],
            rows: vec![vec![mono(&s.name), what, n.to_string(), esc(&s.text)]],
            options: String::new(),
            command,
        },
    )
}
