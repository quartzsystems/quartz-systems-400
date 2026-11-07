// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/files/shares`: My files, the home directory and the shares the
//! user may use, each browsed with one link. Without policy
//! `files.outside` only the home directory is listed.

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{Cell, Col, List, actions, list_table};

use super::{GROUP, SHARES, browse_route, within};
use crate::policy;
use crate::routes::Rq;
use crate::transfer::size;

/// `GET /files/shares`.
pub fn page(rq: &Rq<'_>) -> Response {
    let files = &rq.env().files;
    let home = files.home(rq.user());
    let outside = rq.allowed(policy::FILES_OUTSIDE);
    let rows = files
        .shares(rq.user())
        .into_iter()
        .filter(|s| outside || within(&s.path, &home))
        .map(|s| {
            vec![
                Cell::act(actions(&[(rq.ctx.url(&browse_route(&s.path)), "Browse")])),
                Cell::text(&s.name),
                Cell::mono(&s.path),
                Cell::text(&s.typ),
                Cell::text(&s.used.map_or_else(String::new, size)),
            ]
        })
        .collect();
    let list = List {
        cols: ["Actions", "Name", "Path", "Type", "Size used"]
            .iter()
            .map(|c| Col::plain(c))
            .collect(),
        rows,
        empty: "No shares.".into(),
        keep: Vec::new(),
    };
    let body = format!(
        "<p class=\"small\">Your home directory <span class=\"mono\">{}</span> and the shares you are authorized to.</p>{}",
        esc(&home),
        list_table(&list)
    );
    rq.page(Page {
        title: "My files".into(),
        crumb: vec![GROUP.into(), "My files".into()],
        h1: "My files".into(),
        body,
        help: "my-files".into(),
        nav: Some((GROUP, Some(SHARES))),
        ..Page::default()
    })
}
