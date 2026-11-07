// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/files/props?path=`: an entry's properties, its data authority,
//! `*PUBLIC`'s and the authorization list changed with OK (`CHGAUT`'s
//! check: the owner or `*ALLOBJ`).

use qsweb::frame::Page;
use qsweb::http::Response;
use qsweb::msg::Kind;
use qsweb::partials::{Button, Prop, PropGroup, input, input_cmd, mono, props};

use super::browse::select;
use super::{GROUP, back, error, mode_of, name_of, parent, path_arg, rwx};
use crate::env::files::{AuthChange, Kind as EntryKind};
use crate::msgs;
use crate::routes::Rq;
use crate::transfer::grouped;

/// `*PUBLIC`'s data authorities and their bits.
const PUBLIC: &[(&str, u32)] = &[("*EXCLUDE", 0), ("*R", 4), ("*RW", 6), ("*RWX", 7)];

fn public_of(mode: u32) -> &'static str {
    PUBLIC.iter().find(|p| p.1 == mode & 7).map_or("*EXCLUDE", |p| p.0)
}

/// `GET` and `POST /files/props`.
#[allow(clippy::too_many_lines)]
pub fn page(rq: &Rq<'_>) -> Response {
    let path = path_arg(rq);
    let name = if path == "/" { "/".to_string() } else { name_of(&path).to_string() };
    let crumb = [GROUP, "Browse files", name.as_str()];
    if let Err(r) = super::may_reach(rq, &path, &crumb) {
        return r;
    }
    let files = &rq.env().files;
    let p = match files.props(rq.user(), &path) {
        Ok(p) => p,
        Err(m) => return back(rq, &parent(&path), error(rq, &m)),
    };
    let mut messages = Vec::new();
    if rq.post() {
        let f = rq.form();
        if !f.has("ok") {
            return rq.redirect(&super::browse_route(&parent(&path)));
        }
        let data_auth = f.str("auth");
        let autl = f.str("autl").trim().to_ascii_uppercase();
        match (mode_of(data_auth), PUBLIC.iter().find(|x| x.0 == f.str("pub"))) {
            (Some(mut mode), Some(pb)) if !autl.is_empty() => {
                // `*PUBLIC` decides the public bits when it changed.
                if pb.0 != public_of(p.entry.mode) {
                    mode = (mode & !7) | pb.1;
                }
                let change = AuthChange { mode, autl: autl.clone() };
                let unchanged = mode == p.entry.mode & 0o777 && autl == p.autl;
                if unchanged {
                    return rq.redirect(&super::browse_route(&parent(&path)));
                }
                match files.set_auth(rq.user(), &path, &change) {
                    Ok(()) => {
                        return back(rq, &parent(&path), rq.message(Kind::Done, msgs::AUTH_CHANGED, &[&path]));
                    }
                    Err(m) => messages.push(error(rq, &m)),
                }
            }
            (None, _) => messages.push(rq.message(Kind::Error, msgs::BAD_VALUE, &[data_auth, "Data authority"])),
            _ => messages.push(rq.message(Kind::Error, msgs::BAD_VALUE, &[f.str("pub"), "*PUBLIC"])),
        }
    }
    let e = &p.entry;
    let mut file = vec![
        Prop::new("Path", mono(&path)),
        Prop::new("Type", qsweb::html::esc(&p.typ)),
    ];
    match &e.kind {
        EntryKind::File => {
            file.push(Prop::new("Size", format!("{} bytes", grouped(e.size))));
            file.push(Prop::new("CCSID", e.ccsid.to_string()));
        }
        EntryKind::Link { target, broken, .. } => {
            let t = qsweb::html::esc(target);
            file.push(Prop::new(
                "Link to",
                if *broken { format!("<span class=\"err\">{t} (broken)</span>") } else { format!("<span class=\"mono\">{t}</span>") },
            ));
        }
        EntryKind::Dir | EntryKind::Other(_) => {}
    }
    file.push(Prop::new("Created", rq.when(p.created)));
    file.push(Prop::new("Modified", rq.when(e.modified)));
    file.push(Prop::new("Last used", rq.when(p.accessed)));
    file.push(Prop::new("Change count", p.changes.to_string()));
    let public = public_of(e.mode);
    let auth = vec![
        Prop::new("Owner", mono(&e.owner)),
        Prop::new("Primary group", mono(&p.group)),
        Prop::new(
            "Data authority",
            format!(
                "{} &nbsp; *PUBLIC: {}",
                input_cmd("auth", &rwx(e.mode), 10),
                select("pub", &PUBLIC.iter().map(|x| x.0).collect::<Vec<_>>(), public)
            ),
        ),
        Prop::new("Authorization list", input("autl", &p.autl, 10)),
    ];
    let kind = match &e.kind {
        EntryKind::Dir => "Directory",
        EntryKind::Link { .. } => "Link",
        _ => "File",
    };
    let body = props(
        &rq.ctx,
        &[
            PropGroup {
                title: Some(kind.into()),
                rows: file,
            },
            PropGroup {
                title: Some("Authority".into()),
                rows: auth,
            },
        ],
        &[Button::new("ok", "OK"), Button::new("cancel", "Cancel")],
    );
    let title = format!("Properties: {name}");
    rq.page(Page {
        title: title.clone(),
        crumb: crumb.iter().map(ToString::to_string).collect(),
        h1: title,
        messages,
        body,
        help: "file-props".into(),
        nav: Some((GROUP, Some(super::BROWSE))),
        ..Page::default()
    })
}
