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
    },
    /// `/DEFINE name`.
    Define(String),
    /// `/UNDEFINE name`.
    Undefine(String),
    /// `/IF [NOT] DEFINED(name)`.
    If(bool, String),
    /// `/ELSEIF [NOT] DEFINED(name)`.
    ElseIf(bool, String),
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
        ctdata: false,
        conds: 0,
    }];
    let mut no: u32 = 0;
    while let Some(top) = stack.last_mut() {
        let Some(rec) = r.members[top.member].records.get(top.next).cloned() else {
            // End of member: an open /IF of this member is an error.
            let f = stack.pop().expect("a frame");
            if conds.len() > f.conds {
                r.diags.push(Diag::new(ids::COND_NO_ENDIF, 30, Pos { line: no, col: 0 }));
                conds.truncate(f.conds);
            }
            continue;
        };
        let first = top.next == 0;
        top.next += 1;
        no += 1;
        let depth = u8::try_from(stack.len() - 1).unwrap_or(u8::MAX);
        let top = stack.last_mut().expect("a frame");
        let active = conds.last().is_none_or(|c| c.active);
        let mut class = classify(&rec.text, top.fully_free, first, top.ctdata, top.free_block);
        let at = Pos { line: no, col: 7 };
        // Conditional directives work whatever the state.
        if let Class::Directive(d) = &class {
            match d {
                Directive::If(not, name) => {
                    let def = defined.contains(name);
                    let hit = def != *not;
                    conds.push(Cond {
                        active: active && hit,
                        taken: hit,
                        outer: active,
                        seen_else: false,
                    });
                }
                Directive::ElseIf(not, name) => {
                    if conds.len() <= top.conds {
                        r.diags.push(Diag::new(ids::COND_NO_IF, 30, at));
                    } else if let Some(c) = conds.last_mut() {
                        let hit = defined.contains(name) != *not;
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
            Class::Directive(Directive::Define(n)) => {
                if !defined.contains(n) {
                    defined.push(n.clone());
                }
            }
            Class::Directive(Directive::Undefine(n)) => defined.retain(|x| x != n),
            Class::Directive(Directive::Free) => top.free_block = true,
            Class::Directive(Directive::EndFree) => top.free_block = false,
            Class::CtHeader(..) => top.ctdata = true,
            _ => {}
        }
        let eof = matches!(class, Class::Directive(Directive::Eof));
        let copy = match &class {
            Class::Directive(Directive::Copy {
                lib, file, member, ..
            }) => Some((lib.clone(), file.clone(), member.clone())),
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
        if eof {
            // The rest of this member is not read.
            let top = stack.last_mut().expect("a frame");
            top.next = r.members[top.member].records.len();
            continue;
        }
        if let Some((lib, file, name)) = copy {
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
            let free = is_free_marker(m.records.first().map_or("", |x| x.text.as_str()));
            r.members.push(m);
            stack.push(Frame {
                member: r.members.len() - 1,
                next: 0,
                fully_free: free,
                free_block: false,
                ctdata: false,
                conds: conds.len(),
            });
        }
    }
    if !conds.is_empty() {
        r.diags.push(Diag::new(ids::COND_NO_ENDIF, 30, Pos { line: no, col: 0 }));
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
            Directive::Copy {
                lib,
                file,
                member,
                include: word == "/INCLUDE",
            }
        }
        "/DEFINE" => Directive::Define(first_word(rest)),
        "/UNDEFINE" => Directive::Undefine(first_word(rest)),
        "/IF" => {
            let (not, name) = defined_cond(rest)?;
            Directive::If(not, name)
        }
        "/ELSEIF" => {
            let (not, name) = defined_cond(rest)?;
            Directive::ElseIf(not, name)
        }
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
    fn missing_members_and_recursion() {
        let main = Member::from_text("L", "QRPGLESRC", "MAIN", "      /COPY NOPE\n      /COPY QRPGLESRC,MAIN\n");
        let mut inc = MemIncludes::default();
        inc.members.push(main.clone());
        let r = read(main, &mut inc, &Options::default());
        let ids: Vec<&str> = r.diags.iter().map(|d| d.msgid).collect();
        assert_eq!(ids, vec![ids::COPY_NOT_FOUND, ids::COPY_NESTING]);
    }
}
