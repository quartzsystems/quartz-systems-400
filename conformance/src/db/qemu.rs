// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database steps of the disk suite: the kit built through the
//! service console and compared with the in-memory run, the bench, and
//! the crash under commitment control that C1001082 must undo.

use std::time::{Duration, Instant};

use super::kit::{self, Kit};
use super::local::Local;
use crate::error::HarnessError;
use crate::qemu::session::Session;
use crate::steps::{SRC_IPL_COMPLETE, StepReport, Suite};

const CMD: Duration = Duration::from_secs(20);
const LONG: Duration = Duration::from_secs(300);

/// The SRC of database recovery.
pub const SRC_DB_RECOVERY: &str = "SRC C1001082";

/// Push `text` into the console's xfer area as `name`. Each line is
/// echoed by the tty before the next goes, so nothing is dropped.
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

/// The lines between `--- N records` and `--- end` of a console export.
fn body(out: &str) -> String {
    let mut lines = Vec::new();
    let mut inside = false;
    for l in out.lines() {
        let l = l.trim_end_matches('\r');
        if l.starts_with("--- end") {
            break;
        }
        if inside {
            lines.push(l);
        }
        if l.starts_with("--- ") && l.ends_with(" records") {
            inside = true;
        }
    }
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

fn ok(out: &str, what: &str, needle: &str) -> Result<(), String> {
    if out.contains(needle) {
        Ok(())
    } else {
        Err(format!("{what}:\n{out}"))
    }
}

impl Suite {
    /// Step 13: QSTEST built over the console from the same DDS and CSVs,
    /// the script, every export and query compared with the in-memory
    /// run, then the bench and the access-path check.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn db_kit(&self, s: &mut Session) -> Result<String, String> {
        let kit = Kit::locate();
        let mut local = Local::build(&kit)?;
        local.load(&kit)?;
        local.start_journal()?;
        let report = local.run_rla(&kit)?;
        if report.bad > 0 {
            return Err(format!(
                "the script does not pass in memory:\n{}",
                report.text
            ));
        }
        let queries = local.run_queries(&kit)?;

        let out = s.command(
            &format!("obj crt *LIB QSYS/{} Database conformance", kit::LIBRARY),
            CMD,
        )?;
        ok(&out, "create library", "created")?;
        let out = s.command(&format!("crtsrcpf {}/{}", kit::LIBRARY, kit::SRCFILE), CMD)?;
        ok(&out, "crtsrcpf", "created")?;
        for (name, kind, text) in kit.dds_members().map_err(|e| e.to_string())? {
            push(s, &name, &text)?;
            let out = s.command(
                &format!(
                    "srcput {}/{} {name} SRCTYPE({kind})",
                    kit::LIBRARY,
                    kit::SRCFILE
                ),
                CMD,
            )?;
            ok(&out, &format!("srcput {name}"), "written")?;
            let reuse = if kit::reuse_deleted(&name) {
                " REUSEDLT(*YES)"
            } else {
                ""
            };
            let out = s.command(
                &format!(
                    "crt{} {}/{name} SRCFILE({}/{}){reuse}",
                    kind.to_ascii_lowercase(),
                    kit::LIBRARY,
                    kit::LIBRARY,
                    kit::SRCFILE
                ),
                CMD,
            )?;
            ok(&out, &format!("create {name}"), "created")?;
        }
        let mut loaded = 0u32;
        for name in kit::PHYSICAL {
            let Some(csv) = kit.csv(name) else { continue };
            let csv = csv.map_err(|e| e.to_string())?;
            push(s, &format!("D{name}"), &String::from_utf8_lossy(&csv))?;
            let out = s.command(
                &format!(
                    "cpyfrmimpf FROMSTMF(D{name}) TOFILE({}/{name}) NULLS(*YES) DATFMT(*ISO) TIMFMT(*ISO)",
                    kit::LIBRARY
                ),
                CMD,
            )?;
            ok(&out, &format!("load {name}"), "0 not copied")?;
            loaded += 1;
        }
        let lib = kit::LIBRARY;
        let out = s.command(&format!("crtjrnrcv {lib}/{}", kit::RECEIVER), CMD)?;
        ok(&out, "crtjrnrcv", "created")?;
        let out = s.command(
            &format!(
                "crtjrn {lib}/{} JRNRCV({lib}/{})",
                kit::JOURNAL,
                kit::RECEIVER
            ),
            CMD,
        )?;
        ok(&out, "crtjrn", "created")?;
        let out = s.command(
            &format!(
                "strjrnpf {lib}/CUSTOMER JRN({lib}/{}) IMAGES(*BOTH)",
                kit::JOURNAL
            ),
            CMD,
        )?;
        ok(&out, "strjrnpf", "started")?;

        let script = kit.rla_lines().map_err(|e| e.to_string())?.join("\n");
        push(s, "RLA", &script)?;
        let out = s.command("rla RLA", LONG)?;
        if !out.contains("0 mismatch(es)") {
            return Err(format!("rla over the console:\n{out}"));
        }

        let mut compared = 0u32;
        for name in kit::PHYSICAL.iter().chain(kit::EXPORTED_LOGICAL.iter()) {
            let out = s.command(
                &format!("cpytoimpf {lib}/{name} NULLS(*YES) DATFMT(*ISO) TIMFMT(*ISO)"),
                LONG,
            )?;
            let theirs = body(&out);
            let ours = local.export(name)?;
            if theirs != ours {
                return Err(format!(
                    "{name} differs between the console and memory:\n--- console\n{theirs}--- memory\n{ours}"
                ));
            }
            compared += 1;
        }
        for (i, (q, r)) in queries.iter().enumerate() {
            let rs = r.as_ref().map_err(|e| format!("query {}: {e}", i + 1))?;
            let out = s.command(&format!("sql {q};"), LONG)?;
            let want = rs.render();
            if !out.contains(want.trim_end()) {
                return Err(format!(
                    "query {} differs:\n--- console\n{out}\n--- memory\n{want}",
                    i + 1
                ));
            }
        }
        let n = self.bench;
        let out = s.command(&format!("dbbench {n}"), Duration::from_secs(300 + n / 50))?;
        let bench = out
            .lines()
            .find(|l| l.starts_with("insert"))
            .ok_or_else(|| format!("no bench result:\n{out}"))?
            .to_string();
        let check = s.command("chkstg", LONG)?;
        if !check.contains("access path(s): 0 problem(s)") {
            return Err(format!("chkstg after the kit:\n{check}"));
        }
        Ok(format!(
            "{} files from DDS, {loaded} loaded, script {} ok, {compared} exports and {} queries match memory; {bench}; access paths clean",
            kit::PHYSICAL.len() + kit::LOGICAL.len(),
            report.ok,
            queries.len()
        ))
    }

    /// Step 14: an update of the journaled CUSTOMER under commitment
    /// control, the guest killed before the commit; the next IPL reports
    /// C1001082 and the file is back as it was.
    pub(crate) fn db_crash(&self) -> Result<StepReport, HarnessError> {
        let name = "14 database crash";
        let start = Instant::now();
        let mut s = Session::spawn(&self.launch)?;
        let fail = |s: &Session, reason: String| HarnessError::Failed {
            step: name,
            reason,
            log: s.log.clone(),
        };
        match self.crash_first(&mut s) {
            Ok(()) => {}
            Err(e) => return Err(fail(&s, e)),
        }
        s.kill();
        let mut s = Session::spawn(&self.launch)?;
        let detail = match self.crash_second(&mut s) {
            Ok(d) => d,
            Err(e) => return Err(fail(&s, e)),
        };
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(15));
        Ok(StepReport {
            name,
            elapsed: start.elapsed(),
            detail,
        })
    }

    fn balance(s: &mut Session) -> Result<String, String> {
        let out = s.command(
            &format!("sql SELECT SUM(BAL) FROM {}/CUSTOMER;", kit::LIBRARY),
            CMD,
        )?;
        out.lines()
            .map(str::trim)
            .find(|l| l.parse::<f64>().is_ok())
            .map(str::to_string)
            .ok_or_else(|| format!("no balance in:\n{out}"))
    }

    fn crash_first(&self, s: &mut Session) -> Result<(), String> {
        s.expect(SRC_IPL_COMPLETE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let before = Self::balance(s)?;
        let out = s.command("strcmtctl LCKLVL(*CHG)", CMD)?;
        ok(&out, "strcmtctl", "started")?;
        let out = s.command(
            &format!("sql UPDATE {}/CUSTOMER SET BAL = BAL + 1000;", kit::LIBRARY),
            CMD,
        )?;
        ok(&out, "update under commitment control", "row(s) affected")?;
        let during = Self::balance(s)?;
        if during == before {
            return Err(format!(
                "the update did not show in the job: {before} before and after"
            ));
        }
        Ok(())
    }

    fn crash_second(&self, s: &mut Session) -> Result<String, String> {
        let seen = s.expect(SRC_DB_RECOVERY, self.boot_timeout)?;
        let line = seen
            .lines()
            .find(|l| l.contains("database recovery"))
            .unwrap_or("")
            .to_string();
        s.expect(SRC_IPL_COMPLETE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let after = Self::balance(s)?;
        let kit = Kit::locate();
        let mut local = Local::build(&kit)?;
        local.load(&kit)?;
        local.start_journal()?;
        local.run_rla(&kit)?;
        let want = local.txn(|txn, ctx| {
            let p = sql::prepare(
                &format!("SELECT SUM(BAL) FROM {}/CUSTOMER", kit::LIBRARY),
                sql::Naming::Sys,
            )
            .map_err(|e| e.to_string())?;
            match p.execute(txn, ctx, &[]).map_err(|e| e.to_string())? {
                sql::Outcome::Rows(rs) => Ok(rs.rows[0][0].to_string()),
                _ => Err("not a query".into()),
            }
        })?;
        if after.parse::<f64>().ok() != want.parse::<f64>().ok() {
            return Err(format!("balance after recovery is {after}, want {want}"));
        }
        let check = s.command("chkstg", LONG)?;
        if !check.contains("access path(s): 0 problem(s)") {
            return Err(format!("chkstg after recovery:\n{check}"));
        }
        let jrn = s.command(
            &format!("dspjrn {}/{} JRNCDE(C)", kit::LIBRARY, kit::JOURNAL),
            CMD,
        )?;
        if !jrn.contains(" RB ") {
            return Err(format!("no rollback entry in the journal:\n{jrn}"));
        }
        Ok(format!(
            "{line}; balance back to {after}; C RB journaled; access paths clean"
        ))
    }
}
