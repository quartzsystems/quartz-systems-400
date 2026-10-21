// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `S` and `O` specifications and their AND conditions.

use super::field::{literal, literals};
use super::{FileKind, check_db_columns, count, level_error, parse_name};
use crate::diag::{Diagnostics, ids};
use crate::lex::Spec;
use crate::model::{CompOp, Condition, FieldDef, SelectOmit, Validity};

/// An `S` or `O` line.
pub fn start(
    spec: &Spec,
    select: bool,
    fields: &[FieldDef],
    diags: &mut Diagnostics,
) -> Option<SelectOmit> {
    let what = if select { "select" } else { "omit" };
    if !spec.attributes_blank() {
        diags.push(ids::attributes_not_allowed(spec.line, what));
    }
    check_db_columns(spec, diags);
    let mut group = SelectOmit {
        select,
        all: false,
        conditions: Vec::new(),
        line: spec.line,
    };
    if let Some(kw) = spec.keyword("ALL") {
        count(kw, 0, 0, diags);
        if spec.name.is_some() || spec.keywords.len() > 1 {
            diags.push(ids::all_alone(spec.line));
        }
        group.all = true;
        return Some(group);
    }
    group.conditions.push(condition(spec, fields, diags)?);
    Some(group)
}

/// A line with position 17 blank after an `S` or `O`: one more
/// condition the same group must satisfy.
pub fn and_condition(
    spec: &Spec,
    group: &mut SelectOmit,
    fields: &[FieldDef],
    diags: &mut Diagnostics,
) {
    if !spec.attributes_blank() {
        diags.push(ids::attributes_not_allowed(spec.line, "select/omit"));
    }
    check_db_columns(spec, diags);
    if group.all {
        diags.push(ids::all_alone(spec.line));
        return;
    }
    if let Some(c) = condition(spec, fields, diags) {
        group.conditions.push(c);
    }
}

fn condition(spec: &Spec, fields: &[FieldDef], diags: &mut Diagnostics) -> Option<Condition> {
    let Some(given) = spec.name.as_deref() else {
        diags.push(ids::name_missing(spec.line, "select/omit"));
        return None;
    };
    let name = match parse_name(given) {
        Ok(n) => n,
        Err(why) => {
            diags.push(ids::bad_name(spec.line, given, why));
            return None;
        }
    };
    let Some(field) = fields.iter().find(|f| f.name == name) else {
        diags.push(ids::selomit_field_not_found(spec.line, given));
        return None;
    };
    let Some(test) = tests(spec, field.data_type.is_numeric(), diags) else {
        diags.push(ids::selomit_needs_test(spec.line));
        return None;
    };
    Some(Condition {
        field: name,
        test,
        line: spec.line,
    })
}

/// The one `VALUES`, `RANGE` or `COMP` of a select/omit line.
fn tests(spec: &Spec, numeric: bool, diags: &mut Diagnostics) -> Option<Validity> {
    let mut chosen: Option<(Validity, &str)> = None;
    for kw in &spec.keywords {
        let test = match kw.name.as_str() {
            "VALUES" => {
                if !count(kw, 1, 100, diags) {
                    continue;
                }
                literals(kw, numeric, diags).map(Validity::Values)
            }
            "RANGE" => {
                if !count(kw, 2, 2, diags) {
                    continue;
                }
                literals(kw, numeric, diags).map(|mut l| {
                    let high = l.pop().expect("two");
                    let low = l.pop().expect("two");
                    Validity::Range(low, high)
                })
            }
            "COMP" | "CMP" => {
                if !count(kw, 2, 2, diags) {
                    continue;
                }
                let Some(op) = kw.word(0).and_then(CompOp::from_word) else {
                    diags.push(ids::bad_parm(
                        kw.line,
                        &kw.name,
                        &kw.parms[0],
                        "not a relational operator",
                    ));
                    continue;
                };
                literal(kw, &kw.parms[1], numeric, diags).map(|l| Validity::Comp(op, l))
            }
            _ => {
                level_error(kw, "select/omit", FileKind::Logical, diags);
                continue;
            }
        };
        let Some(test) = test else {
            continue;
        };
        if let Some((_, prev)) = chosen {
            diags.push(ids::conflict(kw.line, &kw.name, prev));
            continue;
        }
        chosen = Some((test, kw.name.as_str()));
    }
    chosen.map(|(test, _)| test)
}
