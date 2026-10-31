// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 11 QEMU step: the problem log, dumps and traces at the service
//! console (`docs/SERVICE.md`).

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

fn want(out: &str, what: &str, needles: &[&str]) -> Result<(), String> {
    if needles.iter().all(|n| out.contains(n)) {
        Ok(())
    } else {
        Err(format!("{what}: expected {needles:?}:\n{out}"))
    }
}

/// The identifier after `Problem ID` in `DSPPRB`'s text.
fn problem_id(out: &str, symptom: &str) -> Option<String> {
    let mut id = None;
    for line in out.lines() {
        if let Some(rest) = line.trim().strip_prefix("Problem ID ") {
            id = rest.split_whitespace().next().map(str::to_string);
        }
        if line.contains(symptom) {
            return id;
        }
    }
    None
}

// The steps share one signature so `step` can take them as fn pointers.
#[allow(clippy::unused_self)]
impl Suite {
    /// Step 74: a severe message to `QSYSOPR` logs a problem `WRKPRB`
    /// and `DSPPRB` show; `DMPOBJ` prints a dump and logs one; a system
    /// dump is taken and listed; `STRTRC`, `ENDTRC` and `PRTTRC` trace
    /// the console's job and print it.
    pub(crate) fn service(&self, s: &mut Session) -> Result<String, String> {
        // A break message to QSYSOPR has severity 99: a problem.
        s.command("sndbrkmsg MSG('QS74 severe test') TOMSGQ(QSYSOPR)", CMD)?;
        let list = s.command("wrkprb", CMD)?;
        want(&list, "WRKPRB", &["OPENED"])?;
        let all = s.command("dspprb", CMD)?;
        let id = problem_id(&all, "QS74 severe test")
            .ok_or_else(|| format!("no problem for the break message:\n{all}"))?;
        let one = s.command(&format!("dspprb PRBID({id})"), CMD)?;
        want(&one, "DSPPRB of the problem", &[&id, "QS74 severe test", "MSG/*IMMED", "type 1"])?;
        // A formatted object dump to QPSRVDMP logs a problem too.
        let dmp = s.command("dmpobj OBJ(QGPL/QDFTJOBD) OBJTYPE(*JOBD)", CMD)?;
        want(&dmp, "DMPOBJ", &["printed", "problem"])?;
        // A system dump: taken, listed, a problem.
        let take = s.command("svc.sysdump take", CMD)?;
        want(&take, "system dump", &["System dump QSD", "taken"])?;
        let name = take
            .split_whitespace()
            .find(|w| w.starts_with("QSD"))
            .unwrap_or_default()
            .to_string();
        let dumps = s.command("svc.sysdump list", CMD)?;
        want(&dumps, "dump list", &[&name])?;
        let shown = s.command(&format!("svc.sysdump show {name}"), CMD)?;
        want(&shown, "dump contents", &["Job table", "Licensed Internal Code log"])?;
        let all = s.command("dspprb", CMD)?;
        want(&all, "problems after the dumps", &[&format!("DUMP/{name}"), "DUMP/QGPL/QDFTJOBD"])?;
        // A job trace of the console's own job.
        let st = s.command("strtrc SSNID(T74) JOB(*)", CMD)?;
        want(&st, "STRTRC", &["T74", "started"])?;
        s.command("crtlib LIB(QS74)", CMD)?;
        s.command("dltlib LIB(QS74)", CMD)?;
        let sts = s.command("dspsrvsts", CMD)?;
        want(&sts, "DSPSRVSTS", &["T74"])?;
        let end = s.command("endtrc SSNID(T74) DTAOPT(*LIB) DTALIB(QGPL)", CMD)?;
        want(&end, "ENDTRC", &["T74", "ended with"])?;
        let prt = s.command("prttrc DTAMBR(T74) DTALIB(QGPL)", CMD)?;
        want(&prt, "PRTTRC", &["T74", "printed"])?;
        let spooled = s.command("wrksplf", CMD)?;
        want(&spooled, "spooled files", &["QPSRVDMP", "QPSRVTRCJ"])?;
        let log = s.command("svc.liclog 0500", CMD)?;
        want(&log, "LIC log", &["0500 0001"])?;
        // Leave the system as it was found.
        s.command(&format!("svc.sysdump delete {name}"), CMD).ok();
        s.command("svc.obj dlt *USRSPC QGPL/T74", CMD).ok();
        Ok(format!(
            "problem {id} from a severe QSYSOPR message; DMPOBJ and {name} logged problems; T74 traced and printed"
        ))
    }
}
