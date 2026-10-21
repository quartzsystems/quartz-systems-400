// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the listing says about a definition, built from a PF or LF.

use std::fmt::Write as _;

use crate::model::{
    DupOrder, FieldSource, FileRef, LfDefinition, LfFormat, NullKeys, PfDefinition, RecordFormat,
    SelectOmit, Validity,
};

/// One record format and what to say about it.
#[derive(Debug)]
pub struct FormatView<'a> {
    /// The format.
    pub format: &'a RecordFormat,
    /// Lines printed under the format heading.
    pub notes: Vec<String>,
    /// The access path line.
    pub access_path: String,
}

/// The whole definition.
#[derive(Debug, Default)]
pub struct Summary<'a> {
    /// Lines printed before the formats.
    pub notes: Vec<String>,
    /// The formats in order.
    pub formats: Vec<FormatView<'a>>,
}

/// The summary of a physical file.
#[must_use]
pub fn pf(def: &PfDefinition) -> Summary<'_> {
    Summary {
        notes: Vec::new(),
        formats: vec![FormatView {
            format: &def.format,
            notes: Vec::new(),
            access_path: access_path(&def.format, def.unique, def.dup_order, def.altseq.as_ref()),
        }],
    }
}

/// The summary of a logical file.
#[must_use]
pub fn lf(def: &LfDefinition) -> Summary<'_> {
    let mut notes = Vec::new();
    if def.dynslt {
        notes.push("Select/omit at read time (DYNSLT)".to_string());
    }
    if def.jdftval {
        notes.push("Default values for unmatched join records (JDFTVAL)".to_string());
    }
    let formats = def
        .formats
        .iter()
        .map(|f| FormatView {
            format: &f.format,
            notes: lf_notes(f),
            access_path: access_path(&f.format, def.unique, def.dup_order, def.altseq.as_ref()),
        })
        .collect();
    Summary { notes, formats }
}

fn lf_notes(f: &LfFormat) -> Vec<String> {
    let mut notes = Vec::new();
    let kind = if f.join { "Join of" } else { "Based on" };
    let files: Vec<String> = f
        .based_on
        .iter()
        .zip(&f.base_formats)
        .enumerate()
        .map(|(i, (file, fmt))| format!("{} {file} ({fmt})", i + 1))
        .collect();
    notes.push(format!("{kind} {}", files.join(", ")));
    for j in &f.joins {
        let pairs: Vec<String> = j.fields.iter().map(|(a, b)| format!("{a} = {b}")).collect();
        let mut s = format!(
            "Join {} to {} on {}",
            j.from + 1,
            j.to + 1,
            pairs.join(", ")
        );
        if let Some((d, desc)) = &j.dupseq {
            let _ = write!(
                s,
                "; duplicates by {d}{}",
                if *desc { " descending" } else { "" }
            );
        }
        notes.push(s);
    }
    for (field, src) in f.format.fields.iter().zip(&f.sources) {
        match src {
            FieldSource::Base { file, field: base } if *base != field.name => {
                notes.push(format!(
                    "{} renames {base} of file {}",
                    field.name,
                    file + 1
                ));
            }
            FieldSource::Base { .. } => {}
            FieldSource::Concat { file, fields } => {
                let list: Vec<String> = fields.iter().map(ToString::to_string).collect();
                notes.push(format!(
                    "{} = CONCAT({}) of file {}",
                    field.name,
                    list.join(" "),
                    file + 1
                ));
            }
            FieldSource::Substring {
                file,
                field: base,
                start,
                length,
            } => notes.push(format!(
                "{} = SST({base} {start} {length}) of file {}",
                field.name,
                file + 1
            )),
        }
    }
    for g in &f.select_omit {
        notes.push(select_omit(g));
    }
    notes
}

fn select_omit(g: &SelectOmit) -> String {
    let mut s = String::from(if g.select { "Select" } else { "Omit" });
    if g.all {
        s.push_str(" ALL");
        return s;
    }
    for (i, c) in g.conditions.iter().enumerate() {
        s.push_str(if i == 0 { " " } else { " AND " });
        let _ = write!(s, "{} ", c.field);
        match &c.test {
            Validity::Values(v) => {
                let list: Vec<String> = v.iter().map(ToString::to_string).collect();
                let _ = write!(s, "VALUES({})", list.join(" "));
            }
            Validity::Range(lo, hi) => {
                let _ = write!(s, "RANGE({lo} {hi})");
            }
            Validity::Comp(op, v) => {
                let _ = write!(s, "COMP({} {v})", op.name());
            }
            Validity::Check(c) => {
                let _ = write!(s, "CHECK({})", c.join(" "));
            }
        }
    }
    s
}

fn access_path(
    fmt: &RecordFormat,
    unique: Option<NullKeys>,
    dup_order: Option<DupOrder>,
    altseq: Option<&FileRef>,
) -> String {
    if fmt.keys.is_empty() {
        return "arrival sequence".to_string();
    }
    let mut path = match unique {
        Some(NullKeys::Include) => "UNIQUE (*INCNULL)".to_string(),
        Some(NullKeys::Exclude) => "UNIQUE (*EXCNULL)".to_string(),
        None => "duplicates allowed".to_string(),
    };
    if let Some(o) = dup_order {
        let _ = write!(path, "; {}", o.name());
    }
    if let Some(a) = altseq {
        let _ = write!(path, "; ALTSEQ({a})");
    }
    let _ = write!(path, "; key length {}", fmt.key_len());
    path
}
