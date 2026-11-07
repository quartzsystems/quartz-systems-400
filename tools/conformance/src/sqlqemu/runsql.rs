// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 131: RUNSQL and RUNSQLSTM from Command Entry.

use super::{LIB, Tn, put, setup, spooled, sql_has};
use crate::qemu::session::Session;
use crate::steps::Suite;

/// The statements RUNSQLSTM runs: two tables, an SQL procedure and an
/// SQL trigger that logs each insert.
const RUNT: &str = "\
-- The tables, the procedure and the trigger of step 131.
CREATE TABLE SQLQ/RUNT (N INT NOT NULL, NOTE VARCHAR(20));
CREATE TABLE SQLQ/RUNLOG (N INT, WHAT VARCHAR(20));
CREATE PROCEDURE SQLQ/ADDROW (IN P INT)
  LANGUAGE SQL
  BEGIN
    INSERT INTO SQLQ/RUNT (N, NOTE) VALUES (P, 'CALLED');
  END;
CREATE TRIGGER SQLQ/RUNTTRG AFTER INSERT ON SQLQ/RUNT
  REFERENCING NEW AS NR FOR EACH ROW MODE DB2ROW
  INSERT INTO SQLQ/RUNLOG VALUES (NR.N, 'TRIGGER');
";

impl Suite {
    /// Step 131: RUNSQLSTM with `OUTPUT(*PRINT)` makes a procedure and
    /// a trigger and spools its listing; RUNSQL calls the procedure and
    /// inserts a row, both firing the trigger; a SELECT is refused.
    pub(crate) fn sql_runsql(&self, s: &mut Session) -> Result<String, String> {
        setup(s)?;
        put(s, "QSQLSRC", "RUNT", "SQL", RUNT)?;
        let mut tn = Tn::open(self)?;
        tn.cmd(
            &format!("RUNSQLSTM SRCFILE({LIB}/QSQLSRC) SRCMBR(RUNT) COMMIT(*NONE) OUTPUT(*PRINT)"),
            "",
        )?;
        sql_has(
            s,
            &format!("SELECT ROUTINE_NAME FROM QSYS2/SYSROUTINES WHERE ROUTINE_SCHEMA = '{LIB}'"),
            &["ADDROW"],
        )?;
        sql_has(
            s,
            &format!("SELECT TRIGGER_NAME FROM QSYS2/SYSTRIGGERS WHERE TRIGGER_SCHEMA = '{LIB}'"),
            &["RUNTTRG"],
        )?;
        let listing = spooled(s, "RUNT")?;
        for want in [
            "CREATE PROCEDURE SQLQ/ADDROW",
            "CREATE TRIGGER SQLQ/RUNTTRG",
        ] {
            if !listing.contains(want) {
                return Err(format!("the listing lacks {want:?}:\n{listing}"));
            }
        }
        tn.cmd(
            &format!("RUNSQL SQL('CALL {LIB}/ADDROW(5)') COMMIT(*NONE)"),
            "",
        )?;
        tn.cmd(
            &format!("RUNSQL SQL('INSERT INTO {LIB}/RUNT VALUES(7, ''DIRECT'')') COMMIT(*NONE)"),
            "",
        )?;
        sql_has(
            s,
            &format!("SELECT N, NOTE FROM {LIB}/RUNT ORDER BY N"),
            &["CALLED", "DIRECT"],
        )?;
        let log = sql_has(
            s,
            &format!("SELECT N, WHAT FROM {LIB}/RUNLOG ORDER BY N"),
            &["TRIGGER", "2 row(s)"],
        )?;
        let shown = tn.cmd(
            &format!("RUNSQL SQL('SELECT * FROM {LIB}/RUNT') COMMIT(*NONE)"),
            "SQL9010",
        )?;
        let after = shown.rsplit("RUNSQL SQL('SELECT").next().unwrap_or("");
        let ids: Vec<&str> = ["SQL0084", "SQL9010"]
            .into_iter()
            .filter(|i| after.contains(i))
            .collect();
        if ids.len() != 2 {
            return Err(format!(
                "the SELECT was not refused with SQL0084 and SQL9010:\n{shown}"
            ));
        }
        tn.close()?;
        let _ = log;
        Ok(format!(
            "RUNSQLSTM made ADDROW and RUNTTRG and spooled RUNT ({} lines); RUNSQL CALL and INSERT each fired the trigger; RUNSQL SELECT refused with {}",
            listing.lines().count(),
            ids.join(", ")
        ))
    }
}
