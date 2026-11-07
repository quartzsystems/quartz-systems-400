// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 132: `JTOpen`'s JDBC scenarios against the guest's database host
//! server, compared with the primary's outcomes.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use super::setup;
use crate::hostsvr::{JOBS, compile, run_class, tool};
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// How a scenario is held to the primary's output.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Match {
    /// The steps and their outcomes.
    Outcomes,
    /// Every line.
    Whole,
    /// Every result line, a failed step by its outcome (the message
    /// texts are each system's own).
    Results,
}

/// The scenarios: name, class, how they compare.
const SCENARIOS: &[(&str, &str, Match)] = &[
    ("jdbc", "Jdbc", Match::Outcomes),
    ("metadata", "MetaData", Match::Whole),
    ("callproc", "CallProc", Match::Results),
    ("callout", "CallOut", Match::Whole),
    ("sqlpkg", "SqlPkg", Match::Outcomes),
];

/// The result lines of a scenario's output.
fn results(out: &str) -> Vec<String> {
    out.lines()
        .filter(|l| l.starts_with("ok") || l.starts_with("fail") || l.starts_with("     "))
        .map(|l| l.trim_end().to_string())
        .collect()
}

/// Each step's outcome and name: `ok   connect`, `fail missing`.
fn outcomes(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| !l.starts_with(' '))
        .map(|l| {
            l.split(':')
                .next()
                .unwrap_or(l)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// Result lines with each failed step cut to its outcome.
fn normalized(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|l| {
            if l.starts_with("fail") {
                outcomes(std::slice::from_ref(l)).concat()
            } else {
                l.clone()
            }
        })
        .collect()
}

/// The first lines that differ, for the report.
fn diff(want: &[String], got: &[String]) -> String {
    let mut out = String::new();
    for i in 0..want.len().max(got.len()) {
        let (w, g) = (want.get(i), got.get(i));
        if w != g {
            let _ = writeln!(out, "  line {}:\n    want {w:?}\n    got  {g:?}", i + 1);
            if out.lines().count() > 30 {
                break;
            }
        }
    }
    out
}

impl Suite {
    /// The host servers up and the `JTOpen` classes compiled: the java
    /// tool and the class path.
    pub(crate) fn jdbc_ready(&self, s: &mut Session) -> Result<(String, String), String> {
        let Ok(jar) = std::env::var("QS_JT400_JAR") else {
            return Err("QS_JT400_JAR is not set: the step needs jt400.jar".into());
        };
        if Command::new(tool("java").0)
            .arg("-version")
            .output()
            .is_err()
        {
            return Err("no java (QS_JAVA_HOME or PATH)".into());
        }
        setup(s)?;
        // New tables in the primary's QSTEST are *PUBLIC *EXCLUDE
        // (oracle/sql-catalog/PRIVS), which the privilege rows show.
        let _ = s.command(
            "CRTLIB LIB(QSTEST) TEXT('Host server tests') CRTAUT(*EXCLUDE)",
            CMD,
        );
        let _ = s.command("CHGLIB LIB(QSTEST) CRTAUT(*EXCLUDE)", CMD);
        let _ = s.command("svc.ifs mkdir /home", CMD);
        let _ = s.command("svc.ifs mkdir /home/QSTEST", CMD);
        s.command("STRHOSTSVR SERVER(*ALL)", CMD)?;
        let until = Instant::now() + Duration::from_secs(90);
        loop {
            let out = s.command("WRKACTJOB", CMD)?;
            if JOBS.iter().all(|j| out.contains(j)) {
                break;
            }
            if Instant::now() >= until {
                return Err(format!("host server jobs missing:\n{out}"));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        compile(self, &jar)
    }

    /// Step 132.
    pub(crate) fn sql_jtopen(&self, s: &mut Session) -> Result<String, String> {
        let (java, cp) = self.jdbc_ready(s)?;
        let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("jtopen/expected/oracle");
        let dir = self.out_dir.join("sql-jtopen");
        let _ = std::fs::create_dir_all(&dir);
        let mut report = Vec::new();
        let mut errors = String::new();
        for (name, class, how) in SCENARIOS {
            let out = run_class(self, &java, &cp, class)?;
            let got = results(&out);
            let _ = std::fs::write(dir.join(format!("{name}.out")), got.join("\n") + "\n");
            let _ = std::fs::write(dir.join(format!("{name}.log")), &out);
            let want_text = std::fs::read_to_string(expected.join(format!("{name}.out")))
                .map_err(|e| format!("{name}.out: {e}"))?;
            let want = results(&want_text);
            let (w, g) = match how {
                Match::Whole => (want, got.clone()),
                Match::Outcomes => (outcomes(&want), outcomes(&got)),
                Match::Results => (normalized(&want), normalized(&got)),
            };
            if w != g {
                let _ = write!(errors, "{name} differs from the primary:\n{}", diff(&w, &g));
            }
            let ok = got.iter().filter(|l| l.starts_with("ok")).count();
            report.push(format!("{name} {ok} ok"));
        }
        if !errors.is_empty() {
            return Err(format!("{errors}(outputs in {})", dir.display()));
        }
        Ok(format!("as the primary: {}", report.join(", ")))
    }
}
