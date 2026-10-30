// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The source reader: the main member with every `/COPY` and
//! `/INCLUDE` expanded in place, conditional compilation applied, and
//! each line numbered as the listing numbers it and classified (a
//! fixed-form specification, free-form text, a comment, a directive,
//! compile-time data). A member whose first line is `**FREE` is fully
//! free; elsewhere a line with columns 6 and 7 blank is free form.

use crate::ast::Pos;
use crate::diag::{Diag, ids};
use crate::source::{Includes, Member};

/// What a line is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Class {
    /// Nothing in the source area.
    Blank,
    /// A comment line.
    Comment,
    /// A compiler directive.
    Directive(Directive),
    /// A fixed-form specification: its letter.
    Spec(char),
    /// Free-form text.
    Free,
    /// The `**FREE` line itself.
    FreeMarker,
    /// A `**` line starting compile-time data: the kind (`CTDATA`,
    /// `FTRANS`, `ALTSEQ`, or empty) and the name after `**CTDATA`.
    CtHeader(String, String),
    /// A compile-time data record.
    CtData,
    /// An embedded SQL line (`C/EXEC SQL`, `C+`, `C/END-EXEC`).
    Sql,
    /// Left out by conditional compilation.
    Skipped,
}

/// A directive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Directive {
    /// `/COPY` or `/INCLUDE` (`include` set) of `lib/file,member`.
    Copy {
        /// Library or `*LIBL`.
        lib: String,
        /// Source file.
        file: String,
        /// Member.
        member: String,
        /// `/INCLUDE`.
        include: bool,
        /// The target as written (quotes removed, case kept): a stream
        /// file's path when the source is a stream file.
        word: String,
    },
    /// `/DEFINE name`.
    Define(String),
    /// `/UNDEFINE name`.
    Undefine(String),
    /// `/IF [NOT] DEFINED(name)`; `None` when the condition is not valid.
    If(Option<(bool, String)>),
    /// `/ELSEIF [NOT] DEFINED(name)`; `None` when it is not valid.
    ElseIf(Option<(bool, String)>),
    /// `/ELSE`.
    Else,
    /// `/ENDIF`.
    EndIf,
    /// `/EOF`.
    Eof,
    /// `/FREE`.
    Free,
    /// `/END-FREE`.
    EndFree,
    /// `/TITLE text`.
    Title(String),
    /// `/SPACE n`.
    Space(u32),
    /// `/EJECT`.
    Eject,
    /// `/SET` keywords.
    Set(String),
    /// `/RESTORE` keywords.
    Restore(String),
    /// Anything else written as a directive.
    Unknown(String),
}

/// One line of the expanded source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// The listing's line number.
    pub no: u32,
    /// Index of the member in [`Read::members`].
    pub member: usize,
    /// The record's sequence number.
    pub seq: String,
    /// The record's change date.
    pub date: String,
    /// The record text.
    pub text: String,
    /// What it is.
    pub class: Class,
    /// The member is fully free.
    pub fully_free: bool,
    /// `/COPY` depth: 0 in the main member.
    pub depth: u8,
}

impl Line {
    /// The free-form area: columns 8 to 80 in fixed-format source,
    /// the whole line in fully free source; and the column it starts.
    #[must_use]
    pub fn free_area(&self) -> (u16, String) {
        if self.fully_free {
            (1, self.text.clone())
        } else {
            (8, rpgspecs::cut(&self.text, 8, 80).trim_end().to_string())
        }
    }

    /// The position of column `col` in this line.
    #[must_use]
    pub fn pos(&self, col: u16) -> Pos {
        Pos { line: self.no, col }
    }
}

/// What the reader produced.
#[derive(Clone, Debug, Default)]
pub struct Read {
    /// Every line, in listing order.
    pub lines: Vec<Line>,
    /// The members read: the main one first, then each copied one in
    /// order of first use.
    pub members: Vec<Member>,
    /// The diagnostics.
    pub diags: Vec<Diag>,
    /// The condition names defined at the end.
    pub defined: Vec<String>,
    /// The members embedded SQL's `INCLUDE` brought in, in order.
    pub sql_includes: Vec<SqlInclude>,
    /// `INCLUDE SQLDA` was seen.
    pub sqlda: bool,
    /// The conditional directives whose group is left out (in an
    /// included group), by line: the listing counts even an empty one.
    pub excluded: Vec<u32>,
}

/// An SQL `INCLUDE` the reader took: of `SQLCA` or `SQLDA`, or of a
/// member (from the main member's source file, `INCFILE(*SRCFILE)`)
/// read in place. The statement itself reads as a comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlInclude {
    /// The statement's (last) line.
    pub line: u32,
    /// Index of the member in [`Read::members`]; none for `SQLCA` and
    /// `SQLDA`.
    pub member: Option<usize>,
    /// The member is fully free.
    pub fully_free: bool,
}

/// An SQL `INCLUDE` ending at the last line read: the name and the
/// statement's lines. Free form takes one line; fixed form a
/// `C/EXEC SQL INCLUDE name` line and its `C/END-EXEC`.
fn sql_include_at(lines: &[Line], pending: &mut Option<(String, usize)>) -> Option<(String, Vec<usize>)> {
    let i = lines.len().checked_sub(1)?;
    let l = &lines[i];
    match l.class {
        Class::Free => sql_include(&l.free_area().1).map(|n| (n, vec![i])),
        Class::Sql => {
            let t = rpgspecs::cut(&l.text, 7, 80).trim().to_string();
            let up = t.to_ascii_uppercase();
            if up.starts_with("/END-EXEC") {
                pending.take().map(|(n, at)| (n, vec![at, i]))
            } else {
                *pending = up.strip_prefix('/').and_then(sql_include).map(|n| (n, i));
                None
            }
        }
        _ => None,
    }
}

/// The member an `EXEC SQL INCLUDE name` statement names.
fn sql_include(stmt: &str) -> Option<String> {
    let up = stmt.trim().trim_end_matches(';').to_ascii_uppercase();
    let mut w = up.split_whitespace();
    if w.next()? != "EXEC" || w.next()? != "SQL" || w.next()? != "INCLUDE" {
        return None;
    }
    let name = w.next()?.to_string();
    w.next().is_none().then_some(name)
}

/// What the reader needs to know.
#[derive(Clone, Debug)]
pub struct Options {
    /// Condition names defined from the start: the command's `DEFINE`
    /// values and the predefined ones.
    pub defines: Vec<String>,
    /// The deepest `/COPY` nesting allowed (`COPYNEST`).
    pub copynest: u8,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            defines: Vec::new(),
            copynest: 32,
        }
    }
}

/// The state of one `/IF` group.
struct Cond {
    /// Lines in the current branch are compiled.
    active: bool,
    /// Some branch of the group was taken.
    taken: bool,
    /// The `/IF` line.
    line: u32,
    /// The group's surroundings are compiled.
    outer: bool,
    /// An `/ELSE` was seen.
    seen_else: bool,
}

/// One member being read.
struct Frame {
    member: usize,
    next: usize,
    fully_free: bool,
    /// Between `**END-FREE` and `**FREE` in a fully free member.
    fixed_part: bool,
    free_block: bool,
    ctdata: bool,
    conds: usize,
}

/// Read the main member and everything it copies.
pub fn read(main: Member, includes: &mut dyn Includes, opts: &Options) -> Read {
    let mut r = Read {
        members: vec![main],
        ..Read::default()
    };
    let mut defined: Vec<String> = opts.defines.iter().map(|d| d.to_ascii_uppercase()).collect();
    let mut conds: Vec<Cond> = Vec::new();
    let mut stack = vec![Frame {
        member: 0,
        next: 0,
        fully_free: is_free_marker(r.members[0].records.first().map_or("", |x| x.text.as_str())),
        free_block: false,
        fixed_part: false,
        ctdata: false,
        conds: 0,
    }];
    let mut no: u32 = 0;
    // A fixed-form `C/EXEC SQL INCLUDE` waiting for its `C/END-EXEC`.
    let mut sql_pending: Option<(String, usize)> = None;
    while let Some(top) = stack.last_mut() {
        let Some(rec) = r.members[top.member].records.get(top.next).cloned() else {
            // End of member: an open /IF of this member is an error.
            let f = stack.pop().expect("a frame");
            for c in conds.drain(f.conds.min(conds.len())..) {
                r.diags.push(Diag::new(ids::COND_NO_ENDIF, 30, Pos { line: c.line, col: 0 }));
            }
            continue;
        };
        let first = top.next == 0;
        top.next += 1;
        no += 1;
        let depth = u8::try_from(stack.len() - 1).unwrap_or(u8::MAX);
        let top = stack.last_mut().expect("a frame");
        let active = conds.last().is_none_or(|c| c.active);
        // A fully free member goes to fixed form at `**END-FREE` and
        // back at `**FREE` (the precompiler's generated source).
        let marker = rec.text.trim().to_ascii_uppercase();
        if !first && ((top.fully_free && marker == "**END-FREE") || (top.fixed_part && marker == "**FREE")) {
            top.fixed_part = !top.fixed_part;
            top.fully_free = !top.fixed_part;
            r.lines.push(Line {
                no,
                member: top.member,
                seq: rec.seq,
                date: rec.date,
                text: rec.text,
                class: Class::FreeMarker,
                fully_free: true,
                depth,
            });
            continue;
        }
        let mut class = classify(&rec.text, top.fully_free, first, top.ctdata, top.free_block);
        let at = Pos { line: no, col: 7 };
        // Conditional directives work whatever the state.
        if let Class::Directive(d) = &class {
            match d {
                Directive::If(cond) => {
                    if cond.is_none() {
                        r.diags.push(Diag::new(ids::BAD_CONDITION, 30, at));
                    }
                    let hit = cond.as_ref().is_some_and(|(not, name)| defined.contains(name) != *not);
                    conds.push(Cond {
                        active: active && hit,
                        taken: hit,
                        line: no,
                        outer: active,
                        seen_else: false,
                    });
                }
                Directive::ElseIf(cond) => {
                    if conds.len() <= top.conds {
                        r.diags.push(Diag::new(ids::COND_NO_IF, 30, at));
                    } else if let Some(c) = conds.last_mut() {
                        if cond.is_none() {
                            r.diags.push(Diag::new(ids::BAD_CONDITION, 30, at));
                        }
                        let hit = cond.as_ref().is_some_and(|(not, name)| defined.contains(name) != *not);
                        c.active = c.outer && !c.taken && hit && !c.seen_else;
                        c.taken |= hit;
                    }
                }
                Directive::Else => {
                    if conds.len() <= top.conds {
                        r.diags.push(Diag::new(ids::COND_NO_IF, 30, at));
                    } else if let Some(c) = conds.last_mut() {
                        c.active = c.outer && !c.taken;
                        c.taken = true;
                        c.seen_else = true;
                    }
                }
                Directive::EndIf => {
                    if conds.len() <= top.conds {
                        r.diags.push(Diag::new(ids::COND_NO_IF, 30, at));
                    } else {
                        conds.pop();
                    }
                }
                _ => {}
            }
            if matches!(
                d,
                Directive::If(..) | Directive::ElseIf(..) | Directive::Else | Directive::EndIf
            ) {
                if !matches!(d, Directive::EndIf) && conds.last().is_some_and(|c| c.outer && !c.active) {
                    r.excluded.push(no);
                }
                r.lines.push(Line {
                    no,
                    member: top.member,
                    seq: rec.seq,
                    date: rec.date,
                    text: rec.text,
                    class,
                    fully_free: top.fully_free,
                    depth,
                });
                continue;
            }
        }
        if !active {
            class = Class::Skipped;
        }
        let member = top.member;
        let fully_free = top.fully_free;
        match &class {
            Class::Directive(Directive::Define(n) | Directive::Undefine(n)) if n.is_empty() => {
                r.diags.push(Diag::new(ids::DEFINE_NO_NAME, 30, at));
            }
            Class::Directive(Directive::Define(n)) => {
                if !defined.contains(n) {
                    defined.push(n.clone());
                }
            }
            Class::Directive(Directive::Undefine(n)) => defined.retain(|x| x != n),
            Class::Directive(Directive::Copy { member, .. }) if member.is_empty() => {
                r.diags.push(Diag::new(ids::COPY_SYNTAX, 30, at));
            }
            Class::Directive(Directive::Unknown(_)) => {
                r.diags.push(Diag::new(ids::BAD_DIRECTIVE, 30, at));
            }
            Class::Directive(Directive::Free) => top.free_block = true,
            Class::Directive(Directive::EndFree) => top.free_block = false,
            Class::CtHeader(..) => top.ctdata = true,
            _ => {}
        }
        let eof = matches!(class, Class::Directive(Directive::Eof));
        let copy = match &class {
            Class::Directive(Directive::Copy {
                lib, file, member, word, ..
            }) if !member.is_empty() => Some((lib.clone(), file.clone(), member.clone(), word.clone())),
            _ => None,
        };
        r.lines.push(Line {
            no,
            member,
            seq: rec.seq,
            date: rec.date,
            text: rec.text,
            class,
            fully_free,
            depth,
        });
        if let Some((name, at)) = sql_include_at(&r.lines, &mut sql_pending) {
            let target = match name.as_str() {
                "SQLCA" => Some(None),
                "SQLDA" => {
                    r.sqlda = true;
                    Some(None)
                }
                _ => {
                    let (lib, file) = (r.members[0].lib.clone(), r.members[0].file.clone());
                    includes.member(&lib, &file, &name).map(Some)
                }
            };
            // A member not found is the precompiler's to report.
            if let Some(m) = target {
                for i in at {
                    r.lines[i].class = Class::Comment;
                }
                let free = m.as_ref().is_some_and(|m| is_free_marker(m.records.first().map_or("", |x| x.text.as_str())));
                let idx = m.map(|m| {
                    r.members.push(m);
                    r.members.len() - 1
                });
                r.sql_includes.push(SqlInclude {
                    line: no,
                    member: idx,
                    fully_free: free,
                });
                if let Some(idx) = idx {
                    stack.push(Frame {
                        member: idx,
                        next: 0,
                        fully_free: free,
                        free_block: false,
                        fixed_part: false,
                        ctdata: false,
                        conds: conds.len(),
                    });
                }
                continue;
            }
        }
        if eof {
            // The rest of this member is not read.
            let top = stack.last_mut().expect("a frame");
            top.next = r.members[top.member].records.len();
            continue;
        }
        if let Some((lib, file, name, word)) = copy {
            // A stream file: quoted, or any name without a comma when the
            // source itself is a stream file. Relative paths are searched
            // from the including stream file's directory too.
            let stmf = lib == STMF || (r.members[0].lib == STMF && !word.contains(','));
            let m = if stmf {
                let cur = &r.members[stack.last().expect("a frame").member];
                let dir = if cur.lib == STMF { cur.file.clone() } else { String::new() };
                let target = if lib == STMF { name.clone() } else { word.clone() };
                if stack.len() > usize::from(opts.copynest) {
                    r.diags.push(Diag::new(ids::COPY_NESTING, 40, at).arg(target));
                    continue;
                }
                let Some(m) = includes.member(STMF, &dir, &target) else {
                    r.diags.push(Diag::new(ids::COPY_NOT_FOUND, 40, at).arg(target));
                    continue;
                };
                let busy = stack.iter().any(|f| {
                    let x = &r.members[f.member];
                    x.lib == STMF && x.name == m.name
                });
                if busy {
                    r.diags.push(Diag::new(ids::COPY_NESTING, 40, at).arg(m.name.clone()));
                    continue;
                }
                m
            } else {
                let busy = stack.iter().any(|f| {
                    let m = &r.members[f.member];
                    m.file.eq_ignore_ascii_case(&file) && m.name.eq_ignore_ascii_case(&name)
                });
                if busy || stack.len() > usize::from(opts.copynest) {
                    r.diags.push(Diag::new(ids::COPY_NESTING, 40, at).arg(format!("{file},{name}")));
                    continue;
                }
                let Some(m) = includes.member(&lib, &file, &name) else {
                    r.diags.push(Diag::new(ids::COPY_NOT_FOUND, 40, at).arg(format!("{lib}/{file},{name}")));
                    continue;
                };
                m
            };
            let free = is_free_marker(m.records.first().map_or("", |x| x.text.as_str()));
            r.members.push(m);
            stack.push(Frame {
                member: r.members.len() - 1,
                next: 0,
                fully_free: free,
                free_block: false,
                fixed_part: false,
                ctdata: false,
                conds: conds.len(),
            });
        }
    }
    for c in conds {
        r.diags.push(Diag::new(ids::COND_NO_ENDIF, 30, Pos { line: c.line, col: 0 }));
    }
    r.defined = defined;
    r
}

/// Whether a record is the `**FREE` marker.
fn is_free_marker(text: &str) -> bool {
    text.len() >= 6 && text[..6].eq_ignore_ascii_case("**FREE") && text[6..].trim().is_empty()
}

/// Classify one record.
#[allow(clippy::fn_params_excessive_bools)]
fn classify(text: &str, fully_free: bool, first: bool, ctdata: bool, free_block: bool) -> Class {
    if first && fully_free {
        return Class::FreeMarker;
    }
    if let Some(rest) = text.strip_prefix("**") {
        // `**` and a blank: the next array's data; the rest is a comment.
        if rest.is_empty() || rest.starts_with(' ') {
            return Class::CtHeader(String::new(), String::new());
        }
        let rest = rest.trim();
        let up = rest.to_ascii_uppercase();
        let mut words = up.split_whitespace();
        let kind = words.next().unwrap_or("").to_string();
        if kind.is_empty() || matches!(kind.as_str(), "CTDATA" | "FTRANS" | "ALTSEQ") {
            let name = if kind == "CTDATA" {
                words.next().unwrap_or("").to_string()
            } else {
                String::new()
            };
            return Class::CtHeader(kind, name);
        }
    }
    if ctdata {
        return Class::CtData;
    }
    if fully_free {
        let t = text.trim_start();
        if t.is_empty() {
            return Class::Blank;
        }
        if t.starts_with("//") {
            return Class::Comment;
        }
        if t.starts_with('/')
            && let Some(d) = directive(t)
        {
            return Class::Directive(d);
        }
        return Class::Free;
    }
    let c6 = text.chars().nth(5).unwrap_or(' ');
    let c7 = text.chars().nth(6).unwrap_or(' ');
    let area = rpgspecs::cut(text, 6, 80);
    if area.trim().is_empty() {
        return Class::Blank;
    }
    if c7 == '*' {
        return Class::Comment;
    }
    if c7 == '/' {
        let t = rpgspecs::cut(text, 7, 80);
        let up = t.trim_end().to_ascii_uppercase();
        if up.starts_with("/EXEC SQL") || up.starts_with("/END-EXEC") {
            return Class::Sql;
        }
        if let Some(d) = directive(t.trim()) {
            return Class::Directive(d);
        }
        return Class::Directive(Directive::Unknown(t.trim().to_string()));
    }
    if c7 == '+' && c6.eq_ignore_ascii_case(&'C') {
        return Class::Sql;
    }
    if c6 == ' ' && c7 == ' ' {
        let t = rpgspecs::cut(text, 8, 80);
        let tt = t.trim_start();
        if tt.starts_with("//") {
            return Class::Comment;
        }
        if tt.starts_with('/')
            && let Some(d) = directive(tt.trim_end())
        {
            return Class::Directive(d);
        }
        return Class::Free;
    }
    if free_block && c6 == ' ' {
        return Class::Free;
    }
    // A calculation line with nothing after the form letter.
    if c6.eq_ignore_ascii_case(&'C') && rpgspecs::cut(text, 7, 80).trim().is_empty() {
        return Class::Blank;
    }
    Class::Spec(c6.to_ascii_uppercase())
}

/// Parse a directive line (starting with `/`).
fn directive(t: &str) -> Option<Directive> {
    let t = t.trim();
    let up = t.to_ascii_uppercase();
    let (word, rest) = match up.find(' ') {
        Some(i) => (&up[..i], up[i..].trim()),
        None => (up.as_str(), ""),
    };
    let rest_orig = t.get(word.len()..).unwrap_or("").trim();
    let first_word = |s: &str| s.split_whitespace().next().unwrap_or("").to_string();
    Some(match word {
        "/COPY" | "/INCLUDE" => {
            let (lib, file, member) = copy_target(rest_orig);
            let target = rest_orig.split_whitespace().next().unwrap_or("").trim_matches('\'').to_string();
            Directive::Copy {
                lib,
                file,
                member,
                include: word == "/INCLUDE",
                word: target,
            }
        }
        "/DEFINE" => Directive::Define(first_word(rest)),
        "/UNDEFINE" => Directive::Undefine(first_word(rest)),
        "/IF" => Directive::If(defined_cond(rest)),
        "/ELSEIF" => Directive::ElseIf(defined_cond(rest)),
        "/ELSE" => Directive::Else,
        "/ENDIF" => Directive::EndIf,
        "/EOF" => Directive::Eof,
        "/FREE" => Directive::Free,
        "/END-FREE" => Directive::EndFree,
        "/TITLE" => Directive::Title(rest_orig.to_string()),
        "/SPACE" => Directive::Space(first_word(rest).parse().unwrap_or(1)),
        "/EJECT" => Directive::Eject,
        "/SET" => Directive::Set(rest.to_string()),
        "/RESTORE" => Directive::Restore(rest.to_string()),
        // A word after the slash is a directive, known or not; a
        // statement's continuation (`/ b;`) is not.
        _ if word[1..].starts_with(|c: char| c.is_ascii_alphabetic())
            && word[1..].chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            && !t.contains(';') =>
        {
            Directive::Unknown(t.to_string())
        }
        _ => return None,
    })
}

/// `DEFINED(name)` or `NOT DEFINED(name)`.
fn defined_cond(rest: &str) -> Option<(bool, String)> {
    let r = rest.trim();
    let (not, r) = match r.strip_prefix("NOT") {
        Some(x) if x.starts_with([' ', 'D']) => (true, x.trim_start()),
        _ => (false, r),
    };
    let inner = r.strip_prefix("DEFINED")?.trim_start().strip_prefix('(')?;
    let end = inner.find(')')?;
    Some((not, inner[..end].trim().to_string()))
}

/// The library a stream file's [`Member`] has: its `file` is the
/// directory and its `name` the path.
pub const STMF: &str = "*STMF";

/// `member`, `file,member`, `lib/file,member` or a quoted stream file.
fn copy_target(rest: &str) -> (String, String, String) {
    let word = rest.split_whitespace().next().unwrap_or("");
    if let Some(path) = word.strip_prefix('\'') {
        let path = path.trim_end_matches('\'');
        return ("*STMF".into(), String::new(), path.to_string());
    }
    let up = word.to_ascii_uppercase();
    let (qual, member) = match up.split_once(',') {
        Some((q, m)) => (q.to_string(), m.to_string()),
        None => (String::new(), up.clone()),
    };
    let (lib, file) = match qual.split_once('/') {
        Some((l, f)) => (l.to_string(), f.to_string()),
        None if qual.is_empty() => ("*LIBL".into(), "QRPGLESRC".into()),
        None => ("*LIBL".into(), qual),
    };
    (lib, file, member)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemIncludes;

    fn classes(r: &Read) -> Vec<(u32, Class)> {
        r.lines.iter().map(|l| (l.no, l.class.clone())).collect()
    }

    #[test]
    fn copies_conditions_and_classes() {
        let main = Member::from_text(
            "L",
            "QRPGLESRC",
            "MAIN",
            "     H DFTACTGRP(*NO)\n      * a comment\n      /COPY CPY\n      /IF DEFINED(X)\n     D A               S             10\n      /ELSE\n     D B               S             10\n      /ENDIF\n       dcl-s c int(10);\n**CTDATA ARR\nabc\n",
        );
        let mut inc = MemIncludes::default();
        inc.members.push(Member::from_text("L", "QRPGLESRC", "CPY", "**FREE\ndcl-c K 1;\n"));
        let r = read(main, &mut inc, &Options::default());
        assert!(r.diags.is_empty(), "{:?}", r.diags);
        let c = classes(&r);
        assert_eq!(c[0].1, Class::Spec('H'));
        assert_eq!(c[1].1, Class::Comment);
        assert!(matches!(c[2].1, Class::Directive(Directive::Copy { .. })));
        assert_eq!(c[3], (4, Class::FreeMarker));
        assert_eq!(c[4], (5, Class::Free));
        assert_eq!(r.lines[4].depth, 1);
        assert_eq!(c[6].1, Class::Skipped);
        assert_eq!(c[8].1, Class::Spec('D'));
        assert_eq!(c[10].1, Class::Free);
        assert_eq!(c[11].1, Class::CtHeader("CTDATA".into(), "ARR".into()));
        assert_eq!(c[12].1, Class::CtData);
    }

    #[test]
    fn sql_include_reads_the_member_in_place() {
        let main = Member::from_text(
            "L",
            "QSRC",
            "MAIN",
            "**FREE\nexec sql include INC;\nexec sql include sqlda;\nexec sql include NOPE;\n",
        );
        let mut inc = MemIncludes::default();
        inc.members.push(Member::from_text("L", "QSRC", "INC", "       dcl-s n int(10);\n"));
        let r = read(main, &mut inc, &Options::default());
        let c = classes(&r);
        assert_eq!(c[1], (2, Class::Comment));
        assert_eq!(c[2], (3, Class::Free));
        assert_eq!(r.lines[2].depth, 1);
        assert_eq!(c[3], (4, Class::Comment));
        // A member not found is left to the precompiler.
        assert_eq!(c[4], (5, Class::Free));
        assert!(r.sqlda);
        assert_eq!(r.sql_includes.len(), 2);
        assert_eq!((r.sql_includes[0].line, r.sql_includes[0].member, r.sql_includes[0].fully_free), (2, Some(1), false));
    }

    #[test]
    fn missing_members_and_recursion() {
        let main = Member::from_text("L", "QRPGLESRC", "MAIN", "      /COPY NOPE\n      /COPY QRPGLESRC,MAIN\n");
        let mut inc = MemIncludes::default();
        inc.members.push(main.clone());
        let r = read(main, &mut inc, &Options::default());
        let ids: Vec<&str> = r.diags.iter().map(|d| d.msgid).collect();
        assert_eq!(ids, vec![ids::COPY_NOT_FOUND, ids::COPY_NESTING]);
    }
}
