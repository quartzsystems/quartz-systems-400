// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/nfs/mounts`: the mounted file systems; `/nfs/mounts/add` mounts an
//! NFS export (`MOUNT TYPE(*NFS)`), `/nfs/mounts/<id>/confirm` unmounts
//! an NFS mount or a UDFS (`UNMOUNT TYPE(*NFS)` or `TYPE(*UDFS)`).

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, actions, buttons, form, input, list_table, mono,
    props_table,
};

use super::{GROUP, need, nfs_tabs, page, run};
use crate::env::nfs::Mount;
use crate::routes::Rq;
use crate::routes::common::{Confirm, confirm_in, quote, word};

/// What the Mount form asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MountForm {
    /// The server.
    pub host: String,
    /// The exported directory there.
    pub path: String,
    /// The directory to mount over.
    pub over: String,
    /// The options; blank for the defaults.
    pub options: String,
}

/// `MOUNT TYPE(*NFS)` for `m`.
#[must_use]
pub fn command(m: &MountForm) -> String {
    let mut s = format!(
        "MOUNT TYPE(*NFS) MFS({}) MNTOVRDIR({})",
        quote(&format!("{}:{}", m.host.trim(), m.path.trim())),
        quote(m.over.trim())
    );
    if !m.options.trim().is_empty() {
        let _ = write!(s, " OPTIONS({})", quote(m.options.trim()));
    }
    s
}

/// `UNMOUNT` of `m`.
#[must_use]
pub fn unmount(m: &Mount) -> String {
    let kind = if m.kind == "udfs" { "*UDFS" } else { "*NFS" };
    format!("UNMOUNT TYPE({kind}) MNTOVRDIR({})", quote(&m.at))
}

/// Route `/nfs/mounts/...`.
pub fn route(rq: &Rq<'_>, rest: &[&str]) -> Response {
    match rest {
        [] => list(rq),
        ["add"] => add(rq),
        [id, "confirm"] => {
            let found = rq
                .env()
                .nfs
                .mounts(rq.user())
                .ok()
                .and_then(|l| l.into_iter().find(|m| m.id.to_string() == *id));
            match found {
                Some(m) if m.unmountable() => remove(rq, &m),
                _ => rq.not_found(&format!("mount {id}")),
            }
        }
        _ => rq.not_found(&format!("page {} of mounts", rest.join("/"))),
    }
}

fn responding(m: &Mount) -> String {
    match m.responding {
        None => String::new(),
        Some(true) => word("Responding", Some(true)),
        Some(false) => word("Not responding", Some(false)),
    }
}

/// `/nfs/mounts`; the buttons post here.
fn list(rq: &Rq<'_>) -> Response {
    if rq.post() {
        if rq.form().has("add") {
            return rq.redirect("/nfs/mounts/add");
        }
        return rq.redirect("/nfs/mounts");
    }
    let (rows, note) = match rq.env().nfs.mounts(rq.user()) {
        Ok(list) => (
            list.iter()
                .map(|m| {
                    let acts = if m.unmountable() {
                        actions(&[(
                            rq.ctx.url(&format!("/nfs/mounts/{}/confirm", m.id)),
                            "Unmount",
                        )])
                    } else {
                        String::new()
                    };
                    vec![
                        Cell::act(acts),
                        Cell::mono(&m.at),
                        Cell::mono(&m.kind),
                        Cell::mono(&m.source),
                        Cell::mono(&m.options),
                        Cell::text(if m.read_only {
                            "Read only"
                        } else {
                            "Read/write"
                        }),
                        Cell::html(responding(m)),
                    ]
                })
                .collect(),
            String::new(),
        ),
        Err(m) => (
            Vec::new(),
            format!("<p class=\"small grey\">{}</p>", esc(&m.text)),
        ),
    };
    let list = List {
        cols: [
            "Actions",
            "Mounted over",
            "Type",
            "File system",
            "Options",
            "Access",
            "Server",
        ]
        .iter()
        .map(|l| Col::plain(l))
        .collect(),
        rows,
        empty: "No file systems are listed.".into(),
        keep: Vec::new(),
    };
    let body = format!(
        "{}{note}{}{}",
        nfs_tabs(rq, 2),
        list_table(&list),
        form(
            &rq.ctx,
            "",
            &buttons(&[
                Button::new("add", "Mount NFS file system..."),
                Button::new("refresh", "Refresh"),
            ])
        )
    );
    page(
        rq,
        &["Mounts"],
        "Mounted file systems",
        body,
        "nfsmounts",
        Some("/nfs"),
    )
}

fn form_page(rq: &Rq<'_>, m: &MountForm) -> Response {
    let hint = |h: &str| format!(" <span class=\"small grey\">{}</span>", esc(h));
    let rows = vec![
        Prop::new("Server", input("host", &m.host, 40)).required(),
        Prop::new(
            "Exported directory",
            format!("{}{}", input("path", &m.path, 60), hint("on the server")),
        )
        .required(),
        Prop::new(
            "Mount over directory",
            format!(
                "{}{}",
                input("over", &m.over, 60),
                hint("an empty directory here")
            ),
        )
        .required(),
        Prop::new(
            "Options",
            format!(
                "{}{}",
                input("options", &m.options, 60),
                hint("blank: *DFT, read-write, hard, version 3")
            ),
        ),
    ];
    let body = form(
        &rq.ctx,
        "",
        &format!(
            "{}{}",
            props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
            buttons(&[Button::new("ok", "OK"), Button::new("cancel", "Cancel")])
        ),
    );
    page(
        rq,
        &["Mounts", "Mount NFS file system"],
        "Mount NFS file system",
        body,
        "nfsmounts",
        None,
    )
}

/// The Mount form and its post.
fn add(rq: &Rq<'_>) -> Response {
    if !rq.post() {
        return form_page(rq, &MountForm::default());
    }
    let f = rq.form();
    if f.has("cancel") {
        return rq.redirect("/nfs/mounts");
    }
    let m = MountForm {
        host: f.str("host").trim().to_string(),
        path: f.str("path").trim().to_string(),
        over: f.str("over").trim().to_string(),
        options: f.str("options").trim().to_string(),
    };
    if let Err(r) = need(rq, "Mount file system", "MOUNT", "Mounts") {
        return r;
    }
    match rq.env().nfs.run(rq.user(), &command(&m)) {
        Ok(msg) => rq.done("/nfs/mounts", msg),
        Err(msg) => {
            rq.flash(msg);
            form_page(rq, &m)
        }
    }
}

/// Unmount, confirmed.
fn remove(rq: &Rq<'_>, m: &Mount) -> Response {
    let command = unmount(m);
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect("/nfs/mounts");
        }
        if let Err(r) = need(rq, "Unmount file system", "UNMOUNT", "Mounts") {
            return r;
        }
        return run(rq, &command, "/nfs/mounts");
    }
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what: "Unmount",
            crumb: &["NFS", "Mounts"],
            sentence: "The file system is unmounted; what it held can no longer be reached through the directory, and the directory's own contents show again. It is refused while anything is open in it. Press OK to unmount the file system listed below, or Cancel to return.".into(),
            cols: &["Mounted over", "Type", "File system"],
            rows: vec![vec![mono(&m.at), mono(&m.kind), mono(&m.source)]],
            options: String::new(),
            command,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mounts_as_commands() {
        let m = MountForm {
            host: "srv".into(),
            path: "/export/data".into(),
            over: "/mnt/data".into(),
            options: "ro,vers=4".into(),
        };
        assert_eq!(
            command(&m),
            "MOUNT TYPE(*NFS) MFS('srv:/export/data') MNTOVRDIR('/mnt/data') OPTIONS('ro,vers=4')"
        );
        let u = Mount {
            kind: "udfs".into(),
            at: "/u".into(),
            ..Mount::default()
        };
        assert_eq!(unmount(&u), "UNMOUNT TYPE(*UDFS) MNTOVRDIR('/u')");
    }
}
