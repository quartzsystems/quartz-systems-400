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

/// The scenarios: name, class, and whether every line must match the
/// primary's (else the steps and their outcomes).
const SCENARIOS: &[(&str, &str, bool)] = &[
    ("jdbc", "Jdbc", false),
    ("metadata", "MetaData", true),
    ("callproc", "CallProc", false),
    ("sqlpkg", "SqlPkg", false),
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
        .map(|l| l.split(':').next().unwrap_or(l).split_whitespace().collect::<Vec<_>>().join(" "))
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
    /// Step 132.
    pub(crate) fn sql_jtopen(&self, s: &mut Session) -> Result<String, String> {
        let Ok(jar) = std::env::var("QS_JT400_JAR") else {
            return Err("QS_JT400_JAR is not set: the step needs jt400.jar".into());
        };
        if Command::new(tool("java").0).arg("-version").output().is_err() {
            return Err("no java (QS_JAVA_HOME or PATH)".into());
        }
        setup(s)?;
        let _ = s.command("CRTLIB LIB(QSTEST) TEXT('Host server tests')", CMD);
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
        let (java, cp) = compile(self, &jar)?;
        let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("jtopen/expected/oracle");
        let dir = self.out_dir.join("sql-jtopen");
        let _ = std::fs::create_dir_all(&dir);
        let mut report = Vec::new();
        let mut errors = String::new();
        for (name, class, whole) in SCENARIOS {
            let out = run_class(self, &java, &cp, class)?;
            let got = results(&out);
            let _ = std::fs::write(dir.join(format!("{name}.out")), got.join("\n") + "\n");
            let _ = std::fs::write(dir.join(format!("{name}.log")), &out);
            let want_text = std::fs::read_to_string(expected.join(format!("{name}.out"))).map_err(|e| format!("{name}.out: {e}"))?;
            let want = results(&want_text);
            let (w, g) = if *whole { (want, got.clone()) } else { (outcomes(&want), outcomes(&got)) };
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
