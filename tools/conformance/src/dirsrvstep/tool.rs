// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The host's side: `tools/ldaptest` (outside the workspace, built here
//! once a run, or `QS400_LDAPTEST`), its runs and their outcomes.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The binary once built.
static BUILT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// The `ldaptest` binary: `QS400_LDAPTEST`, or built into the target
/// directory.
pub(crate) fn binary() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("QS400_LDAPTEST") {
        return Ok(PathBuf::from(p));
    }
    if let Some(p) = BUILT.get() {
        return Ok(p.clone());
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = crate::target_dir().join("ldaptest");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let st = Command::new(cargo)
        .current_dir(&repo)
        .args([
            "build",
            "--release",
            "--locked",
            "--manifest-path",
            "tools/ldaptest/Cargo.toml",
        ])
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .map_err(|e| format!("cargo build ldaptest: {e}"))?;
    if !st.success() {
        return Err("cargo build ldaptest failed".into());
    }
    let p = target.join("release").join(if cfg!(windows) {
        "ldaptest.exe"
    } else {
        "ldaptest"
    });
    Ok(BUILT.get_or_init(|| p).clone())
}

/// One run of `ldaptest`.
#[derive(Debug)]
pub(crate) struct Run {
    /// The command line, for the reports.
    pub line: String,
    /// Its exit code (`None`: killed at the limit).
    pub code: Option<i32>,
    /// What it printed.
    pub out: String,
    /// Its standard error.
    pub err: String,
}

impl Run {
    /// The run as text for an error.
    pub fn text(&self) -> String {
        format!(
            "{} (exit {:?})\n{}{}",
            self.line, self.code, self.out, self.err
        )
    }
}

/// Runs `ldaptest args` with `env`, killed after `limit`.
pub(crate) fn run(args: &[String], env: &[(&str, &str)], limit: Duration) -> Result<Run, String> {
    let bin = binary()?;
    let line = format!("ldaptest {}", args.join(" "));
    let mut cmd = Command::new(&bin);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().map_err(|e| format!("{line}: {e}"))?;
    let mut so = child.stdout.take().ok_or("no stdout")?;
    let mut se = child.stderr.take().ok_or("no stderr")?;
    let out = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = so.read_to_string(&mut s);
        s
    });
    let err = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = se.read_to_string(&mut s);
        s
    });
    let end = Instant::now() + limit;
    let code = loop {
        if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
            break st.code();
        }
        if Instant::now() > end {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    Ok(Run {
        line,
        code,
        out: out.join().unwrap_or_default(),
        err: err.join().unwrap_or_default(),
    })
}

/// A scenario run read back.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Outcome {
    /// Scenarios that passed.
    pub ok: Vec<String>,
    /// `fail NAME: why` lines, with the indented lines after them.
    pub failed: Vec<String>,
    /// `skip NAME: why` lines.
    pub skipped: Vec<String>,
}

impl Outcome {
    /// From `ldaptest`'s output.
    pub fn of(out: &str) -> Self {
        let mut o = Outcome::default();
        let mut in_fail = false;
        for l in out.lines() {
            if let Some(n) = l.strip_prefix("ok ") {
                o.ok.push(n.trim().to_string());
                in_fail = false;
            } else if l.starts_with("fail ") {
                o.failed.push(l.to_string());
                in_fail = true;
            } else if l.starts_with("skip ") {
                o.skipped.push(l.to_string());
                in_fail = false;
            } else if in_fail
                && l.starts_with("  ")
                && let Some(last) = o.failed.last_mut()
            {
                last.push('\n');
                last.push_str(l);
            }
        }
        o
    }

    /// The scenario name of a `fail`/`skip` line.
    pub fn name(line: &str) -> &str {
        line.split_once(' ')
            .map_or(line, |(_, r)| r.split(':').next().unwrap_or(r))
            .trim()
    }

    /// Whether `name` passed.
    pub fn passed(&self, name: &str) -> bool {
        self.ok.iter().any(|n| n == name)
    }
}

/// The lines of `ours` and `theirs` (comments and blank lines left out)
/// that differ, as a unified-looking list: `- theirs`, `+ ours`, by
/// scenario.
pub(crate) fn diff(theirs: &str, ours: &str) -> Vec<String> {
    let body = |t: &str| -> Vec<String> {
        t.lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .map(str::to_string)
            .collect()
    };
    let (a, b) = (body(theirs), body(ours));
    // By scenario: the lines under each `ok`/`fail`/`skip` heading.
    let group = |v: &[String]| -> Vec<(String, Vec<String>)> {
        let mut g: Vec<(String, Vec<String>)> = Vec::new();
        for l in v {
            if l.starts_with("  ") {
                if let Some(last) = g.last_mut() {
                    last.1.push(l.clone());
                }
            } else {
                g.push((l.clone(), Vec::new()));
            }
        }
        g
    };
    let (ga, gb) = (group(&a), group(&b));
    let mut out = Vec::new();
    let key = |h: &str| Outcome::name(h).to_string();
    for (h, lines) in &ga {
        let k = key(h);
        let Some((hb, lb)) = gb.iter().find(|(x, _)| key(x) == k) else {
            out.push(format!("- {h} (no such scenario in ours)"));
            continue;
        };
        let mut d = Vec::new();
        if h != hb {
            d.push(format!("- {h}"));
            d.push(format!("+ {hb}"));
        }
        for l in lines {
            if !lb.contains(l) {
                d.push(format!("- {l}"));
            }
        }
        for l in lb {
            if !lines.contains(l) {
                d.push(format!("+ {l}"));
            }
        }
        if !d.is_empty() {
            out.push(format!("@ {k}"));
            out.extend(d);
        }
    }
    for (h, _) in &gb {
        if !ga.iter().any(|(x, _)| key(x) == key(h)) {
            out.push(format!("+ {h} (not in the primary's)"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_outcomes() {
        let out = "ok rootdse\n  attributes: x\nfail filters: (cn~=x)\n  got 0\nskip sasl-external: no --client-cert\nok compare\n";
        let o = Outcome::of(out);
        assert_eq!(o.ok, ["rootdse", "compare"]);
        assert_eq!(o.failed, ["fail filters: (cn~=x)\n  got 0"]);
        assert_eq!(Outcome::name(&o.failed[0]), "filters");
        assert_eq!(Outcome::name(&o.skipped[0]), "sasl-external");
        assert!(o.passed("compare"));
    }

    #[test]
    fn diffs_by_scenario() {
        let theirs = "# head\n\nok rootdse\n  vendorname: IBM\n  port: 389\nok compare\n  rc 6\n";
        let ours = "ok rootdse\n  vendorname: Quartz\n  port: 389\nfail compare: x\n";
        let d = diff(theirs, ours);
        assert_eq!(
            d,
            [
                "@ rootdse",
                "-   vendorname: IBM",
                "+   vendorname: Quartz",
                "@ compare",
                "- ok compare",
                "+ fail compare: x",
                "-   rc 6",
            ]
        );
    }
}
