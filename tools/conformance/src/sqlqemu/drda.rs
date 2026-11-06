// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 134: an SQLRPGLE program as a DRDA client of this system,
//! under a database monitor.

use super::{LIB, Tn, loop_rdb, put, setup, sql, sql_has};
use crate::qemu::session::Session;
use crate::steps::Suite;

/// CONNECT to `LOOP`, a query there, CONNECT RESET.
const DRDACLI: &str = "**FREE
ctl-opt dftactgrp(*no);
dcl-s n int(10);
dcl-s w char(10);
dcl-s msg char(50);
exec sql set option commit = *none;
exec sql connect to LOOP;
msg = 'connect ' + %char(sqlcode);
dsply msg;
exec sql select count(*), max(w) into :n, :w from sqlq/drdat;
msg = 'remote rows ' + %char(n) + ' ' + %trim(w) + ' code ' + %char(sqlcode);
dsply msg;
exec sql connect reset;
msg = 'reset ' + %char(sqlcode);
dsply msg;
*inlr = *on;
";

impl Suite {
    /// Step 134: `CRTSQLRPGI` builds a program that connects to the
    /// *RDB entry `LOOP` (this system through its DDM server), reads a
    /// count there and resets; `STRDBMON` around the call writes its
    /// statements to the monitor file.
    pub(crate) fn sql_drda(&self, s: &mut Session) -> Result<String, String> {
        setup(s)?;
        loop_rdb(s)?;
        let _ = sql(s, &format!("DROP TABLE {LIB}/DRDAT"))?;
        let _ = sql(s, &format!("DROP TABLE {LIB}/DBMON"))?;
        sql_has(s, &format!("CREATE TABLE {LIB}/DRDAT (N INT, W CHAR(10))"), &["OK"])?;
        sql_has(
            s,
            &format!("INSERT INTO {LIB}/DRDAT VALUES (1, 'ONE'), (2, 'TWO'), (3, 'THREE')"),
            &["3 row(s)"],
        )?;
        put(s, "QRPGLESRC", "DRDACLI", "SQLRPGLE", DRDACLI)?;
        let mut tn = Tn::open(self)?;
        tn.cmd(
            &format!("CRTSQLRPGI OBJ({LIB}/DRDACLI) SRCFILE({LIB}/QRPGLESRC) COMMIT(*NONE)"),
            "Program DRDACLI created",
        )?;
        tn.cmd(&format!("STRDBMON OUTFILE({LIB}/DBMON) JOB(*)"), "")?;
        let shown = tn.cmd(&format!("CALL PGM({LIB}/DRDACLI)"), "reset ")?;
        tn.cmd("ENDDBMON JOB(*)", "")?;
        tn.close()?;
        for want in ["connect 0", "remote rows 3 TWO code 0", "reset 0"] {
            if !shown.contains(want) {
                return Err(format!("the program did not show {want:?}:\n{shown}"));
            }
        }
        // The routed statements are monitored in the requester's job:
        // both CONNECTs and the query run at LOOP.
        let kinds = sql_has(
            s,
            &format!("SELECT QQRID, COUNT(*) FROM {LIB}/DBMON GROUP BY QQRID ORDER BY QQRID"),
            &["row(s)"],
        )?;
        let texts = sql(s, &format!("SELECT QQ1000 FROM {LIB}/DBMON WHERE QQRID = 1000"))?;
        let n = texts.lines().filter(|l| l.to_ascii_uppercase().contains("CONNECT")).count();
        if n < 2 || !texts.to_ascii_uppercase().contains("DRDAT") {
            return Err(format!("the monitor lacks the routed statements (CONNECT TO, CONNECT RESET, the query at LOOP):\n{texts}"));
        }
        let ids: Vec<&str> = kinds
            .lines()
            .filter_map(|l| l.split_whitespace().next())
            .filter(|w| w.len() == 4 && w.chars().all(|c| c.is_ascii_digit()))
            .collect();
        Ok(format!(
            "CRTSQLRPGI built DRDACLI; CONNECT TO LOOP, 3 rows read there, CONNECT RESET, each SQLCODE 0; the monitor file holds record types {} with {n} CONNECT statements and the query run at LOOP",
            ids.join(" ")
        ))
    }
}
