// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sections after the source: additional diagnostics, external
//! references, the message summary and the final summary.

use std::collections::BTreeMap;

use rpgfront::reader::Class;

use super::source::{self, Place};
use super::{Out, spaced};
use crate::compile::Compiled;
use crate::ctl::{Command, Options};
use crate::msgs;

pub(crate) fn additional(out: &mut Out, c: &Compiled) {
    out.new_page(None);
    out.line('0', format!("{:10}{}", "", spaced("Additional Diagnostic Messages")));
    out.line('0', " Msg id  Sv Number Seq     Message text");
    let mut ds: Vec<&rpgfront::Diag> = c.diags.iter().filter(|d| source::place(d) == Place::Additional).collect();
    // By line, then in the order IBM's checks find them.
    let rank = |id: &str| match id {
        "RNF7030" => 0,
        "RNF7421" | "RNF7551" | "RNF7066" => 1,
        _ => 5,
    };
    // The file checks come first, unused formats before blocking.
    let group = |id: &str| match id {
        "RNF7066" => 0,
        "RNF7086" => 1,
        _ => 2,
    };
    ds.sort_by_key(|d| (group(d.msgid), d.at.line, rank(d.msgid)));
    for d in ds {
        let field = format!("{:>6}", if d.at.line == 0 { 0 } else { c.listing_line(d.at.line) });
        source::diag_lines(out, d, &field, &source::seq_of(c, d));
    }
    out.end_line(
        '0',
        " * * * * *   E N D   O F   A D D I T I O N A L   D I A G N O S T I C   M E S S A G E S   * * * * *",
    );
}

/// The output buffer positions of program-described output fields,
/// after the additional diagnostics.
pub(crate) fn buffer(out: &mut Out, c: &Compiled) {
    let fields: Vec<_> = c.module.outputs.iter().filter(|o| !c.table.recfmts.iter().any(|r| r.name.eq_ignore_ascii_case(&o.name))).collect();
    let plan = super::genlines::plan(c);
    let nums = super::source::numbers(c, &plan);
    // Each kind of output operation (WRITE, UPDATE) has its own buffer.
    let calcs: Vec<&rpgfront::ast::Stmt> = c.module.calcs.iter().chain(c.module.procs.iter().flat_map(|p| p.calcs.iter())).collect();
    let ops_naming = |fmt: &str| {
        ["WRITE", "UPDATE"]
            .iter()
            .filter(|op| {
                calcs.iter().any(|s| {
                    s.op.code == **op
                        && s.op.result.is_empty()
                        && s.op.f2.first().or_else(|| s.op.f1.first()).and_then(|e| e.name()).is_some_and(|n| n.eq_ignore_ascii_case(fmt))
                })
            })
            .count()
            .max(1)
    };
    let mut generated: Vec<(u32, u32, u32, String)> = Vec::new();
    for (b, first) in plan.iter().zip(&nums.blocks).filter(|(b, _)| b.kind == 'O') {
        let fmt = b.refs.first().map(|r| r.0.clone()).unwrap_or_default();
        for _ in 0..ops_naming(&fmt) {
            generated.extend(b.positions.iter().map(|(k, s, e, n)| (first + k, *s, *e, n.clone())));
        }
    }
    if fields.iter().all(|o| o.fields.is_empty()) && generated.is_empty() {
        return;
    }
    out.line('-', format!("{:17}{}", "", spaced("Output Buffer Positions")));
    out.line('0', "Line   Start End   Field or Constant");
    out.line('0', "Number Pos   Pos");
    for o in fields {
        let mut prev_end = 0u32;
        for f in &o.fields {
            let width = field_width(c, f);
            let end = match f.end.trim() {
                "" => prev_end + width,
                e if e.starts_with('+') => prev_end + e[1..].trim().parse::<u32>().unwrap_or(0) + width,
                e => e.parse::<u32>().unwrap_or(prev_end + width),
            };
            let start = (end + 1).saturating_sub(width.max(1));
            let what = if f.field.is_empty() {
                match &f.constant {
                    Some(rpgfront::ast::Expr::Lit(rpgfront::ast::Lit::Char(s), _)) => format!("'{s}'"),
                    _ => String::new(),
                }
            } else {
                f.field.to_ascii_uppercase()
            };
            out.line(' ', format!("{:>6}{start:>6}{end:>6} {what}", c.listing_line(f.at.line)));
            prev_end = end;
        }
    }
    for (no, start, end, name) in generated {
        out.line(' ', format!("{no:>6}{start:>6}{end:>6} {name}"));
    }
    out.end_line('0', " * * * * *   E N D   O F   O U T P U T   B U F F E R   P O S I T I O N   *  * * * *");
}

/// The members `/COPY` brought in, after the additional diagnostics.
pub(crate) fn copies(out: &mut Out, c: &Compiled) {
    let mut rows = Vec::new();
    let mut seen = Vec::new();
    for (k, l) in c.read.lines.iter().enumerate() {
        let Some(next) = c.read.lines.get(k + 1) else { continue };
        if next.member == 0 || next.member == l.member || seen.contains(&next.member) || next.depth <= l.depth {
            continue;
        }
        seen.push(next.member);
        let Some(m) = c.read.members.get(next.member) else { continue };
        rows.push((c.listing_line(l.no), seen.len(), m));
    }
    if rows.is_empty() {
        return;
    }
    out.line('-', format!("{:28}{}", "", spaced("/Copy Members")));
    out.line('0', "Line   Src  RPG name   <-------- External name -------> CCSID  <- Last change ->");
    out.line('0', "Number Id              Library    File       Member            Date     Time");
    for (no, id, m) in rows {
        out.line(
            ' ',
            format!("{no:>6}{id:>5} {:<10} {:<10} {:<10} {:<10}{:>6}  dd-dd-dd tt:tt:tt", m.name, m.lib, m.file, m.name, m.ccsid),
        );
    }
    out.end_line('0', format!("{:9}{}", "", "* * * * *   E N D   O F   / C O P Y   M E M B E R S   * * * * *"));
}

/// How many positions an output field takes.
fn field_width(c: &Compiled, f: &rpgfront::ast::OutputField) -> u32 {
    use crate::types::Ty;
    if f.field.is_empty() {
        return match &f.constant {
            Some(rpgfront::ast::Expr::Lit(rpgfront::ast::Lit::Char(s), _)) => u32::try_from(s.chars().count()).unwrap_or(0),
            _ => 0,
        };
    }
    let name = f.field.to_ascii_uppercase();
    if name.starts_with("PAGE") {
        return if f.edit.is_empty() { 4 } else { edit_width(4, 0, &f.edit).unwrap_or(4) };
    }
    if name.starts_with("*IN") {
        return 1;
    }
    let Some(id) = c.table.find(0, &name) else { return 0 };
    let s = &c.table.syms[id];
    if let Some(rpgfront::ast::Expr::Lit(rpgfront::ast::Lit::Char(w), _)) = &f.constant
        && s.ty.is_numeric()
    {
        return u32::try_from(w.chars().count()).unwrap_or(1);
    }
    // A data format writes the number in that form.
    if let Ty::Packed(d, _) | Ty::Zoned(d, _) | Ty::Bin(d, _) | Ty::Int(d) | Ty::Uns(d) = &s.ty {
        match f.format.trim() {
            "P" => return u32::from(*d) / 2 + 1,
            "B" => return if *d <= 4 { 2 } else if *d <= 9 { 4 } else { 8 },
            "S" => return u32::from(*d),
            _ => {}
        }
    }
    match &s.ty {
        Ty::Packed(d, sc) | Ty::Zoned(d, sc) | Ty::Bin(d, sc) if !f.edit.is_empty() => {
            edit_width(*d, *sc, &f.edit).unwrap_or(u32::from(*d))
        }
        Ty::Int(d) | Ty::Uns(d) if !f.edit.is_empty() => edit_width(*d, 0, &f.edit).unwrap_or(u32::from(*d)),
        Ty::Packed(d, _) | Ty::Zoned(d, _) | Ty::Bin(d, _) | Ty::Int(d) | Ty::Uns(d) => u32::from(*d),
        Ty::Ind => 1,
        t => t.size(&|i| c.table.ds_len(i)),
    }
}

fn edit_width(d: u8, s: u8, edit: &str) -> Option<u32> {
    let code = edit.chars().next()?;
    let shape = qdecimal::Shape::new(d.max(1), s).ok()?;
    let w = qdecimal::code_width(shape, code, &qdecimal::EditOpts::default()).ok()?;
    u32::try_from(w).ok()
}

pub(crate) fn external(out: &mut Out, c: &Compiled) {
    let _ = c;
    out.new_page(None);
    out.line('0', format!("{:22}{}", "", spaced("External References")));
    out.line('0', "      Statically bound procedures:");
    out.line(' ', "         Procedure                            References");
    out.line(' ', "         No references in the source.");
    out.line('0', "      Imported fields:");
    out.line(' ', "         Field             Attributes         Defined");
    out.line(' ', "         No references in the source.");
    out.line('0', "      Exported fields:");
    out.line(' ', "         Field             Attributes         Defined");
    out.line(' ', "         No references in the source.");
    out.end_line('0', format!("{:3}{}", "", "* * * * *   E N D   O F   E X T E R N A L   R E F E R E N C E S   * * * * *"));
}

/// Where the summary's texts start, and their width.
const SUMMARY_AT: usize = 19;
const SUMMARY_WIDTH: usize = 61;

pub(crate) fn summary(out: &mut Out, c: &Compiled, unref: usize) {
    out.new_page(None);
    out.line('0', format!("{:26}{}", "", spaced("Message Summary")));
    out.line('0', " Msg id  Sv Number Message text");
    // By severity, then identifier.
    let mut counts: BTreeMap<(u8, &str), usize> = BTreeMap::new();
    for d in &c.diags {
        *counts.entry((d.severity, d.msgid)).or_default() += 1;
    }
    if unref > 0 {
        *counts.entry((0, super::xref::UNREF)).or_default() += unref;
    }
    for ((sev, id), n) in counts {
        let text = super::lines::wrap_to(&msgs::summary_text(id), SUMMARY_WIDTH, super::lines::lines(id, true));
        out.line(' ', format!("*{id} {sev:02} {n:>6} {}", text[0]));
        for t in &text[1..] {
            out.line(' ', format!("{:SUMMARY_AT$}{t}", ""));
        }
    }
    out.end_line('0', format!("{:7}{}", "", "* * * * *   E N D   O F   M E S S A G E   S U M M A R Y   * * * * *"));
}

pub(crate) fn final_summary(out: &mut Out, c: &Compiled, o: &Options, unref: usize) {
    out.new_page(None);
    out.line('0', format!("{:28}{}", "", spaced("Final Summary")));
    let n = |lo: u8, hi: u8| c.diags.iter().filter(|d| d.severity >= lo && d.severity <= hi).count();
    out.line('0', "  Message Totals:");
    out.line('0', format!("    Information  (00) . . . . . . . :  {:>7}", n(0, 9) + unref));
    out.line(' ', format!("    Warning      (10) . . . . . . . :  {:>7}", n(10, 19)));
    out.line(' ', format!("    Error        (20) . . . . . . . :  {:>7}", n(20, 29)));
    out.line(' ', format!("    Severe Error (30+)  . . . . . . :  {:>7}", n(30, 99)));
    out.line(' ', "    ---------------------------------  -------");
    out.line(' ', format!("    Total . . . . . . . . . . . . . :  {:>7}", c.diags.len() + unref));
    let (records, specs, data, comments) = totals(c);
    out.line('-', "  Source Totals:");
    out.line('0', format!("    Records . . . . . . . . . . . . :  {records:>7}"));
    out.line(' ', format!("    Specifications  . . . . . . . . :  {specs:>7}"));
    out.line(' ', format!("    Data records  . . . . . . . . . :  {data:>7}"));
    out.line(' ', format!("    Comments  . . . . . . . . . . . :  {comments:>7}"));
    out.end_line('0', format!("{:9}{}", "", "* * * * *   E N D   O F   F I N A L   S U M M A R Y   * * * * *"));
    out.line(' ', completion(c, o));
    out.line(' ', format!("{:10}{}", "", "* * * * *   E N D   O F   C O M P I L A T I O N   * * * * *"));
}

/// The source totals: records, specifications, data records, comments.
fn totals(c: &Compiled) -> (usize, usize, usize, usize) {
    let made: usize = c.generated.iter().map(|(_, n)| *n as usize).sum();
    let mut records = made;
    let mut specs = made;
    let mut data = 0;
    let mut comments = 0;
    let info = super::stmts::analyze(&c.read.lines);
    for (l, inf) in c.read.lines.iter().zip(&info) {
        if l.class == Class::Skipped {
            continue;
        }
        records += 1;
        match &l.class {
            Class::Comment => comments += 1,
            // Procedure boundaries are not counted as specifications.
            Class::Spec(_) | Class::Free | Class::Sql => {
                if !matches!(inf.ty, super::stmts::Ty::Spec('P') | super::stmts::Ty::EndProc) {
                    specs += 1;
                }
            }
            Class::CtData => data += 1,
            _ => {}
        }
    }
    (records, specs, data, comments)
}

/// The completion message's line: our text of `RNS9304`/`RNS9305`
/// (created) or `RNS9308` (stopped).
fn completion(c: &Compiled, o: &Options) -> String {
    let what = if o.command == Command::CrtRpgMod { "Module" } else { "Program" };
    if c.created {
        format!("{what} {} created in library {}. Highest severity: {:02}.", o.name, o.lib, c.max_severity)
    } else {
        format!("The compile of {} in {} stopped at severity {:02}.", o.name, o.lib, c.max_severity)
    }
}
