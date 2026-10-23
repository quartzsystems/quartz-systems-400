// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The normalized outcome of one corpus member: whether the object was
//! created, the diagnostics, how the call ended, the spooled files it
//! wrote and the messages it left. Both systems are reduced to this
//! text; lines starting `#` are provenance and do not compare.

use std::fmt::Write as _;

use crate::listing::Diag;

/// How the call ended.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Run {
    /// Not called (not created, or compile-only).
    #[default]
    None,
    /// Returned normally.
    Ok,
    /// Ended with an escape message to the caller.
    Escape(String),
}

/// A spooled file the call wrote.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spooled {
    /// The spooled file name (the printer file's).
    pub file: String,
    /// Its printed lines as `page/line text`, blank lines left out.
    pub lines: Vec<String>,
}

/// Records with first-character forms control (`1` new page, ` ` `0`
/// `-` one to three lines, `+` overprint) as `page/line text` rows of
/// the lines that print something. Lines holding `drop` are left out
/// (an oracle's page footer).
#[must_use]
pub fn fcfc_rows(records: &[String], drop: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let (mut page, mut line) = (0u32, 0u32);
    for r in records {
        let mut chars = r.chars();
        let cc = chars.next().unwrap_or(' ');
        let text: String = chars.collect::<String>().trim_end().to_string();
        match cc {
            '1' => {
                page += 1;
                line = 1;
            }
            '0' => line += 2,
            '-' => line += 3,
            '+' => {}
            _ => line += 1,
        }
        if page == 0 {
            page = 1;
        }
        if text.trim().is_empty() || drop.iter().any(|d| text.contains(d)) {
            continue;
        }
        out.push(format!("{page}/{line:<3} {text}"));
    }
    out
}

/// A job log message the call left.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Logged {
    /// The message identifier; blank for an immediate message.
    pub msgid: String,
    /// `*INFO`, `*ESCAPE`, `*INQ`, ...
    pub kind: String,
    /// The replacement data in hexadecimal.
    pub data: String,
}

/// One member's outcome.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Provenance lines, without the leading `# `.
    pub provenance: Vec<String>,
    /// The object was created.
    pub created: bool,
    /// The completion or failure message of the compile command.
    pub cause: String,
    /// The listing's diagnostics.
    pub diags: Vec<Diag>,
    /// The call.
    pub run: Run,
    /// Spooled files of the call, in creation order.
    pub spool: Vec<Spooled>,
    /// Job log messages of the call, in order.
    pub log: Vec<Logged>,
}

impl Outcome {
    /// The text form.
    #[must_use]
    pub fn render(&self) -> String {
        let mut s = String::new();
        for p in &self.provenance {
            let _ = writeln!(s, "# {p}");
        }
        let _ = writeln!(
            s,
            "created: {}{}",
            if self.created { "yes" } else { "no" },
            if self.cause.is_empty() {
                String::new()
            } else {
                format!(" ({})", self.cause)
            }
        );
        for d in &self.diags {
            let _ = writeln!(s, "diag: {} {:02} line {} seq {}", d.msgid, d.severity, d.line, d.seq);
        }
        match &self.run {
            Run::None => {}
            Run::Ok => s.push_str("run: ok\n"),
            Run::Escape(id) => {
                let _ = writeln!(s, "run: escape {id}");
            }
        }
        for f in &self.spool {
            let _ = writeln!(s, "spool: {}", f.file);
            for l in &f.lines {
                let _ = writeln!(s, "  |{l}");
            }
        }
        for m in &self.log {
            let id = if m.msgid.is_empty() { "-" } else { m.msgid.as_str() };
            let _ = writeln!(s, "log: {id} {} {}", m.kind, m.data);
        }
        s
    }
}

/// The text without its provenance lines, for comparison.
#[must_use]
pub fn comparable(text: &str) -> String {
    let mut out = String::new();
    for l in text.lines().filter(|l| !l.starts_with('#')) {
        out.push_str(l.trim_end_matches('\r'));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_and_drops_provenance() {
        let o = Outcome {
            provenance: vec!["pub400 V7R5M0 2026-10-23".into()],
            created: true,
            run: Run::Ok,
            spool: vec![Spooled {
                file: "QSYSPRT".into(),
                lines: fcfc_rows(&["1Hello".into(), "-".into(), " x -=* foot".into()], &["-=*"]),
            }],
            ..Outcome::default()
        };
        let t = o.render();
        assert!(t.starts_with("# pub400"));
        assert_eq!(
            comparable(&t),
            "created: yes\nrun: ok\nspool: QSYSPRT\n  |1/1   Hello\n"
        );
    }
}
