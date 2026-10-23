// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compile listing: header, the source with sequence numbers,
//! imbed member and messages under their lines, the counts and the
//! message summary. Plain text for the spooled file of a later phase.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::compile::CompileOpts;
use crate::diag::{Diagnostic, Severity};
use crate::model::PanelGroup;
use crate::source::Source;

const RULER: &str =
    "....+....1....+....2....+....3....+....4....+....5....+....6....+....7....+....8";

/// Render a listing.
#[must_use]
pub fn render(
    source: &Source,
    group: Option<&PanelGroup>,
    diags: &[Diagnostic],
    opts: &CompileOpts,
) -> String {
    let mut out = String::new();
    header(&mut out, opts);
    source_listing(&mut out, source, diags);
    if let Some(g) = group {
        counts(&mut out, g);
    }
    messages(&mut out, source, diags);
    out
}

fn header(out: &mut String, opts: &CompileOpts) {
    let title = "QS/400 UIM compiler";
    match &opts.timestamp {
        Some(ts) => {
            let _ = writeln!(out, "{title:<100}{ts}");
        }
        None => {
            let _ = writeln!(out, "{title}");
        }
    }
    if let Some(name) = &opts.name {
        let qualified = match &opts.library {
            Some(lib) => format!("{lib}/{name}"),
            None => name.clone(),
        };
        let _ = writeln!(out, "  {}  {qualified}", dots("Panel group"));
    }
    if let Some(src) = &opts.source {
        let _ = writeln!(out, "  {}  {src}", dots("Source"));
    }
    out.push('\n');
}

fn source_listing(out: &mut String, source: &Source, diags: &[Diagnostic]) {
    let _ = writeln!(out, "  SEQNBR  MEMBER      {RULER}");
    let mut by_line: BTreeMap<usize, Vec<&Diagnostic>> = BTreeMap::new();
    for d in diags {
        if let Some(l) = d.line {
            by_line.entry(l).or_default().push(d);
        }
    }
    for (i, line) in source.lines().iter().enumerate() {
        let member = line.imbed.as_deref().unwrap_or("");
        let row = format!("  {}  {member:<10}  {}", line.seq_text(), line.text);
        let _ = writeln!(out, "{}", row.trim_end());
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

fn counts(out: &mut String, g: &PanelGroup) {
    let rows = [
        ("Panels", g.panels.len()),
        ("Print panels", g.print_panels.len()),
        ("Help modules", g.helps.len()),
        ("Classes", g.classes.len()),
        ("Variables", g.variables.len()),
        ("Conditions", g.conditions.len()),
        ("Key lists", g.key_lists.len()),
        ("Imports", g.imports.len()),
    ];
    for (label, n) in rows {
        let _ = writeln!(out, "  {}  {n}", dots(label));
    }
    out.push('\n');
}

fn messages(out: &mut String, source: &Source, diags: &[Diagnostic]) {
    if !diags.is_empty() {
        let _ = writeln!(out, "  Messages");
        for d in diags {
            let seq = d
                .line
                .and_then(|l| source.lines().get(l))
                .map_or_else(|| "      ".to_string(), crate::source::SourceLine::seq_text);
            let _ = writeln!(
                out,
                "  {seq}  {} {:02}  {}",
                d.id,
                d.severity.code(),
                d.text
            );
        }
        out.push('\n');
    }
    let mut by_sev: BTreeMap<u8, usize> = BTreeMap::new();
    for d in diags {
        *by_sev.entry(d.severity.code()).or_default() += 1;
    }
    for sev in [10u8, 20, 30, 40] {
        let n = by_sev.get(&sev).copied().unwrap_or(0);
        let _ = writeln!(out, "  {}  {n}", dots(&format!("Severity {sev}")));
    }
    let highest = diags.iter().map(|d| d.severity).max();
    let _ = writeln!(
        out,
        "  {}  {:02}",
        dots("Highest severity"),
        highest.map_or(0, Severity::code)
    );
    let created = highest.is_none_or(|s| s < Severity::Error);
    let _ = writeln!(
        out,
        "  {}",
        if created {
            "Panel group created"
        } else {
            "Panel group not created"
        }
    );
}

/// `Label . . . . . . :` with the colon in column 28.
fn dots(label: &str) -> String {
    let mut s = format!("{label} ");
    while s.len() + 2 <= 27 {
        s.push_str(". ");
    }
    format!("{s:<27}:")
}
