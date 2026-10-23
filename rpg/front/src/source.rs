// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Source members as the compiler reads them: records with the
//! sequence and change-date columns, and the trait through which
//! `/COPY` and `/INCLUDE` find other members.

/// One source record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// The sequence number as six digits (`000100` for `0001.00`).
    pub seq: String,
    /// The change date as six digits (`yymmdd`), `000000` for none.
    pub date: String,
    /// The source data.
    pub text: String,
}

/// A source member.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Member {
    /// Library.
    pub lib: String,
    /// Source file.
    pub file: String,
    /// Member.
    pub name: String,
    /// The source file's CCSID.
    pub ccsid: u16,
    /// The records.
    pub records: Vec<Record>,
}

impl Member {
    /// A member from plain text: sequence numbers `000100`, `000200`,
    /// ..., zero dates, as an FTP upload or `svc.xfer` makes them.
    #[must_use]
    pub fn from_text(lib: &str, file: &str, name: &str, text: &str) -> Self {
        let records = text
            .replace("\r\n", "\n")
            .lines()
            .enumerate()
            .map(|(i, l)| Record {
                seq: format!("{:06}", (i + 1) * 100),
                date: "000000".into(),
                text: l.trim_end().to_string(),
            })
            .collect();
        Self {
            lib: lib.into(),
            file: file.into(),
            name: name.into(),
            ccsid: 37,
            records,
        }
    }
}

/// How `/COPY` and `/INCLUDE` reach other members.
pub trait Includes {
    /// The member `lib/file,name` (`lib` is `*LIBL` when not given).
    fn member(&mut self, lib: &str, file: &str, name: &str) -> Option<Member>;
}

/// No members at all.
pub struct NoIncludes;

impl Includes for NoIncludes {
    fn member(&mut self, _lib: &str, _file: &str, _name: &str) -> Option<Member> {
        None
    }
}

/// Members held in memory, by `FILE,NAME` (any library).
#[derive(Clone, Debug, Default)]
pub struct MemIncludes {
    /// The members.
    pub members: Vec<Member>,
}

impl Includes for MemIncludes {
    fn member(&mut self, lib: &str, file: &str, name: &str) -> Option<Member> {
        self.members
            .iter()
            .find(|m| {
                m.file.eq_ignore_ascii_case(file)
                    && m.name.eq_ignore_ascii_case(name)
                    && (lib == "*LIBL" || m.lib.eq_ignore_ascii_case(lib))
            })
            .cloned()
    }
}
