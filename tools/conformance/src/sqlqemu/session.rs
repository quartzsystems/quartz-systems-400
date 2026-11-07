// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 138: a saved STRSQL session across a sign-off and a sign-on.

use super::strsql::stmt;
use super::{LIB, Tn, setup, sql, sql_has};
use crate::qemu::session::Session;
use crate::steps::Suite;

/// F3 on Enter SQL Statements, then an option of Exit Interactive SQL.
fn exit(tn: &mut Tn, option: u8) -> Result<(), String> {
    tn.run(&format!(
        "key eraseinput\nkey f3\nwait \"Exit Interactive SQL\"\ncursor 5 31\nkey eraseeof\ntype {option}\nkey enter\nwait \"Command Entry\"\n"
    ))
}

impl Suite {
    /// Step 138: STRSQL in one sign-on runs two statements and exits
    /// with option 1 (save and exit); the user signs off. A new sign-on's
    /// STRSQL shows them again with SQL6102 (F1 on it); it leaves with option 2.
    pub(crate) fn sql_session(&self, s: &mut Session) -> Result<String, String> {
        setup(s)?;
        let _ = sql(s, &format!("DROP TABLE {LIB}/SESST"))?;
        let lib = LIB.to_ascii_lowercase();
        let mut tn = Tn::open(self)?;
        tn.run("key eraseinput\ntype strsql naming(*sys) commit(*none)\nkey enter\nwait \"Enter SQL Statements\"\nsettle 1000\n")?;
        stmt(&mut tn, &format!("create table {lib}/sesst (n int)"))?;
        stmt(&mut tn, &format!("insert into {lib}/sesst values(138)"))?;
        exit(&mut tn, 1)?;
        tn.close()?;
        sql_has(
            s,
            &format!("SELECT N FROM {LIB}/SESST"),
            &["138", "1 row(s)"],
        )?;

        // Signed on again: a new job.
        let mut tn = Tn::open(self)?;
        tn.run(
            "key eraseinput\ntype strsql\nkey enter\nwait \"Enter SQL Statements\"\nsettle 1000\n",
        )?;
        let resumed = tn.screen();
        let back = resumed.contains(&format!("insert into {lib}/sesst values(138)"))
            && resumed.contains(&format!("create table {lib}/sesst"));
        let rows = tn.d.term.text_rows();
        let at = rows
            .iter()
            .rposition(|r| r.contains("saved when STRSQL"))
            .map(|i| (i + 1, rows[i].len() - rows[i].trim_start().len() + 1));
        let id = match at {
            Some((r, c)) => tn.msgid_at(r, c)?,
            None => "no message".into(),
        };
        let result = if back && id == "SQL6102" {
            Ok("STRSQL saved with option 1, signed off; the next sign-on's STRSQL showed both statements again with SQL6102".to_string())
        } else {
            Err(format!(
                "after a new sign-on STRSQL showed {id}, statements back: {back}:\n{resumed}"
            ))
        };
        for _ in 0..3 {
            if tn.screen().contains("Enter SQL Statements") && !tn.screen().contains("Help") {
                break;
            }
            let _ = tn.run("key f12\nsettle 800\n");
        }
        exit(&mut tn, 2)?;
        tn.close()?;
        result
    }
}
