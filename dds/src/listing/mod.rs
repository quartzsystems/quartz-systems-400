// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compile listing: header, source with messages and reference
//! expansions, field and key summary, message summary. Plain text, at
//! most 132 columns, for the spool file of a later phase.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::compile::CompileOpts;
use crate::diag::{Diagnostic, Severity};
use crate::model::{DefaultValue, FieldDef, PfDefinition};
use crate::source::{RECORD_LEN, Source, SourceLine};

const RULER: &str =
    "....+....1....+....2....+....3....+....4....+....5....+....6....+....7....+....8";

/// Render the listing of a physical file compile.
#[must_use]
pub fn render(
    source: &Source,
    def: Option<&PfDefinition>,
    diags: &[Diagnostic],
    opts: &CompileOpts,
    kind: &str,
) -> String {
    let mut out = String::new();
    header(&mut out, opts, kind);
    source_listing(&mut out, source, def, diags);
    if let Some(def) = def {
        format_summary(&mut out, def);
    }
    messages(&mut out, source, diags);
    out
}

fn header(out: &mut String, opts: &CompileOpts, kind: &str) {
    let title = "QS/400 DDS compiler";
    match &opts.timestamp {
        Some(ts) => {
            let _ = writeln!(out, "{title:<100}{ts}");
        }
        None => {
            let _ = writeln!(out, "{title}");
        }
    }
    if let Some(file) = &opts.file {
        let _ = writeln!(out, "  File . . . . . . . . . . . :  {file}");
    }
    if let Some(src) = &opts.source {
        let _ = writeln!(out, "  Source . . . . . . . . . . :  {src}");
    }
    let _ = writeln!(out, "  Type . . . . . . . . . . . :  {kind}");
    out.push('\n');
}

fn source_listing(
    out: &mut String,
    source: &Source,
    def: Option<&PfDefinition>,
    diags: &[Diagnostic],
) {
    let _ = writeln!(out, "  SEQNBR  {RULER}  DATE");
    let mut by_line: BTreeMap<usize, Vec<&Diagnostic>> = BTreeMap::new();
    for d in diags {
        if let Some(l) = d.line {
            by_line.entry(l).or_default().push(d);
        }
    }
    let mut refs: BTreeMap<usize, &FieldDef> = BTreeMap::new();
    if let Some(def) = def {
        for f in def.format.fields.iter().filter(|f| f.reference.is_some()) {
            refs.insert(f.line, f);
        }
    }
    for (i, line) in source.lines().iter().enumerate() {
        let date = if line.date == 0 {
            String::new()
        } else {
            format!("{:06}", line.date)
        };
        let text = line.cols(1, RECORD_LEN);
        let row = format!("{:>8}  {}  {}", line.seq_text(), text.trim_end(), date);
        let _ = writeln!(out, "{}", row.trim_end());
        if let Some(f) = refs.get(&i) {
            let _ = writeln!(out, "       *  {}", expansion(line, f).trim_end());
        }
        for d in by_line.get(&i).into_iter().flatten() {
            let _ = writeln!(
                out,
                "      **  {} {:02}  {}",
                d.id,
                d.severity.code(),
                d.text
            );
        }
    }
    out.push('\n');
}

/// The field line with the referenced attributes filled in.
fn expansion(line: &SourceLine, f: &FieldDef) -> String {
    let mut cols: Vec<char> = line.chars(1, RECORD_LEN);
    for c in &mut cols[28..44] {
        *c = ' ';
    }
    for c in &mut cols[44..] {
        *c = ' ';
    }
    let shape = format!(
        "{:>5}{}{:>2}",
        f.length,
        f.data_type.letter(),
        if f.data_type.is_numeric() {
            f.decimals.to_string()
        } else {
            String::new()
        }
    );
    for (i, ch) in shape.chars().enumerate() {
        cols[29 + i] = ch;
    }
    if let Some(r) = &f.reference {
        let note = format!("{}", r.field);
        let src = match &r.source {
            crate::model::RefSource::File(fr) => format!("REFFLD({note} {fr})"),
            crate::model::RefSource::Source => format!("REFFLD({note} *SRC)"),
        };
        for (i, ch) in src.chars().take(36).enumerate() {
            cols[44 + i] = ch;
        }
    }
    cols.into_iter().collect()
}

fn format_summary(out: &mut String, def: &PfDefinition) {
    let fmt = &def.format;
    let _ = write!(out, "  Record format {}", fmt.name);
    if let Some(t) = &fmt.text {
        let _ = write!(out, "    {t}");
    }
    out.push('\n');
    let _ = writeln!(
        out,
        "  {:<10}  Type  {:>6}  Dec  {:>5}  {:>6}  Attributes",
        "Field", "Length", "Bytes", "Offset"
    );
    for f in &fmt.fields {
        let dec = if f.data_type.is_numeric() {
            f.decimals.to_string()
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "  {:<10}  {}     {:>6}  {:>3}  {:>5}  {:>6}  {}",
            f.name,
            f.data_type.letter(),
            f.length,
            dec,
            f.byte_len,
            f.offset,
            attributes(f)
        );
    }
    let _ = writeln!(
        out,
        "  Record length {}; {} field(s)",
        fmt.record_len,
        fmt.fields.len()
    );
    if fmt.keys.is_empty() {
        out.push_str("  Access path: arrival sequence\n");
    } else {
        out.push_str("  Key fields\n");
        let _ = writeln!(
            out,
            "  {:>3}  {:<10}  Type  {:>5}  Sequence",
            "Seq", "Field", "Bytes"
        );
        for (i, k) in fmt.keys.iter().enumerate() {
            let f = &fmt.fields[k.field];
            let _ = writeln!(
                out,
                "  {:>3}  {:<10}  {}     {:>5}  {}",
                i + 1,
                k.name,
                f.data_type.letter(),
                f.data_len(),
                k.describe()
            );
        }
        let mut path = match def.unique {
            Some(n) => format!(
                "UNIQUE ({})",
                match n {
                    crate::model::NullKeys::Include => "*INCNULL",
                    crate::model::NullKeys::Exclude => "*EXCNULL",
                }
            ),
            None => "duplicates allowed".to_string(),
        };
        if let Some(o) = def.dup_order {
            let _ = write!(path, "; {}", o.name());
        }
        if let Some(a) = &def.altseq {
            let _ = write!(path, "; ALTSEQ({a})");
        }
        let _ = writeln!(out, "  Access path: {path}; key length {}", fmt.key_len());
    }
    out.push('\n');
}

/// `ALWNULL VARLEN(20) CCSID(37) DATFMT(*ISO) DFT('x') TEXT('...')`.
fn attributes(f: &FieldDef) -> String {
    let mut parts: Vec<String> = Vec::new();
    if f.allow_null {
        parts.push("ALWNULL".into());
    }
    if let Some(v) = f.varlen {
        parts.push(match v.allocated {
            Some(a) => format!("VARLEN({a})"),
            None => "VARLEN".into(),
        });
    }
    if let Some(c) = f.ccsid {
        parts.push(format!("CCSID({c})"));
    }
    if let Some(d) = f.date_fmt {
        parts.push(format!("DATFMT({})", d.name()));
        if d.fixed_separator().is_none()
            && let Some(s) = f.date_sep
        {
            parts.push(format!("DATSEP('{s}')"));
        }
    }
    if let Some(t) = f.time_fmt {
        parts.push(format!("TIMFMT({})", t.name()));
        if t.fixed_separator().is_none()
            && let Some(s) = f.time_sep
        {
            parts.push(format!("TIMSEP('{s}')"));
        }
    }
    if f.data_type == crate::model::DataType::Float {
        parts.push(match f.precision {
            crate::model::FloatPrecision::Single => "FLTPCN(*SINGLE)".into(),
            crate::model::FloatPrecision::Double => "FLTPCN(*DOUBLE)".into(),
        });
    }
    if let Some(d) = &f.default {
        parts.push(match d {
            DefaultValue::Null => "DFT(*NULL)".into(),
            DefaultValue::Quoted(q) => format!("DFT('{}')", q.replace('\'', "''")),
            DefaultValue::Hex(h) => {
                let mut s = String::from("DFT(X'");
                for b in h {
                    let _ = write!(s, "{b:02X}");
                }
                s.push_str("')");
                s
            }
            DefaultValue::Number(n) => format!("DFT({n})"),
        });
    }
    if let Some(e) = &f.edit_code {
        parts.push(match &e.modifier {
            Some(m) => format!("EDTCDE({} {m})", e.code),
            None => format!("EDTCDE({})", e.code),
        });
    }
    if f.edit_word.is_some() {
        parts.push("EDTWRD".into());
    }
    if let Some(a) = &f.alias {
        parts.push(format!("ALIAS({a})"));
    }
    if let Some(t) = &f.text {
        parts.push(format!("TEXT('{}')", t.replace('\'', "''")));
    }
    parts.join(" ")
}

fn messages(out: &mut String, source: &Source, diags: &[Diagnostic]) {
    if diags.is_empty() {
        out.push_str("  No messages.\n");
    } else {
        out.push_str("  Messages\n");
        let _ = writeln!(out, "  {:<8} Sev  {:>8}  Text", "ID", "Seq");
        for d in diags {
            let seq = d
                .line
                .and_then(|l| source.lines().get(l))
                .map(SourceLine::seq_text)
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "  {:<8} {:02}   {:>8}  {}",
                d.id,
                d.severity.code(),
                seq,
                d.text
            );
        }
    }
    let worst = diags
        .iter()
        .map(|d| d.severity)
        .max()
        .unwrap_or(Severity::Info);
    let _ = writeln!(
        out,
        "  {} message(s); compilation ended with severity {:02}.",
        diags.len(),
        worst.code()
    );
}
