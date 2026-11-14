// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The lines of a prompt display: one per scalar value, a header for a
//! group, an indented line per qualifier or element, a `+ for more
//! values` line after a list, the `Additional Parameters` title. The
//! geometry is IBM's: the prompt text from column 2 with dot leaders
//! to column 33, the `>` marker of a value given on the command line
//! at 35, the field at 37, the choices after it.

use std::collections::HashMap;

use super::Path;
use crate::def::{Choice, CmdDef, InlPmtLen, Parm, ParmType, PmtCtlRef, RelOp};
use crate::token::Sel;

/// Column of the first character of a prompt text.
pub const LABEL_COL: usize = 2;
/// Column of the last dot leader.
pub const DOTS_END: usize = 33;
/// Column of the `>` marker.
pub const MARKER_COL: usize = 35;
/// Column of the field.
pub const FIELD_COL: usize = 37;
/// First row of parameters.
pub const FIRST_ROW: usize = 5;
/// Rows of parameters on a page.
pub const PAGE_ROWS: usize = 16;
/// The room on the field's first row.
pub const MAX_WIDTH: usize = 44;
/// The field of a long text parameter (it wraps over rows).
pub const LONG_TEXT: usize = 256;
/// The field of a path name.
pub const PATH_WIDTH: usize = 50;

/// One line of the display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    /// Nothing.
    Blank,
    /// A centered title (`Additional Parameters`).
    Title(String),
    /// A group's header: the prompt text and a colon.
    Head {
        /// The text.
        label: String,
        /// Indentation.
        indent: usize,
        /// A list of groups: the one-character field at column 35 that
        /// asks for more entries.
        more: Option<Path>,
    },
    /// A value line.
    Value {
        /// The prompt text, before the dot leaders.
        label: String,
        /// Indentation of the prompt text.
        indent: usize,
        /// Indentation of the field: a qualifier's is indented, an
        /// element's is not.
        field_indent: usize,
        /// The field: width and value; `None` when protected.
        field: Option<(usize, String)>,
        /// The text shown instead of a field (`?*` selective prompting).
        protected: Option<String>,
        /// The choices text (or the keyword under F11).
        choices: String,
        /// The `>` marker: a value from the command line.
        marker: bool,
        /// Where the value belongs.
        path: Path,
    },
    /// `+ for more values` after a list.
    More {
        /// The list parameter.
        path: Path,
        /// Indentation.
        indent: usize,
        /// A list of groups: the text at column 17 and a one-character
        /// field at column 35. A list of values has the text at column
        /// 17 and a field as wide as a value's at column 37, where the
        /// next value can be typed (the primary's prompts).
        groups: bool,
        /// The width of a value's field.
        width: usize,
    },
    /// A row a long field continues on.
    Cont,
}

impl Line {
    /// The rows a field of `width` from column `col` takes.
    #[must_use]
    pub fn rows_of(width: usize, col: usize) -> usize {
        let first = super::COLS + 1 - col.min(super::COLS);
        if width <= first {
            1
        } else {
            1 + (width - first).div_ceil(super::COLS)
        }
    }
}

/// How the display is being looked at.
pub struct View<'a> {
    /// The text of a field.
    pub texts: &'a HashMap<Path, String>,
    /// Parameters given on the command line.
    pub specified: &'a [bool],
    /// Selective-prompt prefixes.
    pub sel: &'a [Option<Sel>],
    /// `F10`: the `*PMTRQS` parameters show.
    pub additional: bool,
    /// `F9`: everything shows.
    pub all: bool,
    /// `F11`: keywords instead of choices.
    pub keywords: bool,
    /// Only the `KEYPARM` parameters show (the first step).
    pub keyparm_only: bool,
    /// How many entries of each list show.
    pub expanded: &'a HashMap<usize, usize>,
    /// The `PMTCTL` conditions are not evaluated yet: the key
    /// parameters' display was left with a key other than Enter.
    pub conds_closed: bool,
}

/// The lines, and whether any parameter waits behind `F10`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lines {
    /// The lines.
    pub items: Vec<Line>,
    /// A `*PMTRQS` parameter exists and is not shown.
    pub has_additional: bool,
    /// Which parameters are shown.
    pub visible: Vec<bool>,
    /// Where a page may break: `true` on the first line of each
    /// parameter's block (a group, a list and the `Additional
    /// Parameters` title stay with their parameter).
    pub starts: Vec<bool>,
}

impl Lines {
    /// The first line of each page: a block that does not fit what is
    /// left of a page starts the next one; one longer than a page is
    /// cut where the page ends.
    #[must_use]
    pub fn pages(&self, rows: usize) -> Vec<usize> {
        let mut out = vec![0];
        let mut used = 0;
        let mut i = 0;
        while i < self.items.len() {
            let mut end = i + 1;
            while end < self.items.len() && !self.starts.get(end).copied().unwrap_or(true) {
                end += 1;
            }
            let len = end - i;
            if used > 0 && used + len > rows && len <= rows {
                out.push(i);
                used = 0;
            }
            for k in i..end {
                if used == rows {
                    out.push(k);
                    used = 0;
                }
                used += 1;
            }
            i = end;
        }
        out
    }

    /// Add a line that starts no block (a source line's fields).
    pub fn push_line(&mut self, at_start: bool, line: Line) {
        if at_start {
            self.items.insert(0, line);
            self.starts.insert(0, true);
        } else {
            self.items.push(line);
            self.starts.push(true);
        }
    }

    fn push_block(&mut self, lines: Vec<Line>) {
        for (k, l) in lines.into_iter().enumerate() {
            self.items.push(l);
            self.starts.push(k == 0);
        }
    }
}

/// Lay the definition out for a view.
#[must_use]
pub fn lines(def: &CmdDef, v: &View<'_>) -> Lines {
    let mut out = Lines::default();
    let mut additional: Vec<(usize, &Parm)> = Vec::new();
    out.visible = vec![false; def.parms.len()];
    let has_keyparm = def.parms.iter().any(|p| p.keyparm);
    let has_pmtrqs = def
        .parms
        .iter()
        .any(|p| matches!(p.pmtctl, Some(PmtCtlRef::PmtRqs)));
    for (i, p) in def.parms.iter().enumerate() {
        if p.constant.is_some() {
            continue;
        }
        if let Some(Sel::Omit | Sel::Hidden) = v.sel.get(i).copied().flatten() {
            continue;
        }
        if v.keyparm_only && has_keyparm && !p.keyparm {
            // IBM offers F10 on the key parameters' display when the
            // command has additional or conditional parameters, unless
            // a prompt override program fills in the rest.
            if p.pmtctl.is_some() && def.pmtovrpgm.is_none() {
                out.has_additional = true;
            }
            continue;
        }
        // A parameter whose condition cannot be decided yet ends the
        // display there: it and those after it show once the
        // controlling parameters have values, or with F10 on a command
        // that has additional parameters, as IBM's prompter does.
        let undecided = matches!(&p.pmtctl, Some(PmtCtlRef::Label { label })
            if v.conds_closed || !pmtctl_decided(def, label, v.texts));
        if undecided && !v.all && (!v.additional || v.conds_closed) {
            if has_pmtrqs {
                out.has_additional = true;
            }
            break;
        }
        let shown = v.all
            || match &p.pmtctl {
                None => true,
                Some(PmtCtlRef::PmtRqs) => {
                    if !v.additional {
                        out.has_additional = true;
                    }
                    v.additional
                }
                Some(PmtCtlRef::Label { .. }) if undecided => true,
                Some(PmtCtlRef::Label { label }) => pmtctl_holds(def, label, v.texts),
            };
        if !shown {
            continue;
        }
        out.visible[i] = true;
        if matches!(p.pmtctl, Some(PmtCtlRef::PmtRqs)) && !v.all {
            additional.push((i, p));
        } else {
            let mut block = Vec::new();
            parm_lines(def, i, p, v, &mut block);
            out.push_block(block);
        }
    }
    for (k, (i, p)) in additional.into_iter().enumerate() {
        let mut block = Vec::new();
        if k == 0 {
            block.push(Line::Blank);
            block.push(Line::Title("Additional Parameters".into()));
            block.push(Line::Blank);
        }
        parm_lines(def, i, p, v, &mut block);
        out.push_block(block);
    }
    out
}

/// Whether a `PMTCTL` chain can be decided: every controlling
/// parameter it compares has a value. IBM shows an undecided
/// parameter only after `F10`.
#[allow(clippy::implicit_hasher)] // one map type throughout the prompter
pub fn pmtctl_decided(def: &CmdDef, label: &str, texts: &HashMap<Path, String>) -> bool {
    def.pmtctl(label).iter().all(|c| {
        let Some(idx) = def.index(&c.ctl) else {
            return true;
        };
        let spcfd = c
            .conds
            .iter()
            .all(|(_, w)| w == "*SPCFD" || w == "*UNSPCFD");
        spcfd
            || texts
                .get(&Path::parm(idx))
                .or_else(|| texts.get(&Path::parm(idx).step(0)))
                .is_some_and(|v| !v.trim().is_empty())
    })
}

/// Whether a `PMTCTL` chain holds for the current values.
#[allow(clippy::implicit_hasher)] // one map type throughout the prompter
pub fn pmtctl_holds(def: &CmdDef, label: &str, texts: &HashMap<Path, String>) -> bool {
    let chain = def.pmtctl(label);
    let mut result = true;
    for (k, c) in chain.iter().enumerate() {
        let Some(idx) = def.index(&c.ctl) else {
            continue;
        };
        let value = texts
            .get(&Path::parm(idx))
            .or_else(|| texts.get(&Path::parm(idx).step(0)))
            .cloned()
            .unwrap_or_default();
        let truths: Vec<bool> = c
            .conds
            .iter()
            .map(|(op, want)| match want.as_str() {
                "*SPCFD" => !value.is_empty(),
                "*UNSPCFD" => value.is_empty(),
                w => compare_text(&value, *op, w),
            })
            .collect();
        let n = truths.iter().filter(|t| **t).count();
        let holds = match c.nbrtrue {
            crate::def::NbrTrue::All => n == truths.len(),
            crate::def::NbrTrue::None => n == 0,
            crate::def::NbrTrue::Count { op, n: want } => op.holds(n.cmp(&usize::from(want))),
        };
        result = match (k, c.lglrel) {
            (0, _) => holds,
            (_, Some(crate::def::LglRel::Or)) => result || holds,
            _ => result && holds,
        };
    }
    result
}

fn compare_text(value: &str, op: RelOp, want: &str) -> bool {
    let v = value.trim().trim_matches('\'').to_ascii_uppercase();
    let w = want.trim().trim_matches('\'').to_ascii_uppercase();
    match (qdecimal::Decimal::parse(&v), qdecimal::Decimal::parse(&w)) {
        (Ok(a), Ok(b)) => op.holds(a.cmp(&b)),
        _ => op.holds(ccsid::ebcdic_cmp_str(&v, &w)),
    }
}

/// The lines of one parameter.
fn parm_lines(def: &CmdDef, i: usize, p: &Parm, v: &View<'_>, out: &mut Vec<Line>) {
    let path = Path::parm(i);
    let sel = v.sel.get(i).copied().flatten();
    let marker = v.specified.get(i).copied().unwrap_or(false);
    let choices = if v.keywords {
        p.kwd.clone()
    } else {
        choices(p)
    };
    item_lines(
        def,
        p,
        &path,
        (0, 0),
        p.prompt_text(),
        sel,
        marker,
        Some(choices),
        v,
        out,
    );
}

/// The lines of a parameter, element or qualifier at `path`.
#[allow(clippy::too_many_arguments)]
fn item_lines(
    def: &CmdDef,
    p: &Parm,
    path: &Path,
    (indent, findent): (usize, usize),
    label: &str,
    sel: Option<Sel>,
    marker: bool,
    choices: Option<String>,
    v: &View<'_>,
    out: &mut Vec<Line>,
) {
    let choices_text = choices.unwrap_or_else(|| self::choices(p));
    if p.is_list() {
        let shown = v.expanded.get(&path.parm).copied().unwrap_or(1).max(1);
        for k in 0..shown {
            let entry = path.step(k);
            let lbl = if k == 0 { label } else { "" };
            if p.ty.is_group() {
                group_or_qual(
                    def,
                    p,
                    &entry,
                    (indent, findent),
                    lbl,
                    sel,
                    marker && k == 0,
                    v,
                    out,
                );
            } else {
                value_line(
                    p,
                    &entry,
                    (indent, findent),
                    lbl,
                    sel,
                    marker && k == 0,
                    choices_text.clone(),
                    v,
                    out,
                );
            }
        }
        out.push(Line::More {
            path: path.clone(),
            indent,
            groups: matches!(p.ty, ParmType::Elem { .. }),
            width: field_width(p).max(1),
        });
        return;
    }
    if p.ty.is_group() {
        group_or_qual(def, p, path, (indent, findent), label, sel, marker, v, out);
        return;
    }
    value_line(
        p,
        path,
        (indent, findent),
        label,
        sel,
        marker,
        choices_text,
        v,
        out,
    );
}

/// A qualified name (the first part on the label's line, the rest
/// indented) or a group of elements (a header, every element
/// indented).
#[allow(clippy::too_many_arguments)]
fn group_or_qual(
    def: &CmdDef,
    p: &Parm,
    path: &Path,
    (indent, findent): (usize, usize),
    label: &str,
    sel: Option<Sel>,
    marker: bool,
    v: &View<'_>,
    out: &mut Vec<Line>,
) {
    match &p.ty {
        ParmType::Qual { .. } => {
            for (k, q) in p.items.iter().enumerate() {
                let lbl = if k == 0 {
                    label.to_string()
                } else {
                    q.prompt_text().to_string()
                };
                let ind = if k == 0 {
                    (indent, findent)
                } else {
                    (indent + 2, findent + 2)
                };
                // The name's row lists the parameter's single values
                // too (`Name, *USRPRF`), as IBM's prompter does.
                let mut c = choices(q);
                if k == 0 && !v.keywords {
                    for s in &p.sngval {
                        if !c.split(", ").any(|x| x == s.from) {
                            if !c.is_empty() {
                                c.push_str(", ");
                            }
                            c.push_str(&s.from);
                        }
                    }
                }
                value_line(
                    q,
                    &path.step(k),
                    ind,
                    &lbl,
                    sel,
                    marker && k == 0,
                    c,
                    v,
                    out,
                );
            }
        }
        ParmType::Elem { .. } => {
            // The first entry of a list of groups carries the field
            // that asks for more entries.
            let more = (p.is_list() && path.steps.last() == Some(&0))
                .then(|| Path::parm(path.parm).step(usize::MAX));
            out.push(Line::Head {
                label: format!("{label}:"),
                indent,
                more,
            });
            for (k, e) in p.items.iter().enumerate() {
                item_lines(
                    def,
                    e,
                    &path.step(k),
                    (indent + 2, findent),
                    e.prompt_text(),
                    sel,
                    marker && k == 0,
                    None,
                    v,
                    out,
                );
            }
        }
        _ => value_line(
            p,
            path,
            (indent, findent),
            label,
            sel,
            marker,
            choices(p),
            v,
            out,
        ),
    }
}

/// One value line.
#[allow(clippy::too_many_arguments)]
fn value_line(
    p: &Parm,
    path: &Path,
    (indent, findent): (usize, usize),
    label: &str,
    sel: Option<Sel>,
    marker: bool,
    choices: String,
    v: &View<'_>,
    out: &mut Vec<Line>,
) {
    let text = v.texts.get(path).cloned().unwrap_or_default();
    let width = field_width(p).max(1);
    let (field, protected) = if sel == Some(Sel::Show) {
        (None, Some(text))
    } else {
        (Some((width, text)), None)
    };
    let rows = if field.is_some() {
        Line::rows_of(width, FIELD_COL + findent)
    } else {
        1
    };
    out.push(Line::Value {
        label: label.to_string(),
        indent,
        field_indent: findent,
        field,
        protected,
        choices,
        marker,
        path: path.clone(),
    });
    out.extend(std::iter::repeat_n(Line::Cont, rows - 1));
}

/// The width of a parameter's field: `INLPMTLEN` when it gives one;
/// else the type's width, widened as IBM's to the longest special,
/// single or listed value (a restricted number is as wide as its
/// longest value).
#[must_use]
pub fn field_width(p: &Parm) -> usize {
    if let Some(InlPmtLen::N { n }) = p.inlpmtlen {
        return usize::from(n).max(1);
    }
    let len = p.length();
    let longest = p
        .spcval
        .iter()
        .chain(&p.sngval)
        .map(|s| s.from.chars().count())
        .chain(p.values.iter().map(|v| v.chars().count()))
        .max()
        .unwrap_or(0);
    let numeric = matches!(
        p.ty,
        ParmType::Dec | ParmType::Int2 | ParmType::Int4 | ParmType::Uint2 | ParmType::Uint4
    );
    if numeric && p.rstd && longest > 0 {
        return longest;
    }
    let base = match p.ty {
        ParmType::Dec | ParmType::X => (len.len as usize + 2).max(4),
        ParmType::Int2 | ParmType::Uint2 => 6,
        ParmType::Int4 | ParmType::Uint4 => 11,
        ParmType::Lgl => 1,
        ParmType::Date => 10,
        ParmType::Time => 8,
        ParmType::Name | ParmType::SName | ParmType::CName => 10,
        ParmType::Generic => 10,
        ParmType::VarName => 11,
        // A path name shows 50 and a long text 256, wrapping onto the
        // rows below; a shorter text shows whole.
        ParmType::PName => (len.len as usize).clamp(1, PATH_WIDTH),
        ParmType::Char if len.len as usize > 80 => LONG_TEXT,
        ParmType::Char => (len.len as usize).max(1),
        _ => (len.len as usize).clamp(1, MAX_WIDTH),
    };
    base.max(longest)
}

/// The choices text of a parameter: the kind of value and its special
/// values, as the right-hand column shows them.
#[must_use]
pub fn choices(p: &Parm) -> String {
    // `CHOICE` text, or `CHOICE(*NONE)`, wins over the values.
    match &p.choice {
        Choice::Text { text } => return text.clone(),
        Choice::None => return String::new(),
        Choice::Values | Choice::Pgm => {}
    }
    let mut parts: Vec<String> = Vec::new();
    match p.ty {
        ParmType::Name | ParmType::SName | ParmType::CName => parts.push("Name".into()),
        ParmType::Generic => parts.push("Name, generic*".into()),
        ParmType::Dec
        | ParmType::Int2
        | ParmType::Int4
        | ParmType::Uint2
        | ParmType::Uint4
        | ParmType::X
            if p.rstd => {}
        ParmType::Dec
        | ParmType::Int2
        | ParmType::Int4
        | ParmType::Uint2
        | ParmType::Uint4
        | ParmType::X => match &p.range {
            Some((lo, hi)) => parts.push(format!("{lo}-{hi}")),
            None => parts.push("Number".into()),
        },
        ParmType::Date => parts.push("Date".into()),
        ParmType::Time => parts.push("Time".into()),
        ParmType::Lgl => parts.push("'0', '1'".into()),
        ParmType::VarName => parts.push("Variable".into()),
        ParmType::Cmd => parts.push("Command".into()),
        ParmType::CmdStr | ParmType::PName | ParmType::Null | ParmType::ZeroElem => {}
        ParmType::Char => {
            if p.values.is_empty() && p.spcval.is_empty() && p.sngval.is_empty() && p.rel.is_none()
            {
                parts.push("Character value".into());
            } else if let Some((lo, hi)) = &p.range {
                parts.push(format!("{lo}-{hi}"));
            }
        }
        ParmType::Elem { .. } | ParmType::Qual { .. } => {}
    }
    for s in &p.sngval {
        parts.push(s.from.clone());
    }
    for v in &p.values {
        parts.push(v.clone());
    }
    for s in &p.spcval {
        if !parts.contains(&s.from) {
            parts.push(s.from.clone());
        }
    }
    parts.join(", ")
}

/// The prompt text with its dot leaders: text from `col`, dots on the
/// odd columns after it, the last at column 33.
#[must_use]
pub fn leaders(label: &str, col: usize) -> String {
    let mut s = String::new();
    let mut at = col;
    for ch in label.chars() {
        if at > DOTS_END {
            break;
        }
        s.push(ch);
        at += 1;
    }
    // Two blanks after the text, then dots on the odd columns.
    at += 1;
    s.push(' ');
    while at <= DOTS_END {
        if at % 2 == 1 {
            s.push('.');
        } else {
            s.push(' ');
        }
        at += 1;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_leaders_end_at_column_33() {
        let l = leaders("Library", LABEL_COL);
        assert_eq!(l, "Library  . . . . . . . . . . . .");
        assert_eq!(LABEL_COL + l.chars().count() - 1, DOTS_END);
        let l = leaders("Text 'description'", LABEL_COL);
        assert_eq!(l, "Text 'description' . . . . . . .");
        let l = leaders("Library", LABEL_COL + 2);
        assert_eq!(l, "Library  . . . . . . . . . . .");
        assert_eq!(LABEL_COL + 2 + l.chars().count() - 1, DOTS_END);
    }
}
