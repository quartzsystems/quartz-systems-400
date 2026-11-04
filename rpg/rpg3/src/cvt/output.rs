// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Output specifications: record lines (with `AND`/`OR` lines) and
//! field lines at the ILE RPG columns.

use super::place::{Rec, cut3, factor, raw3};
use super::{Out, Sect};

/// Whether an output record is a record line: a file name, a type or
/// an `AND`/`OR`.
fn is_record(r: &str) -> bool {
    let rel = cut3("O", r, "relation");
    !cut3("O", r, "name").is_empty() || rel == "AND" || rel == "OR" || !cut3("O", r, "type").is_empty()
}

/// Convert one output specification.
pub(super) fn convert(r: &str, at: usize, out: &mut Out) {
    let mut o;
    let cond = raw3("O", r, "cond");
    if is_record(r) {
        o = Rec::new("O", r);
        let rel = cut3("O", r, "relation");
        if rel == "AND" || rel == "OR" {
            o.put("relation", &rel);
        } else {
            o.put("name", &cut3("O", r, "name")).put("type", &cut3("O", r, "type"));
            let add = cut3("O", r, "add");
            if add == "ADD" || add == "DEL" {
                o.put("add", &add);
            } else {
                o.put("fetch", &cut3("O", r, "fetch"))
                    .right("spaceb", &cut3("O", r, "spaceb"))
                    .right("spacea", &cut3("O", r, "spacea"));
            }
            o.right("skipb", &cut3("O", r, "skipb")).right("skipa", &cut3("O", r, "skipa"));
            o.put("except", &cut3("O", r, "except"));
        }
        o.put("cond", cond.trim_end());
    } else {
        o = Rec::new("OF", r);
        o.put("cond", cond.trim_end())
            .put("field", &factor(&cut3("OF", r, "field")))
            .put("editcode", &cut3("OF", r, "editcode"))
            .put("blank", &cut3("OF", r, "blank"))
            .right("end", &cut3("OF", r, "end"))
            .put("format", &cut3("OF", r, "format"))
            .put("constant", &cut3("OF", r, "constant"));
    }
    out.push(Sect::O, &o.text(), at);
}
