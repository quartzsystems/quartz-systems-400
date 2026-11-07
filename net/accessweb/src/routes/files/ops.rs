// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The operations of Browse files, on the system as the user: New
//! folder (`/files/mkdir`), Rename (`/files/rename`), Cut and Copy
//! (`/files/cut`, `/files/copy`, kept in the session) and Paste, Delete
//! with the count of what goes (`/files/confirm`, confirmed).

use qsweb::frame::Page;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::pages::confirm_body;
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, input_cmd, list_table, mono, props,
};

use super::{
    CLIP, GROUP, back, browse_route, error, good_name, join, name_of, parent, path_arg, token_ok,
};
use crate::msgs;
use crate::policy;
use crate::routes::Rq;

/// A small form page of Browse files.
fn form_page(
    rq: &Rq<'_>,
    title: &str,
    last: &str,
    rows: Vec<Prop>,
    messages: Vec<Message>,
) -> Response {
    let body = props(
        &rq.ctx,
        &[PropGroup { title: None, rows }],
        &[Button::new("ok", "OK"), Button::new("cancel", "Cancel")],
    );
    rq.page(Page {
        title: title.into(),
        crumb: vec![GROUP.into(), "Browse files".into(), last.into()],
        h1: title.into(),
        messages,
        body,
        help: "files".into(),
        nav: Some((GROUP, Some(super::BROWSE))),
        ..Page::default()
    })
}

fn bad(rq: &Rq<'_>, value: &str, field: &str) -> Message {
    rq.message(Kind::Error, msgs::BAD_VALUE, &[value, field])
}

/// `/files/mkdir?path=<dir>`: New folder.
pub fn mkdir(rq: &Rq<'_>) -> Response {
    let dir = path_arg(rq);
    if let Err(r) = super::may_reach(rq, &dir, &[GROUP, "Browse files"]) {
        return r;
    }
    let f = rq.form();
    let mut messages = Vec::new();
    if rq.post() {
        if f.has("cancel") {
            return rq.redirect(&browse_route(&dir));
        }
        let name = f.str("name").trim().to_string();
        if good_name(&name) {
            let p = join(&dir, &name);
            match rq.env().files.mkdir(rq.user(), &p) {
                Ok(()) => {
                    return back(
                        rq,
                        &dir,
                        rq.message(Kind::Done, msgs::FOLDER_CREATED, &[&p]),
                    );
                }
                Err(m) => messages.push(error(rq, &m)),
            }
        } else {
            messages.push(bad(rq, &name, "Folder name"));
        }
    }
    form_page(
        rq,
        "New folder",
        "New folder",
        vec![
            Prop::new("In folder", mono(&dir)),
            Prop::new("Folder name", input_cmd("name", f.str("name"), 40)).required(),
        ],
        messages,
    )
}

/// `/files/rename?path=`: Rename (or move, given a path).
pub fn rename(rq: &Rq<'_>) -> Response {
    let path = path_arg(rq);
    let dir = parent(&path);
    if let Err(r) = super::may_reach(rq, &path, &[GROUP, "Browse files"]) {
        return r;
    }
    let name = name_of(&path).to_string();
    let f = rq.form();
    let mut messages = Vec::new();
    if rq.post() {
        if f.has("cancel") {
            return rq.redirect(&browse_route(&dir));
        }
        let new = f.str("name").trim().to_string();
        // A name stays in the folder; a path moves it.
        let to = if new.starts_with('/') {
            Some(super::clean(&new))
        } else if good_name(&new) {
            Some(join(&dir, &new))
        } else {
            None
        };
        match to {
            Some(to) => {
                if let Err(r) = super::may_reach(rq, &to, &[GROUP, "Browse files"]) {
                    return r;
                }
                match rq.env().files.rename(rq.user(), &path, &to) {
                    Ok(()) => {
                        return back(
                            rq,
                            &dir,
                            rq.message(Kind::Done, msgs::RENAMED, &[&path, &to]),
                        );
                    }
                    Err(m) => messages.push(error(rq, &m)),
                }
            }
            None => messages.push(bad(rq, &new, "New name")),
        }
    }
    let value = if rq.post() {
        f.str("name").to_string()
    } else {
        name.clone()
    };
    form_page(
        rq,
        &format!("Rename: {name}"),
        &name,
        vec![
            Prop::new("Path", mono(&path)),
            Prop::new("New name", input_cmd("name", &value, 40)).required(),
        ],
        messages,
    )
}

/// `/files/cut?path=&t=`, `/files/copy?path=&t=`: one entry to paste.
pub fn clip(rq: &Rq<'_>, how: &str) -> Response {
    if !token_ok(rq) {
        return super::stale(rq);
    }
    let path = path_arg(rq);
    rq.set(&[(CLIP, &format!("{how}\n{path}"))]);
    rq.redirect(&browse_route(&parent(&path)))
}

/// Paste what was cut (moved, then forgotten) or copied (on the
/// system; kept for another paste) into `dir`.
pub fn paste(rq: &Rq<'_>, dir: &str) -> Response {
    let clip = rq.get(CLIP);
    let mut lines = clip.lines();
    let how = lines.next().unwrap_or("").to_string();
    let paths: Vec<String> = lines.map(ToString::to_string).collect();
    if paths.is_empty() {
        return rq.redirect(&browse_route(dir));
    }
    let files = &rq.env().files;
    let mut done = Vec::new();
    for p in &paths {
        let to = join(dir, name_of(p));
        if let Err(r) = super::may_reach(rq, p, &[GROUP, "Browse files"]) {
            return r;
        }
        let r = if how == "cut" {
            files.rename(rq.user(), p, &to)
        } else if to == *p {
            Err(rq.message(Kind::Error, msgs::FILE_EXISTS, &[name_of(p), dir]))
        } else {
            files.copy(rq.user(), p, &to)
        };
        if let Err(m) = r {
            // What was pasted before stays; the rest stays to paste.
            let left: Vec<&str> = paths
                .iter()
                .filter(|q| !done.contains(q))
                .map(String::as_str)
                .collect();
            rq.set(&[(CLIP, &format!("{how}\n{}", left.join("\n")))]);
            return back(rq, dir, error(rq, &m));
        }
        done.push(p);
    }
    if how == "cut" {
        rq.set(&[(CLIP, "")]);
    }
    let what = if paths.len() == 1 {
        paths[0].clone()
    } else {
        format!("{} entries", paths.len())
    };
    let id = if how == "cut" {
        msgs::RENAMED
    } else {
        msgs::COPIED
    };
    back(rq, dir, rq.message(Kind::Done, id, &[&what, dir]))
}

/// `/files/confirm?path=<dir>&s=<name>...`: Delete, with what goes.
pub fn delete(rq: &Rq<'_>) -> Response {
    let dir = path_arg(rq);
    let crumb = [GROUP, "Browse files", "Confirm Delete"];
    if let Err(r) = rq.policy(policy::FILES_DELETE, &crumb) {
        return r;
    }
    if let Err(r) = super::may_reach(rq, &dir, &crumb) {
        return r;
    }
    let names: Vec<String> = rq
        .query()
        .all("s")
        .into_iter()
        .filter(|n| good_name(n))
        .map(ToString::to_string)
        .collect();
    if names.is_empty() {
        return rq.redirect(&browse_route(&dir));
    }
    let files = &rq.env().files;
    if rq.post() {
        if !rq.form().has("ok") {
            return rq.redirect(&browse_route(&dir));
        }
        let mut gone = 0;
        for n in &names {
            let p = join(&dir, n);
            match files.delete(rq.user(), &p) {
                Ok(c) => gone += c,
                Err((at, m, _)) => {
                    let why = format!("{} {}", m.id, m.text);
                    return back(
                        rq,
                        &dir,
                        rq.message(Kind::Error, msgs::DELETE_STOPPED, &[&p, &at, &why]),
                    );
                }
            }
        }
        let what = if names.len() == 1 {
            join(&dir, &names[0])
        } else {
            format!("{} entries ({gone} objects)", names.len())
        };
        return back(rq, &dir, rq.message(Kind::Done, msgs::DELETED, &[&what]));
    }
    let mut rows = Vec::new();
    let mut all = 0;
    for n in &names {
        let p = join(&dir, n);
        let e = match files.entry(rq.user(), &p) {
            Ok(e) => e,
            Err(m) => return back(rq, &dir, error(rq, &m)),
        };
        let c = files.count(rq.user(), &p).unwrap_or(1);
        all += c;
        rows.push(vec![
            Cell::mono(n),
            Cell::text(if e.kind == crate::env::files::Kind::Dir {
                "Directory"
            } else {
                "File"
            }),
            Cell::num(&crate::transfer::grouped(c)),
        ]);
    }
    let table = list_table(&List {
        cols: vec![
            Col::plain("Name"),
            Col::plain("Type"),
            Col::plain("Objects"),
        ],
        rows,
        empty: String::new(),
        keep: Vec::new(),
    });
    let sentence = format!(
        "Deleting removes the entries listed below from {dir}, and everything a folder holds: {all} {} in all. Press OK to delete them, or Cancel to return without deleting.",
        if all == 1 { "object" } else { "objects" }
    );
    let body = format!(
        "{}<p class=\"small grey\">A deleted file cannot be recovered from Access for Web.</p>",
        confirm_body(&rq.ctx, &sentence, &table, "")
    );
    rq.page(Page {
        title: "Confirm Delete".into(),
        crumb: crumb.iter().map(ToString::to_string).collect(),
        h1: "Confirm Delete".into(),
        body,
        help: "confirm".into(),
        nav: Some((GROUP, Some(super::BROWSE))),
        ..Page::default()
    })
}
