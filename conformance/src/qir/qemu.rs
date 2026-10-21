// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 15: the IR kit through the service console. Every case not
//! marked `; qemu: no` is pushed as xfer buffers, assembled with
//! `qir asm`, bound with `crtpgm`, called, and its console lines
//! compared with the expected file's.

use std::fs;
use std::time::Duration;

use super::{Case, Kit};
use crate::qemu::session::Session;
use crate::steps::Suite;

const CMD: Duration = Duration::from_secs(30);

fn push(s: &mut Session, name: &str, text: &str) -> Result<(), String> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return Err(format!("{name}: nothing to push"));
    }
    s.send(&format!("xfer put {name} {}", lines.len()))?;
    for l in &lines {
        s.send(l)?;
        if !l.trim().is_empty() {
            s.expect(l.trim_end(), CMD)?;
        }
    }
    let out = s.expect(crate::PROMPT, CMD)?;
    if !out.contains("lines stored") {
        return Err(format!("xfer put {name}: {out}"));
    }
    Ok(())
}

/// The module name and activation group of a module text.
fn header(text: &str) -> (String, String) {
    let line = text
        .lines()
        .find(|l| l.starts_with("module "))
        .unwrap_or("module X");
    let mut words = line.split_whitespace();
    let name = words.nth(1).unwrap_or("X").to_string();
    let actgrp = words.find(|w| w.starts_with("actgrp(")).map_or_else(
        || "*CALLER".to_string(),
        |w| {
            w.trim_start_matches("actgrp(")
                .trim_end_matches(')')
                .to_string()
        },
    );
    (name, actgrp)
}

/// The console lines of an expected file.
fn expected_console(text: &str) -> Vec<String> {
    text.lines()
        .skip_while(|l| *l != "console:")
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .map(|l| l[2..].to_string())
        .collect()
}

/// The lines a `call` printed: everything up to the prompt, minus
/// the echoed command and the parameter report.
fn call_lines(out: &str) -> Vec<String> {
    out.lines()
        .map(str::trim_end)
        .filter(|l| {
            !l.is_empty()
                && !l.starts_with("call ")
                && !l.starts_with("Parm ")
                && !l.starts_with(crate::PROMPT.trim_end())
        })
        .map(str::to_string)
        .collect()
}

// The steps share one signature so `step` can take them as fn pointers.
#[allow(clippy::unused_self)]
impl Suite {
    /// Step 15: the IR kit over the console.
    pub(crate) fn qir_kit(&self, s: &mut Session) -> Result<String, String> {
        let kit = Kit::locate();
        let cases: Vec<Case> = kit
            .cases()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|c| c.qemu)
            .collect();
        let mut ran = 0;
        let mut mismatched = Vec::new();
        for case in &cases {
            let expected =
                fs::read_to_string(kit.expected_dir().join(format!("{}.out", case.name)))
                    .map_err(|e| format!("{}: {e}", case.name))?;
            let want = expected_console(&expected);
            let mut main = None;
            for (i, m) in case.modules.iter().enumerate() {
                let (name, actgrp) = header(m);
                let buf = format!("{}M{i}", &case.name[..case.name.len().min(7)]);
                push(s, &buf, m)?;
                let out = s.command(&format!("qir asm {buf} MODULE(QGPL/{name})"), CMD)?;
                if !out.contains("created") {
                    return Err(format!("{}: qir asm {name}:\n{out}", case.name));
                }
                let out = s.command(
                    &format!(
                        "crtpgm QGPL/{name} MODULE(QGPL/{name}) ACTGRP({actgrp}) OPTION(*UNRSLVREF)"
                    ),
                    CMD,
                )?;
                if !out.contains("created") {
                    return Err(format!("{}: crtpgm {name}:\n{out}", case.name));
                }
                if main.is_none() {
                    main = Some(name);
                }
            }
            let main = main.ok_or_else(|| format!("{}: no module", case.name))?;
            for r in &case.replies {
                s.command(&format!("rpy {r}"), CMD)?;
            }
            let out = s.command(&format!("call QGPL/{main} {}", case.parms), CMD)?;
            let got = call_lines(&out);
            ran += 1;
            if got != want {
                mismatched.push(format!(
                    "{}:\n  expected {:?}\n  got      {:?}",
                    case.name, want, got
                ));
            }
        }
        if !mismatched.is_empty() {
            return Err(format!(
                "{} of {ran} cases differ from the in-memory run:\n{}",
                mismatched.len(),
                mismatched.join("\n")
            ));
        }
        Ok(format!(
            "{ran} IR kit cases ran over the console and matched"
        ))
    }
}
