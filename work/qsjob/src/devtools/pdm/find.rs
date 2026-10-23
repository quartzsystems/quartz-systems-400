// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Find string over a file's members: the members that hold the
//! string, and the printed report (`FNDSTRPDM` in batch, `PRTMBRLIST`).

use pdm::find::{Hits, Query};
use pdm::subset::matches;

use super::data;
use crate::console::host::Host;

/// What a search was asked for.
pub struct Search {
    /// The library (resolved).
    pub lib: String,
    /// The file.
    pub file: String,
    /// Member names or generic names; `*ALL`.
    pub members: Vec<String>,
    /// The string and its columns.
    pub query: Query,
}

/// Every member searched with its hits, and how many were searched.
pub fn run(host: &Host, s: &Search) -> (Vec<Hits>, Vec<data::Member>) {
    let all = data::members(host, &s.lib, &s.file, &pdm::subset::Subset::default());
    let chosen: Vec<data::Member> = all
        .into_iter()
        .filter(|m| s.members.iter().any(|p| matches(p, &m.item.name)))
        .collect();
    let mut hits = Vec::new();
    for m in &chosen {
        let lines = data::source(host, &s.lib, &s.file, &m.item.name).unwrap_or_default();
        let found = s.query.search(lines.iter().map(String::as_str));
        if !found.is_empty() {
            hits.push(Hits {
                member: m.item.name.clone(),
                ty: m.item.ty.clone(),
                lines: found,
            });
        }
    }
    (hits, chosen)
}
