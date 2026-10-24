// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The source section: each record with its statement number, nesting
//! level, change date, source id and sequence number; a blank line
//! more where the specification type changes; the diagnostics after
//! the statement they belong to, with markers under the columns they
//! point at; the boxes of the options in effect, copy members and
//! files; and the compile-time data section.

use std::collections::BTreeMap;

use rpgfront::Diag;
use rpgfront::reader::{Class, Line};

use super::stmts::{self, Info, Ty};
use super::{Out, genlines, prologue, spaced};
use crate::compile::Compiled;
use crate::ctl::Options;
use crate::msgs;

/// Columns of a diagnostic's text, and its width.
pub(crate) const TEXT_AT: usize = 27;
pub(crate) const TEXT_WIDTH: usize = 59;

thread_local! {
    /// The listing being built numbers statements by sequence number.
    pub(crate) static SRCSTMT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn srcstmt() -> bool {
    SRCSTMT.with(std::cell::Cell::get)
}

/// Rulers headed for statement numbers under `*SRCSTMT`.
fn stmt_rulers((a, b): (String, String)) -> (String, String) {
    if !srcstmt() {
        return (a, b);
    }
    (
        a.replacen("Line   <", "Seq    <", 1).replace("Change Src Seq", "Change Statement"),
        b.replace("Date   Id  Number", "Date   Number"),
    )
}

/// The rulers of free-form source.
pub(crate) fn free_rulers() -> (String, String) {
    stmt_rulers(free_rulers_plain())
}

/// The rulers of fixed-form source.
pub(crate) fn fixed_rulers() -> (String, String) {
    stmt_rulers(fixed_rulers_plain())
}

fn free_rulers_plain() -> (String, String) {
    (
        "Line   <---------------------- Source Specifications ----------------------------------------------------->  Do  Change Src Seq".into(),
        "Number ....+....1....+....2....+....3....+....4....+....5....+....6....+....7....+....8....+....9....+...10  Num Date   Id  Number".into(),
    )
}

fn fixed_rulers_plain() -> (String, String) {
    (
        "Line   <---------------------- Source Specifications ----------------------------><---- Comments ----> Do  Page  Change Src Seq".into(),
        "Number ....1....+....2....+....3....+....4....+....5....+....6....+....7....+....8....+....9....+...10 Num Line  Date   Id  Number".into(),
    )
}

/// The rulers of fixed-form calculations indented by `INDENT`.
fn indent_rulers() -> (String, String) {
    (
        "Line   <--------------------- Source Specifications ----------------------------------------------><---- Comments ----> Src Seq".into(),
        "Number ....1....+....2....+<-------- 26 - 35 -------->....4....+....5....+....6....+....7....+....8....+....9....+...10 Id  Number".into(),
    )
}

/// The `INDENT` marks, when the source listing is indented.
fn indent_marks(c: &Compiled) -> Option<String> {
    let v = c.ctl.indent.trim();
    if v.is_empty() || v.eq_ignore_ascii_case("*NONE") {
        return None;
    }
    Some(v.trim_matches('\'').to_string())
}

/// A fixed-form calculation under `INDENT`: the operation in a wider
/// field after one mark per enclosing level, no nesting column or date.
fn indented(l: &Line, no: Option<u32>, mark: char, info: &Info, srcid: &str, marks: &str) -> String {
    let no = no.map_or_else(|| "------".to_string(), |n| n.to_string());
    let chars: Vec<char> = l.text.chars().collect();
    let cols = |from: usize, to: usize| -> String { (from..=to).map(|k| chars.get(k - 1).copied().unwrap_or(' ')).collect() };
    let depth = match info.level.as_bytes() {
        [b'B' | b'X' | b'E', ..] => info.level[1..].parse::<usize>().unwrap_or(1).saturating_sub(1),
        [] => 0,
        _ => info.level.parse::<usize>().unwrap_or(0),
    };
    let op = cols(26, 35).trim_end().to_string();
    let field = if op.trim().is_empty() { String::new() } else { format!("{}{}", marks.repeat(depth), op.trim_start()) };
    let mut rest = cols(36, 100);
    if !xf2_op(&l.text) {
        let mut all: String = cols(6, 100);
        all = dashes(&all);
        rest = all.chars().skip(30).collect();
    }
    let text = format!("{}{field:<27}{rest}", cols(6, 25));
    format!("{no:>6}{mark}{text:<112} {srcid:>2}  {}", l.seq)
}

/// The rulers of compile-time data.
fn data_rulers() -> (String, String) {
    (
        "Line   <---------------------- Data Records -------------------------------------------------------------->      Change Src Seq".into(),
        "Number ....+....1....+....2....+....3....+....4....+....5....+....6....+....7....+....8....+....9....+...10      Date   Id  Number".into(),
    )
}

/// Where a diagnostic prints.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    /// After its statement.
    Stmt,
    /// After its data structure or prototype group.
    Group,
    /// After the global declarations.
    Decls,
    /// In the additional diagnostics.
    Additional,
    /// At the end of the source: a group never closed.
    End,
    /// Before the end of the procedure it is in.
    ProcEnd,
}

/// Where IBM prints a diagnostic: the checks made after the source is
/// read go to the additional diagnostics.
pub(crate) fn place_in(c: &Compiled, d: &Diag) -> Place {
    // A structure or prototype left open: at the end.
    if d.msgid == "RNF3556" {
        let closed = c.read.lines.iter().find(|l| l.no == d.at.line).is_some_and(|l| l.text.to_ascii_lowercase().contains("end-"));
        return if closed { Place::Group } else { Place::End };
    }
    // DEALLOC's operand is checked with the statement.
    if d.msgid == "RNF7261"
        && c.read.lines.iter().find(|l| l.no == d.at.line).is_some_and(|l| l.text.trim_start().to_ascii_uppercase().starts_with("DEALLOC"))
    {
        return Place::Stmt;
    }
    // A name a declaration names that is not defined: at the end.
    if d.msgid == "RNF7030" {
        let decl = c.read.lines.iter().position(|l| l.no == d.at.line).is_some_and(|i| {
            matches!(stmts::analyze(&c.read.lines)[i].ty, Ty::Spec('D' | 'H' | 'F'))
        });
        // When the keyword that holds it was dropped.
        let dropped = c.diags.iter().any(|e| e.msgid == "RNF3308" && e.at.line == d.at.line);
        if decl && dropped {
            return Place::End;
        }
        // A fixed-form LIKE of a name not defined: at the end (the
        // definition's own goes to the additional diagnostics).
        if let Some(l) = c.read.lines.iter().find(|l| l.no == d.at.line)
            && decl
            && !l.fully_free
            && l.text.chars().nth(5).is_some_and(|ch| ch.eq_ignore_ascii_case(&'D'))
        {
            let own: String = l.text.chars().skip(6).take(15).collect::<String>().trim().to_ascii_uppercase();
            let other = c.diags.iter().any(|e| e.msgid == "RNF7030" && e.at.line == d.at.line && e.args.first().is_some_and(|a| a.eq_ignore_ascii_case(&own)));
            if other && d.args.first().is_some_and(|a| !a.eq_ignore_ascii_case(&own)) {
                return Place::End;
            }
        }
    }
    place(d)
}

/// Where IBM prints a diagnostic, by its identifier.
pub(crate) fn place(d: &Diag) -> Place {
    if d.at.line == 0 {
        return Place::Additional;
    }
    match d.msgid {
        "RNF2318" => Place::Decls,
        "RNF5177" | "RNF1502" => Place::End,
        "RNF5415" => Place::ProcEnd,
        "RNF3530" | "RNF7301" => Place::Group,
        // Checks of expressions and operations made after the source.
        "RNF0328" | "RNF0329" | "RNF0331" | "RNF0334" | "RNF0336" | "RNF0345" | "RNF0362" | "RNF0365" | "RNF0371"
        | "RNF0381" | "RNF0528" | "RNF0530" | "RNF0571" | "RNF0580" | "RNF0581" | "RNF0599" | "RNF0655" | "RNF0752"
        | "RNF5159" | "RNF5191" | "RNF5202" | "RNF5276" | "RNF5343" | "RNF5403" | "RNF6029" | "RNF0203" | "RNF0224"
        | "RNF0574" | "RNF5352" | "RNF0337" | "RNF0342" | "RNF0507" | "RNF0556" | "RNF0565" | "RNF0587" | "RNF5123"
        | "RNF0579" | "RNF5184" | "RNF0592" | "RNF0386" | "RNF0364" => {
            Place::Additional
        }
        id if id.starts_with("RNF7") => Place::Additional,
        _ => Place::Stmt,
    }
}

/// The listing's statement numbers: lines left out by conditional
/// compilation have none; generated lines take numbers before the line
/// they precede.
pub(crate) struct Numbers {
    /// Each source line's number.
    pub of: Vec<Option<u32>>,
    /// Each generated block's first number.
    pub blocks: Vec<u32>,
}

pub(crate) fn numbers(c: &Compiled, plan: &[genlines::Block]) -> Numbers {
    let mut n = 0u32;
    let mut of = Vec::with_capacity(c.read.lines.len());
    let mut blocks = Vec::new();
    for l in &c.read.lines {
        for b in plan.iter().filter(|b| b.at == l.no) {
            blocks.push(n + 1);
            n += u32::try_from(b.rows.iter().filter(|r| matches!(r, genlines::Row::Line(..))).count()).unwrap_or(0);
        }
        if l.class == Class::Skipped || super::unnumbered(l) {
            of.push(None);
        } else {
            n += 1;
            of.push(Some(n));
        }
    }
    let last_no = c.read.lines.last().map_or(0, |l| l.no);
    for b in plan.iter().filter(|b| b.at > last_no) {
        blocks.push(n + 1);
        n += u32::try_from(b.rows.iter().filter(|r| matches!(r, genlines::Row::Line(..))).count()).unwrap_or(0);
    }
    Numbers { of, blocks }
}

/// One diagnostic record and its text's continuation lines.
pub(crate) fn diag_lines(out: &mut Out, d: &Diag, field: &str, seq: &str) {
    let text = super::lines::wrap_to(&msgs::text(d), TEXT_WIDTH, super::lines::lines(d.msgid, false));
    out.line(' ', format!("*{} {:02} {field:<6} {seq:<6}  {}", d.msgid, d.severity, text[0]));
    for t in &text[1..] {
        out.line(' ', format!("{:TEXT_AT$}{t}", ""));
    }
}

/// The diagnostic's sequence number, by its line.
pub(crate) fn seq_of(c: &Compiled, d: &Diag) -> String {
    // A copied member's line is marked.
    c.read
        .lines
        .iter()
        .find(|l| l.no == d.at.line && d.at.line > 0)
        .map(|l| if l.member > 0 { format!("{}+", l.seq) } else { l.seq.clone() })
        .unwrap_or_default()
}

/// Diagnostics IBM prints without a marker.
fn unmarked(id: &str) -> bool {
    matches!(
        id,
        "RNF5005" | "RNF0256" | "RNF3504" | "RNF5418" | "RNF5177" | "RNF5178" | "RNF7301" | "RNF5501" | "RNF1502" | "RNF5415" | "RNF5094"
            | "RNF0706" | "RNF0702" | "RNF0272" | "RNF0701"
    )
}

/// The column a diagnostic points at: the name it is about when the
/// line holds it, else its own column.
fn marker_col(d: &Diag, l: &Line) -> usize {
    let up = l.text.to_ascii_uppercase();
    match d.msgid {
        // The initial value.
        "RNF3431" | "RNF3430" | "RNF3481" => {
            if let Some(p) = up.find("INZ(") {
                return p + 5;
            }
        }
        // A literal's closing apostrophe.
        "RNF0305" | "RNF0306" => {
            let from = usize::from(d.at.col).saturating_sub(1);
            if let Some(open) = l.text.get(from..).and_then(|t| t.find('\'')) {
                let open = from + open;
                if let Some(close) = l.text.get(open + 1..).and_then(|t| t.find('\'')) {
                    return open + 1 + close + 1;
                }
            }
        }
        // The output record's spacing and skipping entries.
        "RNF6011" if !l.fully_free => return 40,
        // A definition's decimals: the name.
        "RNF0512" if !l.fully_free => {
            if let Some(p) = l.text.get(6..21).and_then(|t| t.find(|c: char| c != ' ')) {
                return p + 7;
            }
        }
        _ => {}
    }
    if let Some(a) = d.args.first().filter(|a| !a.is_empty()) {
        let up = l.text.to_ascii_uppercase();
        let a = a.to_ascii_uppercase();
        // A fixed-form specification's own name, in columns 7-21.
        if !l.fully_free
            && matches!(l.class, Class::Spec('D' | 'd' | 'P' | 'p' | 'F' | 'f'))
            && let Some(n) = up.get(6..21).filter(|n| n.trim() == a)
        {
            return 7 + n.len() - n.trim_start().len();
        }
        let name = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'#' | b'@' | b'$');
        let b = up.as_bytes();
        let mut from = 0;
        while let Some(p) = up[from..].find(&a) {
            let at = from + p;
            let before = at == 0 || !name(b[at - 1]);
            let after = at + a.len() >= b.len() || !name(b[at + a.len()]);
            if before && after {
                // A name in quotes: the literal.
                return if at > 0 && b[at - 1] == b'\'' { at } else { at + 1 };
            }
            from = at + 1;
        }
    }
    usize::from(d.at.col)
}

/// The diagnostics in print order, the marker rows under the last line
/// and each diagnostic's letter.
type Marks<'a> = (Vec<&'a Diag>, Vec<Vec<char>>, Vec<Option<char>>);

fn marks<'a>(c: &Compiled, ds: &[&'a Diag], last: Option<(&Line, u32)>) -> Marks<'a> {
    let mut ds: Vec<&Diag> = ds.to_vec();
    // A statement's own first, then a procedure's, then the declarations'.
    // A definition left open and an unbalanced group come after the
    // statement's own too.
    let prio = |d: &Diag| match place_in(c, d) {
        _ if matches!(d.msgid, "RNF3438" | "RNF3556") => 1,
        Place::ProcEnd | Place::Group => 1,
        Place::Decls => 2,
        _ => 0,
    };
    // In the order of their markers on the line.
    let mcol = |d: &Diag| match last {
        Some((l, _)) if l.no == d.at.line => {
            let named = marker_col(d, l);
            super::markers::place(d, l, usize::from(d.at.col), named).map_or(named, |p| p.0)
        }
        _ => usize::from(d.at.col),
    };
    ds.sort_by_key(|d| (prio(d), d.at.line, mcol(d)));
    // Markers: one letter per diagnostic on the last line printed.
    let mut rows: Vec<Vec<char>> = Vec::new();
    let mut letters: Vec<Option<char>> = Vec::new();
    let mut next = b'a';
    for d in &ds {
        let on_last = last.is_some_and(|(l, _)| {
            l.no == d.at.line && (marker_col(d, l) > 0 || super::markers::place(d, l, 0, 0).is_some())
        }) && (!unmarked(d.msgid)
            || (d.msgid == "RNF0256" && super::procs_lost(c))
            || (d.msgid == "RNF5005" && last.is_some_and(|(l, _)| l.text.trim_start().to_ascii_uppercase().starts_with("EXSR"))));
        if !on_last {
            letters.push(None);
            continue;
        }
        let (l, _) = last.expect("a line");
        let base = if l.fully_free { 1 } else { 6 };
        let named = marker_col(d, l);
        let placed = super::markers::place(d, l, usize::from(d.at.col), named);
        let col = placed.map_or(named, |p| p.0);
        if col < base {
            letters.push(None);
            continue;
        }
        let at = col - base;
        let chars: Vec<char> = l.text.chars().collect();
        let len = match (placed, d.msgid) {
            (Some((_, n)), _) => n,
            (_, "RNF5508" | "RNF0305" | "RNF0306") => 1,
            (_, "RNF6011") if !l.fully_free => 12,
            (_, "RNF3431" | "RNF3430" | "RNF3481") => l.text[col - 1..].find(')').unwrap_or(1).max(1),
            _ => super::markers::token(&chars, col - 1),
        };
        let ch = char::from(next);
        next = next.saturating_add(1);
        letters.push(Some(ch));
        let row = rows.iter_mut().find(|r| r.len() <= at || r[at..(at + len).min(r.len())].iter().all(|c| *c == ' '));
        let row = match row {
            Some(r) => r,
            None => {
                rows.push(Vec::new());
                rows.last_mut().expect("a row")
            }
        };
        if row.len() < at + len {
            row.resize(at + len, ' ');
        }
        for slot in &mut row[at..at + len] {
            *slot = ch;
        }
    }
    (ds, rows, letters)
}

/// Print the diagnostics released after a line: markers for those on
/// it, then each record.
fn release(out: &mut Out, c: &Compiled, ds: &[&Diag], last: Option<(&Line, u32)>, nums: &Numbers) {
    if ds.is_empty() {
        return;
    }
    let (ds, rows, letters) = marks(c, ds, last);
    for r in &rows {
        out.line(' ', format!("======>{}", r.iter().collect::<String>()));
    }
    let _ = nums;
    for (d, letter) in ds.iter().zip(letters) {
        let field = match letter {
            Some(ch) => ch.to_string(),
            None if c.srcstmt() => String::new(),
            None => format!("{:>6}", c.listing_line(d.at.line)),
        };
        diag_lines(out, d, &field, &seq_of(c, d));
    }
}

/// A source record.
fn record(l: &Line, no: Option<u32>, mark: char, info: &Info, srcid: &str) -> String {
    // A skipped line shown has dashes for its number.
    let no = if srcstmt() && no.is_some() { l.seq.clone() } else { no.map_or_else(|| "------".to_string(), |n| n.to_string()) };
    if l.fully_free {
        let text: String = l.text.chars().take(100).collect();
        format!("{no:>6}{mark}{text:<100}{:>4}  {:<6}{srcid:>5}{}", info.level, l.date, l.seq)
    } else {
        let mut text: String = l.text.chars().skip(5).take(95).collect();
        if matches!(l.class, Class::Spec('C' | 'c')) && !xf2_op(&l.text) {
            text = dashes(&text);
        }
        // Columns 1-5 print in the page and line column.
        let area: String = l.text.chars().take(5).collect();
        format!("{no:>6}{mark}{text:<95}{:>4} {area:<5} {:<6}{srcid:>5}{}", info.level, l.date, l.seq)
    }
}

/// Whether a fixed calculation's operation takes the extended factor 2
/// (or none: a continuation line).
fn xf2_op(t: &str) -> bool {
    let op: String = t.chars().skip(25).take(10).collect::<String>().trim().to_ascii_uppercase();
    let op = op.split('(').next().unwrap_or("").to_string();
    op.is_empty()
        || matches!(
            op.as_str(),
            "EVAL" | "EVALR" | "EVAL-CORR" | "IF" | "ELSEIF" | "DOW" | "DOU" | "WHEN" | "FOR" | "CALLP" | "RETURN" | "ON-ERROR" | "SORTA" | "DSPLY" | "XML-INTO" | "XML-SAX" | "DATA-INTO" | "DATA-GEN" | "SND-MSG"
        )
}

/// A calculation's resulting indicators: unused positions as `--`
/// when any is used.
fn dashes(text: &str) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    // Columns 71-76, from column 6.
    if chars.len() < 71 {
        chars.resize(71, ' ');
    }
    let area: String = chars[65..71].iter().collect();
    if area.trim().is_empty() {
        return text.to_string();
    }
    for k in [65, 67, 69] {
        if chars[k] == ' ' && chars[k + 1] == ' ' {
            chars[k] = '-';
            chars[k + 1] = '-';
        }
    }
    chars.into_iter().collect::<String>().trim_end().to_string()
}

/// Whether a line prints in the source section.
fn shown(l: &Line, c: &Compiled) -> bool {
    let has = |x: &str| c.ctl.option.iter().any(|v| v.eq_ignore_ascii_case(x));
    match l.class {
        Class::Skipped => has("*SHOWSKP"),
        Class::CtHeader(..) | Class::CtData => false,
        _ => l.member == 0 || !has("*NOSHOWCPY"),
    }
}

pub(crate) fn source(out: &mut Out, c: &Compiled, o: &Options) {
    let lines = &c.read.lines;
    let main_free = lines.first().is_some_and(|l| l.fully_free);
    out.new_page(Some(if main_free { free_rulers() } else { fixed_rulers() }));
    out.line('0', format!("{:26}{}", "", spaced("Source Listing")));
    let index_of = |line: u32| lines.iter().position(|l| l.no == line);
    let info = stmts::analyze(lines);
    let plan = genlines::plan(c);
    let nums = numbers(c, &plan);
    let mut next_id = 1u32;
    let mut member_ids: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
    let likerec_ends: Vec<usize> = c
        .module
        .defs
        .iter()
        .chain(c.module.procs.iter().flat_map(|p| p.defs.iter()))
        .filter(|d| d.kind == rpgfront::ast::DefKind::Ds && d.kw("LIKEREC").is_some())
        .filter_map(|d| index_of(d.at.line).map(|i| info[i].stmt_end))
        .collect();
    // The match field table, after the first record that has match fields.
    let match_after = c
        .module
        .inputs
        .iter()
        .find(|r| !r.external && r.fields.iter().any(|f| !f.matching.is_empty()))
        .and_then(|r| r.fields.last())
        .and_then(|f| index_of(f.at.line))
        .map(|i| info[i].stmt_end);
    // The boxes after externally described files' declarations.
    let mut file_boxes: BTreeMap<usize, Vec<&crate::files::File>> = BTreeMap::new();
    let expdds = !c.ctl.option.iter().any(|x| x.eq_ignore_ascii_case("*NOEXPDDS"));
    // A file parameter has none.
    for f in c.files.iter().filter(|f| f.ext.is_some() && expdds && !f.param) {
        if let Some(i) = index_of(f.def_line) {
            file_boxes.entry(info[i].stmt_end).or_default().push(f);
        }
    }
    // Each diagnostic's release: after a line, or after the declarations.
    let mut after: BTreeMap<usize, Vec<&Diag>> = BTreeMap::new();
    let mut decls: Vec<&Diag> = Vec::new();
    let mut at_end: Vec<&Diag> = Vec::new();
    let mut late: Vec<&Diag> = Vec::new();
    for d in &c.diags {
        let p = place_in(c, d);
        if p == Place::Additional {
            continue;
        }
        if p == Place::Decls {
            decls.push(d);
            continue;
        }
        // INZ(*LIKEDS) without LIKEDS: after the first calculation.
        if d.msgid == "RNF3504" {
            late.push(d);
            continue;
        }
        match index_of(d.at.line) {
            Some(_) if p == Place::End => at_end.push(d),
            Some(i) if p == Place::ProcEnd => {
                let end = (i..lines.len()).find(|&k| info[k].starts && info[k].ty == Ty::EndProc);
                match end {
                    Some(e) => after.entry(e.saturating_sub(1)).or_default().push(d),
                    None => at_end.push(d),
                }
            }
            // Text past column 80 prints after its own line.
            Some(i) if d.msgid == "RNF5501" => after.entry(i).or_default().push(d),
            Some(i) => {
                let r = if p == Place::Group { info[i].group_end } else { info[i].stmt_end };
                after.entry(r).or_default().push(d);
            }
            None => at_end.push(d),
        }
    }
    // The first statement that is not a control specification, and
    // the first that is not a declaration.
    let first_not = |pred: &dyn Fn(Ty) -> bool| {
        (0..lines.len()).find(|&i| info[i].starts && lines[i].member == 0 && !matches!(info[i].ty, Ty::None) && pred(info[i].ty))
    };
    // The box shows when a control keyword sets a compile option.
    let has_h = c.module.ctl.iter().any(|k| !matches!(k.name.to_ascii_uppercase().as_str(), "NOMAIN" | "MAIN"));
    let first_not_h = first_not(&|t| t != Ty::Spec('H'));
    let gen_before: Vec<u32> = plan.iter().filter(|b| b.kind != 'D').map(|b| b.at).collect();
    let first_body = first_not(&|t| matches!(t, Ty::Spec('C' | 'I' | 'O' | 'P') | Ty::EndProc));
    let first_body = match (first_body, gen_before.iter().filter_map(|at| index_of(*at)).min()) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    for d in late {
        match first_body {
            Some(b) => after.entry(info[b].stmt_end).or_default().push(d),
            None => at_end.push(d),
        }
    }
    let mut prev = 'H';
    let mut after_proc = false;
    let mut in_proc = false;
    let indent = indent_marks(c);
    let mut in_calcs = false;
    let mut prev_printed: Option<(&Line, u32)> = None;
    let mut first = true;
    let mut box_done = !has_h;
    let mut decls_done = decls.is_empty();
    let mut cur_member = 0usize;
    let mut deferred: Vec<&Diag> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        // Between the line before and this one: the rulers of the
        // member it is in.
        // A copied member's box prints under the fixed-form rulers, its
        // lines under their own.
        if l.member != cur_member && !matches!(l.class, Class::Skipped | Class::CtHeader(..) | Class::CtData) {
            let entering = l.depth > lines.get(i.wrapping_sub(1)).map_or(0, |p| p.depth);
            if entering && !out.rulers_are(&fixed_rulers()) {
                out.change_rulers(fixed_rulers());
            }
            if entering && shown(l, c) {
                let id = *member_ids.entry(l.member).or_insert_with(|| {
                    next_id += 1;
                    next_id - 1
                });
                copy_box(out, c, l, id);
            }
            // What the copy directive released prints after the box.
            if entering {
                release(out, c, &std::mem::take(&mut deferred), prev_printed, &nums);
            }
            if shown(l, c) {
                let r = if l.fully_free { free_rulers() } else { fixed_rulers() };
                if !out.rulers_are(&r) {
                    out.change_rulers(r);
                }
            }
            cur_member = l.member;
        }
        if !deferred.is_empty() {
            release(out, c, &std::mem::take(&mut deferred), prev_printed, &nums);
        }
        // `**END-FREE` and `**FREE` in a fully free member switch the
        // rulers.
        if i > 0 && l.member == lines[i - 1].member && l.fully_free != lines[i - 1].fully_free && shown(l, c) {
            out.change_rulers(if l.fully_free { free_rulers() } else { fixed_rulers() });
        }
        if Some(i) == first_not_h && !box_done {
            prologue::options_box(out, c, o);
            box_done = true;
        }
        // A structure's generated subfields come before what is due
        // after the declarations.
        if shown(l, c) {
            for (bi, b) in plan.iter().enumerate().filter(|(_, b)| b.at == l.no && b.kind == 'D') {
                let rulers = l.fully_free.then(|| (fixed_rulers(), free_rulers()));
                prev = 'D';
                genlines::print(out, b, nums.blocks[bi], &mut next_id, ' ', rulers);
            }
        }
        if Some(i) == first_body && !decls_done {
            release(out, c, &decls, prev_printed, &nums);
            decls_done = true;
        }
        if l.class == Class::Skipped && !c.ctl.option.iter().any(|x| x.eq_ignore_ascii_case("*SHOWSKP")) {
            if lines.get(i + 1).is_none_or(|n| n.class != Class::Skipped) {
                let n = lines[..=i].iter().rev().take_while(|x| x.class == Class::Skipped).count();
                out.line(' ', format!("{:12}LINES EXCLUDED: {n}", ""));
            }
            continue;
        }
        match &l.class {
            Class::Directive(rpgfront::reader::Directive::Space(n)) => out.space(*n),
            Class::Directive(rpgfront::reader::Directive::Eject) => out.eject(),
            _ => {}
        }
        if !shown(l, c) || super::unnumbered(l) {
            // Diagnostics due after a line not printed still print.
            if let Some(ds) = after.get(&i) {
                release(out, c, ds, prev_printed, &nums);
            }
            continue;
        }
        for (bi, b) in plan.iter().enumerate().filter(|(_, b)| b.at == l.no && b.kind != 'D') {
            let t = b.kind;
            let cc = if t == prev || t == 'O' || t == 'D' { ' ' } else { '-' };
            if t != 'O' {
                prev = t;
            }
            let rulers = (b.kind == 'D' && l.fully_free).then(|| (fixed_rulers(), free_rulers()));
            genlines::print(out, b, nums.blocks[bi], &mut next_id, cc, rulers);
        }
        let inf = &info[i];
        let mut cc = ' ';
        // A declaration out of sequence is ignored: it takes no type.
        let ignored = c.diags.iter().any(|d| d.msgid == "RNF0724" && d.at.line == l.no);
        if inf.starts && !ignored {
            match inf.ty {
                // Calculations between procedures are ignored: no space.
                Ty::Spec(t) if after_proc && t == 'C' => {}
                Ty::Spec(t) => {
                    // A procedure after the output specifications is not
                    // set off.
                    if t != prev {
                        if !(t == 'P' && prev == 'O') {
                            cc = '-';
                        }
                        prev = t;
                    }
                    if t == 'P' {
                        after_proc = false;
                        in_proc = true;
                    }
                }
                // The end of a procedure spaces only when none is open.
                Ty::EndProc => {
                    if !in_proc && prev != 'P' {
                        cc = '-';
                    }
                    prev = 'P';
                    after_proc = true;
                    in_proc = false;
                }
                Ty::None => {}
            }
        }
        let mark = if l.member > 0 { '+' } else { ' ' };
        let srcid = if l.member > 0 {
            member_ids
                .entry(l.member)
                .or_insert_with(|| {
                    next_id += 1;
                    next_id - 1
                })
                .to_string()
        } else {
            String::new()
        };
        let calc = l.member == 0 && matches!(inf.ty, Ty::Spec('C'));
        let mut switched = false;
        if indent.is_some() {
            // The calculations print under their own rulers.
            // INDENT from the control keywords: IBM's rulers for the
            // calculations are blank.
            if calc && inf.starts && !in_calcs && (l.fully_free || matches!(l.class, Class::Spec(_))) {
                let blank = o.indent.eq_ignore_ascii_case("*NONE");
                out.change_rulers_before(if blank { (String::new(), String::new()) } else { indent_rulers() }, 1);
                in_calcs = true;
                switched = true;
            } else if in_calcs && inf.starts && matches!(inf.ty, Ty::Spec('O' | 'P') | Ty::EndProc) {
                out.change_rulers(fixed_rulers());
                in_calcs = false;
            }
        }
        let text = match &indent {
            Some(m) if calc && !l.fully_free && matches!(l.class, Class::Spec(_)) => indented(l, nums.of[i], mark, inf, &srcid, m),
            _ => record(l, nums.of[i], mark, inf, &srcid),
        };
        // The rulers took the spacing.
        if switched {
            cc = ' ';
        }
        // After a hidden block's blank, a change of type spaces twice.
        if std::mem::take(&mut out.after_blank) && cc == '-' {
            cc = '0';
        }
        if first {
            if cc == '-' {
                out.line('-', "");
                cc = ' ';
            } else {
                cc = '0';
            }
            first = false;
        }
        let last = Some((l, nums.of[i].unwrap_or(0)));
        // The declarations' diagnostics due before the next line print
        // with this line's.
        let mut ds: Vec<&Diag> = after.get(&i).cloned().unwrap_or_default();
        let d_next = lines.get(i + 1).is_some_and(|n| plan.iter().any(|b| b.at == n.no && b.kind == 'D'));
        if first_body == Some(i + 1) && !decls_done && !ds.is_empty() && !d_next {
            ds.extend(decls.iter().copied());
            decls_done = true;
        }
        out.line(cc, text);
        prev_printed = last;
        if lines.get(i + 1).is_some_and(|n| n.depth > l.depth) {
            deferred = ds;
        } else {
            release(out, c, &ds, last, &nums);
        }
        // A group left out with no lines in it.
        if c.read.excluded.contains(&l.no)
            && lines.get(i + 1).is_some_and(|n| n.class != Class::Skipped)
            && !c.ctl.option.iter().any(|x| x.eq_ignore_ascii_case("*SHOWSKP"))
        {
            out.line(' ', format!("{:12}LINES EXCLUDED: 0", ""));
        }
        for f in file_boxes.get(&i).into_iter().flatten() {
            genlines::file_box(out, c, f);
        }
        if Some(i) == match_after {
            match_table(out, c);
        }
        // A LIKEREC structure's subfields are generated without lines:
        // only the rulers change and change back.
        if l.fully_free && likerec_ends.contains(&i) {
            out.change_rulers(fixed_rulers());
            out.change_rulers(free_rulers());
        }
    }
    if !decls_done {
        release(out, c, &decls, prev_printed, &nums);
    }
    release(out, c, &at_end, prev_printed, &nums);
    let last_no = lines.last().map_or(0, |l| l.no);
    for (bi, b) in plan.iter().enumerate().filter(|(_, b)| b.at > last_no) {
        let cc = if b.kind == prev || b.kind == 'O' || b.kind == 'D' { ' ' } else { '-' };
        genlines::print(out, b, nums.blocks[bi], &mut next_id, cc, None);
    }
    // After indented calculations the fixed-form rulers come back.
    let r = if main_free && indent.is_none() { free_rulers() } else { fixed_rulers() };
    if !out.rulers_are(&r) {
        out.change_rulers(r);
    }
    out.end_line('0', format!("{:7}{}", "", "* * * * *   E N D   O F   S O U R C E   * * * * *"));
}

/// The match levels and their field lengths.
fn match_table(out: &mut Out, c: &Compiled) {
    let mut levels: Vec<(String, u32)> = Vec::new();
    for f in c.module.inputs.iter().filter(|r| !r.external).flat_map(|r| r.fields.iter()) {
        if f.matching.is_empty() || levels.iter().any(|(l, _)| *l == f.matching) {
            continue;
        }
        let from: u32 = f.from.trim().parse().unwrap_or(0);
        let to: u32 = f.to.trim().parse().unwrap_or(0);
        levels.push((f.matching.clone(), (to + 1).saturating_sub(from)));
    }
    levels.sort();
    out.line(' ', prologue::rule(8, 101));
    out.line(' ', prologue::framed(8, " Match Field Table:", 101));
    out.line(' ', prologue::framed(8, &format!("   Match level{:19}Field length", ""), 101));
    for (l, n) in levels {
        out.line(' ', prologue::framed(8, &format!("     {l:<2}{n:>34}"), 101));
    }
    out.line(' ', prologue::rule(8, 101));
}

/// The box before a copied member's first line.
fn copy_box(out: &mut Out, c: &Compiled, l: &Line, id: u32) {
    let Some(m) = c.read.members.get(l.member) else { return };
    out.line(' ', prologue::rule(8, 101));
    for (label, value) in [
        ("RPG member name", m.name.clone()),
        ("External name", format!("{}/{}({})", m.lib, m.file, m.name)),
        ("Last change", "dd-dd-dd  tt:tt:tt".to_string()),
    ] {
        let inner = format!("{}  {value}", super::leader(1, label, 28));
        out.line(' ', format!("{}{:>22}", prologue::framed(8, &inner, 101), id));
    }
    out.line(' ', prologue::rule(8, 101));
}

/// The compile-time data, after the additional diagnostics.
pub(crate) fn ctdata(out: &mut Out, c: &Compiled) {
    let lines: Vec<(usize, &Line)> = c
        .read
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| matches!(l.class, Class::CtHeader(..) | Class::CtData))
        .collect();
    if lines.is_empty() {
        return;
    }
    let nums = numbers(c, &genlines::plan(c));
    out.new_page(Some(data_rulers()));
    out.line(' ', format!("{:23}{}", "", spaced("Compile Time Data")));
    let mut unnamed = 0;
    for (i, l) in lines {
        let no = nums.of[i].map_or(String::new(), |n| n.to_string());
        let text: String = l.text.chars().take(100).collect();
        let row = format!("{no:>6} {text:<100}      {:<6}     {}", l.date, l.seq);
        if let Class::CtHeader(kind, name) = &l.class {
            out.line('0', row);
            if kind.eq_ignore_ascii_case("CTDATA") || kind.is_empty() {
                // An unnamed `**` takes the next compile-time array.
                let name = if name.is_empty() {
                    unnamed += 1;
                    c.table.syms.iter().filter(|s| s.scope == 0 && s.ctdata).nth(unnamed - 1).map(|s| s.name.clone()).unwrap_or_default()
                } else {
                    name.clone()
                };
                ct_box(out, c, &name);
            }
        } else {
            out.line(' ', row);
        }
    }
    out.end_line('0', format!("{:4}{}", "", "* * * * *   E N D   O F   C O M P I L E   T I M E   D A T A   * * * * *"));
}

/// The box naming a compile-time table or array.
fn ct_box(out: &mut Out, c: &Compiled, name: &str) {
    let name = name.to_ascii_uppercase();
    let sym = c.table.syms.iter().find(|s| s.name == name && s.scope == 0);
    let table = sym.is_some_and(|s| s.table || s.name.starts_with("TAB"));
    let alt = c.table.syms.iter().find(|s| s.scope == 0 && !name.is_empty() && s.alt.eq_ignore_ascii_case(&name)).map(|s| s.name.clone());
    let label = if table { "Table . . . :" } else { "Array . . . :" };
    let mut inner = format!(" {label} {name:<10}");
    if let Some(a) = alt {
        let l2 = if table { "Alternating Table  . . . . :" } else { "Alternating Array  . . . . :" };
        inner = format!("{inner}   {l2} {a}");
    }
    out.line(' ', prologue::rule(8, 77));
    out.line(' ', prologue::framed(8, &inner, 77));
    out.line(' ', prologue::rule(8, 77));
}
