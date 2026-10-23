// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compile listing in the display-file layout: header, the source
//! with its messages and reference expansions, the file keywords, a
//! field summary per record format with the buffers, the indicator
//! usage and the message summary with its totals. Plain text, at most
//! 132 columns, for the spool file.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use dds::lex::keyword::CONSTANT;
use dds::{CompileOpts, Diagnostic, RECORD_LEN, Severity, Source, SourceLine};

use crate::model::{DisplayFile, DisplaySize, Field, KeywordUse, Record, RecordKind, WindowOrigin};

const RULER: &str =
    "....+....1....+....2....+....3....+....4....+....5....+....6....+....7....+....8";
const WIDTH: usize = 132;

/// Render the listing.
#[must_use]
pub fn render(
    source: &Source,
    df: Option<&DisplayFile>,
    diags: &[Diagnostic],
    opts: &CompileOpts,
) -> String {
    let mut out = String::new();
    header(&mut out, opts);
    source_listing(&mut out, source, df, diags);
    if let Some(df) = df {
        file_summary(&mut out, df);
        for r in &df.records {
            record_summary(&mut out, df, r);
        }
        indicators(&mut out, df);
    }
    messages(&mut out, source, diags);
    out
}

fn header(out: &mut String, opts: &CompileOpts) {
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
    let _ = writeln!(out, "  Type . . . . . . . . . . . :  Display file (DSPF)");
    out.push('\n');
}

fn push_row(out: &mut String, row: &str) {
    let row = row.trim_end();
    if row.chars().count() > WIDTH {
        let cut: String = row.chars().take(WIDTH - 3).collect();
        let _ = writeln!(out, "{cut}...");
    } else {
        let _ = writeln!(out, "{row}");
    }
}

fn source_listing(
    out: &mut String,
    source: &Source,
    df: Option<&DisplayFile>,
    diags: &[Diagnostic],
) {
    let _ = writeln!(out, "  SEQNBR  {RULER}  DATE");
    let mut by_line: BTreeMap<usize, Vec<&Diagnostic>> = BTreeMap::new();
    for d in diags {
        if let Some(l) = d.line {
            by_line.entry(l).or_default().push(d);
        }
    }
    let mut refs: BTreeMap<usize, &Field> = BTreeMap::new();
    for r in df.map(|d| d.records.as_slice()).unwrap_or_default() {
        for f in r.fields.iter().filter(|f| f.reference.is_some()) {
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

/// The field line with the referenced shape filled in.
fn expansion(line: &SourceLine, f: &Field) -> String {
    let mut cols: Vec<char> = line.chars(1, RECORD_LEN);
    for c in &mut cols[28..38] {
        *c = ' ';
    }
    let dec = if f.ty.is_numeric() {
        f.decimals.to_string()
    } else {
        String::new()
    };
    let shape = format!("{:>5}{}{:>2}", f.length, f.ty.letter, dec);
    for (i, ch) in shape.chars().enumerate() {
        cols[29 + i] = ch;
    }
    cols[37] = f.usage.letter();
    if let Some((field, file)) = &f.reference {
        let src = match file {
            Some(fr) => format!("REFFLD({field} {fr})"),
            None => format!("REFFLD({field})"),
        };
        for c in &mut cols[44..] {
            *c = ' ';
        }
        for (i, ch) in src.chars().take(36).enumerate() {
            cols[44 + i] = ch;
        }
    }
    cols.into_iter().collect()
}

/// A keyword as the summaries show it: the text, then `<cond>` when
/// conditioned.
fn keyword_text(k: &KeywordUse) -> String {
    if k.name == CONSTANT {
        // Its condition is the field's, shown in the Indicators column.
        return k.parms.first().map(ToString::to_string).unwrap_or_default();
    }
    let mut s = k.to_string();
    if !k.cond.is_always() {
        let _ = write!(s, "<{}>", k.cond);
    }
    s
}

fn keyword_list(kws: &[KeywordUse]) -> String {
    kws.iter().map(keyword_text).collect::<Vec<_>>().join(" ")
}

fn file_summary(out: &mut String, df: &DisplayFile) {
    let sizes: Vec<String> = df.file.sizes.iter().map(DisplaySize::label).collect();
    let _ = writeln!(out, "  Display sizes: {}", sizes.join(", "));
    if df.file.indara {
        out.push_str("  Indicators in a separate 99-byte area (INDARA)\n");
    } else {
        out.push_str("  Indicators at the front of each record buffer\n");
    }
    if !df.file.keywords.is_empty() {
        push_row(
            out,
            &format!("  File keywords: {}", keyword_list(&df.file.keywords)),
        );
    }
    out.push('\n');
}

fn ind_list(v: &[u8]) -> String {
    if v.is_empty() {
        return "none".to_string();
    }
    v.iter()
        .map(|n| format!("{n:02}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[allow(clippy::too_many_lines)] // one block per summary line
fn record_summary(out: &mut String, df: &DisplayFile, r: &Record) {
    let _ = write!(out, "  Record format {}", r.name);
    if let Some(t) = &r.text {
        let _ = write!(out, "    {t}");
    }
    if r.kind != RecordKind::Normal {
        let _ = write!(out, "    ({})", r.kind.name());
    }
    out.push('\n');
    if let Some(s) = &r.subfile {
        let mut parts = Vec::new();
        for (j, size) in df.file.sizes.iter().enumerate() {
            let sz = s.size(j);
            let mut p = format!("SFLSIZ {} SFLPAG {}", sz.size, sz.page);
            let last = s.last_line(j);
            if last > 0 {
                let _ = write!(p, " on lines {}-{}", s.first_line[j], last);
            }
            if df.file.sizes.len() > 1 {
                let _ = write!(p, " ({})", size.name);
            }
            parts.push(p);
        }
        let mut line = format!("    Subfile {}: {}", s.record, parts.join("; "));
        if let Some(e) = s.end {
            let _ = write!(line, "; SFLEND({})", e.name());
        }
        if let Some(l) = s.lin {
            let _ = write!(line, "; SFLLIN({l})");
        }
        if s.message {
            line.push_str("; message subfile");
        }
        push_row(out, &line);
    }
    if let Some(w) = &r.window {
        let origin = match &w.origin {
            WindowOrigin::Default => "placed by the system".to_string(),
            WindowOrigin::At(row, col) => format!("at line {row} column {col}"),
            WindowOrigin::Vars(a, b) => format!("at &{a} &{b}"),
            WindowOrigin::From(n) => format!("as record {n}"),
        };
        let _ = writeln!(
            out,
            "    Window {origin}, {} lines by {} columns inside the border",
            w.lines, w.cols
        );
    }
    if !r.keys.is_empty() {
        let keys: Vec<String> = r
            .keys
            .iter()
            .map(|k| {
                let mut s = k.key.name();
                if let Some(i) = k.ind {
                    let _ = write!(s, "({i:02}");
                    if let Some(t) = &k.text {
                        let _ = write!(s, " '{t}'");
                    }
                    s.push(')');
                }
                if !k.cond.is_always() {
                    let _ = write!(s, "<{}>", k.cond);
                }
                s
            })
            .collect();
        push_row(out, &format!("    Keys: {}", keys.join(" ")));
    }
    let other: Vec<&KeywordUse> = r
        .keywords
        .iter()
        .filter(|k| crate::model::KeyName::parse(&k.name).is_none() && k.name != "TEXT")
        .collect();
    if !other.is_empty() {
        let list = other
            .iter()
            .map(|k| keyword_text(k))
            .collect::<Vec<_>>()
            .join(" ");
        push_row(out, &format!("    Record keywords: {list}"));
    }
    let fmt = |a: &str,
               b: &str,
               c: &str,
               d: &str,
               e: &str,
               f: &str,
               g: &str,
               h: &str,
               i: &str,
               j: &str,
               k: &str,
               l: &str| {
        format!(
            "  {a:<10}  {b:<4}  {c:>6}  {d:>3}  {e:<3}  {f:>3}  {g:>3}  {h:>4}  {i:>5}  {j:>5}  {k:<12}  {l}"
        )
    };
    push_row(
        out,
        &fmt(
            "Field",
            "Type",
            "Length",
            "Dec",
            "Use",
            "Row",
            "Col",
            "Scr",
            "Out",
            "In",
            "Indicators",
            "Keywords",
        ),
    );
    for (i, f) in r.fields.iter().enumerate() {
        let (ty, len, dec, usage) = if f.is_constant() {
            (String::new(), String::new(), String::new(), String::new())
        } else {
            (
                f.ty.letter.to_string(),
                f.length.to_string(),
                if f.ty.is_numeric() {
                    f.decimals.to_string()
                } else {
                    String::new()
                },
                f.usage.letter().to_string(),
            )
        };
        let (row, col) = f.position(0).map_or((String::new(), String::new()), |p| {
            (p.row.to_string(), p.col.to_string())
        });
        let scr = if f.is_displayed() {
            f.screen_len.to_string()
        } else {
            String::new()
        };
        let out_off = r
            .layout
            .output
            .field(i)
            .map(|b| b.offset.to_string())
            .unwrap_or_default();
        let in_off = r
            .layout
            .input
            .field(i)
            .map(|b| b.offset.to_string())
            .unwrap_or_default();
        let mut kws = keyword_list(&f.keywords);
        if f.positions.len() > 1 {
            let alts: Vec<String> = f.positions[1..]
                .iter()
                .enumerate()
                .map(|(j, p)| {
                    format!(
                        "{}:{},{}",
                        df.file.sizes.get(j + 1).map_or("?", |s| s.name.as_str()),
                        p.row,
                        p.col
                    )
                })
                .collect();
            if !kws.is_empty() {
                kws.push(' ');
            }
            let _ = write!(kws, "[{}]", alts.join(" "));
        }
        let name = f.name.clone().unwrap_or_default();
        push_row(
            out,
            &fmt(
                &name,
                &ty,
                &len,
                &dec,
                &usage,
                &row,
                &col,
                &scr,
                &out_off,
                &in_off,
                &f.cond.to_string(),
                &kws,
            ),
        );
    }
    let l = &r.layout;
    let _ = writeln!(
        out,
        "    Output buffer {} bytes (option indicators {}); input buffer {} bytes (response indicators {}); level identifier {}",
        l.output.len,
        ind_list(&l.option_inds),
        l.input.len,
        ind_list(&l.response_inds),
        l.level_id
    );
    out.push('\n');
}

fn seqs(source: &Source, lines: &[usize]) -> String {
    lines
        .iter()
        .filter_map(|l| source.lines().get(*l))
        .map(SourceLine::seq_text)
        .collect::<Vec<_>>()
        .join(" ")
}

fn indicators(out: &mut String, df: &DisplayFile) {
    let usage = df.indicator_usage();
    if usage.is_empty() {
        out.push_str("  No indicators used.\n\n");
        return;
    }
    out.push_str("  Indicator usage\n");
    let _ = writeln!(
        out,
        "  {:>3}  {:<40}  {:<40}  Text",
        "Ind", "Option at", "Response at"
    );
    for (n, u) in &usage {
        let text = df
            .file
            .indtxt
            .iter()
            .find(|(i, _)| i == n)
            .map_or("", |(_, t)| t.as_str());
        push_row(
            out,
            &format!(
                "  {n:>3}  {:<40}  {:<40}  {text}",
                seqs(&df.source, &u.option),
                seqs(&df.source, &u.response)
            ),
        );
    }
    out.push('\n');
}

fn messages(out: &mut String, source: &Source, diags: &[Diagnostic]) {
    if diags.is_empty() {
        out.push_str("  No messages.\n");
    } else {
        out.push_str("  Messages\n");
        let _ = writeln!(out, "  {:<8} Sev  {:>8}  Text", "ID", "Seq");
        let mut ordered: Vec<&Diagnostic> = diags.iter().collect();
        ordered.sort_by_key(|d| d.line.unwrap_or(usize::MAX));
        for d in ordered {
            let seq = d
                .line
                .and_then(|l| source.lines().get(l))
                .map(SourceLine::seq_text)
                .unwrap_or_default();
            push_row(
                out,
                &format!(
                    "  {:<8} {:02}   {:>8}  {}",
                    d.id,
                    d.severity.code(),
                    seq,
                    d.text
                ),
            );
        }
    }
    let count = |s: Severity| diags.iter().filter(|d| d.severity == s).count();
    let _ = writeln!(
        out,
        "  Totals by severity: 00 {}  10 {}  20 {}  30 {}",
        count(Severity::Info),
        count(Severity::Warning),
        count(Severity::Error),
        count(Severity::Severe)
    );
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
