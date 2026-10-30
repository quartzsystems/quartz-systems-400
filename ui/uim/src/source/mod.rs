// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The source as the compiler sees it: one line per source record with
//! a sequence number, `.IM` lines expanded in place through an
//! [`Imbed`] resolver. Line indexes in diagnostics are indexes into
//! [`Source::lines`].

use std::collections::BTreeMap;

use crate::diag::{Diagnostics, ids};

/// Deepest `.IM` nesting.
const MAX_IMBED_DEPTH: usize = 8;

/// One source record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLine {
    /// Sequence number within its member: 100, 200, ...
    pub seq: u32,
    /// The text, tabs replaced by blanks, trailing blanks dropped.
    pub text: String,
    /// The imbedded member the line came from, or `None` for the main source.
    pub imbed: Option<String>,
}

impl SourceLine {
    /// `000100`.
    #[must_use]
    pub fn seq_text(&self) -> String {
        format!("{:06}", self.seq)
    }
}

/// Resolves `.IM name` to a member's text.
pub trait Imbed {
    /// The member's source, or `None` when there is no such member.
    fn imbed(&self, name: &str) -> Option<String>;
}

/// No members: every `.IM` is a diagnostic.
pub struct NoImbeds;

impl Imbed for NoImbeds {
    fn imbed(&self, _name: &str) -> Option<String> {
        None
    }
}

/// Members from a map, keyed by upper-case name.
#[derive(Default)]
pub struct MapImbeds {
    members: BTreeMap<String, String>,
}

impl MapImbeds {
    /// Empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a member.
    pub fn add(&mut self, name: &str, text: &str) {
        self.members
            .insert(name.to_ascii_uppercase(), text.to_string());
    }
}

impl Imbed for MapImbeds {
    fn imbed(&self, name: &str) -> Option<String> {
        self.members.get(&name.to_ascii_uppercase()).cloned()
    }
}

/// The lines of one compilation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Source {
    lines: Vec<SourceLine>,
}

impl Source {
    /// Read the main text and expand its `.IM` lines.
    #[must_use]
    pub fn read(text: &str, imbeds: &dyn Imbed) -> (Self, Diagnostics) {
        let mut src = Self::default();
        let mut diags = Diagnostics::new();
        let mut stack = Vec::new();
        src.append(text, None, imbeds, &mut stack, &mut diags);
        (src, diags)
    }

    /// From lines read back from a payload.
    #[must_use]
    pub fn from_lines(lines: Vec<SourceLine>) -> Self {
        Self { lines }
    }

    fn append(
        &mut self,
        text: &str,
        member: Option<&str>,
        imbeds: &dyn Imbed,
        stack: &mut Vec<String>,
        diags: &mut Diagnostics,
    ) {
        for (i, raw) in text.lines().enumerate() {
            let seq = u32::try_from(i + 1).unwrap_or(u32::MAX).saturating_mul(100);
            let line = self.lines.len();
            let mut t = raw.trim_end_matches(|c: char| c == ' ' || c.is_ascii_control()).to_string();
            if t.contains('\t') {
                diags.push(ids::tab(line));
                t = t.replace('\t', " ");
            }
            self.lines.push(SourceLine {
                seq,
                text: t.clone(),
                imbed: member.map(str::to_string),
            });
            let Some(rest) = t.strip_prefix(".IM") else {
                continue;
            };
            if !rest.starts_with(' ') && !rest.is_empty() {
                continue;
            }
            let Some(name) = rest.split_whitespace().next() else {
                diags.push(ids::imbed_name_missing(line));
                continue;
            };
            let upper = name.to_ascii_uppercase();
            if stack.len() >= MAX_IMBED_DEPTH || stack.contains(&upper) {
                diags.push(ids::imbed_too_deep(line, &upper));
                continue;
            }
            let Some(body) = imbeds.imbed(&upper) else {
                diags.push(ids::imbed_not_found(line, &upper));
                continue;
            };
            stack.push(upper.clone());
            self.append(&body, Some(&upper), imbeds, stack, diags);
            stack.pop();
        }
    }

    /// The lines, main and imbedded, in listing order.
    #[must_use]
    pub fn lines(&self) -> &[SourceLine] {
        &self.lines
    }

    /// The lines, consumed.
    #[must_use]
    pub fn into_lines(self) -> Vec<SourceLine> {
        self.lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imbeds_expand_in_place() {
        let mut m = MapImbeds::new();
        m.add("inner", ":P.inner text\n.IM deeper");
        m.add("DEEPER", ":P.deep");
        let (s, d) = Source::read(":PNLGRP.\n.IM inner\n:EPNLGRP.", &m);
        assert!(d.items().is_empty());
        let texts: Vec<_> = s.lines().iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                ":PNLGRP.",
                ".IM inner",
                ":P.inner text",
                ".IM deeper",
                ":P.deep",
                ":EPNLGRP."
            ]
        );
        assert_eq!(s.lines()[2].imbed.as_deref(), Some("INNER"));
        assert_eq!(s.lines()[4].imbed.as_deref(), Some("DEEPER"));
        assert_eq!(s.lines()[4].seq, 100);
    }

    #[test]
    fn missing_and_cyclic_imbeds() {
        let mut m = MapImbeds::new();
        m.add("LOOP", ".IM loop");
        let (_, d) = Source::read(".IM none\n.IM loop\n.IM", &m);
        let ids: Vec<_> = d.items().iter().map(|d| d.id).collect();
        assert_eq!(ids, ["UIM0101", "UIM0102", "UIM0108"]);
    }
}
