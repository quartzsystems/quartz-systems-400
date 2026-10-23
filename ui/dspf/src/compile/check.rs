// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Checks over the whole file once every record is built: subfile
//! pairing and sizes, windows, menu bars, the fields and records that
//! record keywords name, positions and overlaps on every display size,
//! then the buffers.

use dds::diag::Diagnostics;

use crate::diag::ids;
use crate::kw::{self, SflSite};
use crate::layout;
use crate::model::{
    DisplayFile, DisplaySize, Field, KeywordUse, Record, RecordKind, SflRole, TypeClass, Usage,
    WindowOrigin,
};

/// Check and finish the file.
pub(crate) fn file(df: &mut DisplayFile, diags: &mut Diagnostics) {
    subfiles(df, diags);
    windows(df, diags);
    menus(df, diags);
    references(df, diags);
    positions(df, diags);
    msgloc(df, diags);
    let file_responses: Vec<u8> = df
        .file
        .keys
        .iter()
        .filter_map(|k| k.ind)
        .chain(df.file.responses.iter().map(|r| r.0))
        .collect();
    for r in &mut df.records {
        if r.fields.is_empty() {
            diags.push(ids::empty_record(r.line, &r.name));
        }
        layout::lay_out(r, df.file.indara, &file_responses);
    }
}

/// A field with a subfile role.
fn has_role(r: &Record, want: fn(&SflRole) -> bool) -> bool {
    r.fields
        .iter()
        .any(|f| f.sfl_role.as_ref().is_some_and(want))
}

#[allow(clippy::too_many_lines)] // every subfile rule in one place
fn subfiles(df: &mut DisplayFile, diags: &mut Diagnostics) {
    let sizes = df.file.sizes.clone();
    let n = df.records.len();
    for i in 0..n {
        for f in &df.records[i].fields {
            for k in &f.keywords {
                let Some(info) = kw::lookup(&k.name) else {
                    continue;
                };
                match info.sfl {
                    SflSite::Ctl if df.records[i].kind != RecordKind::SubfileControl => {
                        diags.push(ids::sfl_keyword_site(k.line, &k.name, "subfile control"));
                    }
                    SflSite::Sfl if df.records[i].kind != RecordKind::Subfile => {
                        diags.push(ids::sfl_keyword_site(k.line, &k.name, "subfile"));
                    }
                    _ => {}
                }
            }
        }
        match df.records[i].kind {
            RecordKind::Subfile => {
                let name = df.records[i].name.clone();
                let paired = df.records.get(i + 1).is_some_and(|next| {
                    next.kind == RecordKind::SubfileControl
                        && next.subfile.as_ref().is_some_and(|s| s.record == name)
                });
                if !paired {
                    diags.push(ids::sfl_without_ctl(df.records[i].line, &name));
                }
            }
            RecordKind::SubfileControl => {
                let line = df.records[i].line;
                let target = df.records[i]
                    .subfile
                    .as_ref()
                    .map(|s| s.record.clone())
                    .unwrap_or_default();
                let prev = i.checked_sub(1).map(|p| &df.records[p]);
                let Some(sfl) = prev.filter(|p| p.kind == RecordKind::Subfile && p.name == target)
                else {
                    diags.push(ids::sflctl_target(line, &target));
                    continue;
                };
                let extents: Vec<Option<(u8, u8)>> =
                    (0..sizes.len()).map(|j| sfl.extent(j)).collect();
                let message = sfl.msgrcd.is_some();
                let sfl_line = sfl.line;
                let sfl_ok = has_role(sfl, |r| matches!(r, SflRole::MsgKey))
                    && has_role(sfl, |r| matches!(r, SflRole::PgmQ));
                let ctl_pgmq = has_role(&df.records[i], |r| matches!(r, SflRole::PgmQ));
                let ctl_fields: Vec<(usize, String, Vec<Option<u8>>)> = df.records[i]
                    .fields
                    .iter()
                    .filter(|f| f.is_displayed())
                    .map(|f| {
                        (
                            f.line,
                            f.label(),
                            (0..sizes.len())
                                .map(|j| f.position(j).map(|p| p.row))
                                .collect(),
                        )
                    })
                    .collect();
                let Some(s) = df.records[i].subfile.as_mut() else {
                    continue;
                };
                s.message = message;
                s.record_lines = extents
                    .iter()
                    .flatten()
                    .map(|(lo, hi)| hi - lo + 1)
                    .max()
                    .unwrap_or(1);
                s.first_line = extents.iter().map(|e| e.map_or(0, |(lo, _)| lo)).collect();
                for (j, size) in sizes.iter().enumerate() {
                    let sz = s.size(j);
                    if sz.size == 0 {
                        diags.push(ids::sfl_size_missing(line, "SFLSIZ"));
                    }
                    if sz.page == 0 {
                        diags.push(ids::sfl_size_missing(line, "SFLPAG"));
                    }
                    if sz.size != 0 && sz.page > sz.size {
                        diags.push(ids::sfl_size(
                            line,
                            &format!(
                                "SFLPAG {} exceeds SFLSIZ {} on {}",
                                sz.page, sz.size, size.name
                            ),
                        ));
                    }
                    if s.lin.is_some() {
                        continue;
                    }
                    let last = s.last_line(j);
                    if last > u32::from(size.rows) {
                        diags.push(ids::sfl_page_fit(
                            line, &s.record, last, size.rows, &size.name,
                        ));
                    }
                    let first = u32::from(s.first_line[j]);
                    if first == 0 {
                        continue;
                    }
                    for (fline, label, rows) in &ctl_fields {
                        if let Some(Some(row)) = rows.get(j)
                            && (first..=last).contains(&u32::from(*row))
                        {
                            diags.push(ids::ctl_in_page(*fline, label, *row));
                        }
                    }
                }
                if message {
                    if !sfl_ok {
                        diags.push(ids::requires(
                            sfl_line,
                            "SFLMSGRCD",
                            "SFLMSGKEY and SFLPGMQ fields on the subfile record",
                        ));
                    }
                    if !ctl_pgmq {
                        diags.push(ids::requires(
                            line,
                            "SFLCTL",
                            "an SFLPGMQ field on the control record of a message subfile",
                        ));
                    }
                }
            }
            _ => {}
        }
    }
}

fn windows(df: &mut DisplayFile, diags: &mut Diagnostics) {
    let sizes = df.file.sizes.clone();
    for i in 0..df.records.len() {
        let line = df.records[i].line;
        for k in &df.records[i].keywords {
            if matches!(
                k.name.as_str(),
                "RMVWDW" | "USRRSTDSP" | "WDWTITLE" | "WDWBORDER"
            ) && df.records[i].window.is_none()
            {
                diags.push(ids::requires(k.line, &k.name, "WINDOW on the same record"));
            }
        }
        let Some(w) = df.records[i].window.clone() else {
            continue;
        };
        let name = df.records[i].name.clone();
        let (origin, lines, cols) = match &w.origin {
            WindowOrigin::From(target) => {
                let found = df
                    .record(target)
                    .and_then(|r| r.window.as_ref())
                    .filter(|w| !matches!(w.origin, WindowOrigin::From(_)))
                    .cloned();
                let Some(t) = found else {
                    diags.push(ids::record_not_found(
                        line,
                        "WINDOW",
                        target,
                        "window record",
                    ));
                    continue;
                };
                if let Some(mine) = df.records[i].window.as_mut() {
                    mine.lines = t.lines;
                    mine.cols = t.cols;
                }
                (t.origin, t.lines, t.cols)
            }
            other => (other.clone(), w.lines, w.cols),
        };
        for size in &sizes {
            let fits = u32::from(lines) + 2 <= u32::from(size.rows)
                && u32::from(cols) + 4 <= u32::from(size.cols)
                && match origin {
                    WindowOrigin::At(r, c) => {
                        u32::from(r) + u32::from(lines) < u32::from(size.rows)
                            && u32::from(c) + u32::from(cols) + 3 <= u32::from(size.cols)
                    }
                    _ => true,
                };
            if !fits {
                diags.push(ids::window_fit(line, &name, &size.label()));
            }
        }
    }
}

fn menus(df: &DisplayFile, diags: &mut Diagnostics) {
    for r in &df.records {
        if r.kind == RecordKind::MenuBar
            && !r
                .fields
                .iter()
                .any(|f| f.keywords.iter().any(|k| k.name == "MNUBARCHC"))
        {
            diags.push(ids::menu(
                r.line,
                "a MNUBAR record needs a field with MNUBARCHC",
            ));
        }
        for f in &r.fields {
            for k in f.keywords.iter().filter(|k| k.name == "MNUBARCHC") {
                if r.kind != RecordKind::MenuBar {
                    diags.push(ids::menu(
                        k.line,
                        "MNUBARCHC is only valid on a MNUBAR record",
                    ));
                }
                if let Some(name) = k.parms.get(1).and_then(|p| p.as_word())
                    && !df
                        .record(name)
                        .is_some_and(|t| t.kind == RecordKind::PullDown)
                {
                    diags.push(ids::record_not_found(
                        k.line,
                        "MNUBARCHC",
                        name,
                        "pull-down record",
                    ));
                }
            }
        }
        for k in r.keywords.iter().filter(|k| k.name == "MNUBARDSP") {
            if let Some(name) = k
                .parms
                .first()
                .and_then(|p| p.as_word())
                .filter(|w| !w.starts_with('&'))
                && !df
                    .record(name)
                    .is_some_and(|t| t.kind == RecordKind::MenuBar)
            {
                diags.push(ids::record_not_found(
                    k.line,
                    "MNUBARDSP",
                    name,
                    "menu bar record",
                ));
            }
        }
    }
}

/// The shape a keyword wants of a field it names.
struct Want {
    class: TypeClass,
    length: u32,
    decimals: u8,
    text: &'static str,
}

const HIDDEN_A10: Want = Want {
    class: TypeClass::Char,
    length: 10,
    decimals: 0,
    text: "character, 10 positions, hidden",
};
const HIDDEN_A1: Want = Want {
    class: TypeClass::Char,
    length: 1,
    decimals: 0,
    text: "character, 1 position, hidden",
};
const HIDDEN_S5: Want = Want {
    class: TypeClass::Zoned,
    length: 5,
    decimals: 0,
    text: "zoned 5,0, hidden",
};
const HIDDEN_S4: Want = Want {
    class: TypeClass::Zoned,
    length: 4,
    decimals: 0,
    text: "zoned 4,0, hidden",
};
const HIDDEN_S3: Want = Want {
    class: TypeClass::Zoned,
    length: 3,
    decimals: 0,
    text: "zoned 3,0, hidden",
};

fn named_field(
    rec: &Record,
    k: &KeywordUse,
    parm: usize,
    want: Option<&Want>,
    diags: &mut Diagnostics,
) {
    let Some(word) = k.parms.get(parm).and_then(|p| p.as_word()) else {
        return;
    };
    let name = word.strip_prefix('&').unwrap_or(word);
    let Some((_, f)) = rec.field(name) else {
        diags.push(ids::field_not_found(k.line, &k.name, name));
        return;
    };
    if let Some(want) = want
        && !(f.ty.class == want.class
            && f.length == want.length
            && f.decimals == want.decimals
            && f.usage == Usage::Hidden)
    {
        diags.push(ids::field_shape(k.line, &k.name, name, want.text));
    }
}

fn references(df: &DisplayFile, diags: &mut Diagnostics) {
    for r in &df.records {
        for k in &r.keywords {
            match k.name.as_str() {
                "SFLCSRRRN" => named_field(r, k, 0, Some(&HIDDEN_S5), diags),
                "SFLMODE" => named_field(r, k, 0, Some(&HIDDEN_A1), diags),
                "SFLSCROLL" => named_field(r, k, 0, Some(&HIDDEN_S4), diags),
                "CSRLOC" => {
                    named_field(r, k, 0, Some(&HIDDEN_S3), diags);
                    named_field(r, k, 1, Some(&HIDDEN_S3), diags);
                }
                "RTNCSRLOC" => {
                    let special = k.parms[0].as_word().is_some_and(|w| w.starts_with('*'));
                    if special {
                        for i in 1..k.parms.len() {
                            named_field(r, k, i, None, diags);
                        }
                    } else {
                        named_field(r, k, 0, Some(&HIDDEN_A10), diags);
                        named_field(r, k, 1, Some(&HIDDEN_A10), diags);
                        if k.parms.len() > 2 {
                            named_field(r, k, 2, Some(&HIDDEN_S4), diags);
                        }
                    }
                }
                "ERASE" => {
                    for p in &k.parms {
                        if let Some(name) = p.as_word()
                            && df.record(name).is_none()
                        {
                            diags.push(ids::record_not_found(
                                k.line,
                                "ERASE",
                                name,
                                "record format",
                            ));
                        }
                    }
                }
                "SFLMSGID" | "MNUBARDSP" => {
                    for (i, p) in k.parms.iter().enumerate() {
                        if p.as_word().is_some_and(|w| w.starts_with('&')) {
                            named_field(r, k, i, None, diags);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// One displayed field's span on a size, the attribute byte included.
struct Span {
    start: u32,
    end: u32,
    line: usize,
    label: String,
}

fn positions(df: &DisplayFile, diags: &mut Diagnostics) {
    for r in &df.records {
        for (j, size) in df.file.sizes.iter().enumerate() {
            // A pull-down is shown in a window the system sizes to it.
            let (width, height, in_window) = match &r.window {
                Some(w) if w.lines > 0 => (w.cols, w.lines, true),
                _ if r.kind == RecordKind::PullDown => (size.cols - 4, size.rows - 2, true),
                _ => (size.cols, size.rows, false),
            };
            let mut spans = Vec::new();
            for f in r.fields.iter().filter(|f| f.is_displayed()) {
                if let Some(s) = span(f, j, size, width, height, in_window, diags) {
                    spans.push(s);
                }
            }
            spans.sort_by_key(|s| s.start);
            let mut reach: Option<&Span> = None;
            for s in &spans {
                if let Some(prev) = reach
                    && s.start <= prev.end
                {
                    diags.push(ids::overlap(s.line, &s.label, &prev.label, &size.label()));
                }
                if reach.is_none_or(|p| s.end > p.end) {
                    reach = Some(s);
                }
            }
        }
    }
}

fn span(
    f: &Field,
    j: usize,
    size: &DisplaySize,
    width: u8,
    height: u8,
    in_window: bool,
    diags: &mut Diagnostics,
) -> Option<Span> {
    let p = f.position(j)?;
    let label = f.label();
    let where_ = if in_window {
        format!("the window on {}", size.label())
    } else {
        size.label()
    };
    if p.row > height || p.col > width || p.row == 0 || p.col == 0 {
        diags.push(ids::off_display(
            f.line,
            &label,
            &format!(
                "row {} column {} is outside the {height} lines by {width} columns",
                p.row, p.col
            ),
            &where_,
        ));
        return None;
    }
    let cells = u32::from(width) * u32::from(height);
    let start = (u32::from(p.row) - 1) * u32::from(width) + u32::from(p.col);
    if start == 1 && !in_window {
        diags.push(ids::off_display(
            f.line,
            &label,
            "row 1 column 1 leaves no room for the attribute byte before the field",
            &where_,
        ));
        return None;
    }
    if f.screen_len > cells {
        diags.push(ids::too_long(f.line, &label, f.screen_len, &where_));
        return None;
    }
    let end = start + f.screen_len.max(1) - 1;
    if end > cells {
        diags.push(ids::off_display(
            f.line,
            &label,
            "extends past the last position",
            &where_,
        ));
        return None;
    }
    if f.usage.is_input() && end == cells {
        diags.push(ids::off_display(
            f.line,
            &label,
            "an input field cannot end in the last position; its ending attribute needs one more",
            &where_,
        ));
        return None;
    }
    Some(Span {
        start: start - 1,
        end,
        line: f.line,
        label,
    })
}

fn msgloc(df: &DisplayFile, diags: &mut Diagnostics) {
    let rows = df.file.sizes.iter().map(|s| s.rows).min().unwrap_or(24);
    let check = |k: &KeywordUse, diags: &mut Diagnostics| {
        if k.name == "MSGLOC"
            && let Some(n) = k
                .parms
                .first()
                .and_then(|p| p.as_word())
                .and_then(|w| w.parse::<u32>().ok())
            && n > u32::from(rows)
        {
            diags.push(ids::bad_msgloc(k.line, n, rows));
        }
    };
    for k in &df.file.keywords {
        check(k, diags);
    }
    for r in &df.records {
        for k in &r.keywords {
            check(k, diags);
        }
    }
}
