// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 137: external RPGLE and CL procedures, called from STRSQL,
//! embedded SQL and JDBC.

use super::{LIB, Tn, put, setup, sql, sql_has};
use crate::hostsvr::run_class;
use crate::qemu::session::Session;
use crate::steps::Suite;

/// The RPGLE procedure: its rows up to `N` as a result set, a log row.
const EXTRPG: &str = "**FREE
ctl-opt dftactgrp(*no) actgrp(*caller);
dcl-pi *n;
  n int(10);
end-pi;
exec sql set option commit = *none;
exec sql declare c1 cursor with return to client for
  select id, name from sqlq/extt where id <= :n order by id;
exec sql insert into sqlq/extlog values('RPG', :n);
exec sql open c1;
return;
";

/// The CL procedure: its `INOUT` parameter set.
const EXTCL: &str = "\
             PGM        PARM(&MSG)
             DCL        VAR(&MSG) TYPE(*CHAR) LEN(10)
             CHGVAR     VAR(&MSG) VALUE('FROM CL')
             ENDPGM
";

/// Embedded SQL calling both.
const EXTCALL: &str = "**FREE
ctl-opt dftactgrp(*no);
dcl-s cl char(10) inz('RPG');
dcl-s n int(10);
dcl-s msg char(50);
exec sql set option commit = *none;
exec sql call sqlq/extrpg(3);
msg = 'rpg call ' + %char(sqlcode);
dsply msg;
exec sql call sqlq/extcl(:cl);
msg = 'cl call ' + %char(sqlcode) + ' ' + %trim(cl);
dsply msg;
exec sql select count(*) into :n from sqlq/extlog;
msg = 'logged ' + %char(n);
dsply msg;
*inlr = *on;
";

impl Suite {
    /// Step 137: `CRTSQLRPGI` and `CRTBNDCL` make the programs, `CREATE
    /// PROCEDURE ... LANGUAGE RPGLE` (one result set) and `LANGUAGE CL`
    /// name them; STRSQL calls the RPGLE one, an embedded-SQL program
    /// calls both, and JDBC reads the RPGLE one's result set and the CL
    /// one's `INOUT` value.
    pub(crate) fn sql_extproc(&self, s: &mut Session) -> Result<String, String> {
        let (java, cp) = self.jdbc_ready(s)?;
        setup(s)?;
        for t in ["EXTT", "EXTLOG"] {
            let _ = sql(s, &format!("DROP TABLE {LIB}/{t}"))?;
        }
        for p in ["EXTRPG", "EXTCL"] {
            let _ = sql(s, &format!("DROP PROCEDURE {LIB}/{p}"))?;
        }
        sql_has(
            s,
            &format!("CREATE TABLE {LIB}/EXTT (ID INT NOT NULL, NAME VARCHAR(10))"),
            &["OK"],
        )?;
        sql_has(
            s,
            &format!("INSERT INTO {LIB}/EXTT VALUES (1, 'ANN'), (2, 'BOB'), (3, 'CY')"),
            &["3 row(s)"],
        )?;
        sql_has(
            s,
            &format!("CREATE TABLE {LIB}/EXTLOG (WHO VARCHAR(10), N INT)"),
            &["OK"],
        )?;
        put(s, "QRPGLESRC", "EXTRPG", "SQLRPGLE", EXTRPG)?;
        put(s, "QRPGLESRC", "EXTCALL", "SQLRPGLE", EXTCALL)?;
        put(s, "QCLSRC", "EXTCL", "CLLE", EXTCL)?;
        let mut tn = Tn::open(self)?;
        tn.cmd(
            &format!("CRTSQLRPGI OBJ({LIB}/EXTRPG) SRCFILE({LIB}/QRPGLESRC) COMMIT(*NONE)"),
            "Program EXTRPG created",
        )?;
        tn.cmd(
            &format!("CRTSQLRPGI OBJ({LIB}/EXTCALL) SRCFILE({LIB}/QRPGLESRC) COMMIT(*NONE)"),
            "Program EXTCALL created",
        )?;
        let made = s.command(
            &format!("CRTBNDCL PGM({LIB}/EXTCL) SRCFILE({LIB}/QCLSRC)"),
            std::time::Duration::from_secs(60),
        )?;
        if !made.contains("created") {
            return Err(format!("CRTBNDCL: {made}"));
        }
        sql_has(
            s,
            &format!(
                "CREATE PROCEDURE {LIB}/EXTRPG (IN N INTEGER) LANGUAGE RPGLE EXTERNAL NAME {LIB}/EXTRPG PARAMETER STYLE GENERAL DYNAMIC RESULT SETS 1"
            ),
            &["OK"],
        )?;
        sql_has(
            s,
            &format!(
                "CREATE PROCEDURE {LIB}/EXTCL (INOUT MSG CHAR(10)) LANGUAGE CL EXTERNAL NAME {LIB}/EXTCL PARAMETER STYLE GENERAL"
            ),
            &["OK"],
        )?;

        // STRSQL: the call runs; its result set is not shown, as STRSQL
        // shows none.
        tn.run("key eraseinput\ntype strsql commit(*none) naming(*sys)\nkey enter\nwait \"Enter SQL Statements\"\nsettle 1000\n")?;
        let after = super::strsql::stmt(
            &mut tn,
            &format!("call {}/extrpg(2)", LIB.to_ascii_lowercase()),
        )?;
        tn.run("key eraseinput\nkey f3\nwait \"Exit Interactive SQL\"\ncursor 5 31\nkey eraseeof\ntype 2\nkey enter\nwait \"Command Entry\"\n")?;
        sql_has(
            s,
            &format!("SELECT WHO, N FROM {LIB}/EXTLOG WHERE WHO = 'RPG' AND N = 2"),
            &["1 row(s)"],
        )
        .map_err(|e| format!("{e}\nSTRSQL showed:\n{after}"))?;

        // Embedded SQL.
        let shown = tn.cmd(&format!("CALL PGM({LIB}/EXTCALL)"), "logged ")?;
        tn.close()?;
        let rpg_ok = shown.contains("rpg call 0") || shown.contains("rpg call 466");
        if !rpg_ok || !shown.contains("cl call 0 FROM CL") || !shown.contains("logged 2") {
            return Err(format!(
                "EXTCALL did not show rpg call 0 (or 466), cl call 0 FROM CL and logged 2:\n{shown}"
            ));
        }

        // JDBC through the database host server.
        let out = run_class(self, &java, &cp, "ExtProc")?;
        let _ = std::fs::write(self.out_dir.join("sql-extproc.log"), &out);
        for want in [
            "ok   rpg",
            "result set first=true",
            "cols ID:INTEGER NAME:VARCHAR",
            "row |1 |ANN",
            "row |2 |BOB",
            "rows=2",
            "sets=1",
            "ok   cl",
            "msg=[FROM CL   ]",
        ] {
            if !out.contains(want) {
                return Err(format!("JDBC: no {want:?} in:\n{out}"));
            }
        }
        sql_has(
            s,
            &format!("SELECT WHO, N FROM {LIB}/EXTLOG WHERE WHO = 'RPG'"),
            &["3 row(s)"],
        )?;
        Ok("EXTRPG (RPGLE, one result set) and EXTCL (CL, INOUT) made and called: from STRSQL, from EXTCALL's embedded SQL (FROM CL back), from JDBC (the result set's 2 rows and FROM CL); a log row for each RPGLE call".into())
    }
}
