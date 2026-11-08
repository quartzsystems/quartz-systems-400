// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Types several services share (docs/QAP.md, "Common types"; version
//! 8 adds the source and diagnostic types).

use ipc::{wire_enum, wire_struct};

/// A job: name, user, six-digit number.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct JobId {
    /// The job name.
    pub name: String,
    /// Its user.
    pub user: String,
    /// Its number, six digits.
    pub number: String,
}

wire_struct!(JobId { name, user, number });

/// An object by library and name; the library may be `*LIBL` or
/// `*CURLIB` where a library list search applies.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ObjectName {
    /// The library.
    pub library: String,
    /// The object.
    pub name: String,
}

wire_struct!(ObjectName { library, name });

/// How work ends (`Manage` from version 7; a common type from version
/// 8, `Jobs` using it too). Its values are version 7's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EndHow {
    /// `*CNTRLD`: jobs are told and given the delay.
    #[default]
    Controlled,
    /// `*IMMED`.
    Immediate,
}

wire_enum!(EndHow {
    0 => Controlled,
    1 => Immediate,
});

/// A source member: a source physical file and one of its members
/// (version 8).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct MemberName {
    /// The file; its library may be `*LIBL` or `*CURLIB`.
    pub file: ObjectName,
    /// The member.
    pub member: String,
}

wire_struct!(MemberName { file, member });

/// One record of a source member (version 8).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceRecord {
    /// The sequence number in hundredths: 100 is `0001.00`.
    pub seq: u32,
    /// The date, `yymmdd`; 0 for none.
    pub date: u32,
    /// The data, UTF-8, trailing blanks removed.
    pub data: String,
}

wire_struct!(SourceRecord { seq, date, data });

/// Where a diagnostic points (version 8).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum Where {
    /// A line of a source member.
    Member {
        /// The member.
        member: MemberName,
    },
    /// A line of a stream file.
    Path {
        /// Its path.
        path: String,
    },
    /// Nowhere in the source.
    #[default]
    None,
}

wire_enum!(Where {
    0 => Member { member },
    1 => Path { path },
    2 => None,
});

/// One message a compiler reported (version 8).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagnostic {
    /// The source it points into (`where` in docs/QAP.md).
    pub r#where: Where,
    /// The line, from 1; 0 when unknown.
    pub line: u32,
    /// The column, from 1; 0 when unknown.
    pub column: u16,
    /// The last line it covers; 0 when it is `line`.
    pub end_line: u32,
    /// The last column; 0 when unknown.
    pub end_column: u16,
    /// The message's severity.
    pub severity: u8,
    /// The message identifier (`RNF7030`).
    pub id: String,
    /// The first-level text.
    pub text: String,
    /// The second-level text.
    pub help: String,
}

wire_struct!(Diagnostic {
    r#where,
    line,
    column,
    end_line,
    end_column,
    severity,
    id,
    text,
    help
});

impl From<compdiag::MemberName> for MemberName {
    fn from(m: compdiag::MemberName) -> Self {
        Self {
            file: ObjectName {
                library: m.library,
                name: m.file,
            },
            member: m.member,
        }
    }
}

impl From<MemberName> for compdiag::MemberName {
    fn from(m: MemberName) -> Self {
        Self {
            library: m.file.library,
            file: m.file.name,
            member: m.member,
        }
    }
}

impl From<compdiag::Where> for Where {
    fn from(w: compdiag::Where) -> Self {
        match w {
            compdiag::Where::Member(m) => Where::Member { member: m.into() },
            compdiag::Where::Path(path) => Where::Path { path },
            compdiag::Where::None => Where::None,
        }
    }
}

impl From<Where> for compdiag::Where {
    fn from(w: Where) -> Self {
        match w {
            Where::Member { member } => compdiag::Where::Member(member.into()),
            Where::Path { path } => compdiag::Where::Path(path),
            Where::None => compdiag::Where::None,
        }
    }
}

impl From<compdiag::Diagnostic> for Diagnostic {
    fn from(d: compdiag::Diagnostic) -> Self {
        Self {
            r#where: d.at.into(),
            line: d.line,
            column: d.column,
            end_line: d.end_line,
            end_column: d.end_column,
            severity: d.severity,
            id: d.id,
            text: d.text,
            help: d.help,
        }
    }
}

impl From<Diagnostic> for compdiag::Diagnostic {
    fn from(d: Diagnostic) -> Self {
        Self {
            at: d.r#where.into(),
            line: d.line,
            column: d.column,
            end_line: d.end_line,
            end_column: d.end_column,
            severity: d.severity,
            id: d.id,
            text: d.text,
            help: d.help,
        }
    }
}
