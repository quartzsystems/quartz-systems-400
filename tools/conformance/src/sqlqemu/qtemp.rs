// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 136: one QTEMP per job, for CL and SQL alike.

use std::time::Duration;

use super::{Tn, setup, sql};
use crate::qemu::session::Session;
use crate::steps::{CMD, SRC_SEQUENCE, Suite};

/// A query typed on Enter SQL Statements: Display Data's screen.
fn query(tn: &mut Tn, text: &str) -> Result<String, String> {
    tn.run(&format!(
        "key eraseinput\npick \"===>\" {text}\nkey enter\nwait \"Display Data\"\nsettle 800\n"
    ))?;
    let shown = tn.screen();
    tn.run("key f3\nwait \"Enter SQL Statements\"\n")?;
    Ok(shown)
}

fn stmt(tn: &mut Tn, text: &str) -> Result<String, String> {
    tn.run(&format!(
        "key eraseinput\npick \"===>\" {text}\nkey enter\nsettle 1500\nunlock\n"
    ))?;
    Ok(tn.screen())
}

/// The serial log's lines after `from` that hold `word`.
fn lines_with(log: &str, from: usize, word: &str) -> Vec<String> {
    log.get(from..)
        .unwrap_or("")
        .lines()
        .filter(|l| l.contains(word))
        .map(str::to_string)
        .collect()
}

impl Suite {
    /// Step 136: `CRTPF QTEMP/X` from Command Entry, then RUNSQL and
    /// STRSQL in the same job see it; a declared global temporary table
    /// lives there too; another job does not see it; signing off drops
    /// it, and the next IPL finds nothing left to purge.
    pub(crate) fn sql_qtemp(&self, s: &mut Session) -> Result<String, String> {
        setup(s)?;
        let mut tn = Tn::open(self)?;
        tn.cmd("CRTPF FILE(QTEMP/X) RCDLEN(20)", "")?;
        tn.cmd(
            "RUNSQL SQL('INSERT INTO QTEMP/X VALUES(''QTEMPROW'')') COMMIT(*NONE)",
            "",
        )?;
        tn.run(
            "key eraseinput\ntype strsql\nkey enter\nwait \"Enter SQL Statements\"\nsettle 1000\n",
        )?;
        let x = query(&mut tn, "select * from qtemp/x")?;
        if !x.contains("QTEMPROW") {
            return Err(format!(
                "STRSQL does not see the row RUNSQL put in QTEMP/X:\n{x}"
            ));
        }
        stmt(&mut tn, "declare global temporary table gtt (a int)")?;
        stmt(&mut tn, "insert into qtemp/gtt values (42)")?;
        let g = query(&mut tn, "select a * 10 as big from qtemp/gtt")?;
        if !g.contains("420") {
            return Err(format!(
                "the global temporary table did not answer 420:\n{g}"
            ));
        }
        // Another job, the console's, has a QTEMP of its own.
        let other = sql(s, "SELECT * FROM QTEMP/X")?;
        if !other.contains("SQL0204") {
            return Err(format!("another job sees QTEMP/X:\n{other}"));
        }
        tn.run(
            "key eraseinput\nkey f3\nwait \"Exit Interactive SQL\"\ncursor 5 31\nkey eraseeof\ntype 2\nkey enter\nwait \"Command Entry\"\n",
        )?;
        let mark = s.log.len();
        tn.close()?;
        std::thread::sleep(Duration::from_secs(3));
        let _ = s.command("WRKACTJOB", CMD)?;
        let errs = lines_with(&s.log, mark, "QTEMP of");
        if !errs.is_empty() {
            return Err(format!(
                "the job's end did not drop its QTEMP:\n{}",
                errs.join("\n")
            ));
        }
        // An IPL: nothing is left for the purge of ended jobs' QTEMPs.
        let mark = s.log.len();
        s.send("pwrdwn restart")?;
        s.expect("SRC D1001090", Duration::from_secs(60))?;
        s.expect_in_order(SRC_SEQUENCE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let left = lines_with(&s.log, mark, "SQL QTEMP");
        if left.iter().any(|l| l.contains("dropped")) {
            return Err(format!(
                "the IPL found QTEMP libraries left:\n{}",
                left.join("\n")
            ));
        }
        crate::tcp::guest_address(s)?;
        Ok("CRTPF QTEMP/X, RUNSQL INSERT and STRSQL SELECT in one job; DECLARE GLOBAL TEMPORARY TABLE answered 420; the console's job got SQL0204; sign-off dropped QTEMP and the IPL purged nothing".into())
    }
}
