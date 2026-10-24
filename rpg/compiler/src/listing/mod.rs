// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compile listing in the ILE RPG layout, record for record: the
//! prologue of command options, the source with statement numbers,
//! sequence numbers and change dates and each diagnostic after its
//! statement, the additional diagnostics, the compile-time data, the
//! key fields, the cross-reference, the external references, the
//! message summary, the final summary and the end lines. Each record
//! begins with its forms control (`1` new page, ` ` `0` `-` one to
//! three lines), as the spooled file holds it. The message texts are
//! ours; the layout, the labels and the section order are IBM's.

mod page;
mod prologue;
mod refs;
mod source;
mod stmts;
mod genlines;
mod lines;
mod tail;
mod xref;

use crate::compile::Compiled;
use crate::ctl::Options;

pub(crate) use page::Out;

/// Build the listing.
#[must_use]
pub fn listing(c: &Compiled, o: &Options) -> Vec<String> {
    let mut out = Out::new(format!("{}/{}", o.lib, o.name));
    prologue::prologue(&mut out, o);
    source::source(&mut out, c, o);
    tail::additional(&mut out, c);
    tail::buffer(&mut out, c);
    tail::copies(&mut out, c);
    source::ctdata(&mut out, c);
    let has = |x: &str| c.ctl.option.iter().any(|v| v.eq_ignore_ascii_case(x));
    if !has("*NOEXPDDS") {
        xref::keys(&mut out, c);
    }
    let model = xref::model(c, !has("*NOUNREF"));
    let xref = !has("*NOXREF");
    if xref {
        xref::xref(&mut out, c, &model);
    }
    let unref = if xref { model.unreferenced() } else { 0 };
    if !has("*NOEXT") {
        tail::external(&mut out, c);
    }
    tail::summary(&mut out, c, unref);
    tail::final_summary(&mut out, c, o, unref);
    out.lines
}

/// `S o u r c e` from `Source`.
pub(crate) fn spaced(s: &str) -> String {
    s.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ")
}

/// A label and IBM's dot leader up to the colon at `colon` (columns
/// from 0, the forms control at 0): a blank after the label, then a
/// dot on every column of the colon's parity.
pub(crate) fn leader(start: usize, label: &str, colon: usize) -> String {
    let mut s = format!("{:start$}{label} ", "");
    while s.len() < colon {
        s.push(if (s.len() + 1) % 2 == (colon + 1) % 2 { '.' } else { ' ' });
    }
    s.push(':');
    s
}

/// Words wrapped to `width` columns.
pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for w in text.split(' ').filter(|w| !w.is_empty()) {
        if !cur.is_empty() && cur.len() + 1 + w.len() > width {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(w);
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaders_are_ibms() {
        assert_eq!(leader(2, "Command", 35), "  Command  . . . . . . . . . . . . :");
        assert_eq!(leader(2, "Module", 35), "  Module . . . . . . . . . . . . . :");
        assert_eq!(leader(4, "Issued by", 35), "    Issued by  . . . . . . . . . . :");
        assert_eq!(leader(2, "Enable performance collection", 35), "  Enable performance collection  . :");
    }
}
