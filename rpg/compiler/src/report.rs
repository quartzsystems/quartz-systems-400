// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compile's diagnostics as `compdiag` values: each one the
//! listing prints, pointing at the line of the member it is in (a
//! `/COPY` member's own line for one in a copied member).

use compdiag::{Diagnostic, Help, MemberName, Sink, Where};
use rpgfront::reader::Line;

use crate::Compiled;

/// Where listing line `no` is: its member and its line in that member,
/// from 1. A line no member holds (0, a generated line) is nowhere.
#[must_use]
pub fn place(c: &Compiled, no: u32, main_path: Option<&str>) -> (Where, u32) {
    let lines = &c.read.lines;
    let Some(at) = lines.iter().position(|l| l.no == no) else {
        return (Where::None, 0);
    };
    let line = member_line(lines, at);
    let member = lines[at].member;
    let at = match (member, main_path) {
        (0, Some(p)) => Where::Path(p.to_string()),
        _ => c.read.members.get(member).map_or(Where::None, |m| {
            Where::Member(MemberName {
                library: m.lib.clone(),
                file: m.file.clone(),
                member: m.name.clone(),
            })
        }),
    };
    (at, line)
}

/// The line in its member of `lines[at]`: the records of one copy of a
/// member are consecutive at one depth, a copied member's between.
fn member_line(lines: &[Line], at: usize) -> u32 {
    // (member, depth, records so far) of each open copy.
    let mut open: Vec<(usize, u8, u32)> = Vec::new();
    for l in &lines[..=at] {
        while open.last().is_some_and(|o| o.1 > l.depth) {
            open.pop();
        }
        match open.last_mut() {
            Some(o) if o.1 == l.depth && o.0 == l.member => o.2 += 1,
            Some(o) if o.1 == l.depth => *o = (l.member, l.depth, 1),
            _ => open.push((l.member, l.depth, 1)),
        }
    }
    open.last().map_or(0, |o| o.2)
}

/// Every diagnostic of `c` into `sink`, in the listing's order. A
/// source from a stream file is `main_path`.
pub fn report(c: &Compiled, main_path: Option<&str>, help: Help<'_>, sink: &mut dyn Sink) {
    for d in &c.diags {
        let (at, line) = place(c, d.at.line, main_path);
        sink.report(Diagnostic {
            at,
            line,
            column: if line == 0 { 0 } else { d.at.col },
            end_line: 0,
            end_column: 0,
            severity: d.severity,
            id: d.msgid.to_string(),
            text: crate::msgs::text(d),
            help: help(d.msgid, &d.args),
        });
    }
}

/// [`report`] into a list.
#[must_use]
pub fn diagnostics(c: &Compiled, main_path: Option<&str>, help: Help<'_>) -> Vec<Diagnostic> {
    let mut v = Vec::new();
    report(c, main_path, help, &mut v);
    v
}
