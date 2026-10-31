// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! No command of `QSYS` is the Phase 7 text-listing shim: every command
//! that opens a display at a 5250 session runs in the job, where a panel
//! driver answers it (a match arm naming it in `work/qsjob/src`), and no
//! command's bridge word is one only the console process answers.
//! `docs/CPP_AUDIT.md` is the table behind this.

use std::path::{Path, PathBuf};

use sys_cpps::inventory::{JOB_SITE, is_display};
use sys_cpps::{Site, all};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The command names a match arm of the job's sources names.
fn job_arms() -> Vec<String> {
    let mut files = Vec::new();
    sources(&root().join("work/qsjob/src"), &mut files);
    let mut out = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        let arm = |l: &&str| l.contains("=>") || l.contains(".name == \"") || l.contains(".name != \"");
        for line in text.lines().filter(arm) {
            let head = line.split("=>").next().unwrap_or("");
            for part in head.split('"').skip(1).step_by(2) {
                out.push(part.to_string());
            }
        }
    }
    out
}

/// The console process's own words (`lic/slic/src/procs/svcjob.rs`).
fn console_words() -> Vec<String> {
    let text = std::fs::read_to_string(root().join("lic/slic/src/procs/svcjob.rs")).expect("svcjob.rs");
    let at = text.find("pub const LOCAL").expect("LOCAL");
    let body = &text[at..];
    let body = &body[..body.find("];").expect("end of LOCAL")];
    body.split('"').skip(1).step_by(2).map(str::to_string).collect()
}

#[test]
fn every_display_runs_in_the_job() {
    let mut wrong = Vec::new();
    for e in all().iter().filter(|e| is_display(&e.name) && e.name != "GO") {
        if e.site != Site::Job || !JOB_SITE.contains(&e.name.as_str()) {
            wrong.push(format!("{} ({:?})", e.name, e.site));
        }
    }
    assert!(wrong.is_empty(), "displays answered by the service's text:\n  {}", wrong.join("\n  "));
}

#[test]
fn every_display_has_a_driver() {
    let arms = job_arms();
    let missing: Vec<&str> = all()
        .iter()
        .filter(|e| is_display(&e.name) && e.name != "GO")
        .map(|e| e.name.as_str())
        .filter(|n| !arms.iter().any(|a| a == n))
        .collect();
    assert!(missing.is_empty(), "displays with no panel driver:\n  {}", missing.join("\n  "));
}

/// Bridged to `hw` until the hardware resource service answers them
/// (the hwrsrc agent's change removes them from this list).
const HWRSRC: &[&str] = &["DSPHDWRSC", "WRKHDWRSC"];

#[test]
fn no_command_bridges_to_a_console_word() {
    let words = console_words();
    assert!(words.iter().any(|w| w == "hw"), "LOCAL read: {words:?}");
    let wrong: Vec<String> = all()
        .iter()
        .filter(|e| words.contains(&e.word) && !HWRSRC.contains(&e.name.as_str()))
        .map(|e| format!("{} (word {})", e.name, e.word))
        .collect();
    assert!(wrong.is_empty(), "commands bridged to the console process:\n  {}", wrong.join("\n  "));
}
