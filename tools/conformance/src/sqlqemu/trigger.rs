// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 135: triggers fired by an RPG program's write.

use super::{LIB, Tn, put, setup, sql, sql_has};
use crate::qemu::session::Session;
use crate::steps::Suite;

/// The SQL before trigger: it stamps the new row.
const TRGB: &str = "\
CREATE TRIGGER SQLQ/TRGTB BEFORE INSERT ON SQLQ/TRGT
  REFERENCING NEW AS NR FOR EACH ROW MODE DB2ROW
  SET NR.STAMP = 'BEFORE';
";

/// The external after trigger: logs the buffer's file, event, time
/// and the new record's key and stamp.
const TRGAFT: &str = "**FREE
ctl-opt dftactgrp(*no) actgrp(*caller);
dcl-f TRGLOG usage(*output) rename(TRGLOG:LOGR);
dcl-pi *n;
  buf char(4096) options(*varsize);
  len int(10);
end-pi;
dcl-ds hdr qualified based(ph);
  file char(10);
  lib char(10);
  mbr char(10);
  event char(1);
  time char(1);
  lock char(1);
  res1 char(3);
  ccsid int(10);
  rrn int(10);
  res2 char(4);
  oldoff int(10);
  oldlen int(10);
  oldnoff int(10);
  oldnlen int(10);
  newoff int(10);
  newlen int(10);
end-ds;
dcl-ds rec qualified based(pr);
  id int(10);
  name char(10);
  stamp char(10);
end-ds;
dcl-s ph pointer;
dcl-s pr pointer;
ph = %addr(buf);
pr = ph + hdr.newoff;
LFILE = hdr.file;
LEVENT = hdr.event;
LTIME = hdr.time;
LID = rec.id;
LSTAMP = rec.stamp;
write LOGR;
return;
";

/// The program whose write fires both.
const RPGWRT: &str = "**FREE
ctl-opt dftactgrp(*no);
dcl-f TRGT usage(*output) rename(TRGT:TRGR);
ID = 1;
NAME = 'RPGWRITE';
STAMP = 'NONE';
write TRGR;
dsply 'RPG write done';
*inlr = *on;
";

impl Suite {
    /// Step 135: an RPG `WRITE` to a table with an SQL `BEFORE INSERT`
    /// trigger and an external `*AFTER *INSERT` trigger program (RPG,
    /// `ADDPFTRG`): the row carries the before trigger's stamp, and the
    /// after trigger saw it in its buffer.
    pub(crate) fn sql_rpg_triggers(&self, s: &mut Session) -> Result<String, String> {
        setup(s)?;
        let _ = sql(s, &format!("DROP TABLE {LIB}/TRGT"))?;
        let _ = sql(s, &format!("DROP TABLE {LIB}/TRGLOG"))?;
        sql_has(
            s,
            &format!("CREATE TABLE {LIB}/TRGT (ID INT NOT NULL, NAME CHAR(10), STAMP CHAR(10))"),
            &["OK"],
        )?;
        sql_has(
            s,
            &format!(
                "CREATE TABLE {LIB}/TRGLOG (LFILE CHAR(10), LEVENT CHAR(1), LTIME CHAR(1), LID INT, LSTAMP CHAR(10))"
            ),
            &["OK"],
        )?;
        put(s, "QSQLSRC", "TRGB", "SQL", TRGB)?;
        put(s, "QRPGLESRC", "TRGAFT", "RPGLE", TRGAFT)?;
        put(s, "QRPGLESRC", "RPGWRT", "RPGLE", RPGWRT)?;
        let mut tn = Tn::open(self)?;
        tn.cmd(
            &format!("RUNSQLSTM SRCFILE({LIB}/QSQLSRC) SRCMBR(TRGB) COMMIT(*NONE)"),
            "",
        )?;
        sql_has(
            s,
            &format!(
                "SELECT TRIGGER_NAME, ACTION_TIMING FROM QSYS2/SYSTRIGGERS WHERE TRIGGER_SCHEMA = '{LIB}'"
            ),
            &["TRGTB"],
        )?;
        for p in ["TRGAFT", "RPGWRT"] {
            tn.cmd(
                &format!("CRTBNDRPG PGM({LIB}/{p}) SRCFILE({LIB}/QRPGLESRC)"),
                &format!("Program {p} created"),
            )?;
        }
        tn.cmd(
            &format!(
                "ADDPFTRG FILE({LIB}/TRGT) TRGTIME(*AFTER) TRGEVENT(*INSERT) PGM({LIB}/TRGAFT)"
            ),
            "",
        )?;
        let shown = tn.cmd(&format!("CALL PGM({LIB}/RPGWRT)"), "RPG write done")?;
        tn.close()?;
        let _ = shown;
        sql_has(
            s,
            &format!("SELECT ID, NAME, STAMP FROM {LIB}/TRGT"),
            &["RPGWRITE", "BEFORE", "1 row(s)"],
        )?;
        let log = sql_has(
            s,
            &format!("SELECT LFILE, LEVENT, LTIME, LID, LSTAMP FROM {LIB}/TRGLOG"),
            &["TRGT", "BEFORE", "1 row(s)"],
        )?;
        let row = log
            .lines()
            .find(|l| l.contains("TRGT"))
            .unwrap_or("")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !row.contains("TRGT 1 1 1 BEFORE") {
            return Err(format!(
                "the after trigger's buffer was not an after insert of row 1 stamped BEFORE:\n{log}"
            ));
        }
        Ok(format!(
            "RPG WRITE fired the SQL before trigger (the row is stamped BEFORE) and the RPG after trigger program (logged {row})"
        ))
    }
}
