// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The precompiler's message texts, Quartz-written. The identifiers
//! and severities are IBM's; `QSQLMSG` (`runtime/msgf/msgs/qsqlmsg.toml`)
//! repeats each text word for word, and a test keeps the two in step.

use crate::Diag;

const TEXTS: &[(&str, &str)] = &[
    ("SQL0004", "At column &1: member &2 named by INCLUDE was not found."),
    ("SQL0053", "The source holds no SQL statements."),
    ("SQL0084", "The statement is not supported: &1."),
    ("SQL0088", "&1 changes every row of the table."),
    ("SQL0104", "Syntax error at line &1 column &2: &3."),
    ("SQL0199", "&1 must be the first SQL statement of the source."),
    ("SQL0312", "Host variable &1 is not defined or cannot be used here."),
    ("SQL0504", "Cursor &1 is not declared before it is used."),
    ("SQL1103", "The columns of table &1 in &2 could not be found."),
    ("SQL5066", "Precompile option &1 is set by SET OPTION."),
    ("SQL9001", "The SQL precompile of &1 failed; the object was not created."),
];

/// A diagnostic's text with its values.
#[must_use]
pub fn text(d: &Diag) -> String {
    let t = TEXTS.iter().find(|(id, _)| *id == d.msgid).map_or("Diagnostic &1.", |(_, t)| *t);
    let mut s = t.to_string();
    for (i, a) in d.args.iter().enumerate().rev() {
        s = s.replace(&format!("&{}", i + 1), a);
    }
    s
}

/// Every text, for the test that compares them with `QSQLMSG`.
#[must_use]
pub fn texts() -> &'static [(&'static str, &'static str)] {
    TEXTS
}

#[cfg(test)]
mod tests {
    #[test]
    fn texts_match_the_message_file() {
        let files = msgf::source::builtin();
        let f = files.iter().find(|f| f.name == "QSQLMSG").expect("QSQLMSG");
        for (id, t) in super::texts() {
            let d = f.find(id).unwrap_or_else(|| panic!("{id} is not in QSQLMSG"));
            assert_eq!(&d.text, t, "{id}");
        }
    }
}
