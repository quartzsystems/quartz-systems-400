// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The report: the machine's identity, one line per check and the
//! result, as 80-column text (the installer's panel, the console) and
//! as the spooled file's lines.

use std::fmt::Write as _;

/// How a check came out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grade {
    /// Information only; no effect on the result.
    Info,
    /// Usable.
    Pass,
    /// Usable with a limit the line names.
    Warn,
    /// Not usable as it is.
    Fail,
}

impl Grade {
    /// As the report shows it.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Grade::Info => "INFO",
            Grade::Pass => "PASS",
            Grade::Warn => "WARN",
            Grade::Fail => "FAIL",
        }
    }
}

/// One line of the report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    /// The grade.
    pub grade: Grade,
    /// The resource (`DD001`), or the area (`TPM`, `UEFI`).
    pub resource: String,
    /// Type and model as `WRKHDWRSC` shows them; empty for an area.
    pub type_model: String,
    /// What was found and, for a warning or failure, why it matters.
    pub text: String,
}

impl Check {
    /// A check.
    #[must_use]
    pub fn new(grade: Grade, resource: &str, type_model: &str, text: impl Into<String>) -> Self {
        Self {
            grade,
            resource: resource.to_string(),
            type_model: type_model.to_string(),
            text: text.into(),
        }
    }
}

/// The whole run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// The machine as the firmware names it (`Dell Inc. PowerEdge R240`).
    pub machine: String,
    /// The hypervisor, empty on bare metal.
    pub hypervisor: String,
    /// When it ran, `YYYY-MM-DD HH:MM:SS` UTC.
    pub when: String,
    /// The lines, by section.
    pub checks: Vec<(String, Vec<Check>)>,
}

/// The widest line of the text.
pub const WIDTH: usize = 78;

impl Report {
    /// The worst grade of any line; `Pass` when none is worse.
    #[must_use]
    pub fn result(&self) -> Grade {
        self.checks
            .iter()
            .flat_map(|(_, c)| c.iter().map(|c| c.grade))
            .filter(|g| *g != Grade::Info)
            .max()
            .unwrap_or(Grade::Pass)
    }

    /// Lines of each grade: passed, warnings, failed.
    #[must_use]
    pub fn counts(&self) -> (usize, usize, usize) {
        let n = |g| self.checks.iter().flat_map(|(_, c)| c).filter(|c| c.grade == g).count();
        (n(Grade::Pass), n(Grade::Warn), n(Grade::Fail))
    }

    /// The summary line: `Result: WARN (14 passed, 2 warnings, 0 failed)`.
    #[must_use]
    pub fn summary(&self) -> String {
        let (p, w, f) = self.counts();
        format!("{} ({p} passed, {w} warnings, {f} failed)", self.result().word())
    }

    /// The report's lines, none wider than [`WIDTH`].
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut v = vec![
            "QS/400 Hardware Qualification (QSHWQUAL)".to_string(),
            String::new(),
            format!(" Machine  . . . . . . :   {}", self.machine),
            format!(
                " Hypervisor . . . . . :   {}",
                if self.hypervisor.is_empty() { "none (physical machine)" } else { &self.hypervisor }
            ),
            format!(" Release  . . . . . . :   {}", release::RELEASE),
            format!(" Date and time  . . . :   {} UTC", self.when),
            format!(" Result . . . . . . . :   {}", self.summary()),
        ];
        for (section, checks) in &self.checks {
            v.push(String::new());
            v.push(format!(" {section}"));
            v.push(" Result  Resource    Type      Description".to_string());
            for c in checks {
                let head = format!("  {:<6} {:<11} {:<9} ", c.grade.word(), c.resource, c.type_model);
                let mut text = c.text.as_str();
                let room = WIDTH - head.len();
                let mut first = true;
                while !text.is_empty() || first {
                    let cut = wrap_at(text, room);
                    let (part, rest) = text.split_at(cut);
                    if first {
                        v.push(format!("{head}{}", part.trim_end()));
                    } else {
                        v.push(format!("{:width$}{}", "", part.trim_end(), width = head.len()));
                    }
                    text = rest.trim_start();
                    first = false;
                }
            }
        }
        v.push(String::new());
        v.push(" End of report".to_string());
        v
    }

    /// The lines as one text, each ended by a newline.
    #[must_use]
    pub fn text(&self) -> String {
        let mut s = String::new();
        for l in self.lines() {
            let _ = writeln!(s, "{l}");
        }
        s
    }
}

/// Where to cut `text` to fit `room` columns: at the last blank inside
/// it, else at `room` characters.
fn wrap_at(text: &str, room: usize) -> usize {
    if text.chars().count() <= room {
        return text.len();
    }
    let end = text.char_indices().nth(room).map_or(text.len(), |(i, _)| i);
    text[..end].rfind(' ').filter(|i| *i > 0).unwrap_or(end)
}

/// `YYYY-MM-DD HH:MM:SS` of Unix seconds.
#[must_use]
pub fn timestamp(secs: u64) -> String {
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_and_wrapping() {
        let mut r = Report {
            machine: "QEMU Standard PC (Q35 + ICH9, 2009)".into(),
            hypervisor: "KVM".into(),
            when: timestamp(1_793_664_000),
            checks: Vec::new(),
        };
        assert_eq!(r.result(), Grade::Pass);
        r.checks.push((
            "Firmware".into(),
            vec![
                Check::new(Grade::Pass, "UEFI", "", "UEFI firmware"),
                Check::new(Grade::Info, "DMI", "", "x"),
                Check::new(
                    Grade::Warn,
                    "TPM",
                    "",
                    "No TPM was found: QS/400 runs without one, but cannot seal keys to the machine or measure its boot.",
                ),
            ],
        ));
        assert_eq!(r.result(), Grade::Warn);
        assert_eq!(r.summary(), "WARN (1 passed, 1 warnings, 0 failed)");
        let lines = r.lines();
        assert!(lines.iter().all(|l| l.chars().count() <= WIDTH), "{lines:#?}");
        let tpm = lines.iter().position(|l| l.contains("No TPM")).unwrap();
        assert!(lines[tpm + 1].starts_with("                              "));
        assert!(r.text().ends_with(" End of report\n"));
        r.checks[0].1.push(Check::new(Grade::Fail, "DD001", "SSD", "too small"));
        assert_eq!(r.result(), Grade::Fail);
    }

    #[test]
    fn timestamps() {
        assert_eq!(timestamp(0), "1970-01-01 00:00:00");
        assert_eq!(timestamp(1_793_664_000), "2026-11-03 00:00:00");
        assert_eq!(timestamp(951_782_400 + 3661), "2000-02-29 01:01:01");
    }
}
