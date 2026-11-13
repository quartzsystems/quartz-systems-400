// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/nfs`: the export table; `/nfs/add` and `/nfs/exp/<id>` an export's
//! options (`EXPORTFS OPTIONS('-I -O ...')`, with `-F` to keep it in
//! `/etc/exports` too), `/nfs/exp/<id>/confirm` its end (`EXPORTFS
//! OPTIONS('-U')`), `/nfs/confirm` every entry of `/etc/exports`
//! (`EXPORTFS OPTIONS('-A')`).

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::{Form, Response};
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, actions, buttons, form, input, list_table, mono,
    props_table,
};

use super::{GROUP, arg, need, nfs_tabs, page, run, when, yes};
use crate::env::nfs::{Export, HostOption};
use crate::routes::Rq;
use crate::routes::common::{Confirm, confirm_in, pick, quote, strings};

/// Host option rows on the form.
const HOST_ROWS: usize = 4;

/// The options list of `EXPORTFS -O` for `e`; the defaults left out.
#[must_use]
pub fn options(e: &Export) -> String {
    let mut v: Vec<String> = Vec::new();
    let list = |s: &str| {
        s.split([' ', ','])
            .filter(|h| !h.is_empty())
            .collect::<Vec<_>>()
            .join(":")
    };
    if e.read_only {
        v.push("ro".into());
    }
    for (k, hosts) in [
        ("rw", &e.rw_hosts),
        ("access", &e.access_hosts),
        ("root", &e.root_hosts),
    ] {
        let l = list(hosts);
        if !l.is_empty() {
            v.push(format!("{k}={l}"));
        }
    }
    if !e.anon.trim().is_empty() {
        v.push(format!("anon={}", e.anon.trim()));
    }
    if e.nosuid {
        v.push("nosuid".into());
    }
    if !e.versions.is_empty() && e.versions != "3:4" {
        v.push(format!("vers={}", e.versions));
    }
    if e.public {
        v.push("public".into());
    }
    if e.pseudo_root {
        v.push("root".into());
    }
    for (k, p) in [("nfsroot", &e.nfsroot), ("exname", &e.exname)] {
        if !p.trim().is_empty() {
            v.push(format!("{k}={}", p.trim()));
        }
    }
    v.join(",")
}

/// `EXPORTFS` exporting `e` (`-F`: and putting it in `/etc/exports`).
#[must_use]
pub fn command(e: &Export, file: bool) -> String {
    let mut flags = String::from("-I");
    if file {
        flags.push_str(" -F");
    }
    let o = options(e);
    if !o.is_empty() {
        flags.push_str(" -O ");
        flags.push_str(&o);
    }
    let mut s = format!(
        "EXPORTFS OPTIONS({}) DIR({})",
        quote(&flags),
        quote(e.path.trim())
    );
    let hosts: Vec<String> = e
        .hosts
        .iter()
        .filter(|h| !h.host.trim().is_empty())
        .map(|h| {
            format!(
                "({} {} {} {})",
                quote(h.host.trim()),
                arg(if h.data.trim().is_empty() {
                    "*BINARY"
                } else {
                    &h.data
                }),
                arg(if h.path.trim().is_empty() {
                    "*ASCII"
                } else {
                    &h.path
                }),
                if h.sync { "*SYNC" } else { "*ASYNC" }
            )
        })
        .collect();
    if !hosts.is_empty() {
        let _ = write!(s, " HOSTOPT({})", hosts.join(" "));
    }
    s
}

/// `EXPORTFS` ending the export of `path` (`-I -F`: and taking it out
/// of `/etc/exports`).
#[must_use]
pub fn unexport(path: &str, file: bool) -> String {
    let flags = if file { "-U -I -F" } else { "-U" };
    format!("EXPORTFS OPTIONS({}) DIR({})", quote(flags), quote(path))
}

fn access(e: &Export) -> &'static str {
    if e.read_only {
        "Read only"
    } else {
        "Read/write"
    }
}

fn dash(s: &str) -> String {
    if s.is_empty() {
        "-".into()
    } else {
        s.to_string()
    }
}

fn anon(e: &Export) -> String {
    match e.anon_uid {
        None => "Refused".into(),
        Some(u) => format!("{u} ({})", e.anon_user),
    }
}

/// `/nfs`; the buttons post here.
pub fn list(rq: &Rq<'_>) -> Response {
    if rq.post() {
        let f = rq.form();
        if f.has("add") {
            return rq.redirect("/nfs/add");
        }
        if f.has("all") {
            return rq.redirect("/nfs/confirm");
        }
        return rq.redirect("/nfs");
    }
    let exports = match rq.env().nfs.exports(rq.user()) {
        Ok(e) => e,
        Err(m) => return rq.done("/", m),
    };
    let rows = exports
        .iter()
        .map(|e| {
            let u = |t: &str| rq.ctx.url(&format!("/nfs/exp/{}{t}", e.id));
            vec![
                Cell::act(actions(&[(u(""), "Properties"), (u("/confirm"), "Remove")])),
                Cell::mono(&e.path),
                Cell::text(access(e)),
                Cell::mono(&dash(&e.rw_hosts)),
                Cell::mono(&dash(&e.root_hosts)),
                Cell::mono(&e.versions),
                Cell::text(&anon(e)),
                Cell::mono(&e.by),
                Cell::text(&when(rq, e.since)),
            ]
        })
        .collect();
    let list = List {
        cols: [
            "Actions",
            "Path",
            "Access",
            "Read-write hosts",
            "Root hosts",
            "Versions",
            "Anonymous user",
            "Exported by",
            "Since",
        ]
        .iter()
        .map(|l| Col::plain(l))
        .collect(),
        rows,
        empty: "No directories are exported.".into(),
        keep: Vec::new(),
    };
    let body = format!(
        "{}{}{}",
        nfs_tabs(rq, 0),
        list_table(&list),
        form(
            &rq.ctx,
            "",
            &buttons(&[
                Button::new("add", "Add export..."),
                Button::new("all", "Export all from /etc/exports..."),
                Button::new("refresh", "Refresh"),
            ])
        )
    );
    page(rq, &["Exports"], "NFS exports", body, "nfs", Some("/nfs"))
}

/// `/nfs/exp/<id>/...`.
pub fn one(rq: &Rq<'_>, id: &str, rest: &[&str]) -> Response {
    let exports = match rq.env().nfs.exports(rq.user()) {
        Ok(e) => e,
        Err(m) => return rq.done("/nfs", m),
    };
    let Some(e) = exports.into_iter().find(|e| e.id.to_string() == *id) else {
        return rq.not_found(&format!("export {id}"));
    };
    match rest {
        [] => edit(rq, Some(&e)),
        ["confirm"] => remove(rq, &e),
        _ => rq.not_found(&format!("export {id}")),
    }
}

fn hint(h: &str) -> String {
    format!(" <span class=\"small grey\">{}</span>", esc(h))
}

/// The export the form describes, from `base` (the export, or none).
fn posted(f: &Form, base: Option<&Export>) -> Export {
    let mut e = base.cloned().unwrap_or_default();
    if base.is_none() {
        e.path = f.str("path").trim().to_string();
    }
    let t = |k: &str| f.str(k).trim().to_string();
    e.read_only = f.str("ro") == "*YES";
    e.rw_hosts = t("rw");
    e.access_hosts = t("access");
    e.root_hosts = t("root");
    e.anon = t("anon");
    e.nosuid = f.str("nosuid") == "*YES";
    e.versions = match f.str("vers") {
        "3" | "4" => f.str("vers").to_string(),
        _ => "3:4".into(),
    };
    e.public = f.str("public") == "*YES";
    e.pseudo_root = f.str("pseudo") == "*YES";
    e.nfsroot = t("nfsroot");
    e.exname = t("exname");
    e.hosts = (0..HOST_ROWS)
        .map(|i| HostOption {
            host: t(&format!("h{i}")),
            data: t(&format!("d{i}")),
            path: t(&format!("p{i}")),
            sync: f.str(&format!("w{i}")) != "*ASYNC",
        })
        .filter(|h| !h.host.is_empty())
        .collect();
    e
}

/// The form's rows for `e`; `add` asks for the path too.
fn rows(e: &Export, add: bool) -> Vec<Prop> {
    let yn = |name: &str, b: bool| pick(name, &strings(&["*NO", "*YES"]), yes(b));
    let mut rows = vec![if add {
        Prop::new("Directory", input("path", &e.path, 60)).required()
    } else {
        Prop::new("Directory", mono(&e.path))
    }];
    rows.push(Prop::new("Read only", yn("ro", e.read_only)));
    rows.push(Prop::new(
        "Read-write hosts",
        format!(
            "{}{}",
            input("rw", &e.rw_hosts, 50),
            hint("with read only: the hosts that may still write")
        ),
    ));
    rows.push(Prop::new(
        "Access hosts",
        format!(
            "{}{}",
            input("access", &e.access_hosts, 50),
            hint("the only hosts that may mount; blank: every host")
        ),
    ));
    rows.push(Prop::new(
        "Root hosts",
        format!(
            "{}{}",
            input("root", &e.root_hosts, 50),
            hint("hosts whose root user is QSECOFR")
        ),
    ));
    rows.push(Prop::new(
        "Anonymous UID",
        format!(
            "{}{}",
            input("anon", &e.anon, 11),
            hint("blank: QNFSANON's; -1 refuses unknown users")
        ),
    ));
    rows.push(Prop::new("No set-user-ID", yn("nosuid", e.nosuid)));
    rows.push(Prop::new(
        "NFS versions",
        pick("vers", &strings(&["3:4", "3", "4"]), &e.versions),
    ));
    rows.push(Prop::new("Public file handle", yn("public", e.public)));
    rows.push(Prop::new("NFSv4 pseudo-root", yn("pseudo", e.pseudo_root)));
    rows.push(Prop::new(
        "NFS root",
        format!("{}{}", input("nfsroot", &e.nfsroot, 50), hint("blank: /")),
    ));
    rows.push(Prop::new(
        "Export name",
        format!(
            "{}{}",
            input("exname", &e.exname, 50),
            hint("the name NFSv4 clients see; blank: the directory")
        ),
    ));
    for i in 0..HOST_ROWS {
        let h = e.hosts.get(i).cloned().unwrap_or(HostOption {
            sync: true,
            ..HostOption::default()
        });
        rows.push(Prop::new(
            &format!("Host option {}", i + 1),
            format!(
                "{} {} {} {}{}",
                input(&format!("h{i}"), &h.host, 30),
                input(&format!("d{i}"), &h.data, 8),
                input(&format!("p{i}"), &h.path, 8),
                pick(
                    &format!("w{i}"),
                    &strings(&["*SYNC", "*ASYNC"]),
                    if h.sync { "*SYNC" } else { "*ASYNC" }
                ),
                if i == 0 {
                    hint("host, data CCSID (*BINARY), path CCSID (*ASCII), writes")
                } else {
                    String::new()
                }
            ),
        ));
    }
    rows.push(Prop::new(
        "Update /etc/exports",
        format!(
            "{}{}",
            yn("file", false),
            hint("also keep the entry in /etc/exports, for later IPLs")
        ),
    ));
    rows
}

/// The Add form (`e` none) or an export's options; `typed` what a
/// refused post asked for.
fn form_page(rq: &Rq<'_>, e: Option<&Export>, typed: Option<&Export>) -> Response {
    let blank = Export {
        versions: "3:4".into(),
        ..Export::default()
    };
    let add = e.is_none();
    let shown = typed.or(e).unwrap_or(&blank);
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
    let title = match e {
        None => "Add NFS export".to_string(),
        Some(e) => format!("Export {}", e.path),
    };
    page(rq, &["Exports", &title], &title, body, "nfs", None)
}

/// The Add form or an export's options, and their posts.
pub fn edit(rq: &Rq<'_>, e: Option<&Export>) -> Response {
    if !rq.post() {
        return form_page(rq, e, None);
    }
    let f = rq.form();
    if f.has("cancel") {
        return rq.redirect("/nfs");
    }
    let function = if e.is_some() {
        "Change NFS export"
    } else {
        "Add NFS export"
    };
    if let Err(r) = need(rq, function, "EXPORTFS", "Exports") {
        return r;
    }
    let to = posted(&f, e);
    match rq
        .env()
        .nfs
        .run(rq.user(), &command(&to, f.str("file") == "*YES"))
    {
        Ok(m) => {
            let back = match (&e, f.has("apply")) {
                (Some(e), true) => format!("/nfs/exp/{}", e.id),
                _ => "/nfs".to_string(),
            };
            rq.done(&back, m)
        }
        Err(m) => {
            rq.flash(m);
            form_page(rq, e, Some(&to))
        }
    }
}

/// Remove, confirmed.
fn remove(rq: &Rq<'_>, e: &Export) -> Response {
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/nfs");
        }
        if let Err(r) = need(rq, "Remove NFS export", "EXPORTFS", "Exports") {
            return r;
        }
        return run(rq, &unexport(&e.path, f.str("file") == "*YES"), "/nfs");
    }
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what: "Remove NFS Export",
            crumb: &["NFS", "Exports"],
            sentence: "Ending an export stops NFS clients from reaching the directory through it at once; the directory stays. Press OK to end the export listed below, or Cancel to return.".into(),
            cols: &["Path", "Access", "Exported by"],
            rows: vec![vec![mono(&e.path), esc(access(e)), mono(&e.by)]],
            options: format!(
                "<p>Also remove it from /etc/exports: {}</p>",
                pick("file", &strings(&["*NO", "*YES"]), "*NO")
            ),
            command: unexport(&e.path, false),
        },
    )
}

/// Export every entry of `/etc/exports`, confirmed.
pub fn export_all(rq: &Rq<'_>) -> Response {
    const ALL: &str = "EXPORTFS OPTIONS('-A')";
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect("/nfs");
        }
        if let Err(r) = need(rq, "Export all", "EXPORTFS", "Exports") {
            return r;
        }
        return run(rq, ALL, "/nfs");
    }
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what: "Export All",
            crumb: &["NFS", "Exports"],
            sentence: "Every entry of /etc/exports is exported with its options, replacing the options of a directory exported already. Press OK to export them, or Cancel to return.".into(),
            cols: &["File"],
            rows: vec![vec![mono("/etc/exports")]],
            options: String::new(),
            command: ALL.into(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_export_as_the_command() {
        let e = Export {
            path: "/home/shared".into(),
            read_only: true,
            rw_hosts: "admin, pc2".into(),
            anon: "-1".into(),
            versions: "4".into(),
            exname: "/shared".into(),
            hosts: vec![HostOption {
                host: "@10.0.0.0/8".into(),
                data: "819".into(),
                path: String::new(),
                sync: false,
            }],
            ..Export::default()
        };
        assert_eq!(options(&e), "ro,rw=admin:pc2,anon=-1,vers=4,exname=/shared");
        assert_eq!(
            command(&e, true),
            "EXPORTFS OPTIONS('-I -F -O ro,rw=admin:pc2,anon=-1,vers=4,exname=/shared') DIR('/home/shared') HOSTOPT(('@10.0.0.0/8' 819 *ASCII *ASYNC))"
        );
        let d = Export {
            path: "/x".into(),
            versions: "3:4".into(),
            ..Export::default()
        };
        assert_eq!(command(&d, false), "EXPORTFS OPTIONS('-I') DIR('/x')");
        assert_eq!(
            unexport("/x", true),
            "EXPORTFS OPTIONS('-U -I -F') DIR('/x')"
        );
    }
}
