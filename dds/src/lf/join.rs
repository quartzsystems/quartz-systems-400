// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `J` specifications of a join logical file.

use super::Current;
use super::fields::file_index;
use crate::compile::{self, FileKind, count, parse_name};
use crate::diag::{Diagnostics, ids};
use crate::lex::{Keyword, Spec};
use crate::model::Join;

/// A `J` line: `JOIN`, `JFLD` and `JDUPSEQ`.
pub(crate) fn add(spec: &Spec, cur: &mut Current, diags: &mut Diagnostics) {
    if !cur.lf.join {
        diags.push(ids::join_spec_not_in_join(spec.line));
        return;
    }
    if !spec.attributes_blank() {
        diags.push(ids::attributes_not_allowed(spec.line, "join"));
    }
    compile::check_db_columns(spec, diags);
    let n = cur.bases.len();
    let mut join = Join {
        from: 0,
        to: 1,
        fields: Vec::new(),
        dupseq: None,
        line: spec.line,
    };
    let mut have_join = false;
    for kw in &spec.keywords {
        match kw.name.as_str() {
            "JOIN" => {
                if compile::seen_before(have_join, kw, diags) || !count(kw, 2, 2, diags) {
                    continue;
                }
                let (Some(from), Some(to)) =
                    (file_index(kw, 0, &cur.bases), file_index(kw, 1, &cur.bases))
                else {
                    diags.push(ids::bad_parm(
                        kw.line,
                        &kw.name,
                        kw,
                        "use file numbers 1 to the JFILE count, or JFILE names",
                    ));
                    continue;
                };
                join.from = from;
                join.to = to;
                have_join = true;
            }
            "JFLD" => {
                if !count(kw, 2, 2, diags) {
                    continue;
                }
                if let (Some(a), Some(b)) = (field_parm(kw, 0, diags), field_parm(kw, 1, diags)) {
                    join.fields.push((a, b));
                }
            }
            "JDUPSEQ" => {
                if compile::seen_before(join.dupseq.is_some(), kw, diags) || !count(kw, 1, 2, diags)
                {
                    continue;
                }
                let Some(f) = field_parm(kw, 0, diags) else {
                    continue;
                };
                let descend = match kw.word(1) {
                    None => false,
                    Some("*DESCEND") => true,
                    Some(_) => {
                        diags.push(ids::bad_parm(
                            kw.line,
                            &kw.name,
                            &kw.parms[1],
                            "use *DESCEND",
                        ));
                        continue;
                    }
                };
                join.dupseq = Some((f, descend));
            }
            _ => compile::level_error(kw, "join", FileKind::Logical, diags),
        }
    }
    if !have_join && n > 2 {
        diags.push(ids::join_required(spec.line));
    }
    if join.fields.is_empty() {
        diags.push(ids::jfld_required(spec.line));
    }
    check_fields(&join, cur, diags);
    cur.lf.joins.push(join);
}

/// Every `JFLD` and `JDUPSEQ` field must exist in its file.
fn check_fields(join: &Join, cur: &Current, diags: &mut Diagnostics) {
    let from = &cur.bases[join.from];
    let to = &cur.bases[join.to];
    let missing = |diags: &mut Diagnostics, name: &qsname::Name, base: &super::Base| {
        diags.push(ids::base_field_not_found(
            join.line,
            &name.to_string(),
            &base.file,
        ));
    };
    for (a, b) in &join.fields {
        if from.format.field(a).is_none() {
            missing(diags, a, from);
        }
        if to.format.field(b).is_none() {
            missing(diags, b, to);
        }
    }
    if let Some((f, _)) = &join.dupseq
        && to.format.field(f).is_none()
    {
        missing(diags, f, to);
    }
}

fn field_parm(kw: &Keyword, i: usize, diags: &mut Diagnostics) -> Option<qsname::Name> {
    let p = &kw.parms[i];
    let Ok(n) = p.as_word().ok_or_else(String::new).and_then(parse_name) else {
        diags.push(ids::bad_parm(kw.line, &kw.name, p, "must be a field name"));
        return None;
    };
    Some(n)
}

/// A join format needs one `J` line per secondary file.
pub(crate) fn check(cur: &Current, diags: &mut Diagnostics) {
    if !cur.lf.join {
        return;
    }
    let want = cur.bases.len() - 1;
    let got = cur.lf.joins.len();
    if want != got {
        diags.push(ids::join_count(cur.lf.format.line, want, got));
    }
}
