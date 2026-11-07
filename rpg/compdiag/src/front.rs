// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a front end's `analyze` hands a language server: the sources
//! read, every declaration and reference with its span, the `/COPY`
//! and `/INCLUDE` requests and the diagnostics. Each front end
//! (`rpgfront`, `cl`, `dds`, `sql`, `cmd`) fills one [`Analysis`] with
//! its own tree.

use crate::{Diagnostic, MemberName, Where};

/// A source as the caller has it: where it lives and its text, one
/// record a line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Source {
    /// The member or stream file.
    pub at: Where,
    /// The text, lines ending in `\n` (or `\r\n`).
    pub text: String,
}

impl Source {
    /// A source member's text.
    #[must_use]
    pub fn member(library: &str, file: &str, member: &str, text: &str) -> Self {
        Self {
            at: Where::Member(MemberName {
                library: library.into(),
                file: file.into(),
                member: member.into(),
            }),
            text: text.into(),
        }
    }

    /// A stream file's text.
    #[must_use]
    pub fn path(path: &str, text: &str) -> Self {
        Self {
            at: Where::Path(path.into()),
            text: text.into(),
        }
    }

    /// Text with no name.
    #[must_use]
    pub fn text(text: &str) -> Self {
        Self {
            at: Where::None,
            text: text.into(),
        }
    }
}

/// A stretch of a source: byte offsets into its text, and lines and
/// columns from 1 (columns count characters, as fixed-form columns do).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    /// The first byte.
    pub start: usize,
    /// The byte after the last.
    pub end: usize,
    /// The first line.
    pub line: u32,
    /// The first column.
    pub column: u32,
    /// The last line.
    pub end_line: u32,
    /// The column after the last character.
    pub end_column: u32,
}

/// Where each line of a text starts, to turn lines and columns into
/// [`Span`]s.
#[derive(Clone, Debug, Default)]
pub struct Lines {
    /// Each line's first byte and its text, line end removed.
    lines: Vec<(usize, String)>,
}

impl Lines {
    /// The lines of `text`, split as [`str::lines`] splits them.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let mut lines = Vec::new();
        let mut at = 0;
        for raw in text.split_inclusive('\n') {
            let body = raw.strip_suffix('\n').unwrap_or(raw);
            let body = body.strip_suffix('\r').unwrap_or(body);
            lines.push((at, body.to_string()));
            at += raw.len();
        }
        Self { lines }
    }

    /// How many lines.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// No lines at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Line `line`'s text (from 1), line end removed.
    #[must_use]
    pub fn text(&self, line: u32) -> &str {
        self.get(line).map_or("", |(_, t)| t.as_str())
    }

    fn get(&self, line: u32) -> Option<&(usize, String)> {
        let i = usize::try_from(line).ok()?.checked_sub(1)?;
        self.lines.get(i)
    }

    /// The byte of column `column` (from 1) in line `line`, held to the
    /// line's end.
    #[must_use]
    pub fn byte(&self, line: u32, column: u32) -> usize {
        let Some((start, text)) = self.get(line) else {
            return self.lines.last().map_or(0, |(s, t)| s + t.len());
        };
        let skip = usize::try_from(column.saturating_sub(1)).unwrap_or(usize::MAX);
        start + text.char_indices().nth(skip).map_or(text.len(), |(b, _)| b)
    }

    /// `chars` characters from column `column` of line `line`.
    #[must_use]
    pub fn span(&self, line: u32, column: u32, chars: u32) -> Span {
        let column = column.max(1);
        let end_column = column + chars;
        Span {
            start: self.byte(line, column),
            end: self.byte(line, end_column),
            line,
            column,
            end_line: line,
            end_column,
        }
    }

    /// The whole of line `line`.
    #[must_use]
    pub fn line_span(&self, line: u32) -> Span {
        let chars = u32::try_from(self.text(line).chars().count()).unwrap_or(u32::MAX);
        self.span(line, 1, chars)
    }

    /// From line `first` to the end of line `last`.
    #[must_use]
    pub fn lines_span(&self, first: u32, last: u32) -> Span {
        let a = self.line_span(first);
        let b = self.line_span(last.max(first));
        Span {
            end: b.end,
            end_line: b.end_line,
            end_column: b.end_column,
            ..a
        }
    }

    /// The span of `name` in line `line`: the first occurrence at or
    /// after column `from` standing as a word (ASCII case ignored),
    /// else the first anywhere in the line, else `name`'s length at
    /// `from`.
    #[must_use]
    pub fn find(&self, line: u32, from: u32, name: &str) -> Span {
        let chars = u32::try_from(name.chars().count()).unwrap_or(0);
        match self.find_word(line, from, name) {
            Some(col) => self.span(line, col, chars),
            None => self.span(line, from, chars),
        }
    }

    /// The column where `name` stands as a word in line `line`, at or
    /// after `from` first, then anywhere.
    #[must_use]
    pub fn find_word(&self, line: u32, from: u32, name: &str) -> Option<u32> {
        self.find_from(line, from, name)
            .or_else(|| self.find_from(line, 1, name))
    }

    /// The column where `name` stands as a word in line `line`, at or
    /// after `from` only.
    #[must_use]
    pub fn find_from(&self, line: u32, from: u32, name: &str) -> Option<u32> {
        let text: Vec<char> = self.text(line).chars().collect();
        let want: Vec<char> = name.chars().collect();
        if want.is_empty() || want.len() > text.len() {
            return None;
        }
        let word = |c: char| c.is_alphanumeric() || "_#@$".contains(c);
        let hit = |i: usize| {
            text[i..i + want.len()]
                .iter()
                .zip(&want)
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
                && (i == 0 || !word(text[i - 1]) || !word(want[0]))
                && text
                    .get(i + want.len())
                    .is_none_or(|c| !word(*c) || !word(want[want.len() - 1]))
        };
        let from = usize::try_from(from.saturating_sub(1)).unwrap_or(0);
        let last = text.len() - want.len();
        (from..=last)
            .find(|&i| hit(i))
            .map(|i| u32::try_from(i + 1).unwrap_or(u32::MAX))
    }
}

/// Names found one after another in a range of lines, each after the
/// one before: the way a statement's words are written.
#[derive(Clone, Debug)]
pub struct Cursor<'a> {
    lines: &'a Lines,
    line: u32,
    column: u32,
    last: u32,
}

impl<'a> Cursor<'a> {
    /// Over lines `first` to `last`.
    #[must_use]
    pub fn new(lines: &'a Lines, first: u32, last: u32) -> Self {
        Self {
            lines,
            line: first,
            column: 1,
            last,
        }
    }

    /// The same, from column `column` of the first line.
    #[must_use]
    pub fn from_column(mut self, column: u32) -> Self {
        self.column = column.max(1);
        self
    }

    /// The next `name` standing as a word; the cursor moves past it.
    /// `None`, and the cursor stays, when the rest of the range has none.
    pub fn next(&mut self, name: &str) -> Option<Span> {
        let chars = u32::try_from(name.chars().count()).unwrap_or(0);
        for l in self.line..=self.last {
            let from = if l == self.line { self.column } else { 1 };
            if let Some(c) = self.lines.find_from(l, from, name) {
                self.line = l;
                self.column = c + chars;
                return Some(self.lines.span(l, c, chars));
            }
        }
        None
    }
}

/// What a name is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    /// A file (`DCL-F`, `DCLF`, a DDS file).
    File,
    /// A record format.
    Format,
    /// A field of a record format or a table's column.
    Field,
    /// A variable or standalone field.
    #[default]
    Variable,
    /// A named constant.
    Constant,
    /// A data structure.
    Structure,
    /// A subfield.
    Subfield,
    /// A procedure.
    Procedure,
    /// A prototype.
    Prototype,
    /// A parameter.
    Parameter,
    /// A subroutine.
    Subroutine,
    /// A label (`TAG`, a CL label, a command definition's `ELEM` or
    /// `QUAL` list).
    Label,
    /// A key list or parameter list.
    List,
    /// An SQL table, view or alias.
    Table,
    /// An SQL index.
    Index,
    /// An SQL schema.
    Schema,
    /// An SQL routine, trigger or sequence.
    Routine,
    /// A cursor.
    Cursor,
    /// A command (a command definition's `CMD`).
    Command,
    /// A keyword of a command (`PARM KWD`).
    Keyword,
}

/// A name being declared.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Declaration {
    /// The name, upper case as the compiler folds it.
    pub name: String,
    /// What it is.
    pub kind: SymbolKind,
    /// The source, an index into [`Analysis::sources`].
    pub source: usize,
    /// Where the name is written.
    pub span: Span,
    /// A short description (type and length, a keyword's type).
    pub detail: String,
    /// The procedure, structure or table it belongs to, if any.
    pub scope: Option<String>,
}

/// A use of a name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reference {
    /// The name, upper case as the compiler folds it.
    pub name: String,
    /// What the use expects it to be.
    pub kind: SymbolKind,
    /// The source, an index into [`Analysis::sources`].
    pub source: usize,
    /// Where the name is written.
    pub span: Span,
    /// The declaration it resolves to, an index into
    /// [`Analysis::declarations`]; `None` when it is not declared in
    /// what was read (a field of an external file, a system object).
    pub declaration: Option<usize>,
}

/// What a `/COPY` or `/INCLUDE` asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IncludeRequest {
    /// The library, `*LIBL` when not given; empty for a stream file.
    pub library: String,
    /// The source file; for a stream file, the including stream
    /// file's directory (empty when the includer is a member).
    pub file: String,
    /// The member; empty for a stream file.
    pub member: String,
    /// The stream file's path as written, for a stream file.
    pub path: Option<String>,
}

/// One `/COPY` or `/INCLUDE` the source holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Include {
    /// The directive as written (`/COPY`, `/INCLUDE`, `EXEC SQL
    /// INCLUDE`).
    pub directive: String,
    /// What it asked for.
    pub request: IncludeRequest,
    /// The source holding the directive.
    pub source: usize,
    /// The directive's line.
    pub span: Span,
    /// The source it brought in, an index into [`Analysis::sources`];
    /// `None` when the caller had nothing for it.
    pub resolved: Option<usize>,
}

/// How a front end asks its caller for an included source.
pub type Fetch<'a> = &'a dyn Fn(&IncludeRequest) -> Option<Source>;

/// What `analyze` produced.
#[derive(Clone, Debug, Default)]
pub struct Analysis<A> {
    /// The syntax tree.
    pub ast: A,
    /// The sources read: the one analyzed first, then each included
    /// one in order of use.
    pub sources: Vec<Source>,
    /// Every declaration, in source order.
    pub declarations: Vec<Declaration>,
    /// Every reference, in source order.
    pub references: Vec<Reference>,
    /// Every `/COPY` and `/INCLUDE`.
    pub includes: Vec<Include>,
    /// Every diagnostic, as the listing has them.
    pub diagnostics: Vec<Diagnostic>,
}

impl<A> Analysis<A> {
    /// Link each reference without a declaration to the declaration of
    /// its name (case ignored): one in the reference's scope first,
    /// then one with no scope, then any.
    pub fn link(&mut self, scope_of: impl Fn(&Reference) -> Option<String>) {
        for r in &mut self.references {
            if r.declaration.is_some() {
                continue;
            }
            let scope = scope_of(r);
            let same = |d: &Declaration| d.name.eq_ignore_ascii_case(&r.name);
            let pick = self
                .declarations
                .iter()
                .position(|d| same(d) && scope.is_some() && d.scope == scope)
                .or_else(|| {
                    self.declarations
                        .iter()
                        .position(|d| same(d) && d.scope.is_none())
                })
                .or_else(|| self.declarations.iter().position(same));
            r.declaration = pick;
        }
    }

    /// The declarations and references of the source `source`.
    pub fn symbols_of(
        &self,
        source: usize,
    ) -> (
        impl Iterator<Item = &Declaration>,
        impl Iterator<Item = &Reference>,
    ) {
        (
            self.declarations.iter().filter(move |d| d.source == source),
            self.references.iter().filter(move |r| r.source == source),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_count_characters_and_bytes() {
        let l = Lines::new("ab\r\n  é name\nlast");
        assert_eq!(l.len(), 3);
        assert_eq!(l.text(2), "  é name");
        let s = l.find(2, 1, "NAME");
        assert_eq!((s.line, s.column, s.end_column), (2, 5, 9));
        assert_eq!(&"ab\r\n  é name\nlast"[s.start..s.end], "name");
        let w = l.line_span(3);
        assert_eq!((w.start, w.end), (14, 18));
    }

    #[test]
    fn find_wants_a_word() {
        let l = Lines::new("     DCUSTNO  CUST");
        assert_eq!(l.find_word(1, 1, "CUST"), Some(15));
        assert_eq!(l.find_word(1, 1, "&X"), None);
        let l = Lines::new("CHGVAR &A (&A + 1)");
        assert_eq!(l.find_word(1, 9, "&A"), Some(12));
    }
}
